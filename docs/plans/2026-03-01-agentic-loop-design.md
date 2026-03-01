# Agentic Loop Architecture Design

**Date:** 2026-03-01
**Status:** Approved

## Goal

Evolve Karuna from one-shot skill execution to autonomous agents with think-act-observe reasoning loops.

**Current:** `Orchestrator -> skill.execute(input) -> output` (single function call)
**Target:** `Orchestrator -> Agent(goal) -> [think -> act -> observe]* -> output` (agentic loop)

## Decisions

- Each agent has its own LLM reasoning loop (not orchestrator-driven)
- Agents can delegate sub-goals to other agents (depth-limited to 3)
- Turn limit per agent (default 20 think-act cycles) for cost control
- agent-browser (Vercel Labs) replaces Playwright script generation for browser automation
- Include a new API orchestration agent
- Natural language display messages for all tool calls (no raw tool names shown to users)

## Core Abstractions

### AgentTool

Atomic capability. A function the LLM can choose to call.

```rust
#[async_trait]
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;
    fn display_message(&self, params: &Value) -> String;
    async fn execute(&self, params: Value, sandbox: &SandboxHandle) -> Result<ToolResult, AppError>;
}

pub struct ToolResult {
    pub output: Value,
    pub artifacts: Vec<String>,
    pub display: String,
}
```

### Agent

Defines who the agent is and what tools it has. Does not contain loop logic.

```rust
#[async_trait]
pub trait Agent: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn system_prompt(&self) -> String;
    fn tools(&self) -> Vec<Arc<dyn AgentTool>>;
    fn max_turns(&self) -> u32 { 20 }
}
```

### AgentRuntime

Generic loop runner. Runs the think-act-observe cycle for any agent.

```
AgentRuntime::run(agent, goal, sandbox, event_emitter):
    messages = [system_prompt, user_goal]
    turns = 0
    artifacts = []

    loop:
        // THINK
        response = llm.chat(messages, tool_definitions)

        // DONE — LLM returned final answer
        if response.is_text_only():
            emit("agent_completed", {output, artifacts, turns})
            return AgentResult { output, artifacts }

        // ACT — execute chosen tools
        for tool_call in response.tool_calls:
            emit("agent_tool_call", {tool, params, display})
            result = execute_tool(tool_call, sandbox)
            emit("agent_tool_result", {tool, result, display})
            artifacts.extend(result.artifacts)

            // OBSERVE — feed back to LLM
            messages.push(tool_result(result))

        turns += 1
        if turns >= agent.max_turns():
            messages.push("Turn limit reached. Summarize your work.")
            final = llm.chat(messages)
            return AgentResult { output: final, artifacts }
```

### SandboxHandle

Thin wrapper tools receive. Hides Docker/container details.

```rust
pub struct SandboxHandle {
    sandbox: SandboxManager,
    container_id: String,
}

impl SandboxHandle {
    pub async fn exec(&self, cmd: &[&str]) -> Result<ExecResult, AppError>;
    pub async fn write_file(&self, path: &str, content: &str) -> Result<(), AppError>;
    pub async fn read_file(&self, path: &str) -> Result<String, AppError>;
}
```

## Tools

### Shared tools (all agents)

| Tool | Description |
|------|-------------|
| run_shell | Execute shell command in sandbox |
| read_file | Read file from sandbox |
| write_file | Write file to sandbox |
| delegate | Assign sub-goal to another agent (depth limit 3) |

### Browser agent tools (via agent-browser CLI)

| Tool | CLI command |
|------|-------------|
| navigate | agent-browser navigate url |
| snapshot | agent-browser snapshot |
| click | agent-browser click --ref ref |
| fill | agent-browser fill --ref ref value |
| extract | agent-browser extract --ref ref |
| screenshot | agent-browser screenshot |

### Code agent tools

| Tool | Description |
|------|-------------|
| run_code | Write and execute code (any language) |
| run_shell | Direct shell execution |
| read_file / write_file | File I/O |

### Research agent tools

| Tool | Description |
|------|-------------|
| web_search | Search via DuckDuckGo/curl |
| delegate | To BrowserAgent for deep page extraction |
| write_file | Save reports |

### API agent tools (new)

