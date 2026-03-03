# Result Experience Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** When a task completes, surface the result and artifacts front-and-center instead of burying them in event noise.

**Architecture:** New `ResultCard` component renders summary + key outputs + inline artifact preview (iframe/img/pre depending on MIME type) above a collapsible events timeline. The task page restructures its layout: ResultCard when completed, then a "Show execution trace" toggle, then the existing events list.

**Tech Stack:** React 19, Next.js 16, TypeScript, Tailwind CSS 4, CSS custom properties (no external UI libs)

---

### Task 1: Create ResultCard component

**Files:**
- Create: `frontend/src/components/ResultCard.tsx`

**Step 1: Write the component**

```tsx
"use client";

import { useState, useEffect } from "react";
import { type Artifact } from "@/components/ArtifactViewer";

interface ResultData {
  summary?: string;
  key_outputs?: string[];
  artifacts?: string[];
  next_steps?: string[];
}

interface ResultCardProps {
  result: ResultData;
  artifacts: Artifact[];
  taskId: string;
}

function contentUrl(taskId: string, artifactId: string): string {
  return `/api/tasks/${taskId}/artifacts/${artifactId}/content`;
}

function ArtifactPreview({ artifact, taskId }: { artifact: Artifact; taskId: string }) {
  const [textContent, setTextContent] = useState<string | null>(null);
  const url = contentUrl(taskId, artifact.id);
  const mime = artifact.mime_type ?? "";

  useEffect(() => {
    const isText =
      (mime.startsWith("text/") && mime !== "text/html") ||
      mime === "application/json";
    if (isText) {
      fetch(url)
        .then((r) => r.text())
        .then(setTextContent)
        .catch(() => {});
    }
  }, [url, mime]);

  if (mime === "text/html") {
    return (
      <iframe
        src={url}
        className="w-full rounded-lg"
        style={{ height: 480, border: "1px solid var(--border-subtle)" }}
        sandbox="allow-scripts allow-same-origin"
        title={artifact.name}
      />
    );
  }

  if (mime.startsWith("image/")) {
    return (
      <img
        src={url}
        alt={artifact.name}
        className="w-full rounded-lg object-contain"
        style={{ maxHeight: 480, border: "1px solid var(--border-subtle)" }}
      />
    );
  }

  if (textContent !== null) {
    return (
      <pre
        className="w-full rounded-lg p-4 overflow-auto text-[12px] leading-relaxed"
        style={{
          maxHeight: 480,
          background: "var(--bg-elevated)",
          border: "1px solid var(--border-subtle)",
          color: "var(--text-primary)",
          fontFamily: "monospace",
        }}
      >
        {textContent}
      </pre>
    );
  }

  // Fallback: download button
  return (
    <a
      href={url}
      download={artifact.name}
      className="inline-flex items-center gap-2 rounded-lg px-4 py-2.5 text-[13px] font-medium"
      style={{ background: "var(--accent)", color: "var(--text-on-accent)" }}
    >
      <svg width="14" height="14" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
        <path strokeLinecap="round" strokeLinejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5M16.5 12L12 16.5m0 0L7.5 12m4.5 4.5V3" />
      </svg>
      Download {artifact.name}
    </a>
  );
}

export default function ResultCard({ result, artifacts, taskId }: ResultCardProps) {
  // Filter out input artifacts (uploaded by user)
  const outputArtifacts = artifacts.filter((a) => a.artifact_type !== "input");
  const [selectedId, setSelectedId] = useState<string | null>(
    outputArtifacts.length > 0 ? outputArtifacts[0].id : null
  );
  const selected = outputArtifacts.find((a) => a.id === selectedId) ?? null;

  return (
    <div
      className="rounded-2xl p-6 animate-fade-in-up"
      style={{
        background: "var(--bg-raised)",
        border: "1px solid var(--border-subtle)",
      }}
    >
      {/* Summary */}
      {result.summary && (
        <p
          className="text-[15px] leading-relaxed mb-5"
          style={{ color: "var(--text-primary)" }}
        >
          {result.summary}
        </p>
      )}

      {/* Key outputs */}
      {result.key_outputs && result.key_outputs.length > 0 && (
        <ul className="flex flex-col gap-1.5 mb-5">
          {result.key_outputs.map((output, i) => (
            <li
              key={i}
              className="flex items-start gap-2 text-[13px]"
              style={{ color: "var(--text-secondary)" }}
            >
              <span style={{ color: "var(--status-success)", marginTop: 2 }}>✓</span>
              {output}
            </li>
          ))}
        </ul>
      )}

      {/* Artifact preview */}
      {outputArtifacts.length > 0 && (
        <div>
          {/* Tabs for multiple artifacts */}
          {outputArtifacts.length > 1 && (
            <div className="flex gap-2 mb-3 overflow-x-auto pb-1">
              {outputArtifacts.map((a) => (
                <button
                  key={a.id}
                  onClick={() => setSelectedId(a.id)}
                  className="flex-shrink-0 rounded-lg px-3 py-1.5 text-[12px] font-medium transition-all cursor-pointer"
                  style={{
                    background:
                      selectedId === a.id ? "var(--accent)" : "var(--bg-elevated)",
                    color:
                      selectedId === a.id
                        ? "var(--text-on-accent)"
                        : "var(--text-secondary)",
                  }}
                >
                  {a.name}
                </button>
              ))}
            </div>
          )}

          {selected && <ArtifactPreview artifact={selected} taskId={taskId} />}
        </div>
      )}

      {/* Next steps */}
      {result.next_steps && result.next_steps.length > 0 && (
        <div
          className="mt-5 pt-4"
          style={{ borderTop: "1px solid var(--border-subtle)" }}
        >
          <p
            className="text-[11px] font-semibold uppercase tracking-wider mb-2"
            style={{ color: "var(--text-tertiary)" }}
          >
            Suggested Next Steps
          </p>
          <ul className="flex flex-col gap-1">
            {result.next_steps.map((step, i) => (
              <li
                key={i}
                className="text-[12px]"
                style={{ color: "var(--text-secondary)" }}
              >
                → {step}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
```

