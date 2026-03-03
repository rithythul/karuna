"use client";

import { useEffect, useState, useRef, useCallback } from "react";
import { useParams, useRouter } from "next/navigation";
import Link from "next/link";
import ArtifactViewer, { type Artifact } from "@/components/ArtifactViewer";
import StepsSidebar from "@/components/StepsSidebar";
import { useAuth } from "@/components/AuthProvider";
import ResultCard from "@/components/ResultCard";

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

type ConnectionStatus = "connecting" | "live" | "completed" | "failed" | "disconnected" | "reconnecting";

/* ─── Utilities ───────────────────────────────────── */

async function fetchJson<T>(url: string, context: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`${context}: ${res.status} ${res.statusText}${body ? ` — ${body}` : ""}`);
  }
  return res.json();
}

function handleFetchError(context: string) {
  return (err: unknown) => {
    console.error(`[${context}]`, err instanceof Error ? err.message : err);
  };
}

/** Convert known JSON step-result patterns into readable text */
function formatStepResult(data: Record<string, unknown>): string {
  const rp = data.result_preview;
  if (typeof rp !== "string") return "";

  // Try to parse as JSON for known patterns
  try {
    const parsed = JSON.parse(rp);
    if (typeof parsed === "object" && parsed !== null) {
      // File operations
      if (parsed.operation && parsed.path) {
        const op = String(parsed.operation);
        const opVerb = op === "create" ? "Created" : op === "edit" ? "Edited" : op === "read" ? "Read" : op === "list" ? "Listed" : op.charAt(0).toUpperCase() + op.slice(1);
        return `${opVerb} ${parsed.path}`;
      }
      // Shell commands
      if (parsed.command) {
        const cmd = String(parsed.command);
        const exit = parsed.exit_code != null ? ` (exit ${parsed.exit_code})` : "";
        const stdout = parsed.stdout ? `: ${String(parsed.stdout).trim().slice(0, 120)}` : "";
        return `Ran \`${cmd}\`${exit}${stdout}`;
      }
      // Code generation
      if (parsed.language && parsed.file) {
        return `Generated ${parsed.language} code → ${parsed.file}`;
      }
      if (parsed.language) {
        return `Generated ${parsed.language} code`;
      }
      // Research
      if (parsed.summary) {
        const s = String(parsed.summary);
        return s.length > 200 ? s.slice(0, 200) + "..." : s;
      }
    }
  } catch {
    // Not JSON — use raw preview
  }

  // Fallback: truncate raw preview
  return rp.length > 200 ? rp.slice(0, 200) + "..." : rp;
}

/** Strip markdown code fences from a string */
function stripCodeFences(s: string): string {
  const trimmed = s.trim();
  if (trimmed.startsWith("```")) {
    const lines = trimmed.split("\n");
    const start = 1;
    let end = lines.length;
    for (let i = lines.length - 1; i > 0; i--) {
      if (lines[i].startsWith("```")) {
        end = i;
        break;
      }
    }
    return lines.slice(start, end).join("\n").trim();
  }
  return trimmed;
}

/* ─── WebSocket URL ──────────────────────────────── */

function getWsUrl(taskId: string): string {
  if (typeof window === "undefined") return "";
  const envWs = process.env.NEXT_PUBLIC_BACKEND_WS_URL;
  if (envWs) {
    return `${envWs}/ws/tasks/${taskId}`;
  }
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  const host = process.env.NEXT_PUBLIC_BACKEND_HOST || "localhost:8000";
  return `${protocol}//${host}/ws/tasks/${taskId}`;
}

/* ─── Event grouping for narrative timeline ──────── */

interface StepGroup {
  type: "step";
  stepNumber: number;
  skill: string;
  description: string;
  events: TaskEvent[];
  status: "running" | "completed" | "failed" | "pending";
}

interface SystemEvent {
  type: "system";
  event: TaskEvent;
}

type TimelineEntry = StepGroup | SystemEvent;

const SYSTEM_EVENTS = new Set([
  "task_started", "planning", "plan_ready", "sandbox_ready",
  "replanning", "replan_ready", "task_completed", "task_failed", "task_cancelled",
]);

