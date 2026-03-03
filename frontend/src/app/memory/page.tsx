"use client";

import { useEffect, useState, useCallback } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { useAuth } from "@/components/AuthProvider";

interface Memory {
  id: string;
  category: "preference" | "fact" | "learning";
  key: string;
  value: unknown;
  access_count: number;
  created_at: string;
  updated_at: string;
}

interface GroupedMemories {
  preference: Memory[];
  fact: Memory[];
  learning: Memory[];
}

const CATEGORY_CONFIG: Record<
  string,
  { label: string; description: string; color: string }
> = {
  preference: {
    label: "Preferences",
    description: "how you like things done",
    color: "var(--accent)",
  },
  fact: {
    label: "Facts",
    description: "things known about your domain",
    color: "var(--status-info)",
  },
  learning: {
    label: "Learnings",
    description: "techniques that worked well",
    color: "var(--status-success)",
  },
};

function formatValue(value: unknown): string {
  if (typeof value === "string") return value;
  return JSON.stringify(value, null, 2);
}

function SpinnerIcon() {
  return (
    <svg className="animate-spin" width="24" height="24" viewBox="0 0 24 24" fill="none">
      <circle
        cx="12" cy="12" r="10"
        stroke="var(--accent)" strokeWidth="2.5"
        strokeDasharray="31.416" strokeDashoffset="10" strokeLinecap="round"
      />
    </svg>
  );
}

