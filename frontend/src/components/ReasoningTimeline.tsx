"use client";

import { useEffect, useState } from "react";

interface ToolCallFunction {
  name: string;
  arguments: string;
}

interface ToolCall {
  id: string;
  type: string;
  function: ToolCallFunction;
}

interface ReasoningTrace {
  id: string;
  task_id: string;
  step_id: string;
  agent_name: string;
  turn: number;
  role: string;
  content: string | null;
  tool_calls: ToolCall[] | null;
  created_at: string;
}

interface ReasoningTimelineProps {
  taskId: string;
  stepId: string;
}

function truncate(text: string, max: number): { text: string; truncated: boolean } {
  if (text.length <= max) return { text, truncated: false };
  return { text: text.slice(0, max) + "...", truncated: true };
}

function summarizeArgs(argsJson: string): string {
  try {
    const parsed = JSON.parse(argsJson);
    const keys = Object.keys(parsed);
    if (keys.length === 0) return "()";
    const parts = keys.slice(0, 3).map((k) => {
      const v = parsed[k];
      const val = typeof v === "string" ? (v.length > 40 ? v.slice(0, 40) + "..." : v) : JSON.stringify(v);
      return `${k}: ${val}`;
    });
    if (keys.length > 3) parts.push(`+${keys.length - 3} more`);
    return parts.join(", ");
  } catch {
    return argsJson.length > 60 ? argsJson.slice(0, 60) + "..." : argsJson;
  }
}

function AssistantThought({ content }: { content: string }) {
  const [expanded, setExpanded] = useState(false);
  const { text: short, truncated } = truncate(content, 200);

  return (
    <div className="flex gap-2">
      <div
        className="flex-shrink-0 mt-0.5 w-1 rounded-full"
        style={{ background: "var(--accent)" }}
      />
      <div className="min-w-0 flex-1">
        <span className="text-[10px] font-semibold uppercase" style={{ color: "var(--accent)" }}>
          Thought
        </span>
        <p className="text-[11px] mt-0.5 whitespace-pre-wrap" style={{ color: "var(--text-secondary)" }}>
          {expanded ? content : short}
        </p>
        {truncated && (
          <button
            onClick={() => setExpanded(!expanded)}
            className="text-[10px] mt-0.5 cursor-pointer border-none bg-transparent p-0"
            style={{ color: "var(--accent)" }}
          >
            {expanded ? "Show less" : "Show more"}
          </button>
        )}
      </div>
    </div>
  );
}

function ToolCallEntry({ toolCall }: { toolCall: ToolCall }) {
  return (
    <div className="flex gap-2">
      <div
        className="flex-shrink-0 mt-0.5 w-1 rounded-full"
        style={{ background: "var(--status-info, var(--accent))" }}
      />
      <div className="min-w-0 flex-1">
        <span className="text-[10px] font-semibold uppercase" style={{ color: "var(--status-info, var(--accent))" }}>
          Tool Call
        </span>
        <div className="flex items-center gap-1.5 mt-0.5">
          <span
            className="text-[11px] font-mono font-medium rounded px-1 py-0.5"
            style={{
              background: "color-mix(in srgb, var(--accent) 10%, transparent)",
              color: "var(--text-primary)",
            }}
          >
            {toolCall.function.name}
          </span>
        </div>
        <p className="text-[10px] font-mono mt-0.5 break-all" style={{ color: "var(--text-tertiary)" }}>
          {summarizeArgs(toolCall.function.arguments)}
        </p>
      </div>
    </div>
  );
}