| Tool | Description |
|------|-------------|
| http_request | HTTP GET/POST/PUT/DELETE with headers, body, auth |
| run_code | Complex data transforms |
| read_file / write_file | File I/O |

## Agent Types

### BrowserAgent (replaces BrowseSkill)

- System prompt: Expert browser automation, step-by-step navigation
- Tools: navigate, snapshot, click, fill, extract, screenshot, run_shell, write_file, delegate
- agent-browser called step-by-step via CLI (not script generation)

### CodeAgent (replaces CodeSkill)

- System prompt: Expert programmer, write-execute-debug cycle
- Tools: run_code, run_shell, read_file, write_file, delegate
- Language detection and execution helpers move into run_code tool

### ResearchAgent (replaces ResearchSkill)

- System prompt: Research analyst, search-read-synthesize
- Tools: web_search, delegate (to BrowserAgent), write_file, read_file
- Agent decides its own search strategy (no more pre-generated queries)

### ApiAgent (new)

- System prompt: API integration specialist, call/parse/chain external APIs
- Tools: http_request, run_code, read_file, write_file, delegate

### DataAnalysisAgent (replaces DataAnalysisSkill)

- System prompt: Data analyst, analyze-visualize-report
- Tools: run_code, read_file, write_file, run_shell

### DeployAgent (replaces DeploySkill)

- System prompt: Deployment specialist
- Tools: run_shell, read_file, write_file, delegate

### FileAgent / ShellAgent

- Absorbed as tools available to other agents (not standalone agents)

## Sandbox Changes

- Install agent-browser CLI (npm package) in sandbox Dockerfile
- Keep Chromium (agent-browser uses it)
- Remove Playwright Python package
- agent-browser daemon starts on first CLI call, persists for session

Inter-agent delegation shares the same sandbox container per task.

## Orchestrator Changes

Stays lean. Changes:
- SkillRegistry becomes AgentRegistry
- execute_step_with_reflection() deleted (agents self-reflect in their loop)
- reflect_on_failure() / reflect_on_error() deleted
- Step execution calls AgentRuntime::run(agent, goal) instead of skill.execute(input)

Unchanged:
- create_plan() still LLM-powered planning
- replan() still adaptive re-planning on step failure
- synthesize_result() still final synthesis
- User memory, soul system, event emission

## Event Streaming

New WebSocket events for agent reasoning traces:

| Event | Data |
|-------|------|
| agent_started | agent, goal, step |
| agent_thinking | agent, thought |
| agent_tool_call | agent, tool, params, display |
| agent_tool_result | agent, tool, result_preview, display |
| agent_delegating | agent, child_agent, sub_goal |
| agent_completed | agent, output, artifacts, turns_used |
| agent_error | agent, error, turn |

Every event includes a display field with natural language for the UI.

## Frontend Changes

Task page renders nested reasoning traces with natural language:

```
Step 1: Research competitors

  Thinking    "I'll start by searching for the top project management tools"
  Searching   for competitor information
  Found       3 results about project management platforms
  Thinking    "I need detailed pricing. Let me check their websites."
  Browsing    asana.com to get pricing
    Opening     Asana's pricing page
    Reading     the pricing details
    Done        "Free, Starter $10.99, Advanced $24.99"
  Thinking    "I have enough data to write the comparison report"
  Saving      the research report
  Completed   Report with comparison table ready
```

Sub-agent traces render as collapsible nested sections. Raw tool params available via expand.

## What Gets Deleted

- backend/src/skills/ directory (all 8 files)
- Skill trait, SkillRegistry, SkillContext, SkillOutput
- Orchestrator reflection/retry methods
- Playwright Python dependency in sandbox

## What's New

- backend/src/agents/ directory (browser, code, research, api, data_analysis, deploy, mod)
- backend/src/tools/ directory (run_shell, read_file, write_file, run_code, navigate, click, fill, extract, snapshot, screenshot, web_search, http_request, delegate, mod)
- backend/src/agent_runtime.rs (generic agentic loop)
- Sandbox Dockerfile: agent-browser installation
- Frontend: nested reasoning trace rendering

## What Stays the Same

- Orchestrator planning, re-planning, result synthesis
- Models, DB schema, migrations
- Redis Pub/Sub, WebSocket streaming infrastructure
- SandboxManager
- Soul system, user memory
- Auth (KOOMPI KID OAuth)
- Config system
