# Agentic Loop Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace Hanuman's one-shot skill system with autonomous agents that run think-act-observe reasoning loops.

**Architecture:** Each agent gets a system prompt, a set of tools, and runs in a generic AgentRuntime loop. The LLM picks tools via OpenRouter's tool-calling API, the runtime executes them in the sandbox, and observations feed back to the LLM. The orchestrator plans steps and dispatches to agents instead of skills.

**Tech Stack:** Rust (Axum/Tokio), OpenRouter tool-calling API, agent-browser CLI (npm), Next.js frontend

**Design doc:** docs/plans/2026-03-01-agentic-loop-design.md

---

## Task 1: Add tool-calling support to LlmClient

**Files:**
- Modify: backend/src/llm.rs

**Why first:** Everything depends on the LLM being able to return structured tool calls instead of just text.

**Step 1:** Add tool-calling types (ToolDefinition, FunctionDefinition, ToolCall, FunctionCall, LlmResponse enum) after existing types in llm.rs.

**Step 2:** Add optional `tools` field to ChatRequest. Change `messages` field type to `Vec<serde_json::Value>` for flexibility with tool result messages.

**Step 3:** Update ChatChoice and ChatMessageResponse to handle optional content and optional tool_calls fields.

**Step 4:** Add `chat_with_tools()` method to LlmClient. It sends tools alongside messages. Parses response: if tool_calls present, returns LlmResponse::ToolCalls; otherwise returns LlmResponse::Text.

**Step 5:** Keep existing chat(), plan(), fast() methods unchanged. No breaking changes.

**Step 6:** Verify: `SQLX_OFFLINE=true cargo check`

**Step 7:** Commit: `feat: add tool-calling support to LlmClient`

---

## Task 2: Create the tools infrastructure

**Files:**
- Create: backend/src/tools/mod.rs
- Create: backend/src/tools/run_shell.rs
- Create: backend/src/tools/read_file.rs
- Create: backend/src/tools/write_file.rs
- Modify: backend/src/main.rs (add mod tools)

**Step 1:** Create tools/mod.rs with:
- SandboxHandle struct (wraps SandboxManager + container_id, provides exec/read_file/write_file)
- ToolResult struct (output: Value, artifacts: Vec<String>, display: String)
- AgentTool trait (name, description, parameters_schema, display_message, execute, to_tool_definition)

**Step 2:** Create tools/run_shell.rs - executes shell commands in sandbox with timeout.

**Step 3:** Create tools/read_file.rs - reads files from sandbox via cat.

**Step 4:** Create tools/write_file.rs - writes files to sandbox with mkdir -p for parents.

**Step 5:** Add `mod tools;` to main.rs.

**Step 6:** Verify: `SQLX_OFFLINE=true cargo check`

**Step 7:** Commit: `feat: add tools infrastructure with run_shell, read_file, write_file`

---

## Task 3: Create browser, code, search, and HTTP tools

**Files:**
- Create: backend/src/tools/navigate.rs
- Create: backend/src/tools/snapshot.rs
- Create: backend/src/tools/click.rs
- Create: backend/src/tools/fill.rs
- Create: backend/src/tools/extract.rs
- Create: backend/src/tools/screenshot.rs
- Create: backend/src/tools/run_code.rs
- Create: backend/src/tools/web_search.rs
- Create: backend/src/tools/http_request.rs
- Modify: backend/src/tools/mod.rs (add module declarations)

**Step 1:** Create 6 browser tools. Each formats an agent-browser CLI command, executes it in sandbox, returns output. Pattern: navigate(url), snapshot(), click(ref), fill(ref, value), extract(ref), screenshot(path).

**Step 2:** Create run_code.rs - writes code to file, executes with language-specific runner. Carries over language detection from old CodeSkill (extension_for, run_command helpers).

**Step 3:** Create web_search.rs - DuckDuckGo search via curl in sandbox (same approach as old ResearchSkill).

