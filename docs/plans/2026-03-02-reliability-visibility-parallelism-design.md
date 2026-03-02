# Hanuman: Reliability, Visibility & Parallelism

**Date:** 2026-03-02
**Status:** Approved

## North Star

> Autonomous AI agent platform. Give Hanuman a goal — it plans, executes, and delivers.
> Think about the goal → pick a tool → execute → observe result → decide next action → repeat until done.
> Each executor is an autonomous reasoning loop, not a one-shot function call.

The core agentic loop works. This design closes the gaps between "works in demo" and "works reliably on real tasks."

## Phase 1: Reliability & Robustness

### 1.1 Step-level retry with reflection

**Problem:** When a step fails, the orchestrator attempts a full re-plan but the result is discarded (the `continue` statement skips to the next original step instead of executing the new plan).

**Fix:**
- Before re-planning, retry the failed step up to 2 times
- On retry, inject reflection: "Previous attempt failed: {error}. Try a different approach."
- Increment `retry_count` in DB on each retry
- Store reflection text in `task_steps.reflection`
- Only re-plan remaining steps if all retries exhaust

### 1.2 Fix broken re-planning

**Problem:** `replan()` returns new steps but the orchestrator loop ignores them.

**Fix:**
- When re-plan succeeds, create new step records in DB
- Replace the remaining iteration with the new plan's steps
- Emit `replan_ready` with the actual new steps
- Continue executing from the new plan

### 1.3 Tool execution timeouts

**Problem:** No timeout on tool execution. A hung command blocks the agent forever.

**Fix:**
- Wrap `tool.execute()` in `tokio::time::timeout()`
- Default: 60s for shell/file tools, 120s for browser tools, 300s for code execution
- On timeout, return error to LLM so it can adapt (not crash the agent)

### 1.4 LLM call retry with backoff

**Problem:** A transient OpenRouter 429/500 kills the entire task.

**Fix:**
- Retry LLM calls up to 3 times for retryable HTTP errors (429, 500, 502, 503, timeout)
- Exponential backoff: 1s, 2s, 4s
- Non-retryable errors (400, 401, 403) fail immediately

## Phase 2: Reasoning Visibility

### 2.1 Persist reasoning traces

**Problem:** Agent conversation history is ephemeral — lost after execution.

**Fix:**
- New table: `reasoning_traces (id, task_id, step_id, agent_name, turn, role, content, tool_calls JSONB, created_at)`
- Write each message in the agent loop to this table
- New API endpoint: `GET /api/tasks/{id}/steps/{step_id}/reasoning`

### 2.2 Frontend reasoning timeline

**Problem:** Frontend shows brief status events but not the full thought process.

**Fix:**
- Collapsible "Reasoning" panel per step in the task detail page
- Each turn displayed as: Agent thought → Tool called → Observation received
- Lazy-loaded from the reasoning API endpoint

## Phase 3: Parallel Execution

### 3.1 Step dependency graph

**Problem:** All steps execute sequentially even when independent.

**Fix:**
- Planning prompt asks LLM to annotate `depends_on: [step_indices]`
- Steps with satisfied dependencies run concurrently

### 3.2 Concurrent dispatch

**Fix:**
- Use `tokio::JoinSet` for parallel agent execution
- Each parallel step gets its own sandbox container
- Results merged when all parallel steps complete
- Dependent steps wait for their prerequisites

## Implementation Order

Phase 1 (Reliability) first — ship each fix independently:
1. Tool execution timeouts (smallest, most impact)
2. LLM retry with backoff (prevents transient failures)
3. Step-level retry with reflection (the core reliability fix)
4. Fix broken re-planning (makes adaptive recovery work)

Phase 2 (Visibility) after reliability is solid:
5. Reasoning traces table + persistence
6. Reasoning API endpoint
7. Frontend reasoning timeline

Phase 3 (Parallelism) last:
8. Dependency annotations in planning
9. Concurrent dispatch with JoinSet
