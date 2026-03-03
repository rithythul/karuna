"use client";

import { useState, useRef } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { useAuth } from "@/components/AuthProvider";

const EXAMPLES = [
  {
    title: "Competitive landscape report",
    prompt: "Research the top 5 competitors in the project management space. Compare their pricing, key features, funding, and recent news. Create a structured report with a comparison table and strategic recommendations.",
  },
  {
    title: "Build & deploy a landing page",
    prompt: "Build a modern, responsive landing page for a SaaS product called 'Beacon' — include a hero section with signup, feature highlights, pricing table, and testimonials. Deploy it and give me the live URL.",
  },
  {
    title: "Market data dashboard",
    prompt: "Fetch the latest cryptocurrency market data for the top 20 coins. Analyze 7-day price trends, trading volume, and market dominance. Generate charts and create an interactive HTML dashboard.",
  },
  {
    title: "Deep research with citations",
    prompt: "Research the current state of AI regulation across the US, EU, and China. Compare policy approaches, key legislation, enforcement mechanisms, and implications for startups. Produce a report with citations.",
  },
  {
    title: "Automate a data pipeline",
    prompt: "Write a Python pipeline that reads sales data from a CSV, cleans invalid records, computes monthly revenue trends and top products, generates visualizations, and exports everything as a formatted PDF report.",
  },
  {
    title: "Scrape, analyze & visualize",
    prompt: "Go to Product Hunt, extract the top 50 products launched this month with their names, descriptions, upvotes, and categories. Analyze category distribution and trends, then create a summary report with charts.",
  },
];

