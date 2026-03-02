"use client";

import { useState } from "react";
import ReasoningTimeline from "./ReasoningTimeline";

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

interface StepsSidebarProps {
  steps: TaskStep[];
  currentStep: number | null;
  taskId: string;
}

function skillIcon(skill: string): string {
  switch (skill) {
    case "browser": return "\u{1F310}";
    case "code": return "\u{1F4BB}";
    case "research": return "\u{1F52C}";
    case "api": return "\u{1F50C}";
    case "data_analysis": return "\u{1F4CA}";
    case "deploy": return "\u{1F680}";
    // Legacy fallbacks
    case "browse": return "\u{1F310}";
    case "file": return "\u{1F4C1}";
    case "shell": return "\u2699\uFE0F";
    default: return "\u{1F527}";
  }
}

function statusIndicator(status: string, isCurrent: boolean) {
  const s = status.toLowerCase();

  if (s === "completed") {
    return (
      <div className="flex items-center justify-center w-6 h-6 rounded-full" style={{ background: "color-mix(in srgb, var(--status-success) 20%, transparent)" }}>
        <svg className="w-3.5 h-3.5" style={{ color: "var(--status-success)" }} fill="none" viewBox="0 0 24 24" strokeWidth={2.5} stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12.75l6 6 9-13.5" />
        </svg>
      </div>
    );
  }

  if (s === "failed") {
    return (
      <div className="flex items-center justify-center w-6 h-6 rounded-full" style={{ background: "color-mix(in srgb, var(--status-error) 20%, transparent)" }}>
        <svg className="w-3.5 h-3.5" style={{ color: "var(--status-error)" }} fill="none" viewBox="0 0 24 24" strokeWidth={2.5} stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </div>
    );
  }

  if (s === "running" || isCurrent) {
    return (
      <div className="flex items-center justify-center w-6 h-6">
        <span className="relative flex h-3 w-3">
          <span className="absolute inline-flex h-full w-full animate-ping rounded-full opacity-75" style={{ background: "var(--accent)" }}></span>
          <span className="relative inline-flex h-3 w-3 rounded-full" style={{ background: "var(--accent)" }}></span>
        </span>
      </div>
    );
  }

  return (
    <div className="flex items-center justify-center w-6 h-6">
      <div className="w-2.5 h-2.5 rounded-full" style={{ border: "2px solid var(--border-strong)" }} />
    </div>
  );
}

function canExpand(status: string): boolean {
  const s = status.toLowerCase();
  return s === "completed" || s === "failed";
}

export type { TaskStep };

export default function StepsSidebar({ steps, currentStep, taskId }: StepsSidebarProps) {
  const [expandedStepId, setExpandedStepId] = useState<string | null>(null);

  if (steps.length === 0) return null;

  function toggleStep(stepId: string) {
    setExpandedStepId((prev) => (prev === stepId ? null : stepId));
  }

  return (
    <div className="rounded-xl p-4" style={{ background: "var(--bg-raised)", border: "1px solid var(--border-subtle)" }}>
      <h2 className="mb-3 text-[12px] font-semibold uppercase tracking-wider" style={{ color: "var(--text-secondary)" }}>
        Execution Plan ({steps.length} steps)
      </h2>
      <div className="relative flex flex-col gap-1">
        {/* Connector line */}
        <div
          className="absolute left-[11px] top-4 bottom-4 w-px"
          style={{ background: "var(--border-subtle)" }}
        />

        {steps.map((step, i) => {
          const isCurrent = currentStep === i + 1;
          const isExpanded = expandedStepId === step.id;
          const expandable = canExpand(step.status);
          return (
            <div key={step.id}>
              <div
                className="relative flex items-start gap-3 rounded-lg px-2 py-2 transition-all"
                style={{
                  background: isCurrent ? "var(--bg-hover)" : "transparent",
                }}
              >
                <div className="relative z-10 flex-shrink-0 mt-0.5">
                  {statusIndicator(step.status, isCurrent)}
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="text-[12px]">{skillIcon(step.skill)}</span>
                    <span className="text-[12px] font-medium" style={{
                      color: isCurrent ? "var(--text-primary)" : "var(--text-secondary)",
                    }}>
                      {step.skill}
                    </span>
                    <span className="text-[10px] font-mono" style={{ color: "var(--text-tertiary)" }}>
                      {i + 1}/{steps.length}
                    </span>
                    {step.retry_count > 0 && (
                      <span className="text-[10px] rounded-full px-1.5 py-0.5"
                        style={{ background: "color-mix(in srgb, var(--accent) 15%, transparent)", color: "var(--accent)" }}>
                        {step.retry_count} retries
                      </span>
                    )}
                    {expandable && (
                      <button
                        onClick={() => toggleStep(step.id)}
                        className="ml-auto flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] cursor-pointer border-none transition-colors"
                        style={{
                          background: isExpanded
                            ? "color-mix(in srgb, var(--accent) 15%, transparent)"
                            : "transparent",
                          color: "var(--accent)",
                        }}
                        title={isExpanded ? "Hide reasoning" : "Show reasoning"}
                      >
                        <svg
                          className="w-3 h-3 transition-transform"
                          style={{
                            transform: isExpanded ? "rotate(90deg)" : "rotate(0deg)",
                          }}
                          fill="none"
                          viewBox="0 0 24 24"
                          strokeWidth={2}
                          stroke="currentColor"
                        >
                          <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" />
                        </svg>
                        <span>{isExpanded ? "Hide" : "Trace"}</span>
                      </button>
                    )}
                  </div>
                  <p className="text-[11px] mt-0.5 line-clamp-2" style={{ color: "var(--text-tertiary)" }}>
                    {step.description}
                  </p>
                  {step.error && (
                    <p className="text-[10px] mt-1 line-clamp-1" style={{ color: "var(--status-error)" }}>
                      {step.error}
                    </p>
                  )}
                  {step.reflection && (
                    <p className="text-[10px] mt-1 italic line-clamp-2" style={{ color: "var(--accent)" }}>
                      {"\u{1F4AD}"} {step.reflection}
                    </p>
                  )}
                </div>
              </div>
              {isExpanded && (
                <ReasoningTimeline taskId={taskId} stepId={step.id} />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