**Step 4:** Create http_request.rs - HTTP requests via curl in sandbox. Supports GET/POST/PUT/DELETE with headers and body. Parses status code from curl output.

**Step 5:** Add all module declarations to tools/mod.rs.

**Step 6:** Verify: `SQLX_OFFLINE=true cargo check`

**Step 7:** Commit: `feat: add browser, code, search, and HTTP tools`

---

## Task 4: Create the AgentRuntime and Agent trait

**Files:**
- Create: backend/src/agent_runtime.rs
- Modify: backend/src/main.rs (add mod agent_runtime)

**Step 1:** Create agent_runtime.rs with:
- AgentResult struct (output, artifacts, turns_used)
- Agent trait (name, description, system_prompt, tools, max_turns)
- AgentRegistry (register, get, list)
- AgentRuntime struct with run() method implementing the think-act-observe loop

**Step 2:** The run() method:
- Builds tool definitions from agent.tools()
- Loops: call llm.chat_with_tools -> if text, return final answer; if tool_calls, execute each, push observations
- Intercepts "delegate" tool specially (calls handle_delegation)
- Enforces max_turns limit
- Emits WebSocket events: agent_started, agent_tool_call, agent_tool_result, agent_delegating, agent_completed

**Step 3:** handle_delegation method:
- Checks depth < 3
- Looks up child agent in registry
- Calls self.run() recursively with depth + 1
- Same sandbox (shared container)

**Step 4:** Add `mod agent_runtime;` to main.rs.

**Step 5:** Verify: `SQLX_OFFLINE=true cargo check`

**Step 6:** Commit: `feat: add AgentRuntime with think-act-observe loop and delegation`

---

## Task 5: Create agent definitions

**Files:**
- Create: backend/src/agents/mod.rs
- Create: backend/src/agents/browser.rs
- Create: backend/src/agents/code.rs
- Create: backend/src/agents/research.rs
- Create: backend/src/agents/api.rs
- Create: backend/src/agents/data_analysis.rs
- Create: backend/src/agents/deploy.rs
- Modify: backend/src/main.rs (add mod agents)

Each agent implements the Agent trait with a system prompt and tool list.

**Step 1:** Create agents/mod.rs with default_registry() function that registers all agents.

**Step 2:** Create agents/browser.rs - BrowserAgent with navigate/snapshot/click/fill/extract/screenshot/run_shell/write_file/read_file tools.

**Step 3:** Create agents/code.rs - CodeAgent with run_code/run_shell/read_file/write_file tools.

**Step 4:** Create agents/research.rs - ResearchAgent with web_search/read_file/write_file/run_shell tools. max_turns=25.

**Step 5:** Create agents/api.rs - ApiAgent with http_request/run_code/read_file/write_file/run_shell tools.

**Step 6:** Create agents/data_analysis.rs - DataAnalysisAgent with run_code/read_file/write_file/run_shell tools.

**Step 7:** Create agents/deploy.rs - DeployAgent with run_shell/read_file/write_file/run_code tools.

**Step 8:** Add `mod agents;` to main.rs.

**Step 9:** Verify: `SQLX_OFFLINE=true cargo check`

**Step 10:** Commit: `feat: add agent definitions (browser, code, research, api, data, deploy)`

---

## Task 6: Create the delegate tool

**Files:**
- Create: backend/src/tools/delegate.rs
- Modify: backend/src/tools/mod.rs

**Step 1:** Create tools/delegate.rs - DelegateTool with available_agents constructor. Schema includes enum of agent names and descriptions. Execute method returns error (handled by AgentRuntime instead).

**Step 2:** Add `pub mod delegate;` to tools/mod.rs.

**Step 3:** Verify: `SQLX_OFFLINE=true cargo check`

**Step 4:** Commit: `feat: add delegate tool for inter-agent delegation`

---

## Task 7: Rewire orchestrator to use agents

