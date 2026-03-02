"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { useAuth } from "@/components/AuthProvider";

interface Task {
  id: string;
  user_id: string;
  goal: string;
  status: "pending" | "planning" | "running" | "paused" | "completed" | "failed";
  result: Record<string, unknown> | null;
  error: string | null;
  token_usage: Record<string, unknown> | null;
  total_duration_ms: number | null;
  created_at: string;
  updated_at: string;
}

/* ─── Helpers ───────────────────────────────────── */

function timeAgo(dateStr: string): string {
  const now = Date.now();
  const then = new Date(dateStr).getTime();
  const seconds = Math.floor((now - then) / 1000);

  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months}mo ago`;
  const years = Math.floor(months / 12);
  return `${years}y ago`;
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const seconds = Math.floor(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const remainSec = seconds % 60;
  if (minutes < 60) return remainSec > 0 ? `${minutes}m ${remainSec}s` : `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const remainMin = minutes % 60;
  return remainMin > 0 ? `${hours}h ${remainMin}m` : `${hours}h`;
}

function truncateGoal(goal: string, maxLen = 100): string {
  if (goal.length <= maxLen) return goal;
  return goal.slice(0, maxLen).trimEnd() + "\u2026";
}

const STATUS_CONFIG: Record<
  Task["status"],
  { bg: string; color: string; dot?: string; label: string }