export default function MemoryPage() {
  const router = useRouter();
  const { user, loading: authLoading } = useAuth();
  const [grouped, setGrouped] = useState<GroupedMemories>({
    preference: [],
    fact: [],
    learning: [],
  });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.replace("/");
  }, [authLoading, user, router]);

  const fetchMemories = useCallback(() => {
    if (!user) return;
    setLoading(true);
    setError(null);
    fetch("/api/memory")
      .then((r) => {
        if (!r.ok) throw new Error(`Failed to load memories: ${r.status}`);
        return r.json();
      })
      .then((data) => {
        const all: Memory[] = data.memories ?? [];
        setGrouped({
          preference: all.filter((m) => m.category === "preference"),
          fact: all.filter((m) => m.category === "fact"),
          learning: all.filter((m) => m.category === "learning"),
        });
      })
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load"))
      .finally(() => setLoading(false));
  }, [user]);

  useEffect(() => {
    fetchMemories();
  }, [fetchMemories]);

  const handleDelete = async (category: string, key: string) => {
    const lockId = `${category}/${key}`;
    setDeleting(lockId);
    try {
      const r = await fetch(
        `/api/memory/${encodeURIComponent(category)}/${encodeURIComponent(key)}`,
        { method: "DELETE" }
      );
      if (!r.ok) throw new Error(`Delete failed: ${r.status}`);
      fetchMemories();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Delete failed");
    } finally {
      setDeleting(null);
    }
  };

  const totalCount =
    grouped.preference.length + grouped.fact.length + grouped.learning.length;

  if (authLoading || (!user && !authLoading)) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <SpinnerIcon />
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
        <div className="flex items-center gap-3">
          <h1
            className="text-[20px] tracking-[-0.01em]"
            style={{ fontFamily: "var(--font-display)", color: "var(--text-primary)" }}
          >
            Memory
          </h1>
          {!loading && (
            <span
              className="text-[12px] rounded-full px-2.5 py-0.5"
              style={{
                background: "var(--bg-elevated)",
                color: "var(--text-tertiary)",
              }}
            >
              {totalCount}
            </span>
          )}
        </div>

        <div className="flex items-center gap-4">
          <Link
            href="/tasks"
            className="text-[12px] font-medium transition-colors"
            style={{ color: "var(--text-secondary)" }}
          >
            Task History
          </Link>
          <Link
            href="/"
            className="flex items-center gap-2 rounded-xl px-4 py-2 text-[13px] font-medium transition-all duration-200"
            style={{ background: "var(--accent)", color: "var(--text-on-accent)" }}
          >
            <svg width="14" height="14" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
            </svg>
            New Task
          </Link>
        </div>
      </header>

      {/* Content */}
      <main className="mx-auto max-w-[800px] px-6 py-8">
        {loading ? (
          <div className="flex flex-col items-center justify-center py-24 animate-fade-in-up">
            <SpinnerIcon />
          </div>
        ) : error ? (
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
        ) : totalCount === 0 ? (
          <div className="flex flex-col items-center justify-center py-24 animate-fade-in-up text-center">
            <svg
              width="48" height="48" fill="none" viewBox="0 0 24 24"
              stroke="var(--text-tertiary)" strokeWidth={1}
            >
              <path
                strokeLinecap="round" strokeLinejoin="round"
                d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09z"
              />
            </svg>
            <p
              className="mt-4 text-[15px] font-medium"
              style={{ color: "var(--text-secondary)" }}
            >
              No memories yet
            </p>
            <p
              className="mt-2 text-[13px] max-w-[320px]"
              style={{ color: "var(--text-tertiary)" }}
            >
              Hanuman learns from your tasks automatically — your preferences, domain
              facts, and effective techniques. Complete a few tasks and they&apos;ll
              appear here.
            </p>
            <Link
              href="/"
              className="mt-6 rounded-xl px-5 py-2.5 text-[13px] font-medium"
              style={{ background: "var(--accent)", color: "var(--text-on-accent)" }}
            >
              Start a task
            </Link>
          </div>
        ) : (
          <div className="flex flex-col gap-8">
            {(["preference", "fact", "learning"] as const).map((category) => {
              const items = grouped[category];
              if (items.length === 0) return null;
              const cfg = CATEGORY_CONFIG[category];
              return (
                <section key={category} className="animate-fade-in-up">
                  <div className="flex items-center gap-2.5 mb-3">
                    <h2
                      className="text-[15px] font-semibold"
                      style={{ color: "var(--text-primary)" }}
                    >
                      {cfg.label}
                    </h2>
                    <span
                      className="text-[11px] rounded-full px-2 py-0.5 font-medium"
                      style={{
                        background: `color-mix(in srgb, ${cfg.color} 15%, transparent)`,
                        color: cfg.color,
                      }}
                    >
                      {items.length}
                    </span>
                    <span
                      className="text-[12px]"
                      style={{ color: "var(--text-tertiary)" }}
                    >
                      — {cfg.description}
                    </span>
                  </div>

                  <div className="flex flex-col gap-2">
                    {items.map((memory) => {
                      const lockId = `${memory.category}/${memory.key}`;
                      const isDeleting = deleting === lockId;
                      return (
                        <div
                          key={memory.id}
                          className="rounded-xl px-4 py-3"
                          style={{
                            background: "var(--bg-raised)",
                            border: "1px solid var(--border-subtle)",
                            opacity: isDeleting ? 0.5 : 1,
                            transition: "opacity 0.15s",
                          }}
                        >
                          <div className="flex items-start justify-between gap-3">
                            <div className="flex-1 min-w-0">
                              <div className="flex items-center gap-2 mb-1">
                                <span
                                  className="text-[12px] font-semibold font-mono"
                                  style={{ color: cfg.color }}
                                >
                                  {memory.key}
                                </span>
                                {memory.access_count > 0 && (
                                  <span
                                    className="text-[10px]"
                                    style={{ color: "var(--text-tertiary)" }}
                                  >
                                    used {memory.access_count}×
                                  </span>
                                )}
                              </div>
                              <p
                                className="text-[13px] leading-relaxed"
                                style={{ color: "var(--text-primary)" }}
                              >
                                {formatValue(memory.value)}
                              </p>
                            </div>
                            <button
                              onClick={() =>
                                !isDeleting && handleDelete(memory.category, memory.key)
                              }
                              disabled={isDeleting}
                              className="flex-shrink-0 rounded-lg px-2.5 py-1.5 text-[11px] font-medium transition-colors cursor-pointer disabled:cursor-not-allowed"
                              style={{
                                color: "var(--text-tertiary)",
                                background: "var(--bg-elevated)",
                              }}
                              onMouseEnter={(e) =>
                                (e.currentTarget.style.color = "var(--status-error)")
                              }
                              onMouseLeave={(e) =>
                                (e.currentTarget.style.color = "var(--text-tertiary)")
                              }
                            >
                              {isDeleting ? "…" : "Remove"}
                            </button>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </section>
              );
            })}
          </div>
        )}
      </main>
    </div>
  );
}
