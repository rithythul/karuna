# Hanuman Enhancement Design

**Date:** 2026-03-03
**Status:** Approved
**Strategy:** Four independent parallel workstreams

---

## Context

Hanuman's backend execution engine is solid: parallel dependency-aware steps, retry with reflection, agent loop, Docker sandboxes, user memory, reasoning traces, WebSocket streaming. The gaps are in the experience layer and research quality.

The vision is "turn ideas into real artifacts — here's your live dashboard." Four areas fall short:

1. Result delivery is buried in event noise
2. Web search is fragile DuckDuckGo scraping with no structure
3. User memory exists in the backend but is invisible to the user
4. Tasks always start cold — no way to provide files as context

---

## Workstream 1: Result Experience (Frontend)

### Problem
When a task completes, the result JSON blob is buried after a wall of chronological events. Artifacts — the actual value delivery — live in a secondary tab that users may never open.

### Design

**ResultCard component** rendered above the timeline when `task.status === "completed"`:
- Large `result.summary` text as the hero
- `result.key_outputs` as a clean checklist
- Artifacts displayed as visual cards, auto-previewing the first one:
  - HTML → iframe preview
  - Images → inline display
  - Code files → syntax-highlighted block
  - Others → download button with file type icon

**Artifact viewer elevation:**
- Moves from a secondary tab to the primary content area
- Navigation dots/tabs for multiple artifacts
- Preview renders inline rather than requiring a download

**Events timeline:**
- Collapses to a toggleable "Show execution trace" panel by default
- Still available for debugging, just not the default view

### Files
- `frontend/src/app/tasks/[id]/page.tsx` — restructure layout, add tab logic
- `frontend/src/components/ArtifactViewer.tsx` — elevate to primary viewer, add iframe support
- `frontend/src/components/ResultCard.tsx` — new component (summary + key outputs + artifact grid)

---

## Workstream 2: Memory UI (Frontend)

### Problem
Hanuman learns from every completed task — user preferences, domain facts, effective techniques — but the user has no visibility into what it knows. The "partner, not a tool" narrative is invisible.

### Design

**New `/memory` page:**
- Accessible from the nav bar alongside "Task History"
- Three sections: Preferences, Facts, Learnings (matching `category` enum values)
- Each memory card shows:
  - `key` as heading
  - `value` as body text
  - `access_count` as a subtle "used N times" indicator
  - Delete button → calls `DELETE /api/memory/{category}/{key}`
- Empty state explains what memory is, why it helps, and that it builds automatically from tasks
- Pull-to-refresh / reload button

**Nav updates:**
- Add "Memory" link in the top-right nav on the home page alongside "Task History"
- Add it to the task history page nav as well

### Files
- `frontend/src/app/memory/page.tsx` — new page
- `frontend/src/app/page.tsx` — add Memory nav link
- `frontend/src/app/tasks/page.tsx` — add Memory nav link

No backend changes needed — `/api/memory` and `DELETE /api/memory/{category}/{key}` already exist.

---

## Workstream 3: Search Quality (Backend)

### Problem
`web_search` uses `curl | sed` against DuckDuckGo Lite — no API contract, no structured results, easily rate-limited, returns raw HTML fragments. The research agent's quality is bottlenecked by this.

### Design

**Tiered provider strategy:**

| Priority | Provider | Trigger |
|----------|----------|---------|
| 1 | Brave Search API | `HANUMAN_SEARCH_API_KEY` set, `HANUMAN_SEARCH_PROVIDER=brave` (default) |
| 2 | Serper API | `HANUMAN_SEARCH_PROVIDER=serper` |
| 3 | DuckDuckGo curl | No API key configured (current fallback) |

**Brave Search API response format** → structured tool output:
```json
{
  "query": "...",
  "results": [
    {"title": "...", "url": "...", "description": "...", "age": "..."},
    ...
  ]
}
```

Returns top 10 results. The agent gets clean, citable, structured data instead of raw HTML scraps.

**Config changes:**
- `HANUMAN_SEARCH_API_KEY` — API key for the selected provider
- `HANUMAN_SEARCH_PROVIDER` — `"brave"` (default) or `"serper"`

### Files
- `backend/src/tools/web_search.rs` — add provider selection, Brave/Serper HTTP calls, structured output
- `backend/src/config.rs` — add `search_api_key` and `search_provider` fields
- `.env.example` — document new vars

---

## Workstream 4: File/Context Input (Backend + Frontend)

### Problem
Every task starts cold. Users can't hand Hanuman a CSV, document, codebase snippet, or any context. This forces users to describe their data in text, which is lossy and limits use cases.

### Design

**Backend — Endpoint change:**

`POST /api/tasks` accepts both:
- `application/json` → `{"goal": "..."}` (existing, unchanged)
- `multipart/form-data` → `goal` text field + up to 5 file parts (10 MB each)

Input files stored in `artifacts` table as `artifact_type = "input"` using the existing `content` column (base64-encoded for binary). No new migration needed.

**Orchestrator changes:**
- At task start, before the planning LLM call, load input artifacts and include them in the planning prompt:
  ```
  User-provided files:
  - report.csv (12 KB, text/csv)
  - notes.md (2 KB, text/markdown)
  These files are available in /workspace/ inside the sandbox.
  ```
- Before each step's sandbox execution, copy input artifact content into `/workspace/<filename>` inside the container

**Frontend:**
- Paperclip icon / drop zone below the textarea
- Shows attached file chips (name + size + × to remove)
- Accepted: `.csv`, `.json`, `.txt`, `.pdf`, `.xlsx`, `.py`, `.js`, `.ts`, `.md`, images
- Submits as `FormData` when files present, JSON otherwise (backwards compatible)
- File size validation client-side (10 MB per file, 5 file max)

### Files
- `backend/src/api.rs` — multipart handler, file validation, store as input artifacts
- `backend/src/orchestrator.rs` — load input artifacts, inject into planning prompt, copy to sandbox
- `backend/src/config.rs` — optional: `max_upload_size_mb`
- `frontend/src/app/page.tsx` — file attachment UI

---

## Parallelism

All four workstreams are fully independent. No shared files, no shared state, no ordering constraints. Each gets its own git worktree:

| Worktree | Branch | Touches |
|----------|--------|---------|
| `result-experience` | feat/result-experience | Frontend only |
| `memory-ui` | feat/memory-ui | Frontend only |
| `search-quality` | feat/search-quality | Backend only |
| `file-input` | feat/file-input | Backend + Frontend |

Merge order: any. All four can be reviewed and merged independently.

---

## Success Criteria

- **Result experience:** A completed task immediately shows the summary and artifact preview without any tab switching or scrolling
- **Memory UI:** User can navigate to `/memory` and see, understand, and delete what Hanuman knows about them
- **Search quality:** `web_search` tool returns structured JSON results with title/url/snippet; research agent produces more accurate, citable output
- **File input:** User can attach a file on the home page, and Hanuman's agents can read it from `/workspace/` during execution
