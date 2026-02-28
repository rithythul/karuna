"use client";

import { useEffect, useState, useRef, useCallback } from "react";
import { useParams } from "next/navigation";
import Link from "next/link";

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
}

interface TaskData {
  id: string;
  goal: string;
  status: string;
  result: Record<string, unknown> | null;
  error: string | null;
}

type ConnectionStatus = "connecting" | "live" | "completed" | "failed" | "disconnected";

export default function TaskPage() {
  const { id } = useParams<{ id: string }>();
  const [events, setEvents] = useState<TaskEvent[]>([]);
  const [task, setTask] = useState<TaskData | null>(null);
  const [steps, setSteps] = useState<TaskStep[]>([]);
  const [status, setStatus] = useState<ConnectionStatus>("connecting");
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

        const isCompleted = event.event_type === "task_completed";
        const isFailed = event.event_type === "task_failed";

        if (isCompleted || isFailed) {
          terminalRef.current = true;
          setStatus(isCompleted ? "completed" : "failed");
          fetch(`/api/tasks/${id}`)
            .then((res) => res.json())
            .then((data) => {
              if (data.task) setTask(data.task);
              if (data.steps) setSteps(data.steps);
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

  return (
    <div className="min-h-screen" style={{ background: "var(--bg-deep)" }}>
      {/* Top bar */}
      <header
        className="sticky top-0 z-40 flex items-center justify-between px-6 py-4"
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

        <div
          className="flex items-center gap-3 text-[13px] font-mono"
          style={{ color: "var(--text-tertiary)" }}
        >
          <span>{formatTime(elapsed)}</span>
        </div>
      </header>

      <div className="mx-auto max-w-[860px] px-6 pt-8 pb-24">
        {/* Goal */}
        <div className="mb-8 animate-fade-in-up">
          <h1
            className="text-[1.75rem] leading-snug tracking-[-0.01em] mb-2"
            style={{
              fontFamily: "var(--font-display)",
              color: "var(--text-primary)",
            }}
          >
            {task?.goal ?? "Loading..."}
          </h1>
          <p
            className="text-[12px] font-mono"
            style={{ color: "var(--text-tertiary)" }}
          >
            {id}
          </p>
        </div>

        {/* Steps progress */}
        {steps.length > 0 && (
          <div
            className="mb-8 rounded-xl p-5 animate-fade-in-up"
            style={{
              background: "var(--bg-raised)",
              border: "1px solid var(--border-subtle)",
              animationDelay: "50ms",
            }}
          >
            {/* Progress bar */}
            <div className="flex items-center justify-between mb-4">
              <span
                className="text-[11px] font-medium uppercase tracking-[0.08em]"
                style={{ color: "var(--text-tertiary)" }}
              >
                Execution Progress
              </span>
              <span
                className="text-[12px] font-mono"
                style={{ color: "var(--text-secondary)" }}
              >
                {completedSteps}/{steps.length}
              </span>
            </div>

            {/* Bar */}
            <div
              className="h-1 rounded-full overflow-hidden mb-5"
              style={{ background: "var(--bg-elevated)" }}
            >
              <div
                className="h-full rounded-full transition-all duration-700 ease-out"
                style={{
                  width: `${steps.length > 0 ? (completedSteps / steps.length) * 100 : 0}%`,
                  background: status === "failed" ? "var(--status-error)" : "var(--accent)",
                }}
              />
            </div>

            {/* Step list */}
            <div className="flex flex-col gap-2">
              {steps.map((step, i) => (
                <div
                  key={step.id}
                  className="flex items-center gap-3 py-1.5"
                >
                  <StepIndicator status={step.status} />
                  <span
                    className="text-[12px] font-mono shrink-0"
                    style={{ color: "var(--text-tertiary)", width: "32px" }}
                  >
                    {i + 1}.
                  </span>
                  <span
                    className="text-[13px] font-medium shrink-0 px-2 py-0.5 rounded-md"
                    style={{
                      background: "var(--bg-elevated)",
                      color: "var(--accent)",
                      fontSize: "11px",
                      fontFamily: "var(--font-mono)",
                    }}
                  >
                    {step.skill}
                  </span>
                  <span
                    className="text-[13px] truncate"
                    style={{ color: "var(--text-secondary)" }}
                  >
                    {step.description}
                  </span>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Event stream */}
        <div className="mb-8">
          <div className="flex items-center gap-2 mb-4">
            <span
              className="text-[11px] font-medium uppercase tracking-[0.08em]"
              style={{ color: "var(--text-tertiary)" }}
            >
              Event Stream
            </span>
            {status === "live" && (
              <span
                className="flex items-center gap-1.5 text-[11px] font-mono"
                style={{ color: "var(--status-running)" }}
              >
                <span className="relative flex h-1.5 w-1.5">
                  <span
                    className="absolute inline-flex h-full w-full rounded-full opacity-75"
                    style={{
                      background: "var(--status-running)",
                      animation: "pulse-ring 1.5s ease-out infinite",
                    }}
                  />
                  <span
                    className="relative inline-flex h-1.5 w-1.5 rounded-full"
                    style={{ background: "var(--status-running)" }}
                  />
                </span>
                streaming
              </span>
            )}
          </div>

          {events.length === 0 ? (
            <div
              className="flex items-center gap-3 py-6 text-[13px]"
              style={{ color: "var(--text-tertiary)" }}
            >
              <div className="animate-shimmer h-4 w-48 rounded" />
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

function StepIndicator({ status }: { status: string }) {
  const s = status.toLowerCase();

  if (s === "completed") {
    return (
      <div
        className="flex items-center justify-center w-5 h-5 rounded-full"
        style={{ background: "var(--status-success-dim)" }}
      >
        <svg width="12" height="12" fill="none" viewBox="0 0 24 24" stroke="var(--status-success)" strokeWidth={3}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12.75l6 6 9-13.5" />
        </svg>
      </div>
    );
  }

  if (s === "failed") {
    return (
      <div
        className="flex items-center justify-center w-5 h-5 rounded-full"
        style={{ background: "var(--status-error-dim)" }}
      >
        <svg width="12" height="12" fill="none" viewBox="0 0 24 24" stroke="var(--status-error)" strokeWidth={3}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </div>
    );
  }

  if (s === "running") {
    return (
      <div className="flex items-center justify-center w-5 h-5">
        <span className="relative flex h-3 w-3">
          <span
            className="absolute inline-flex h-full w-full rounded-full opacity-75"
            style={{
              background: "var(--status-running)",
              animation: "pulse-ring 1.5s ease-out infinite",
            }}
          />
          <span
            className="relative inline-flex h-3 w-3 rounded-full"
            style={{ background: "var(--status-running)" }}
          />
        </span>
      </div>
    );
  }

  return (
    <div
      className="w-5 h-5 rounded-full"
      style={{ border: "1.5px solid var(--border-default)" }}
    />
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

  // Extract meaningful data preview
  const preview = getEventPreview(event);

  return (
    <div
      className="flex items-start gap-3 py-3 animate-fade-in-up"
      style={{
        borderBottom: "1px solid var(--border-subtle)",
        animationDelay: `${Math.min(index * 30, 300)}ms`,
      }}
    >
      {/* Icon */}
      <span
        className="mt-0.5 text-[12px] shrink-0 w-5 text-center"
        style={{ color: cfg.color }}
      >
        {cfg.icon}
      </span>

      {/* Content */}
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
      return d.container_id ? `Container ${String(d.container_id)}` : null;
    case "step_started":
      return d.description
        ? `Step ${d.step}/${d.total}: ${d.description}`
        : null;
    case "step_completed": {
      const rp = d.result_preview ? String(d.result_preview) : null;
      return rp && rp.length > 120 ? rp.slice(0, 120) + "..." : rp;
    }
    case "step_failed":
      return d.error ? String(d.error) : null;
    case "task_completed":
      return "All steps completed successfully";
    case "task_failed":
      return d.error ? String(d.error) : null;
    default:
      return null;
  }
}

function ResultBlock({ result }: { result: Record<string, unknown> }) {
  const output = result.output ? String(result.output) : null;
  const code = result.code ? String(result.code) : null;
  const language = result.language ? String(result.language) : null;

  return (
    <div className="animate-fade-in-up flex flex-col gap-4">
      {/* Output */}
      {output && (
        <div
          className="rounded-xl overflow-hidden"
          style={{
            border: "1px solid var(--border-accent)",
          }}
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
            style={{
              background: "var(--bg-raised)",
              color: "var(--text-secondary)",
            }}
          >
            {output}
          </pre>
        </div>
      )}

      {/* Code */}
      {code && (
        <div
          className="rounded-xl overflow-hidden"
          style={{
            border: "1px solid var(--border-subtle)",
          }}
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
            style={{
              background: "var(--bg-base)",
              color: "var(--text-secondary)",
            }}
          >
            {code}
          </pre>
        </div>
      )}
    </div>
  );
}
