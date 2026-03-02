# Reliability, Visibility & Parallelism Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make Hanuman's agentic loop reliable (retry, timeout, backoff), visible (persisted reasoning traces), and fast (parallel step execution).

**Architecture:** Four layers of improvement applied to the existing orchestrator→agent_runtime→tools stack. Tool timeouts wrap sandbox.exec(). LLM retry wraps llm.chat_with_tools(). Step retry wraps the orchestrator's per-step loop. Reasoning traces persist every message in the agent loop to a new DB table. Parallel execution replaces the sequential step loop with JoinSet-based concurrent dispatch.

**Tech Stack:** Rust (Axum/Tokio), PostgreSQL (sqlx), Redis Pub/Sub, Next.js frontend

---

## Task 1: Tool Execution Timeouts

**Files:**
- Modify: `backend/src/agent_runtime.rs:310` (tool execute call)

**Step 1: Add timeout wrapper around tool execution in agent_runtime.rs**

In `agent_runtime.rs`, the tool execution at line 310 is:

```rust
match tool.execute(params, sandbox).await {
```

Wrap it with tokio::time::timeout. Determine timeout based on tool name:
- Browser tools (navigate, click, fill, extract, snapshot, screenshot): 120s
- Code execution (run_code): 300s
- Everything else: 60s

On timeout, push a tool-result message saying it timed out so the LLM can adapt (not crash the agent).

**Step 2: Run `SQLX_OFFLINE=true cargo check` to verify it compiles**

**Step 3: Commit**

```
feat: add tool execution timeouts in agent runtime
```

---

## Task 2: LLM Call Retry with Exponential Backoff

**Files:**
- Modify: `backend/src/llm.rs` (add retry wrapper)

**Step 1: Add a retry helper method to LlmClient**

Add `is_retryable_status()` helper that returns true for 429, 500, 502, 503, 504.

Add `request_with_retry()` method that:
- Accepts a closure that builds a reqwest::RequestBuilder (so it can be re-invoked per retry)
- Retries up to 3 times with exponential backoff (1s, 2s, 4s)
- Returns immediately for non-retryable errors (400, 401, 403)
- Returns immediately on success

**Step 2: Update `chat()` and `chat_with_tools()` to use the retry wrapper**

Replace the direct `.send().await` + status check in both methods with `self.request_with_retry(...)`.

**Step 3: Run `SQLX_OFFLINE=true cargo check`**

**Step 4: Commit**

```
feat: add LLM retry with exponential backoff
```

---

## Task 3: Step-Level Retry with Reflection

**Files:**
- Modify: `backend/src/orchestrator.rs:169-284` (the step execution loop)
- Modify: `backend/src/db.rs` (add `increment_step_retry`, `set_step_reflection`, `set_step_error`)

**Step 1: Add DB helper functions in db.rs**

- `increment_step_retry(pool, step_id) -> Result<i32>`: increments retry_count, returns new value
- `set_step_reflection(pool, step_id, reflection)`: updates reflection column
- `set_step_error(pool, step_id, error)`: updates error column

**Step 2: Wrap step execution in a retry loop in orchestrator.rs**

Replace the single agent `run()` call and its match block with a loop of up to 3 attempts (1 initial + 2 retries). On retry:
- Increment retry_count in DB
- Store reflection text
- Prepend reflection to the agent's goal: "IMPORTANT: Previous attempt failed with: {error}. Try a completely different approach."
- Emit `step_retrying` event with attempt number and reflection
- On final failure, emit `step_failed` with total attempts

**Step 3: Run `SQLX_OFFLINE=true cargo check`**

**Step 4: Commit**

```
feat: add step-level retry with reflection in orchestrator
```

---

## Task 4: Fix Broken Re-Planning

**Files:**
- Modify: `backend/src/orchestrator.rs` (the re-planning logic after step failure)

**Step 1: Replace the broken re-plan handling**

Current broken code (lines 257-275): calls `replan()`, gets result, then `continue` which skips to next original step — new plan is never executed.

