"use client";

import { useEffect, useState, useRef } from "react";
import { useParams } from "next/navigation";
import Link from "next/link";
import ArtifactViewer, { type Artifact } from "@/components/ArtifactViewer";
import StepsSidebar from "@/components/StepsSidebar";

interface TaskEvent {
  id?: string;
  task_id?: string;
  event_type: string;
  data: Record<string, unknown>;
  created_at?: string;
}

interface TaskStep {
  id: string;
  skill: string;
  description: string;
  step_order: number;
  status: string;
  error: string | null;
  retry_count: number;
  reflection: string | null;
}

interface TaskData {
  id: string;
  goal: string;
  status: string;
  result: Record<string, unknown> | null;
  error: string | null;
  token_usage: Record<string, unknown> | null;
  total_duration_ms: number | null;
}

type ConnectionStatus = "connecting" | "live" | "completed" | "failed" | "disconnected";

export default function TaskPage() {
  const { id } = useParams<{ id: string }>();
  const [events, setEvents] = useState<TaskEvent[]>([]);
  const [task, setTask] = useState<TaskData | null>(null);
  const [steps, setSteps] = useState<TaskStep[]>([]);
  const [artifacts, setArtifacts] = useState<Artifact[]>([]);
  const [status, setStatus] = useState<ConnectionStatus>("connecting");
  const [currentStep, setCurrentStep] = useState<number | null>(null);
  const [activeTab, setActiveTab] = useState<"events" | "artifacts">("events");
  const terminalRef = useRef(false);
  const wsRef = useRef<WebSocket | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const [elapsed, setElapsed] = useState(0);
  const startTimeRef = useRef(Date.now());

  // Timer
  useEffect(() => {
    if (status === "completed" || status === "failed") return;
    const interval = setInterval(() => {
      setElapsed(Math.floor((Date.now() - startTimeRef.current) / 1000));
    }, 1000);
    return () => clearInterval(interval);
  }, [status]);

  // Fetch initial task data
  useEffect(() => {
    if (!id) return;
    fetch(`/api/tasks/${id}`)
      .then((res) => res.json())
      .then((data) => {
        if (data.task) setTask(data.task);
        if (data.steps) setSteps(data.steps);
        if (data.artifacts) setArtifacts(data.artifacts);
        if (data.task?.status === "Completed" || data.task?.status === "completed") {
          terminalRef.current = true;
          setStatus("completed");
        } else if (data.task?.status === "Failed" || data.task?.status === "failed") {
          terminalRef.current = true;
          setStatus("failed");
        }
      })
      .catch(() => {});

    fetch(`/api/tasks/${id}/events`)
      .then((res) => res.json())
      .then((data: TaskEvent[]) => {
        if (Array.isArray(data) && data.length > 0) {
          setEvents(data);
        }
      })
      .catch(() => {});

    // Fetch artifacts
    fetch(`/api/tasks/${id}/artifacts`)
      .then((res) => res.json())
      .then((data: Artifact[]) => {
        if (Array.isArray(data)) setArtifacts(data);
      })
      .catch(() => {});
  }, [id]);

  // WebSocket
  useEffect(() => {
    if (!id || terminalRef.current) return;

    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const wsUrl = `${protocol}//localhost:8000/ws/tasks/${id}`;
    const ws = new WebSocket(wsUrl);
    wsRef.current = ws;

    ws.onopen = () => setStatus("live");

    ws.onmessage = (msg) => {
      try {
        const event: TaskEvent = JSON.parse(msg.data);
        if (!event.created_at) event.created_at = new Date().toISOString();
        setEvents((prev) => [...prev, event]);

        // Track current step
        if (event.event_type === "step_started" && event.data.step) {
          setCurrentStep(Number(event.data.step));
        }

        // Update steps on step events
        if (event.event_type === "step_completed" || event.event_type === "step_failed") {
          fetch(`/api/tasks/${id}`)
            .then((res) => res.json())
            .then((data) => {
              if (data.steps) setSteps(data.steps);
              if (data.artifacts) setArtifacts(data.artifacts);
            })
            .catch(() => {});
        }

        // Handle reflection events
        if (event.event_type === "step_reflection") {
          fetch(`/api/tasks/${id}`)
            .then((res) => res.json())
            .then((data) => { if (data.steps) setSteps(data.steps); })
            .catch(() => {});
        }

        // Handle replan events
        if (event.event_type === "replan_ready") {
          fetch(`/api/tasks/${id}`)
            .then((res) => res.json())
            .then((data) => { if (data.steps) setSteps(data.steps); })
            .catch(() => {});
        }

        const isCompleted = event.event_type === "task_completed";
        const isFailed = event.event_type === "task_failed";

        if (isCompleted || isFailed) {
          terminalRef.current = true;
          setStatus(isCompleted ? "completed" : "failed");
          setCurrentStep(null);
          fetch(`/api/tasks/${id}`)
            .then((res) => res.json())
            .then((data) => {
              if (data.task) setTask(data.task);
              if (data.steps) setSteps(data.steps);
              if (data.artifacts) setArtifacts(data.artifacts);
            })
            .catch(() => {});
          // Final artifact fetch
          fetch(`/api/tasks/${id}/artifacts`)
            .then((res) => res.json())
            .then((data: Artifact[]) => {
              if (Array.isArray(data)) setArtifacts(data);
            })
            .catch(() => {});
        }
      } catch {}
    };

    ws.onclose = () => {
      if (!terminalRef.current) setStatus("disconnected");
    };
    ws.onerror = () => {
      if (!terminalRef.current) setStatus("disconnected");
    };

    return () => ws.close();
  }, [id]);

  // Auto-scroll
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [events]);

  const formatTime = (secs: number) => {
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}:${s.toString().padStart(2, "0")}`;
  };

  const formatTimestamp = (iso?: string) => {
    if (!iso) return "";
    try {
      return new Date(iso).toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
      });
    } catch {
      return "";
    }
  };

  const completedSteps = steps.filter(
    (s) => s.status.toLowerCase() === "completed"
  ).length;

  const progressPct = steps.length > 0 ? (completedSteps / steps.length) * 100 : 0;

  return (
    <div className="min-h-screen" style={{ background: "var(--bg-deep)" }}>
      {/* Top bar */}
      <header
        className="sticky top-0 z-40 flex items-center justify-between px-6 py-3"
        style={{
          background: "rgba(8,8,10,0.85)",
          backdropFilter: "blur(12px)",
          borderBottom: "1px solid var(--border-subtle)",
        }}
      >
        <div className="flex items-center gap-4">
          <Link
            href="/"
            className="flex items-center gap-2 text-[13px] transition-colors"
            style={{ color: "var(--text-tertiary)" }}
            onMouseEnter={(e) =>
              (e.currentTarget.style.color = "var(--text-primary)")
            }
            onMouseLeave={(e) =>
              (e.currentTarget.style.color = "var(--text-tertiary)")
            }
          >
            <svg width="16" height="16" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M10.5 19.5L3 12m0 0l7.5-7.5M3 12h18" />
            </svg>
            Karuna
          </Link>

          <div
            className="h-4"
            style={{ width: "1px", background: "var(--border-default)" }}
          />

          <StatusPill status={status} />
        </div>

        <div className="flex items-center gap-4">
          {/* Progress mini-bar in header */}
          {steps.length > 0 && (
            <div className="flex items-center gap-2">
              <div
                className="h-1 w-20 rounded-full overflow-hidden"
                style={{ background: "var(--bg-elevated)" }}
              >
                <div
                  className="h-full rounded-full transition-all duration-700 ease-out"
                  style={{
                    width: `${progressPct}%`,
                    background: status === "failed" ? "var(--status-error)" : "var(--accent)",
                  }}
                />
              </div>
              <span
                className="text-[11px] font-mono"
                style={{ color: "var(--text-tertiary)" }}
              >
                {completedSteps}/{steps.length}
              </span>
            </div>
          )}
          <span
            className="text-[13px] font-mono"
            style={{ color: "var(--text-tertiary)" }}
          >
            {formatTime(elapsed)}
          </span>
        </div>
      </header>

      {/* Main two-column layout */}
      <div className="flex" style={{ minHeight: "calc(100vh - 52px)" }}>
        {/* Left sidebar - Steps */}
        <aside
          className="w-[300px] shrink-0 overflow-y-auto p-4"
          style={{
            borderRight: "1px solid var(--border-subtle)",
            maxHeight: "calc(100vh - 52px)",
            position: "sticky",
            top: "52px",
          }}
        >
          <StepsSidebar steps={steps} currentStep={currentStep} />

          {/* Duration */}
          {task?.total_duration_ms && (
            <div
              className="mt-3 rounded-lg p-3 text-center"
              style={{ background: "var(--bg-raised)", border: "1px solid var(--border-subtle)" }}
            >
              <span className="text-[10px] uppercase tracking-wider font-medium" style={{ color: "var(--text-tertiary)" }}>
                Total Duration
              </span>
              <p className="text-[16px] font-mono mt-1" style={{ color: "var(--accent)" }}>
                {(task.total_duration_ms / 1000).toFixed(1)}s
              </p>
            </div>
          )}
        </aside>

        {/* Right main content */}
        <main className="flex-1 min-w-0 overflow-y-auto">
          <div className="max-w-[720px] mx-auto px-6 pt-6 pb-24">
            {/* Goal */}
            <div className="mb-6 animate-fade-in-up">
              <h1
                className="text-[1.5rem] leading-snug tracking-[-0.01em] mb-1"
                style={{
                  fontFamily: "var(--font-display)",
                  color: "var(--text-primary)",
                }}
              >
                {task?.goal ?? "Loading..."}
              </h1>
              <p
                className="text-[11px] font-mono"
                style={{ color: "var(--text-tertiary)" }}
              >
                {id}
              </p>
            </div>

            {/* Tabs */}
            <div
              className="flex items-center gap-0 mb-5 rounded-lg overflow-hidden"
              style={{ background: "var(--bg-raised)", border: "1px solid var(--border-subtle)" }}
            >
              {(["events", "artifacts"] as const).map((tab) => (
                <button
                  key={tab}
                  onClick={() => setActiveTab(tab)}
                  className="flex-1 py-2.5 text-[12px] font-medium uppercase tracking-wider transition-all"
                  style={{
                    background: activeTab === tab ? "var(--bg-elevated)" : "transparent",
                    color: activeTab === tab ? "var(--text-primary)" : "var(--text-tertiary)",
                    borderBottom: activeTab === tab ? "2px solid var(--accent)" : "2px solid transparent",
                  }}
                >
                  {tab === "events" ? `Events (${events.length})` : `Artifacts (${artifacts.length})`}
                </button>
              ))}
            </div>

            {/* Events tab */}
            {activeTab === "events" && (
              <div className="mb-8">
                {status === "live" && (
                  <div className="flex items-center gap-1.5 mb-3">
                    <span className="relative flex h-1.5 w-1.5">
                      <span
                        className="absolute inline-flex h-full w-full rounded-full opacity-75"
                        style={{ background: "var(--status-running)", animation: "pulse-ring 1.5s ease-out infinite" }}
                      />
                      <span
                        className="relative inline-flex h-1.5 w-1.5 rounded-full"
                        style={{ background: "var(--status-running)" }}
                      />
                    </span>
                    <span className="text-[11px] font-mono" style={{ color: "var(--status-running)" }}>
                      streaming
                    </span>
                  </div>
                )}

                {events.length === 0 ? (
                  <div className="flex flex-col gap-3 py-8">
                    <div className="animate-shimmer h-4 w-48 rounded" />
                    <div className="animate-shimmer h-4 w-32 rounded" />
                  </div>
                ) : (
                  <div className="flex flex-col gap-0">
                    {events.map((event, i) => (
                      <EventRow
                        key={event.id ?? `ws-${i}`}
                        event={event}
                        index={i}
                        formatTimestamp={formatTimestamp}
                      />
                    ))}
                  </div>
                )}
                <div ref={bottomRef} />
              </div>
            )}

            {/* Artifacts tab */}
            {activeTab === "artifacts" && (
              <div className="mb-8">
                {artifacts.length === 0 ? (
                  <div
                    className="rounded-xl p-8 text-center"
                    style={{ background: "var(--bg-raised)", border: "1px solid var(--border-subtle)" }}
                  >
                    <span className="text-2xl block mb-2">📦</span>
                    <p className="text-[13px]" style={{ color: "var(--text-tertiary)" }}>
                      {status === "live" ? "Artifacts will appear here as they are created..." : "No artifacts produced for this task."}
                    </p>
                  </div>
                ) : (
                  <ArtifactViewer artifacts={artifacts} />
                )}
              </div>
            )}

            {/* Result */}
            {status === "completed" && task?.result && (
              <ResultBlock result={task.result} />
            )}

            {/* Error */}
            {status === "failed" && task?.error && (
              <div
                className="rounded-xl p-5 animate-fade-in-up"
                style={{
                  background: "var(--status-error-dim)",
                  border: "1px solid rgba(248,113,113,0.15)",
                }}
              >
                <div className="flex items-center gap-2 mb-3">
                  <svg width="16" height="16" fill="none" viewBox="0 0 24 24" stroke="var(--status-error)" strokeWidth={2}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z" />
                  </svg>
                  <span
                    className="text-[11px] font-medium uppercase tracking-[0.08em]"
                    style={{ color: "var(--status-error)" }}
                  >
                    Task Failed
                  </span>
                </div>
                <pre
                  className="text-[13px] leading-relaxed font-mono whitespace-pre-wrap break-words"
                  style={{ color: "rgba(248,113,113,0.8)" }}
                >
                  {task.error}
                </pre>
              </div>
            )}
          </div>
        </main>
      </div>
    </div>
  );
}

/* ─── Sub-components ───────────────────────────────── */

function StatusPill({ status }: { status: ConnectionStatus }) {
  const config: Record<
    ConnectionStatus,
    { bg: string; color: string; dot?: string; label: string }
  > = {
    connecting: {
      bg: "var(--bg-elevated)",
      color: "var(--text-tertiary)",
      label: "Connecting",
    },
    live: {
      bg: "var(--status-running-dim)",
      color: "var(--status-running)",
      dot: "var(--status-running)",
      label: "Running",
    },
    completed: {
      bg: "var(--status-success-dim)",
      color: "var(--status-success)",
      label: "Completed",
    },
    failed: {
      bg: "var(--status-error-dim)",
      color: "var(--status-error)",
      label: "Failed",
    },
    disconnected: {
      bg: "var(--bg-elevated)",
      color: "var(--text-tertiary)",
      label: "Disconnected",
    },
  };
  const c = config[status];

  return (
    <span
      className="inline-flex items-center gap-2 rounded-full px-3 py-1 text-[12px] font-medium"
      style={{ background: c.bg, color: c.color }}
    >
      {c.dot && (
        <span className="relative flex h-2 w-2">
          <span
            className="absolute inline-flex h-full w-full rounded-full opacity-75"
            style={{
              background: c.dot,
              animation: "pulse-ring 1.5s ease-out infinite",
            }}
          />
          <span
            className="relative inline-flex h-2 w-2 rounded-full"
            style={{ background: c.dot }}
          />
        </span>
      )}
      {c.label}
    </span>
  );
}

function EventRow({
  event,
  index,
  formatTimestamp,
}: {
  event: TaskEvent;
  index: number;
  formatTimestamp: (iso?: string) => string;
}) {
  const typeConfig: Record<string, { color: string; icon: string }> = {
    task_started: { color: "var(--text-secondary)", icon: "▶" },
    planning: { color: "var(--status-planning)", icon: "◆" },
    plan_ready: { color: "var(--status-planning)", icon: "◆" },
    sandbox_ready: { color: "var(--text-secondary)", icon: "□" },
    step_started: { color: "var(--status-running)", icon: "●" },
    step_completed: { color: "var(--status-success)", icon: "✓" },
    step_failed: { color: "var(--status-error)", icon: "✗" },
    step_reflection: { color: "var(--accent)", icon: "💭" },
    step_warning: { color: "var(--accent)", icon: "⚠" },
    replanning: { color: "var(--status-planning)", icon: "↻" },
    replan_ready: { color: "var(--status-planning)", icon: "◆" },
    artifact_created: { color: "var(--status-success)", icon: "📎" },
    task_completed: { color: "var(--status-success)", icon: "★" },
    task_failed: { color: "var(--status-error)", icon: "✗" },
  };

  const cfg = typeConfig[event.event_type] ?? {
    color: "var(--text-tertiary)",
    icon: "·",
  };

  const label = event.event_type
    .split("_")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");

  const preview = getEventPreview(event);

  return (
    <div
      className="flex items-start gap-3 py-3 animate-fade-in-up"
      style={{
        borderBottom: "1px solid var(--border-subtle)",
        animationDelay: `${Math.min(index * 30, 300)}ms`,
      }}
    >
      <span
        className="mt-0.5 text-[12px] shrink-0 w-5 text-center"
        style={{ color: cfg.color }}
      >
        {cfg.icon}
      </span>

      <div className="flex-1 min-w-0">
        <div className="flex items-baseline gap-3">
          <span
            className="text-[13px] font-medium"
            style={{ color: cfg.color }}
          >
            {label}
          </span>
          <span
            className="text-[11px] font-mono"
            style={{ color: "var(--text-tertiary)" }}
          >
            {formatTimestamp(event.created_at)}
          </span>
        </div>

        {preview && (
          <p
            className="mt-1 text-[13px] leading-relaxed"
            style={{ color: "var(--text-secondary)" }}
          >
            {preview}
          </p>
        )}
      </div>
    </div>
  );
}

function getEventPreview(event: TaskEvent): string | null {
  const d = event.data;
  if (!d || Object.keys(d).length === 0) return null;

  switch (event.event_type) {
    case "task_started":
      return d.goal ? String(d.goal) : null;
    case "plan_ready":
      return d.steps ? `${d.steps} steps planned` : null;
    case "sandbox_ready":
      return d.container_id ? `Container ${String(d.container_id).slice(0, 12)}` : null;
    case "step_started":
      return d.description
        ? `Step ${d.step}/${d.total}: ${d.description}`
        : null;
    case "step_completed": {
      const rp = d.result_preview ? String(d.result_preview) : null;
      return rp && rp.length > 160 ? rp.slice(0, 160) + "..." : rp;
    }
    case "step_failed":
      return d.error ? String(d.error) : null;
    case "step_reflection":
      return d.reflection ? `🔍 ${String(d.reflection)}` : null;
    case "step_warning":
      return d.message ? String(d.message) : null;
    case "replanning":
      return "Re-evaluating approach based on execution results...";
    case "replan_ready":
      return d.new_steps ? `Replanned: ${d.new_steps} new steps` : "Plan updated";
    case "artifact_created":
      return d.name ? `Created: ${String(d.name)}` : null;
    case "task_completed":
      return d.summary ? String(d.summary) : "All steps completed successfully";
    case "task_failed":
      return d.error ? String(d.error) : null;
    default:
      return d.message ? String(d.message) : null;
  }
}

function ResultBlock({ result }: { result: Record<string, unknown> }) {
  const summary = result.summary ? String(result.summary) : null;
  const output = result.output ? String(result.output) : null;
  const keyOutputs = result.key_outputs as string[] | undefined;
  const nextSteps = result.next_steps as string[] | undefined;
  const code = result.code ? String(result.code) : null;
  const language = result.language ? String(result.language) : null;

  return (
    <div className="animate-fade-in-up flex flex-col gap-4">
      {/* Summary */}
      {summary && (
        <div
          className="rounded-xl p-5"
          style={{
            background: "var(--accent-glow)",
            border: "1px solid var(--border-accent)",
          }}
        >
          <div className="flex items-center gap-2 mb-3">
            <span className="text-[14px]">✨</span>
            <span
              className="text-[11px] font-medium uppercase tracking-[0.08em]"
              style={{ color: "var(--accent)" }}
            >
              Summary
            </span>
          </div>
          <p
            className="text-[14px] leading-relaxed"
            style={{ color: "var(--text-primary)" }}
          >
            {summary}
          </p>
        </div>
      )}

      {/* Key outputs */}
      {keyOutputs && keyOutputs.length > 0 && (
        <div
          className="rounded-xl p-5"
          style={{ background: "var(--bg-raised)", border: "1px solid var(--border-subtle)" }}
        >
          <span
            className="text-[11px] font-medium uppercase tracking-[0.08em] block mb-3"
            style={{ color: "var(--text-tertiary)" }}
          >
            Key Outputs
          </span>
          <ul className="flex flex-col gap-2">
            {keyOutputs.map((item, i) => (
              <li key={i} className="flex items-start gap-2 text-[13px]" style={{ color: "var(--text-secondary)" }}>
                <span style={{ color: "var(--status-success)" }}>✓</span>
                {item}
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* Next steps */}
      {nextSteps && nextSteps.length > 0 && (
        <div
          className="rounded-xl p-5"
          style={{ background: "var(--bg-raised)", border: "1px solid var(--border-subtle)" }}
        >
          <span
            className="text-[11px] font-medium uppercase tracking-[0.08em] block mb-3"
            style={{ color: "var(--text-tertiary)" }}
          >
            Suggested Next Steps
          </span>
          <ul className="flex flex-col gap-2">
            {nextSteps.map((item, i) => (
              <li key={i} className="flex items-start gap-2 text-[13px]" style={{ color: "var(--text-secondary)" }}>
                <span style={{ color: "var(--accent)" }}>→</span>
                {item}
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* Raw output */}
      {output && (
        <div
          className="rounded-xl overflow-hidden"
          style={{ border: "1px solid var(--border-accent)" }}
        >
          <div
            className="flex items-center gap-2 px-4 py-2.5"
            style={{
              background: "var(--accent-glow)",
              borderBottom: "1px solid var(--border-accent)",
            }}
          >
            <svg width="14" height="14" fill="none" viewBox="0 0 24 24" stroke="var(--accent)" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M6.75 7.5l3 2.25-3 2.25m4.5 0h3m-9 8.25h13.5A2.25 2.25 0 0021 18V6a2.25 2.25 0 00-2.25-2.25H5.25A2.25 2.25 0 003 6v12a2.25 2.25 0 002.25 2.25z" />
            </svg>
            <span
              className="text-[11px] font-medium uppercase tracking-[0.08em]"
              style={{ color: "var(--accent)" }}
            >
              Output
            </span>
          </div>
          <pre
            className="p-4 text-[13px] leading-relaxed overflow-auto max-h-[400px] font-mono whitespace-pre-wrap break-words"
            style={{ background: "var(--bg-raised)", color: "var(--text-secondary)" }}
          >
            {output}
          </pre>
        </div>
      )}

      {/* Code */}
      {code && (
        <div
          className="rounded-xl overflow-hidden"
          style={{ border: "1px solid var(--border-subtle)" }}
        >
          <div
            className="flex items-center gap-2 px-4 py-2.5"
            style={{
              background: "var(--bg-elevated)",
              borderBottom: "1px solid var(--border-subtle)",
            }}
          >
            <span
              className="text-[11px] font-medium uppercase tracking-[0.08em]"
              style={{ color: "var(--text-tertiary)" }}
            >
              {language ?? "Code"}
            </span>
          </div>
          <pre
            className="p-4 text-[13px] leading-relaxed overflow-auto max-h-[400px] font-mono whitespace-pre"
            style={{ background: "var(--bg-base)", color: "var(--text-secondary)" }}
          >
            {code}
          </pre>
        </div>
      )}
    </div>
  );
}
