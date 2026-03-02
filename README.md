# Hanuman

Autonomous AI agent platform. Give Hanuman a goal — it plans, executes, and delivers.

## Vision

Most AI tools are one-shot: you ask, they answer, you copy-paste. Hanuman is different. It thinks, acts, observes, and adapts — looping until the job is done.

```
You: "Research the top 5 competitors and build a comparison dashboard"

Hanuman:
  think → break goal into research + code + deploy steps
  act   → browser agent searches the web, extracts data
  observe → raw data collected, gaps identified
  act   → code agent builds an interactive dashboard
  observe → dashboard renders, but missing pricing column
  act   → browser agent fetches missing pricing data
  act   → code agent updates dashboard with complete data
  act   → deploy agent packages and serves it
  done  → here's your live dashboard
```

Each agent runs its own reasoning loop with real tools — a browser, a code interpreter, API access — not a single LLM call pretending to work. When something fails, the agent reflects, adjusts, and retries. No hand-holding required.

## Architecture

```
Goal → Orchestrator → [Agent₁, Agent₂, ...] → Result
                         ↓
                   think → act → observe (loop)
```

- **Backend** — Rust (Axum + Tokio). REST API, WebSocket streaming, Redis job queue.
- **Frontend** — Next.js + TypeScript + Tailwind CSS. Real-time task view with reasoning traces.
- **Orchestrator** — LLM-powered planner decomposes goals into agent steps and dispatches them.
- **LLM** — OpenRouter (model-agnostic: Claude, GPT, Gemini, Llama).
- **Sandboxes** — Pre-warmed Docker containers with browser automation (agent-browser + Chromium), Python data science stack, and Node.js.
- **Queue** — Redis FIFO queue + Pub/Sub for horizontal scaling.

## Agents

| Agent | What it does |
|-------|-------------|
| `browser` | Browse websites, fill forms, extract data, take screenshots |
| `code` | Write, execute, and debug code in multiple languages |
| `research` | Search the web, read pages, synthesize reports |
| `api` | Make HTTP API calls, parse responses, chain workflows |
| `data_analysis` | Analyze data, create visualizations, generate reports |
| `deploy` | Package and deploy applications |

Each agent gets a system prompt, a set of tools, and runs autonomously inside a sandboxed Docker container. The LLM picks tools via OpenRouter's tool-calling API, the runtime executes them, and observations feed back to the LLM until the goal is met.

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

Backend: http://localhost:8000 | Frontend: http://localhost:3000

## API

```
POST /api/tasks                — Submit a goal
GET  /api/tasks                — List all tasks
GET  /api/tasks/{id}           — Task status + steps
GET  /api/tasks/{id}/events    — Event log
GET  /api/tasks/{id}/artifacts — Generated artifacts
POST /api/tasks/{id}/cancel    — Cancel a running task
GET  /api/skills               — List available agents
GET  /api/status               — System status (queue, pool, agents)
GET  /api/memory               — List user memories
WS   /ws/tasks/{id}            — Live event stream
GET  /health                   — Health check
```

## Development

```bash
make infra     # Start PostgreSQL + Redis
make sandbox   # Build sandbox Docker image
make setup     # Full setup (infra + sandbox + frontend deps)
make dev       # Run backend + frontend
make test      # Run tests
make build     # Production build
make clean     # Tear down everything
```