function groupEventsByStep(events: TaskEvent[]): TimelineEntry[] {
  const entries: TimelineEntry[] = [];
  let currentGroup: StepGroup | null = null;

  for (const event of events) {
    if (SYSTEM_EVENTS.has(event.event_type)) {
      // Flush current step group
      if (currentGroup) {
        entries.push(currentGroup);
        currentGroup = null;
      }
      entries.push({ type: "system", event });
    } else if (event.event_type === "step_started") {
      // Flush previous step group
      if (currentGroup) {
        entries.push(currentGroup);
      }
      currentGroup = {
        type: "step",
        stepNumber: Number(event.data.step) || 0,
        skill: String(event.data.skill || ""),
        description: String(event.data.description || ""),
        events: [event],
        status: "running",
      };
    } else if (currentGroup) {
      currentGroup.events.push(event);
      if (event.event_type === "step_completed") currentGroup.status = "completed";
      if (event.event_type === "step_failed") currentGroup.status = "failed";
    } else {
      // Orphan event — show as system
      entries.push({ type: "system", event });
    }
  }

  if (currentGroup) entries.push(currentGroup);
  return entries;
}

/* ─── Main Component ─────────────────────────────── */

export default function TaskPage() {
  const { id } = useParams<{ id: string }>();
  const router = useRouter();
  const { user, loading: authLoading, logout } = useAuth();
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
  const [cancelling, setCancelling] = useState(false);
  const [showTrace, setShowTrace] = useState(false);

  // Reconnection state
  const reconnectAttempt = useRef(0);
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const MAX_RECONNECT_DELAY = 30000;

  // Timer
  useEffect(() => {
    if (status === "completed" || status === "failed") return;
    const interval = setInterval(() => {
      setElapsed(Math.floor((Date.now() - startTimeRef.current) / 1000));
    }, 1000);
    return () => clearInterval(interval);
  }, [status]);

  // Refetch all task data (used on reconnect and terminal events)
  const refetchAll = useCallback(() => {
    if (!id) return;
    fetchJson<{ task: TaskData; steps: TaskStep[]; artifacts: Artifact[] }>(`/api/tasks/${id}`, "refetch task")
      .then((data) => {
        if (data.task) setTask(data.task);
        if (data.steps) setSteps(data.steps);
        if (data.artifacts) setArtifacts(data.artifacts);
      })
      .catch(handleFetchError("refetchAll"));
  }, [id]);

  // Fetch initial task data
  useEffect(() => {
    if (!id) return;

    fetchJson<{ task: TaskData; steps: TaskStep[]; artifacts: Artifact[] }>(`/api/tasks/${id}`, "initial task fetch")
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
      .catch(handleFetchError("initial task"));

    fetchJson<TaskEvent[]>(`/api/tasks/${id}/events`, "initial events fetch")
      .then((data) => {
        if (Array.isArray(data) && data.length > 0) {
          setEvents(data);
        }
      })
      .catch(handleFetchError("initial events"));

    fetchJson<Artifact[]>(`/api/tasks/${id}/artifacts`, "initial artifacts fetch")
      .then((data) => {
        if (Array.isArray(data)) setArtifacts(data);
      })
      .catch(handleFetchError("initial artifacts"));
  }, [id]);

  // WebSocket with reconnection
  const connectWs = useCallback(() => {
    if (!id || terminalRef.current) return;

    const wsUrl = getWsUrl(id);
    const ws = new WebSocket(wsUrl);
    wsRef.current = ws;

    ws.onopen = () => {
      setStatus("live");

      // Refetch on reconnect to catch missed events
      if (reconnectAttempt.current > 0) {
        refetchAll();
      }
      reconnectAttempt.current = 0;
    };

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
          fetchJson<{ steps: TaskStep[]; artifacts: Artifact[] }>(`/api/tasks/${id}`, "step update")
            .then((data) => {
              if (data.steps) setSteps(data.steps);
              if (data.artifacts) setArtifacts(data.artifacts);
            })
            .catch(handleFetchError("step update"));
        }

        // Handle step retry events (backend emits "step_retrying")
        if (event.event_type === "step_retrying") {
          fetchJson<{ steps: TaskStep[] }>(`/api/tasks/${id}`, "retry update")
            .then((data) => { if (data.steps) setSteps(data.steps); })
            .catch(handleFetchError("retry update"));
        }

        // Handle replan events
        if (event.event_type === "replan_ready") {
          fetchJson<{ steps: TaskStep[] }>(`/api/tasks/${id}`, "replan update")
            .then((data) => { if (data.steps) setSteps(data.steps); })
            .catch(handleFetchError("replan update"));
        }

        const isCompleted = event.event_type === "task_completed";
        const isFailed = event.event_type === "task_failed";
        const isCancelled = event.event_type === "task_cancelled";

        if (isCompleted || isFailed || isCancelled) {
          terminalRef.current = true;
          setStatus(isCompleted ? "completed" : "failed");
          setCurrentStep(null);
          setCancelling(false);
          refetchAll();
        }
      } catch (err) {
        console.error("[ws:parse]", err);
      }
    };

    ws.onclose = () => {
      if (!terminalRef.current) {
        scheduleReconnect();
      }
    };
    ws.onerror = () => {
      // onclose will fire after this
    };

    return ws;
  }, [id, refetchAll]);

  // Reconnection scheduler with exponential backoff
  const scheduleReconnect = useCallback(() => {
    if (terminalRef.current) return;
    setStatus("reconnecting");

    const delay = Math.min(1000 * Math.pow(2, reconnectAttempt.current), MAX_RECONNECT_DELAY);
    reconnectAttempt.current += 1;

    reconnectTimer.current = setTimeout(() => {
      if (!terminalRef.current) {
        connectWs();
        // Refetch to catch events missed while disconnected
        refetchAll();
      }
    }, delay);
  }, [connectWs, refetchAll]);

  // Initial WebSocket connection
  useEffect(() => {
    if (!id || terminalRef.current) return;
    const ws = connectWs();
    return () => {
      if (reconnectTimer.current) clearTimeout(reconnectTimer.current);
      ws?.close();
    };
  }, [id, connectWs]);

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

  const handleCancel = async () => {
    if (!id || cancelling) return;
    setCancelling(true);
    try {
      const res = await fetch(`/api/tasks/${id}/cancel`, { method: "POST" });
      if (!res.ok) {
        const body = await res.json().catch(() => ({}));
        console.error("[cancel]", body);
        setCancelling(false);
      }
      // On success, the task_cancelled event will arrive via WS and update UI
    } catch (err) {
      console.error("[cancel]", err);
      setCancelling(false);
    }
  };

  const completedSteps = steps.filter(
    (s) => s.status.toLowerCase() === "completed"
  ).length;

  const progressPct = steps.length > 0 ? (completedSteps / steps.length) * 100 : 0;
  const canCancel = status === "live" || status === "connecting" || status === "reconnecting";

  // Group events into narrative timeline
  const timeline = groupEventsByStep(events);

  // Auth gate
  if (authLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center" style={{ background: "var(--bg-deep)" }}>
        <svg className="animate-spin" width="24" height="24" viewBox="0 0 24 24" fill="none">
          <circle cx="12" cy="12" r="10" stroke="var(--accent)" strokeWidth="2.5" strokeDasharray="31.416" strokeDashoffset="10" strokeLinecap="round"/>
        </svg>
      </div>
    );
  }
  if (!user) {
    router.replace("/");
    return null;
  }

  return (
    <div className="min-h-screen" style={{ background: "var(--bg-deep)" }}>
      {/* Top bar */}
      <header
        className="sticky top-0 z-40 flex items-center justify-between px-6 py-3"
        style={{
          background: "var(--bg-header)",
          backdropFilter: "blur(12px)",
          borderBottom: "1px solid var(--border-subtle)",
        }}
      >
        <div className="flex items-center gap-4">
          <Link
            href="/tasks"
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
            Task History
          </Link>

          <div
            className="h-4"
            style={{ width: "1px", background: "var(--border-default)" }}
          />

          <StatusPill status={status} />
        </div>

        <div className="flex items-center gap-4">
          {/* Stop button */}
          {canCancel && (
            <button
              onClick={handleCancel}
              disabled={cancelling}
              className="flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-[12px] font-medium transition-all cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
              style={{
                background: "var(--status-error-dim)",
                border: "1px solid color-mix(in srgb, var(--status-error) 25%, transparent)",
                color: "var(--status-error)",
              }}
            >
              {cancelling ? (
                <svg className="animate-spin" width="12" height="12" viewBox="0 0 24 24" fill="none">
                  <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="2.5" strokeDasharray="31.416" strokeDashoffset="10" strokeLinecap="round"/>
                </svg>
              ) : (
                <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
                  <rect x="6" y="6" width="12" height="12" rx="2" />
                </svg>
              )}
              {cancelling ? "Stopping..." : "Stop"}
            </button>
          )}

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

          {/* Memory link */}
          <Link
            href="/memory"
            className="text-[12px] font-medium transition-colors"
            style={{ color: "var(--text-secondary)" }}
          >
            Memory
          </Link>

          {/* User */}
          <div className="flex items-center gap-2 ml-2 pl-2" style={{ borderLeft: "1px solid var(--border-subtle)" }}>
            <span className="text-[12px]" style={{ color: "var(--text-tertiary)" }}>
              {user.full_name}
            </span>
            <button
              onClick={logout}
              className="rounded-md px-2 py-1 text-[11px] transition-colors cursor-pointer"
              style={{ color: "var(--text-tertiary)", background: "var(--bg-elevated)" }}
            >
              Sign out
            </button>
          </div>
        </div>
      </header>

      {/* Reconnecting banner */}
      {status === "reconnecting" && (
        <div
          className="flex items-center justify-center gap-2 py-2 text-[12px]"
          style={{ background: "var(--accent-glow)", color: "var(--accent)" }}
        >
          <svg className="animate-spin" width="12" height="12" viewBox="0 0 24 24" fill="none">
            <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="2.5" strokeDasharray="31.416" strokeDashoffset="10" strokeLinecap="round"/>
          </svg>
          Reconnecting...
        </div>
      )}

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
          <StepsSidebar steps={steps} currentStep={currentStep} taskId={id} />

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

            {/* ── Result Card (completed tasks only) ─────────── */}
            {task?.status === "completed" && task.result && (
              <div className="mx-auto max-w-[900px] px-4 pt-4">
                <ResultCard
                  result={task.result as unknown as { summary?: string; key_outputs?: string[]; artifacts?: string[]; next_steps?: string[] }}
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

            {(task?.status !== "completed" || showTrace) && (
              <>
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

                {/* Events tab — Narrative Timeline */}
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
                        {timeline.map((entry, i) => {
                          if (entry.type === "system") {
                            return (
                              <EventRow
                                key={entry.event.id ?? `sys-${i}`}
                                event={entry.event}
                                index={i}
                                formatTimestamp={formatTimestamp}
                              />
                            );
                          }
                          return (
                            <StepBlock
                              key={`step-${entry.stepNumber}-${i}`}
                              group={entry}
                              formatTimestamp={formatTimestamp}
                              defaultOpen={entry.status === "running"}
                            />
                          );
                        })}
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
                        <p className="text-[13px]" style={{ color: "var(--text-tertiary)" }}>
                          {status === "live" ? "Artifacts will appear here as they are created..." : "No artifacts produced for this task."}
                        </p>
                      </div>
                    ) : (
                      <ArtifactViewer artifacts={artifacts} taskId={id} />
                    )}
                  </div>
                )}

                {/* Result */}
                {status === "completed" && task?.result && (
                  <ResultBlock result={task.result} />
                )}
              </>
            )}

            {/* Error */}
            {status === "failed" && task?.error && (
              <div
                className="rounded-xl p-5 animate-fade-in-up"
                style={{
                  background: "var(--status-error-dim)",
                  border: "1px solid color-mix(in srgb, var(--status-error) 15%, transparent)",
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
                  style={{ color: "var(--status-error)" }}
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
    reconnecting: {
      bg: "var(--accent-glow)",
      color: "var(--accent)",
      dot: "var(--accent)",
      label: "Reconnecting",
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

/** Narrative step block — collapsible group of events for one step */
function StepBlock({
  group,
  formatTimestamp,
  defaultOpen,
}: {
  group: StepGroup;
  formatTimestamp: (iso?: string) => string;
  defaultOpen: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);

  // Auto-open when step starts running
  useEffect(() => {
    if (group.status === "running") setOpen(true);
  }, [group.status]);

  const statusConfig: Record<string, { color: string; icon: string }> = {
    running: { color: "var(--status-running)", icon: "●" },
    completed: { color: "var(--status-success)", icon: "✓" },
    failed: { color: "var(--status-error)", icon: "✗" },
    pending: { color: "var(--text-tertiary)", icon: "○" },
  };
  const cfg = statusConfig[group.status] || statusConfig.pending;

  const skillIcons: Record<string, string> = {
    code: "{ }",
    browser: "🌐",
    research: "🔍",
    api: "🔌",
    data_analysis: "📊",
    deploy: "🚀",
    // Keep old names as fallbacks
    shell: ">_",
    file: "📄",
    browse: "🌐",
  };
  const skillIcon = skillIcons[group.skill] || "⚡";

  return (
    <div
      className="mb-1 rounded-lg overflow-hidden animate-fade-in-up"
      style={{
        background: "var(--bg-raised)",
        border: `1px solid ${group.status === "running" ? "var(--border-accent)" : "var(--border-subtle)"}`,
      }}
    >
      {/* Header — always visible */}
      <button
        onClick={() => setOpen(!open)}
        className="w-full flex items-center gap-3 px-4 py-3 text-left cursor-pointer transition-colors"
        style={{ background: open ? "var(--bg-elevated)" : "transparent" }}
      >
        <span className="text-[13px] shrink-0" style={{ color: cfg.color }}>
          {cfg.icon}
        </span>
        <span className="text-[12px] shrink-0 font-mono" style={{ color: "var(--text-tertiary)" }}>
          {skillIcon}
        </span>
        <span className="flex-1 min-w-0 text-[13px] font-medium truncate" style={{ color: "var(--text-primary)" }}>
          Step {group.stepNumber}: {group.description}
        </span>
        <span
          className="text-[10px] font-medium uppercase tracking-wider px-2 py-0.5 rounded-full shrink-0"
          style={{ background: cfg.color + "20", color: cfg.color }}
        >
          {group.status}
        </span>
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="var(--text-tertiary)"
          strokeWidth={2}
          className={`shrink-0 transition-transform ${open ? "rotate-180" : ""}`}
        >
          <path strokeLinecap="round" strokeLinejoin="round" d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {/* Sub-events — collapsible */}
      {open && (
        <div className="px-4 pb-3">
          {group.events.map((event, i) => (
            <EventRow
              key={event.id ?? `step-ev-${i}`}
              event={event}
              index={i}
              formatTimestamp={formatTimestamp}
              compact
            />
          ))}
        </div>
      )}
    </div>
  );
}

function EventRow({
  event,
  index,
  formatTimestamp,
  compact,
}: {
  event: TaskEvent;
  index: number;
  formatTimestamp: (iso?: string) => string;
  compact?: boolean;
}) {
  const [expanded, setExpanded] = useState(false);

  const typeConfig: Record<string, { color: string; icon: string }> = {
    task_started: { color: "var(--text-secondary)", icon: "▶" },
    planning: { color: "var(--status-planning)", icon: "◆" },
    plan_ready: { color: "var(--status-planning)", icon: "◆" },
    sandbox_ready: { color: "var(--text-secondary)", icon: "□" },
    step_started: { color: "var(--status-running)", icon: "●" },
    step_completed: { color: "var(--status-success)", icon: "✓" },
    step_failed: { color: "var(--status-error)", icon: "✗" },
    step_reflection: { color: "var(--accent)", icon: "↻" },
    step_warning: { color: "var(--accent)", icon: "⚠" },
    replanning: { color: "var(--status-planning)", icon: "↻" },
    replan_ready: { color: "var(--status-planning)", icon: "◆" },
    artifact_created: { color: "var(--status-success)", icon: "+" },
    task_completed: { color: "var(--status-success)", icon: "★" },
    task_failed: { color: "var(--status-error)", icon: "✗" },
    task_cancelled: { color: "var(--status-error)", icon: "■" },
    agent_started: { color: "var(--accent)", icon: "▸" },
    agent_tool_call: { color: "var(--text-secondary)", icon: "⚙" },
    agent_tool_result: { color: "var(--text-secondary)", icon: "↩" },
    agent_delegating: { color: "var(--accent)", icon: "⤳" },
    agent_completed: { color: "var(--status-success)", icon: "✓" },
    agent_error: { color: "var(--status-error)", icon: "✗" },
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
  const isExpandable = event.event_type === "step_completed"
    || event.event_type === "step_failed"
    || event.event_type === "step_reflection"
    || event.event_type === "agent_completed"
    || event.event_type === "agent_error"
    || event.event_type === "agent_tool_result";

  return (
    <div
      className={`flex items-start gap-3 animate-fade-in-up ${isExpandable ? "cursor-pointer" : ""}`}
      style={{
        borderBottom: compact ? "none" : "1px solid var(--border-subtle)",
        padding: compact ? "6px 0" : "12px 0",
        animationDelay: `${Math.min(index * 30, 300)}ms`,
      }}
      onClick={isExpandable ? () => setExpanded(!expanded) : undefined}
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
            className={`font-medium ${compact ? "text-[12px]" : "text-[13px]"}`}
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
          {isExpandable && (
            <svg
              width="10"
              height="10"
              viewBox="0 0 24 24"
              fill="none"
              stroke="var(--text-tertiary)"
              strokeWidth={2}
              className={`transition-transform ${expanded ? "rotate-180" : ""}`}
            >
              <path strokeLinecap="round" strokeLinejoin="round" d="M19 9l-7 7-7-7" />
            </svg>
          )}
        </div>

        {preview && (
          <p
            className={`mt-1 leading-relaxed ${compact ? "text-[12px]" : "text-[13px]"}`}
            style={{ color: "var(--text-secondary)" }}
          >
            {preview}
          </p>
        )}

        {/* Expanded detail for step events */}
        {expanded && event.data && (
          <pre
            className="mt-2 p-3 rounded-lg text-[11px] font-mono overflow-auto max-h-[300px] whitespace-pre-wrap break-words"
            style={{
              background: "var(--bg-base)",
              border: "1px solid var(--border-subtle)",
              color: "var(--text-secondary)",
            }}
          >
            {JSON.stringify(event.data, null, 2)}
          </pre>
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
    case "step_completed":
      return formatStepResult(d);
    case "step_failed":
      return d.error ? String(d.error) : null;
    case "step_reflection":
      return d.reflection ? String(d.reflection) : null;
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
    case "task_cancelled":
      return d.reason ? String(d.reason) : "Task cancelled";
    case "agent_started":
      return d.agent ? `Agent "${d.agent}" started${d.goal ? `: ${d.goal}` : ""}` : null;
    case "agent_tool_call":
      return d.display ? String(d.display) : d.tool ? `Using ${d.tool}` : null;
    case "agent_tool_result":
      return d.display ? String(d.display) : d.tool ? `${d.tool} completed` : null;
    case "agent_delegating":
      return d.child_agent ? `Delegating to ${d.child_agent}: ${d.sub_goal || ""}` : null;
    case "agent_completed":
      return d.output_preview ? String(d.output_preview) : d.agent ? `Agent "${d.agent}" finished (${d.turns_used || 0} turns)` : null;
    case "agent_error":
      return d.error ? `Agent error: ${d.error}` : null;
    default:
      return d.message ? String(d.message) : null;
  }
}

function safeString(val: unknown): string | null {
  if (val == null) return null;
  if (typeof val === "string") return val;
  return JSON.stringify(val, null, 2);
}

function ResultBlock({ result }: { result: Record<string, unknown> }) {
  // Try to parse if result is a string (LLM sometimes wraps in code fences)
  let parsed = result;
  if (typeof result === "string") {
    try {
      parsed = JSON.parse(stripCodeFences(result as string));
    } catch {
      parsed = { summary: result };
    }
  }

  const rawSummary = safeString(parsed.summary);
  // Strip code fences from summary if the LLM wrapped it
  const summary = rawSummary ? stripCodeFences(rawSummary) : null;
  const output = safeString(parsed.output);

  // Handle key_outputs that might be objects instead of strings
  const keyOutputs = Array.isArray(parsed.key_outputs)
    ? parsed.key_outputs.map((v) => {
        if (typeof v === "string") return v;
        if (typeof v === "object" && v !== null) {
          // Try to make it readable
          if ("path" in v && "operation" in v) {
            return `${(v as Record<string, string>).operation}: ${(v as Record<string, string>).path}`;
          }
          if ("file" in v) return String((v as Record<string, string>).file);
          if ("name" in v) return String((v as Record<string, string>).name);
        }
        return JSON.stringify(v);
      })
    : undefined;

  const nextSteps = Array.isArray(parsed.next_steps)
    ? parsed.next_steps.map((v) => (typeof v === "string" ? v : JSON.stringify(v)))
    : undefined;
  const code = safeString(parsed.code);
  const language = safeString(parsed.language);

  const hasStructured = summary || output || keyOutputs?.length || nextSteps?.length || code;
  const fallback = !hasStructured ? JSON.stringify(parsed, null, 2) : null;

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
            style={{ background: "#1e1e23", color: "#d4d4d8" }}
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
            style={{ background: "#1e1e23", color: "#d4d4d8" }}
          >
            {code}
          </pre>
        </div>
      )}

      {/* Fallback: raw JSON when no structured fields matched */}
      {fallback && (
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
            <span
              className="text-[11px] font-medium uppercase tracking-[0.08em]"
              style={{ color: "var(--accent)" }}
            >
              Result
            </span>
          </div>
          <pre
            className="p-4 text-[13px] leading-relaxed overflow-auto max-h-[400px] font-mono whitespace-pre-wrap break-words"
            style={{ background: "#1e1e23", color: "#d4d4d8" }}
          >
            {fallback}
          </pre>
        </div>
      )}
    </div>
  );
}