**Files:**
- Modify: backend/src/orchestrator.rs
- Modify: backend/src/main.rs
- Modify: backend/src/api.rs

**Step 1:** Update Orchestrator struct: replace `skills: Arc<SkillRegistry>` with `agents: Arc<AgentRegistry>`, add `runtime: AgentRuntime`.

**Step 2:** Update Orchestrator::new to accept AgentRegistry, create AgentRuntime.

**Step 3:** Rewrite execute_task_inner step loop: create SandboxHandle, look up agent by name, call self.runtime.run(agent, description, sandbox_handle, ...). Agent result output becomes step result.

**Step 4:** Delete: execute_step_with_reflection, reflect_on_failure, reflect_on_error.

**Step 5:** Update create_plan: change "Available skills" to "Available agents" with agent descriptions.

**Step 6:** Update main.rs: swap SkillRegistry for AgentRegistry, update AppState.

**Step 7:** Update api.rs: list_skills uses agents.list(), update system_status.

**Step 8:** Verify: `SQLX_OFFLINE=true cargo check`

**Step 9:** Commit: `feat: rewire orchestrator to dispatch agents instead of skills`

---

## Task 8: Delete the old skills directory

**Files:**
- Delete: backend/src/skills/ (entire directory)
- Modify: backend/src/main.rs (remove mod skills if still present)

**Step 1:** Remove backend/src/skills/ directory.

**Step 2:** Grep for remaining references: `grep -r "use crate::skills" backend/src/`

**Step 3:** Fix any remaining references.

**Step 4:** Verify: `SQLX_OFFLINE=true cargo check`

**Step 5:** Commit: `refactor: remove old skills directory (replaced by agents + tools)`

---

## Task 9: Update sandbox Dockerfile for agent-browser

**Files:**
- Modify: sandbox/Dockerfile

**Step 1:** Update Dockerfile:
- Install Node.js 20 via nodesource
- Install agent-browser via npm install -g
- Install Chromium via npx playwright install --with-deps chromium
- Remove `playwright` from pip install list
- Keep all Python data science packages

**Step 2:** Commit: `feat: update sandbox with agent-browser, remove Playwright Python`

---

## Task 10: Update frontend for agent reasoning traces

**Files:**
- Modify: frontend/src/components/StepsSidebar.tsx
- Modify: frontend/src/app/tasks/[id]/page.tsx

**Step 1:** Update StepsSidebar to render agent trace events within steps. Update skillIcon to use agent names. Add nested rendering for agent_tool_call, agent_tool_result, agent_delegating events using display text.

**Step 2:** Update task page event handling to recognize new event types: agent_started, agent_thinking, agent_tool_call, agent_tool_result, agent_delegating, agent_completed, agent_error.

**Step 3:** Update groupEventsByStep to group agent events within their step.

**Step 4:** Render agent traces as nested list items with natural language display text and collapsible sub-agent sections.

**Step 5:** Verify: `cd frontend && bun run build`

**Step 6:** Commit: `feat: update frontend to render agent reasoning traces`

---

## Task 11: Fix sandbox test and final cleanup

**Files:**
- Modify: backend/src/sandbox.rs (fix test_config)
- Modify: backend/src/api.rs

**Step 1:** Fix test_config() in sandbox.rs to include koompi_client_id, koompi_client_secret, koompi_redirect_uri, public_url fields.

**Step 2:** Verify full compile and tests: `SQLX_OFFLINE=true cargo check && SQLX_OFFLINE=true cargo test`

**Step 3:** Verify frontend: `cd frontend && bun run build`

**Step 4:** Commit: `fix: update test config and cleanup for agent system`

---

## Task 12: End-to-end verification

**Step 1:** `SQLX_OFFLINE=true cargo check` - expect no errors

**Step 2:** `SQLX_OFFLINE=true cargo test` - expect all tests pass

**Step 3:** `cd frontend && bun run build` - expect success

**Step 4:** Final commit if any fixes needed.
