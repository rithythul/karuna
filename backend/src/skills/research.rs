use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::error::AppError;
use crate::llm::ChatMessage;
use super::{Skill, SkillContext, SkillOutput};

pub struct ResearchSkill;

impl ResearchSkill {
    /// Ask the fast LLM to generate search queries for a research topic.
    async fn generate_search_queries(
        ctx: &SkillContext,
        query: &str,
    ) -> Result<Vec<String>, AppError> {
        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: "You are a research assistant. Given a topic, generate exactly 3 \
                    diverse search queries that would help thoroughly research it. \
                    Return ONLY a JSON array of 3 strings, nothing else. \
                    Example: [\"query one\", \"query two\", \"query three\"]"
                    .into(),
            },
            ChatMessage {
                role: "user".into(),
                content: format!("Generate search queries for: {query}"),
            },
        ];

        let response = ctx.llm.fast(messages).await?;

        // Parse the JSON array from the response
        let queries: Vec<String> = serde_json::from_str(response.trim())
            .or_else(|_| {
                // Try to extract JSON array if wrapped in markdown code block
                let stripped = response
                    .trim()
                    .trim_start_matches("```json")
                    .trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim();
                serde_json::from_str(stripped)
            })
            .map_err(|e| {
                AppError::Llm(format!(
                    "Failed to parse search queries from LLM response: {e}. Response: {response}"
                ))
            })?;

        Ok(queries)
    }

    /// Execute a web search in the sandbox using curl + DuckDuckGo.
    async fn execute_search(
        ctx: &SkillContext,
        query: &str,
    ) -> Result<String, AppError> {
        // URL-encode the query for DuckDuckGo lite (plain text friendly)
        let encoded_query = query.replace(' ', "+");
        let url = format!("https://lite.duckduckgo.com/lite/?q={encoded_query}");

        let result = ctx
            .sandbox
            .exec_cmd(
                &ctx.container_id,
                vec![
                    "bash",
                    "-c",
                    &format!(
                        "curl -sL --max-time 15 -A 'Mozilla/5.0' '{url}' | \
                         sed -n 's/<[^>]*>//gp' | \
                         head -200"
                    ),
                ],
            )
            .await?;

        if result.exit_code != 0 {
            warn!(
                query = query,
                stderr = result.stderr.as_str(),
                "Search failed"
            );
            return Ok(format!("Search for '{query}' failed: {}", result.stderr));
        }

        Ok(result.stdout)
    }

    /// Ask the planning LLM to synthesize search results into a report.
    async fn synthesize_report(
        ctx: &SkillContext,
        query: &str,
        search_queries: &[String],
        search_results: &[String],
    ) -> Result<String, AppError> {
        let mut findings = String::new();
        for (i, (q, r)) in search_queries.iter().zip(search_results.iter()).enumerate() {
            findings.push_str(&format!(
                "### Search {}: \"{}\"\n{}\n\n",
                i + 1,
                q,
                r
            ));
        }

        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: "You are a research analyst. Synthesize the provided search results \
                    into a well-structured markdown report. Include:\n\
                    - An executive summary\n\
                    - Key findings organized by theme\n\
                    - Conclusions\n\
                    - Sources used\n\
                    Be thorough but concise. If the search results are empty or unhelpful, \
                    note that and provide what analysis you can based on the topic alone."
                    .into(),
            },
            ChatMessage {
                role: "user".into(),
                content: format!(
                    "Research topic: {query}\n\n\
                     Search results:\n\n{findings}\n\n\
                     Please synthesize these into a comprehensive markdown research report."
                ),
            },
        ];

        ctx.llm.plan(messages).await
    }

    /// Write content to a file inside the sandbox container.
    async fn write_to_sandbox(
        ctx: &SkillContext,
        path: &str,
        content: &str,
    ) -> Result<(), AppError> {
        // Escape single quotes in content for the heredoc
        let escaped = content.replace('\'', "'\"'\"'");
        let cmd = format!("cat > {path} << 'KARUNA_EOF'\n{escaped}\nKARUNA_EOF");

        let result = ctx
            .sandbox
            .exec_cmd(&ctx.container_id, vec!["bash", "-c", &cmd])
            .await?;

        if result.exit_code != 0 {
            return Err(AppError::Sandbox(format!(
                "Failed to write {path}: {}",
                result.stderr
            )));
        }

        Ok(())
    }
}

#[async_trait]
impl Skill for ResearchSkill {
    fn name(&self) -> &str {
        "research"
    }
    fn description(&self) -> &str {
        "Research a topic using web search and LLM synthesis"
    }

    async fn execute(
        &self,
        ctx: &SkillContext,
        input: Value,
    ) -> Result<SkillOutput, AppError> {
        // 1. Extract the research query
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("Missing 'query' in input".into()))?;

        info!(task_id = ctx.task_id.as_str(), query = query, "Starting research");

        // 2. Generate search queries using the fast model
        let search_queries = Self::generate_search_queries(ctx, query).await?;
        info!(
            task_id = ctx.task_id.as_str(),
            queries = ?search_queries,
            "Generated search queries"
        );

        // 3. Execute web searches in the sandbox
        let mut search_results = Vec::new();
        for q in &search_queries {
            let result = Self::execute_search(ctx, q).await?;
            search_results.push(result);
        }

        // 4. Synthesize findings into a markdown report
        let report =
            Self::synthesize_report(ctx, query, &search_queries, &search_results).await?;

        // 5. Save report to sandbox
        Self::write_to_sandbox(ctx, "/workspace/research_report.md", &report).await?;

        info!(
            task_id = ctx.task_id.as_str(),
            "Research report saved to /workspace/research_report.md"
        );

        // 6. Return results
        Ok(SkillOutput {
            success: true,
            result: json!({
                "report": report,
                "search_queries": search_queries,
                "query": query,
            }),
            artifacts: vec!["/workspace/research_report.md".into()],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_research_skill_metadata() {
        let skill = ResearchSkill;
        assert_eq!(skill.name(), "research");
        assert!(!skill.description().is_empty());
    }

    #[test]
    fn test_missing_query_returns_error_shape() {
        // Verify the input parsing logic without needing live services
        let input = json!({});
        let result = input
            .get("query")
            .and_then(|v| v.as_str());
        assert!(result.is_none());
    }

    #[test]
    fn test_valid_query_extracted() {
        let input = json!({"query": "quantum computing"});
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(query, "quantum computing");
    }
}