export default function Home() {
  const router = useRouter();
  const { user, loading, login, logout } = useAuth();
  const [goal, setGoal] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [attachments, setAttachments] = useState<File[]>([]);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const ACCEPTED_EXTENSIONS = ".csv,.json,.txt,.pdf,.xlsx,.py,.js,.ts,.md,.png,.jpg,.jpeg,.gif,.webp";
  const MAX_FILES = 5;
  const MAX_FILE_SIZE = 10 * 1024 * 1024; // 10 MB

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const selected = Array.from(e.target.files ?? []);
    const remaining = MAX_FILES - attachments.length;
    const toAdd = selected.slice(0, remaining).filter(
      (f) => f.size <= MAX_FILE_SIZE
    );
    setAttachments((prev) => [...prev, ...toAdd]);
    // Reset input so the same file can be re-selected
    if (fileInputRef.current) fileInputRef.current.value = "";
  };

  const removeAttachment = (index: number) => {
    setAttachments((prev) => prev.filter((_, i) => i !== index));
  };

  const submitGoal = async (text: string) => {
    const trimmed = text.trim();
    if (!trimmed || isLoading) return;

    setIsLoading(true);
    setError(null);

    try {
      let body: BodyInit;
      let headers: Record<string, string> = {};

      if (attachments.length > 0) {
        const formData = new FormData();
        formData.append("goal", trimmed);
        attachments.forEach((file) => formData.append("file", file, file.name));
        body = formData;
        // Don't set Content-Type for FormData — browser sets it with boundary
      } else {
        body = JSON.stringify({ goal: trimmed });
        headers["Content-Type"] = "application/json";
      }

      const res = await fetch("/api/tasks", {
        method: "POST",
        headers,
        body,
      });

      if (!res.ok) {
        const errText = await res.text();
        throw new Error(errText || `Request failed: ${res.status}`);
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

  // Loading state while checking auth
  if (loading) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <svg className="animate-spin" width="24" height="24" viewBox="0 0 24 24" fill="none">
          <circle cx="12" cy="12" r="10" stroke="var(--accent)" strokeWidth="2.5" strokeDasharray="31.416" strokeDashoffset="10" strokeLinecap="round"/>
        </svg>
      </div>
    );
  }

  // Not logged in — show login page
  if (!user) {
    return (
      <div className="flex min-h-screen flex-col items-center justify-center px-6">
        <div className="relative w-full max-w-[480px] flex flex-col items-center gap-8">
          <div className="text-center animate-fade-in-up">
            <h1
              className="text-[3.5rem] leading-[1.1] tracking-[-0.02em]"
              style={{ fontFamily: "var(--font-display)" }}
            >
              Welcome to
              <br />
              <span style={{ color: "var(--accent)" }}>Hanuman</span>
            </h1>
            <p className="mt-4 text-[14px]" style={{ color: "var(--text-tertiary)" }}>
              Sign in with your KOOMPI ID to get started
            </p>
          </div>

          <button
            onClick={login}
            className="flex items-center gap-3 rounded-xl px-6 py-3.5 text-[15px] font-medium transition-all duration-200 cursor-pointer animate-fade-in-up"
            style={{
              background: "var(--accent)",
              color: "var(--text-on-accent)",
              animationDelay: "100ms",
            }}
          >
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M15.75 6a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0zM4.501 20.118a7.5 7.5 0 0114.998 0A17.933 17.933 0 0112 21.75c-2.676 0-5.216-.584-7.499-1.632z" />
            </svg>
            Sign in with KOOMPI KID
          </button>
        </div>

        <div className="fixed bottom-0 left-0 right-0 flex justify-center py-5 text-[12px]"
          style={{ color: "var(--text-tertiary)" }}>
          Hanuman
        </div>
      </div>
    );
  }

  // Logged in — show main app
  return (
    <div className="flex min-h-screen flex-col items-center justify-center px-6">
      {/* User menu — top right */}
      <div className="fixed top-4 right-4 z-50 flex items-center gap-3 animate-fade-in-up">
        <Link
          href="/tasks"
          className="rounded-lg px-3 py-1.5 text-[12px] font-medium transition-colors"
          style={{
            color: "var(--text-secondary)",
          }}
          onMouseEnter={(e) => (e.currentTarget.style.color = "var(--accent-dim)")}
          onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-secondary)")}
        >
          Task History
        </Link>
        <Link
          href="/memory"
          className="rounded-lg px-3 py-1.5 text-[12px] font-medium transition-colors"
          style={{ color: "var(--text-secondary)" }}
          onMouseEnter={(e) => (e.currentTarget.style.color = "var(--accent-dim)")}
          onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-secondary)")}
        >
          Memory
        </Link>
        <span className="text-[13px]" style={{ color: "var(--text-secondary)" }}>
          {user.full_name}
        </span>
        <button
          onClick={logout}
          className="rounded-lg px-3 py-1.5 text-[12px] font-medium transition-colors cursor-pointer"
          style={{
            background: "var(--bg-raised)",
            color: "var(--text-tertiary)",
            border: "1px solid var(--border-subtle)",
          }}
        >
          Sign out
        </button>
      </div>

      <div className="relative w-full max-w-[720px] flex flex-col items-center gap-10">
        {/* Branding */}
        <div className="text-center animate-fade-in-up">
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
          <div
            className="relative rounded-2xl transition-all duration-300"
            style={{
              background: "var(--bg-raised)",
              boxShadow: "0 1px 3px rgba(0,0,0,0.06), 0 1px 2px rgba(0,0,0,0.04)",
            }}
          >
            <textarea
              ref={textareaRef}
              value={goal}
              onChange={(e) => setGoal(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Assign a task or ask anything"
              rows={3}
              disabled={isLoading}
              className="w-full bg-transparent px-5 pt-5 pb-14 text-[15px] leading-relaxed resize-none placeholder:text-[var(--text-tertiary)] focus:outline-none disabled:opacity-50"
              style={{ color: "var(--text-primary)" }}
            />
            {/* File attachments */}
            <div className="mt-2 px-5 pb-2">
              {/* Hidden file input */}
              <input
                ref={fileInputRef}
                type="file"
                multiple
                accept={ACCEPTED_EXTENSIONS}
                onChange={handleFileChange}
                className="hidden"
              />

              {/* Attached file chips */}
              {attachments.length > 0 && (
                <div className="flex flex-wrap gap-1.5 mb-2">
                  {attachments.map((file, i) => (
                    <div
                      key={i}
                      className="flex items-center gap-1.5 rounded-lg px-2.5 py-1 text-[11px]"
                      style={{
                        background: "var(--bg-elevated)",
                        border: "1px solid var(--border-subtle)",
                        color: "var(--text-secondary)",
                      }}
                    >
                      <span className="max-w-[120px] truncate font-mono">{file.name}</span>
                      <span style={{ color: "var(--text-tertiary)" }}>
                        {(file.size / 1024).toFixed(0)}KB
                      </span>
                      <button
                        type="button"
                        onClick={() => removeAttachment(i)}
                        className="cursor-pointer transition-colors"
                        style={{ color: "var(--text-tertiary)" }}
                        onMouseEnter={(e) => (e.currentTarget.style.color = "var(--status-error)")}
                        onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-tertiary)")}
                      >
                        ×
                      </button>
                    </div>
                  ))}
                </div>
              )}

              {/* Paperclip button — only show if under the limit */}
              {attachments.length < MAX_FILES && (
                <button
                  type="button"
                  onClick={() => fileInputRef.current?.click()}
                  className="flex items-center gap-1.5 text-[12px] transition-colors cursor-pointer"
                  style={{ color: "var(--text-tertiary)" }}
                  onMouseEnter={(e) => (e.currentTarget.style.color = "var(--text-secondary)")}
                  onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-tertiary)")}
                >
                  <svg width="14" height="14" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M18.375 12.739l-7.693 7.693a4.5 4.5 0 01-6.364-6.364l10.94-10.94A3 3 0 1119.5 7.372L8.552 18.32m.009-.01l-.01.01m5.699-9.941l-7.81 7.81a1.5 1.5 0 002.112 2.13" />
                  </svg>
                  Attach file{attachments.length > 0 ? ` (${attachments.length}/${MAX_FILES})` : ""}
                </button>
              )}
            </div>

            <div className="absolute bottom-0 left-0 right-0 flex items-center justify-end px-4 pb-3.5">
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
          <p className="mb-3 text-center text-[12px]" style={{ color: "var(--text-tertiary)" }}>Try something</p>
          <div className="grid grid-cols-2 sm:grid-cols-3 gap-2">
            {EXAMPLES.map((ex) => (
              <button
                key={ex.title}
                onClick={() => {
                  setGoal(ex.prompt);
                  textareaRef.current?.focus();
                }}
                disabled={isLoading}
                className="rounded-lg px-3.5 py-2.5 text-[13px] text-left transition-all duration-200 cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
                style={{
                  color: "var(--text-secondary)",
                  background: "var(--bg-elevated)",
                }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.color = "var(--accent-dim)";
                  e.currentTarget.style.background = "var(--bg-hover)";
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.color = "var(--text-secondary)";
                  e.currentTarget.style.background = "var(--bg-elevated)";
                }}
              >
                {ex.title}
              </button>
            ))}
          </div>
        </div>

        {/* Error */}
        {error && (
          <div className="w-full rounded-xl px-4 py-3 text-[13px]"
            style={{ background: "var(--status-error-dim)", border: "1px solid rgba(220,38,38,0.2)", color: "var(--status-error)" }}>
            {error}
          </div>
        )}
      </div>

      <div className="fixed bottom-0 left-0 right-0 flex justify-center py-5 text-[12px]"
        style={{ color: "var(--text-tertiary)" }}>
        Hanuman
      </div>
    </div>
  );
}