function ToolResult({ content }: { content: string }) {
  const [expanded, setExpanded] = useState(false);
  const { text: short, truncated } = truncate(content, 150);

  return (
    <div className="flex gap-2">
      <div
        className="flex-shrink-0 mt-0.5 w-1 rounded-full"
        style={{ background: "var(--text-tertiary)" }}
      />
      <div className="min-w-0 flex-1">
        <span className="text-[10px] font-semibold uppercase" style={{ color: "var(--text-tertiary)" }}>
          Result
        </span>
        <pre
          className="text-[10px] font-mono mt-0.5 whitespace-pre-wrap break-all rounded p-1.5"
          style={{
            color: "var(--text-secondary)",
            background: "color-mix(in srgb, var(--bg-elevated, var(--bg-hover)) 60%, transparent)",
          }}
        >
          {expanded ? content : short}
        </pre>
        {truncated && (
          <button
            onClick={() => setExpanded(!expanded)}
            className="text-[10px] mt-0.5 cursor-pointer border-none bg-transparent p-0"
            style={{ color: "var(--accent)" }}
          >
            {expanded ? "Show less" : "Show more"}
          </button>
        )}
      </div>
    </div>
  );
}

function TraceEntry({ trace }: { trace: ReasoningTrace }) {
  if (trace.role === "assistant" && trace.content) {
    return <AssistantThought content={trace.content} />;
  }

  if (trace.role === "assistant" && trace.tool_calls) {
    return (
      <>
        {trace.tool_calls.map((tc) => (
          <ToolCallEntry key={tc.id} toolCall={tc} />
        ))}
      </>
    );
  }

  if (trace.role === "tool" && trace.content) {
    return <ToolResult content={trace.content} />;
  }

  return null;
}

export default function ReasoningTimeline({ taskId, stepId }: ReasoningTimelineProps) {
  const [traces, setTraces] = useState<ReasoningTrace[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function fetchTraces() {
      setLoading(true);
      setError(null);
      try {
        const res = await fetch(`/api/tasks/${taskId}/steps/${stepId}/reasoning`);
        if (!res.ok) {
          throw new Error(`${res.status} ${res.statusText}`);
        }
        const data: ReasoningTrace[] = await res.json();
        if (!cancelled) {
          setTraces(data);
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : "Failed to load reasoning");
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    fetchTraces();
    return () => { cancelled = true; };
  }, [taskId, stepId]);

  if (loading) {
    return (
      <div className="flex items-center gap-2 py-2 pl-4">
        <div
          className="w-3 h-3 rounded-full animate-pulse"
          style={{ background: "var(--accent)" }}
        />
        <span className="text-[10px]" style={{ color: "var(--text-tertiary)" }}>
          Loading reasoning...
        </span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="py-2 pl-4">
        <span className="text-[10px]" style={{ color: "var(--status-error)" }}>
          Failed to load: {error}
        </span>
      </div>
    );
  }

  if (traces.length === 0) {
    return (
      <div className="py-2 pl-4">
        <span className="text-[10px] italic" style={{ color: "var(--text-tertiary)" }}>
          No reasoning traces available
        </span>
      </div>
    );
  }

  // Group traces by turn number
  const turns = new Map<number, ReasoningTrace[]>();
  for (const trace of traces) {
    const group = turns.get(trace.turn) ?? [];
    group.push(trace);
    turns.set(trace.turn, group);
  }

  return (
    <div
      className="mt-2 ml-4 rounded-lg p-2 flex flex-col gap-2"
      style={{
        background: "color-mix(in srgb, var(--bg-hover) 50%, transparent)",
        borderLeft: "2px solid var(--border-subtle)",
      }}
    >
      <span className="text-[10px] font-semibold uppercase tracking-wider" style={{ color: "var(--text-tertiary)" }}>
        Reasoning ({traces.length} traces)
      </span>

      {Array.from(turns.entries()).map(([turn, turnTraces]) => (
        <div key={turn} className="flex flex-col gap-1.5">
          {turns.size > 1 && (
            <span className="text-[9px] font-mono uppercase" style={{ color: "var(--text-tertiary)" }}>
              Turn {turn + 1}
            </span>
          )}
          {turnTraces.map((trace) => (
            <TraceEntry key={trace.id} trace={trace} />
          ))}
        </div>
      ))}
    </div>
  );
}