> = {
  pending: {
    bg: "var(--bg-elevated)",
    color: "var(--text-tertiary)",
    label: "Pending",
  },
  planning: {
    bg: "var(--status-planning-dim)",
    color: "var(--status-planning)",
    dot: "var(--status-planning)",
    label: "Planning",
  },
  running: {
    bg: "var(--status-running-dim)",
    color: "var(--status-running)",
    dot: "var(--status-running)",
    label: "Running",
  },
  paused: {
    bg: "var(--accent-glow)",
    color: "var(--accent)",
    label: "Paused",
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
};

/* ─── Component ─────────────────────────────────── */

export default function TaskHistoryPage() {
  const router = useRouter();
  const { user, loading: authLoading } = useAuth();
  const [tasks, setTasks] = useState<Task[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Redirect if not logged in
  useEffect(() => {
    if (!authLoading && !user) {
      router.replace("/");
    }
  }, [authLoading, user, router]);

  // Fetch tasks
  useEffect(() => {
    if (!user) return;

    fetch("/api/tasks")
      .then((res) => {
        if (!res.ok) throw new Error(`Failed to fetch tasks: ${res.status}`);
        return res.json();
      })
      .then((data) => {
        setTasks(data.tasks ?? []);
      })
      .catch((err) => {
        setError(err instanceof Error ? err.message : "Failed to load tasks");
      })
      .finally(() => setLoading(false));
  }, [user]);

  // Auth loading state
  if (authLoading || (!user && !authLoading)) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <svg className="animate-spin" width="24" height="24" viewBox="0 0 24 24" fill="none">
          <circle
            cx="12" cy="12" r="10"
            stroke="var(--accent)" strokeWidth="2.5"
            strokeDasharray="31.416" strokeDashoffset="10" strokeLinecap="round"
          />
        </svg>
      </div>
    );
  }

  return (
    <div className="min-h-screen" style={{ background: "var(--bg-deep)" }}>
      {/* Header */}
      <header
        className="sticky top-0 z-40 flex items-center justify-between px-6 py-4 animate-fade-in-up"
        style={{
          background: "var(--bg-base)",
          borderBottom: "1px solid var(--border-subtle)",
        }}
      >
        <h1
          className="text-[20px] tracking-[-0.01em]"
          style={{ fontFamily: "var(--font-display)", color: "var(--text-primary)" }}
        >
          Task History
        </h1>
        <Link
          href="/"
          className="flex items-center gap-2 rounded-xl px-4 py-2 text-[13px] font-medium transition-all duration-200"
          style={{
            background: "var(--accent)",
            color: "var(--text-on-accent)",
          }}
        >
          <svg width="14" height="14" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
          </svg>
          New Task
        </Link>
      </header>

      {/* Content */}
      <main className="mx-auto max-w-[800px] px-6 py-8">
        {loading ? (
          /* Loading state */
          <div className="flex flex-col items-center justify-center py-24 animate-fade-in-up">
            <svg className="animate-spin" width="24" height="24" viewBox="0 0 24 24" fill="none">
              <circle
                cx="12" cy="12" r="10"
                stroke="var(--accent)" strokeWidth="2.5"
                strokeDasharray="31.416" strokeDashoffset="10" strokeLinecap="round"
              />
            </svg>
            <p className="mt-4 text-[13px]" style={{ color: "var(--text-tertiary)" }}>
              Loading tasks...
            </p>
          </div>
        ) : error ? (
          /* Error state */
          <div
            className="rounded-xl px-4 py-3 text-[13px] animate-fade-in-up"
            style={{
              background: "var(--status-error-dim)",
              border: "1px solid rgba(220,38,38,0.2)",
              color: "var(--status-error)",
            }}
          >
            {error}
          </div>
        ) : tasks.length === 0 ? (
          /* Empty state */
          <div className="flex flex-col items-center justify-center py-24 animate-fade-in-up">
            <svg
              width="48" height="48" fill="none" viewBox="0 0 24 24"
              stroke="var(--text-tertiary)" strokeWidth={1}
            >
              <path
                strokeLinecap="round" strokeLinejoin="round"
                d="M9 12h3.75M9 15h3.75M9 18h3.75m3 .75H18a2.25 2.25 0 002.25-2.25V6.108c0-1.135-.845-2.098-1.976-2.192a48.424 48.424 0 00-1.123-.08m-5.801 0c-.065.21-.1.433-.1.664 0 .414.336.75.75.75h4.5a.75.75 0 00.75-.75 2.25 2.25 0 00-.1-.664m-5.8 0A2.251 2.251 0 0113.5 2.25H15a2.25 2.25 0 012.15 1.586m-5.8 0c-.376.023-.75.05-1.124.08C9.095 4.01 8.25 4.973 8.25 6.108V8.25m0 0H4.875c-.621 0-1.125.504-1.125 1.125v11.25c0 .621.504 1.125 1.125 1.125h9.75c.621 0 1.125-.504 1.125-1.125V9.375c0-.621-.504-1.125-1.125-1.125H8.25z"
              />
            </svg>
            <p className="mt-4 text-[15px] font-medium" style={{ color: "var(--text-secondary)" }}>
              No tasks yet
            </p>
            <p className="mt-1 text-[13px]" style={{ color: "var(--text-tertiary)" }}>
              Start by creating your first task
            </p>
            <Link
              href="/"
              className="mt-6 flex items-center gap-2 rounded-xl px-5 py-2.5 text-[13px] font-medium transition-all duration-200"
              style={{
                background: "var(--accent)",
                color: "var(--text-on-accent)",
              }}
            >
              <svg width="14" height="14" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
              </svg>
              New Task
            </Link>
          </div>
        ) : (
          /* Task list */
          <div className="flex flex-col gap-2">
            {tasks.map((task, i) => {
              const sc = STATUS_CONFIG[task.status] ?? STATUS_CONFIG.pending;
              return (
                <Link
                  key={task.id}
                  href={`/tasks/${task.id}`}
                  className="group rounded-xl px-5 py-4 transition-all duration-200 animate-fade-in-up"
                  style={{
                    background: "var(--bg-raised)",
                    border: "1px solid var(--border-subtle)",
                    animationDelay: `${Math.min(i * 40, 400)}ms`,
                  }}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.background = "var(--bg-hover)";
                    e.currentTarget.style.borderColor = "var(--border-default)";
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.background = "var(--bg-raised)";
                    e.currentTarget.style.borderColor = "var(--border-subtle)";
                  }}
                >
                  <div className="flex items-start justify-between gap-4">
                    {/* Left: status + goal */}
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2.5 mb-1.5">
                        {/* Status badge */}
                        <span
                          className="inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-[11px] font-medium shrink-0"
                          style={{ background: sc.bg, color: sc.color }}
                        >
                          {sc.dot ? (
                            <span className="relative flex h-1.5 w-1.5">
                              <span
                                className="absolute inline-flex h-full w-full rounded-full opacity-75"
                                style={{
                                  background: sc.dot,
                                  animation: "pulse-ring 1.5s ease-out infinite",
                                }}
                              />
                              <span
                                className="relative inline-flex h-1.5 w-1.5 rounded-full"
                                style={{ background: sc.dot }}
                              />
                            </span>
                          ) : null}
                          {sc.label}
                        </span>
                      </div>
                      {/* Goal text */}
                      <p
                        className="text-[13px] leading-relaxed"
                        style={{ color: "var(--text-primary)" }}
                      >
                        {truncateGoal(task.goal)}
                      </p>
                    </div>

                    {/* Right: meta info */}
                    <div className="flex flex-col items-end gap-1 shrink-0 pt-0.5">
                      <span className="text-[12px]" style={{ color: "var(--text-tertiary)" }}>
                        {timeAgo(task.created_at)}
                      </span>
                      {task.total_duration_ms != null && (
                        <span className="text-[11px]" style={{ color: "var(--text-tertiary)" }}>
                          {formatDuration(task.total_duration_ms)}
                        </span>
                      )}
                    </div>
                  </div>
                </Link>
              );
            })}
          </div>
        )}
      </main>
    </div>
  );
}
