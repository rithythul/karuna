"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";

const EXAMPLES = [
  {
    icon: "🖥️",
    title: "Build a web app",
    prompt: "Build a Python Flask todo list web application with a SQLite database, REST API, and simple HTML frontend",
  },
  {
    icon: "🔬",
    title: "Research & report",
    prompt: "Research the current state of quantum computing, including breakthroughs, key players, and practical applications. Create a comprehensive report.",
  },
  {
    icon: "📊",
    title: "Analyze data",
    prompt: "Create a dataset of 500 synthetic sales records, analyze trends by month and category, and generate chart visualizations",
  },
  {
    icon: "🌐",
    title: "Browse & extract",
    prompt: "Go to news.ycombinator.com, extract the top 10 stories with titles, points, and URLs, and save as JSON",
  },
  {
    icon: "⚡",
    title: "Full-stack project",
    prompt: "Create a React + Express weather dashboard that fetches from a public API and displays current conditions with charts",
  },
  {
    icon: "🤖",
    title: "Automate workflow",
    prompt: "Write a Python script that monitors a directory for CSV files, validates data, generates statistics, and creates a summary report",
  },
];

export default function Home() {
  const router = useRouter();
  const [goal, setGoal] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submitGoal = async (text: string) => {
    const trimmed = text.trim();
    if (!trimmed || isLoading) return;

    setIsLoading(true);
    setError(null);

    try {
      const res = await fetch("/api/tasks", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ goal: trimmed }),
      });

      if (!res.ok) {
        const body = await res.text();
        throw new Error(body || `Request failed: ${res.status}`);
      }

      const data = await res.json();
      router.push(`/tasks/${data.task_id}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Something went wrong");
      setIsLoading(false);
    }
  };

  const handleSubmit = (e?: React.FormEvent) => {
    e?.preventDefault();
    submitGoal(goal);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  return (
    <div className="flex min-h-screen flex-col items-center justify-center px-6">
      <div
        className="pointer-events-none fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2"
        style={{
          width: "800px",
          height: "600px",
          background: "radial-gradient(ellipse, rgba(212,160,74,0.04) 0%, transparent 70%)",
        }}
      />

      <div className="relative w-full max-w-[720px] flex flex-col items-center gap-10">
        {/* Branding */}
        <div className="text-center animate-fade-in-up">
          <div className="mb-4 inline-flex items-center gap-2 rounded-full px-4 py-1.5 text-sm"
            style={{ background: "var(--bg-raised)", border: "1px solid var(--border-subtle)", color: "var(--text-secondary)" }}>
            <span className="relative flex h-2 w-2">
              <span className="absolute inline-flex h-full w-full animate-ping rounded-full opacity-75" style={{ background: "var(--accent)" }}></span>
              <span className="relative inline-flex h-2 w-2 rounded-full" style={{ background: "var(--accent)" }}></span>
            </span>
            Autonomous AI Agent
          </div>
          <h1
            className="text-[3.5rem] leading-[1.1] tracking-[-0.02em]"
            style={{ fontFamily: "var(--font-display)" }}
          >
            What can I do
            <br />
            <span style={{ color: "var(--accent)" }}>for you?</span>
          </h1>
        </div>

        {/* Input area */}
        <form onSubmit={handleSubmit} className="w-full animate-fade-in-up" style={{ animationDelay: "100ms" }}>
          <div className="relative rounded-2xl transition-all duration-300"
            style={{ background: "var(--bg-raised)", border: "1px solid var(--border-subtle)" }}>
            <textarea
              value={goal}
              onChange={(e) => setGoal(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Describe your task..."
              rows={3}
              disabled={isLoading}
              className="w-full bg-transparent px-5 pt-5 pb-14 text-[15px] leading-relaxed resize-none placeholder:text-[var(--text-tertiary)] focus:outline-none disabled:opacity-50"
              style={{ color: "var(--text-primary)" }}
            />
            <div className="absolute bottom-0 left-0 right-0 flex items-center justify-between px-4 pb-3.5">
              <div className="flex items-center gap-1">
                {["Research", "Code", "Browse", "Files", "Data", "Shell", "Deploy"].map((s) => (
                  <span key={s} className="rounded-full px-2 py-0.5 text-[10px]"
                    style={{ background: "var(--bg-elevated)", color: "var(--text-tertiary)", border: "1px solid var(--border-subtle)" }}>
                    {s}
                  </span>
                ))}
              </div>
              <button
                type="submit"
                disabled={!goal.trim() || isLoading}
                className="flex items-center justify-center w-9 h-9 rounded-xl transition-all duration-200 cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed"
                style={{
                  background: goal.trim() ? "var(--accent)" : "var(--bg-elevated)",
                  color: goal.trim() ? "var(--text-on-accent)" : "var(--text-tertiary)",
                }}
              >
                {isLoading ? (
                  <svg className="animate-spin" width="18" height="18" viewBox="0 0 24 24" fill="none">
                    <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="2.5" strokeDasharray="31.416" strokeDashoffset="10" strokeLinecap="round"/>
                  </svg>
                ) : (
                  <svg width="18" height="18" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12h15m0 0l-6.75-6.75M19.5 12l-6.75 6.75" />
                  </svg>
                )}
              </button>
            </div>
          </div>
        </form>

        {/* Example prompts */}
        <div className="w-full animate-fade-in-up" style={{ animationDelay: "200ms" }}>
          <p className="mb-3 text-center text-[12px]" style={{ color: "var(--text-tertiary)" }}>Try an example</p>
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2.5">
            {EXAMPLES.map((ex) => (
              <button
                key={ex.title}
                onClick={() => submitGoal(ex.prompt)}
                disabled={isLoading}
                className="group text-left rounded-xl p-3.5 transition-all duration-200 cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
                style={{ background: "var(--bg-raised)", border: "1px solid var(--border-subtle)" }}
                onMouseEnter={(e) => { e.currentTarget.style.borderColor = "var(--border-accent)"; }}
                onMouseLeave={(e) => { e.currentTarget.style.borderColor = "var(--border-subtle)"; }}
              >
                <div className="flex items-center gap-2 mb-1.5">
                  <span className="text-base">{ex.icon}</span>
                  <span className="text-[13px] font-medium" style={{ color: "var(--text-primary)" }}>{ex.title}</span>
                </div>
                <p className="text-[11px] line-clamp-2" style={{ color: "var(--text-tertiary)" }}>{ex.prompt}</p>
              </button>
            ))}
          </div>
        </div>

        {/* Error */}
        {error && (
          <div className="w-full rounded-xl px-4 py-3 text-[13px]"
            style={{ background: "var(--status-error-dim)", border: "1px solid rgba(248,113,113,0.2)", color: "var(--status-error)" }}>
            {error}
          </div>
        )}
      </div>

      <div className="fixed bottom-0 left-0 right-0 flex justify-center py-5 text-[12px]"
        style={{ color: "var(--text-tertiary)" }}>
        Karuna &middot; Autonomous AI Agent &middot; 7 Skills
      </div>
    </div>
  );
}