**Step 2: Verify TypeScript compiles**

```bash
cd frontend && bun run build 2>&1 | tail -20
```

Expected: no TypeScript errors for `ResultCard.tsx`.

**Step 3: Commit**

```bash
git add frontend/src/components/ResultCard.tsx
git commit -m "feat: add ResultCard component with inline artifact preview"
```

---

### Task 2: Integrate ResultCard into the task page

**Files:**
- Modify: `frontend/src/app/tasks/[id]/page.tsx`

**Step 1: Add ResultCard import and collapsible trace state**

At the top of `page.tsx`, add the import after the existing imports:

```tsx
import ResultCard from "@/components/ResultCard";
```

After the existing `useState` declarations (around line 200), add:

```tsx
const [showTrace, setShowTrace] = useState(false);
```

**Step 2: Replace the main content area layout**

Find the section in the return JSX that renders the main content area (the two-column layout with events and sidebar). It's the large `<div>` that contains `activeTab`, the events list, and `StepsSidebar`.

Replace the content area — right after the header block that shows goal/status/timer and before the closing outer div — with:

```tsx
{/* ── Result Card (completed tasks only) ─────────── */}
{task?.status === "completed" && task.result && (
  <div className="mx-auto max-w-[900px] px-4 pt-4">
    <ResultCard
      result={task.result as { summary?: string; key_outputs?: string[]; artifacts?: string[]; next_steps?: string[] }}
      artifacts={artifacts}
      taskId={id}
    />
    <button
      onClick={() => setShowTrace((v) => !v)}
      className="mt-3 flex items-center gap-1.5 text-[12px] cursor-pointer transition-colors"
      style={{ color: "var(--text-tertiary)" }}
      onMouseEnter={(e) => (e.currentTarget.style.color = "var(--text-secondary)")}
      onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-tertiary)")}
    >
      <svg
        width="12" height="12" fill="none" viewBox="0 0 24 24"
        stroke="currentColor" strokeWidth={2}
        style={{ transform: showTrace ? "rotate(90deg)" : "none", transition: "transform 0.15s" }}
      >
        <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" />
      </svg>
      {showTrace ? "Hide" : "Show"} execution trace
    </button>
  </div>
)}
```

**Step 3: Wrap the existing events/sidebar layout in the collapsible**

Find the outer two-column grid div (the one that renders the events timeline and sidebar). Wrap it with a conditional:

```tsx
{/* Only show trace if: task not completed, OR user toggled it open */}
{(task?.status !== "completed" || showTrace) && (
  // ... existing two-column layout JSX stays here unchanged ...
)}
```

**Step 4: Verify build**

```bash
cd frontend && bun run build 2>&1 | tail -30
```

Expected: clean build, no TypeScript errors.

**Step 5: Manual smoke test**

Start the app (`make dev`), open a completed task. Verify:
- ResultCard renders above the events trace
- "Show execution trace" toggle collapses/expands the events
- For running/pending tasks, events show normally without ResultCard

**Step 6: Commit**

```bash
git add frontend/src/app/tasks/[id]/page.tsx
git commit -m "feat: show ResultCard above collapsible trace on completed tasks"
```

---

### Task 3: Add Memory link to nav

**Files:**
- Modify: `frontend/src/app/tasks/[id]/page.tsx` (header nav)

**Step 1: Find the header nav in the task page**

The task page has a header with a back arrow / "New Task" link and user info. Find the `<header>` or nav area.

**Step 2: Add Memory link**

In the top nav (alongside the existing home/task history links), add:

```tsx
<Link
  href="/memory"
  className="text-[12px] font-medium transition-colors"
  style={{ color: "var(--text-secondary)" }}
>
  Memory
</Link>
```

**Step 3: Commit**

```bash
git add frontend/src/app/tasks/[id]/page.tsx
git commit -m "feat: add Memory nav link to task page header"
```
