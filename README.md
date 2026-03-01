# Karuna

Autonomous AI agent platform. Give Karuna a goal — it plans, executes, and delivers.

A true agent would do: think about the goal → pick a tool → execute → observe result → decide next action → repeat until done. Each executor would be an autonomous reasoning loop, not a one-shot function call.  

## Architecture

- **Backend**: Rust (Axum + Tokio) — REST API, WebSocket streaming, Redis job queue
- **Frontend**: Next.js + TypeScript + Tailwind CSS — real-time task view
- **Orchestrator**: LLM-powered planner decomposes goals into skill steps, executes in Docker sandboxes
- **LLM**: OpenRouter (model-agnostic — Claude, GPT, Gemini, Llama)
- **Sandboxes**: Pre-warmed Docker containers with browser automation (Playwright)
- **Queue**: Redis-backed job queue + Pub/Sub for horizontal scaling

## Quick Start

```bash
# Prerequisites: Docker, Rust, Bun

# 1. Setup infrastructure
make setup

# 2. Configure
cp .env.example .env
# Edit .env with your OpenRouter API key

# 3. Run
make dev
```

Backend: http://localhost:8000
Frontend: http://localhost:3000

## API

```
POST /api/tasks           — Submit a goal
GET  /api/tasks/{id}      — Get task status + steps
GET  /api/tasks/{id}/events — Get event log
GET  /api/skills          — List available skills
WS   /ws/tasks/{id}       — Live event stream
GET  /health              — Health check
```

## Skills

| Skill | Description |
|-------|-------------|
| research | Web search + LLM synthesis into reports |
| code | Code generation + sandbox execution with auto-retry |

## Development

```bash
make test    # Run tests
make build   # Production build
make clean   # Clean everything
```