After all step retries exhaust, if there are remaining steps:
1. Call `replan()` to get new steps
2. Create new step records in DB via `db::create_task_step()`
3. Execute the new steps with the same agent-lookup + run logic
4. Break out of the original step loop (don't execute remaining original steps)
5. If re-plan itself fails, fall through to task failure

**Step 2: Run `SQLX_OFFLINE=true cargo check`**

**Step 3: Commit**

```
fix: wire re-planning to actually execute new plan steps
```

---

## Task 5: Reasoning Traces — Database Migration & Persistence

**Files:**
- Create: `backend/migrations/004_reasoning_traces.sql`
- Modify: `backend/src/models.rs` (add `ReasoningTrace` model)
- Modify: `backend/src/db.rs` (add `insert_reasoning_trace` and `get_reasoning_traces`)
- Modify: `backend/src/agent_runtime.rs` (persist messages during the loop)

**Step 1: Create the migration**

Table: `reasoning_traces` with columns:
- id UUID PK
- task_id UUID FK -> tasks
- step_id UUID FK -> task_steps (nullable)
- agent_name TEXT
- turn INTEGER
- role TEXT (system/user/assistant/tool)
- content TEXT (nullable)
- tool_calls JSONB (nullable)
- created_at TIMESTAMPTZ

Index on (task_id, step_id, turn).

**Step 2: Add ReasoningTrace model in models.rs**

Standard sqlx::FromRow struct matching the table.

**Step 3: Add DB functions in db.rs**

- `insert_reasoning_trace(pool, task_id, step_id, agent_name, turn, role, content, tool_calls)`
- `get_reasoning_traces(pool, task_id, step_id) -> Vec<ReasoningTrace>` (filter by step_id if provided)

**Step 4: Pass DB pool and step_id into AgentRuntime::run()**

Add `pool: &PgPool` and `step_id: Option<Uuid>` parameters. Parse task_id string to Uuid.

Insert traces at these points in the loop:
1. After LlmResponse::Text — persist assistant text
2. After LlmResponse::ToolCalls — persist assistant tool-call message
3. After each tool result — persist tool result message

Update the caller in orchestrator.rs to pass the pool and step.id.

**Step 5: Run `SQLX_OFFLINE=true cargo check`**

**Step 6: Commit**

```
feat: persist reasoning traces for agent loops
```

---

## Task 6: Reasoning Traces — API Endpoint

**Files:**
- Modify: `backend/src/api.rs` (add reasoning endpoint + route)

**Step 1: Add endpoint handler**

`get_step_reasoning(auth, state, Path((task_id, step_id)))` that calls `db::get_reasoning_traces()`.

**Step 2: Register route**

Add `.route("/api/tasks/{id}/steps/{step_id}/reasoning", get(get_step_reasoning))` to the router.

**Step 3: Run `SQLX_OFFLINE=true cargo check`**

**Step 4: Commit**

```
feat: add reasoning traces API endpoint
```

---

## Task 7: Frontend — Reasoning Timeline Component

**Files:**
- Create: `frontend/src/components/ReasoningTimeline.tsx`
- Modify: `frontend/src/components/StepsSidebar.tsx` (add expand button per step)

**Step 1: Create ReasoningTimeline component**

A collapsible panel fetching `GET /api/tasks/{id}/steps/{step_id}/reasoning`. Each turn shows:
- Agent thought (assistant message content)
- Tool call (name + parameters summary)
- Tool result (truncated output)

Use the existing CSS variables (--bg-raised, --text-secondary, --accent, etc.) to match the current design.

**Step 2: Integrate into StepsSidebar**

Add expand/collapse toggle per completed step that reveals the ReasoningTimeline.

**Step 3: Commit**

```
feat: add reasoning timeline component to frontend
```

---

## Task 8: Parallel Step Execution — Planning with Dependencies

**Files:**
- Modify: `backend/src/orchestrator.rs` (update `create_plan` prompt)

**Step 1: Update the planning prompt in create_plan()**

Add to planning guidelines:
- Each step should include a "depends_on" field: array of 0-based step indices
- Steps with empty depends_on can run in parallel
- Only add dependencies where output of a previous step is genuinely needed

Update JSON format instruction to include depends_on.

**Step 2: Commit**

```
feat: add dependency annotations to execution plans
```

---

## Task 9: Parallel Step Execution — Concurrent Dispatch

**Files:**
- Modify: `backend/src/orchestrator.rs` (replace sequential loop with dependency-aware dispatch)

**Step 1: Implement parallel execution**

Replace sequential for loop with:
1. Parse depends_on from each plan step
2. Track completed step indices in a HashSet
3. Loop: find steps whose dependencies are all in the completed set
4. Spawn those via tokio::task::JoinSet, each with its own sandbox container
5. Wait for any to complete, update completed set, repeat
6. Each parallel step acquires its own container from the pool

**Step 2: Run `SQLX_OFFLINE=true cargo check`**

**Step 3: Commit**

```
feat: parallel step execution with dependency graph
```

---

## Task 10: Update sqlx Offline Cache

**Files:**
- Modify: `backend/.sqlx/` (regenerate)

**Step 1: Start infrastructure and run migrations**

```
make infra
cd backend && DATABASE_URL=$HANUMAN_DATABASE_URL cargo sqlx migrate run
```

**Step 2: Regenerate offline cache**

```
cd backend && DATABASE_URL=$HANUMAN_DATABASE_URL cargo sqlx prepare
```

**Step 3: Verify offline build**

```
SQLX_OFFLINE=true cargo check
```

**Step 4: Commit**

```
chore: update sqlx offline cache for new queries
```
