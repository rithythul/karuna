# Karuna

Autonomous AI agent platform — a Manus.ai competitor. Give Karuna a goal — it plans, executes, and delivers.

## Capabilities

- **Autonomous planning**: LLM decomposes complex goals into executable step sequences
- **Self-reflection & retry**: Failed steps trigger LLM-based reflection with automatic retries
- **Adaptive re-planning**: If a step fails, Karuna generates a new approach for remaining work
- **Browser automation**: Navigate, screenshot, extract data, fill forms via Playwright
- **Code generation & execution**: Write, run, and auto-fix code in Python, JS, Rust, Go, etc.
- **File management**: Create, read, edit, list, and organize files in the sandbox
- **Data analysis & visualization**: Pandas + Matplotlib analysis with chart generation
- **Web research**: Multi-query search + LLM synthesis into structured reports
- **Shell commands**: Run arbitrary commands, install packages, build projects
- **Deployment packaging**: Auto-generate deployment configs, Dockerfiles, archives
- **Artifact tracking**: All outputs (files, screenshots, charts) tracked and downloadable
- **Memory system**: Persistent context across steps within a task
- **Real-time streaming**: WebSocket-based live event updates

## Architecture

- **Backend**: Rust (Axum + Tokio) — REST API, WebSocket streaming, Redis job queue
- **Frontend**: Next.js + TypeScript + Tailwind CSS — real-time task view with two-panel layout
- **Orchestrator**: LLM-powered planner → sequential execution → self-reflection → result synthesis
- **LLM**: OpenRouter (model-agnostic — Claude, GPT, Gemini, Llama)
- **Sandboxes**: Pre-warmed Docker containers with Python, Node.js, Playwright, data science tools
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
POST /api/tasks              — Submit a goal
GET  /api/tasks              — List recent tasks
GET  /api/tasks/{id}         — Get task status + steps + artifacts
GET  /api/tasks/{id}/events  — Get event log
GET  /api/tasks/{id}/artifacts — Get task artifacts
GET  /api/skills             — List available skills
GET  /api/status             — System status (queue, pool, skills)
WS   /ws/tasks/{id}          — Live event stream
GET  /health                 — Health check
```

## Skills

| Skill | Description |
|-------|-------------|
| research | Web search + LLM synthesis into structured reports |
| code | Code generation + sandbox execution with auto-retry |
| browse | Playwright browser automation — navigate, screenshot, extract, interact |
| file | File CRUD — create, read, edit, list, delete files in the sandbox |
| data_analysis | Data analysis with pandas, matplotlib, seaborn — charts + CSV output |
| shell | Execute arbitrary shell commands — install packages, run builds |
| deploy | Package and deploy web apps with auto-generated configs |

## Orchestration Flow

```
1. User submits goal
2. LLM creates execution plan (skill + description + input per step)
3. Docker sandbox acquired from pool
4. Steps execute sequentially:
   - Each step runs in the sandbox with context from previous steps
   - On failure: LLM self-reflection → retry with modified approach (up to 3x)
   - On persistent failure: adaptive re-planning for remaining steps
5. Result synthesis: LLM summarizes outputs into structured response
6. Artifacts tracked and stored in database
```

## Development

```bash
make setup   # Start infra + build sandbox + install frontend deps
make dev     # Start backend + frontend
make test    # Run tests
make build   # Production build
make clean   # Clean everything
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| KARUNA_DATABASE_URL | postgresql://karuna:karuna@localhost:5432/karuna | PostgreSQL URL |
| KARUNA_REDIS_URL | redis://localhost:6379 | Redis URL |
| KARUNA_OPENROUTER_API_KEY | (required) | OpenRouter API key |
| KARUNA_DEFAULT_MODEL | anthropic/claude-sonnet-4 | Default LLM model |
| KARUNA_PLANNING_MODEL | anthropic/claude-sonnet-4 | Model for planning/code generation |
| KARUNA_FAST_MODEL | anthropic/claude-haiku-4 | Fast model for reflection/queries |
| KARUNA_SANDBOX_IMAGE | karuna-sandbox:latest | Docker image for sandboxes |
| KARUNA_SANDBOX_MEMORY_MB | 512 | Memory limit per sandbox (MB) |
| KARUNA_SANDBOX_CPU_QUOTA | 50000 | CPU quota per sandbox |
