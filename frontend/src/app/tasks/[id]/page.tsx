"use client";

import { useEffect, useState, useRef, useCallback } from "react";
import { useParams } from "next/navigation";
import Link from "next/link";
import TaskTimeline, { type TaskEvent } from "@/components/TaskTimeline";

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

type TaskStatus = "connecting" | "live" | "completed" | "failed" | "disconnected";

export default function TaskPage() {
  const { id } = useParams<{ id: string }>();
  const [events, setEvents] = useState<TaskEvent[]>([]);
  const [task, setTask] = useState<TaskData | null>(null);
  const [steps, setSteps] = useState<TaskStep[]>([]);
  const [status, setStatus] = useState<TaskStatus>("connecting");
  const terminalRef = useRef(false);
  const wsRef = useRef<WebSocket | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  // Fetch initial task data
  useEffect(() => {
    if (!id) return;
    fetch(`/api/tasks/${id}`)
      .then((res) => res.json())
      .then((data) => {
        if (data.task) setTask(data.task);
        if (data.steps) setSteps(data.steps);
        // If task is already terminal, mark status
        if (data.task?.status === "Completed" || data.task?.status === "completed") {
          terminalRef.current = true;
          setStatus("completed");
        } else if (data.task?.status === "Failed" || data.task?.status === "failed") {
          terminalRef.current = true;
          setStatus("failed");
        }
      })
      .catch(() => {});

    // Also fetch historical events
    fetch(`/api/tasks/${id}/events`)
      .then((res) => res.json())
      .then((data: TaskEvent[]) => {
        if (Array.isArray(data) && data.length > 0) {
          setEvents(data);
        }
      })
      .catch(() => {});
  }, [id]);

  // WebSocket connection
  useEffect(() => {
    if (!id || terminalRef.current) return;

    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const wsUrl = `${protocol}//localhost:8000/ws/tasks/${id}`;
    const ws = new WebSocket(wsUrl);
    wsRef.current = ws;

    ws.onopen = () => {
      setStatus("live");
    };

    ws.onmessage = (msg) => {
      try {
        const event: TaskEvent = JSON.parse(msg.data);
        setEvents((prev) => [...prev, event]);

        if (event.event_type === "task_completed") {
          terminalRef.current = true;
          setStatus("completed");
          fetch(`/api/tasks/${id}`)
            .then((res) => res.json())
            .then((data) => {
              if (data.task) setTask(data.task);
              if (data.steps) setSteps(data.steps);
            })
            .catch(() => {});
        } else if (event.event_type === "task_failed") {
          terminalRef.current = true;
          setStatus("failed");
          fetch(`/api/tasks/${id}`)
            .then((res) => res.json())
            .then((data) => {
              if (data.task) setTask(data.task);
              if (data.steps) setSteps(data.steps);
            })
            .catch(() => {});
        }
      } catch {
        // Ignore non-JSON messages
      }
    };

    ws.onclose = () => {
      if (!terminalRef.current) {
        setStatus("disconnected");
      }
    };

    ws.onerror = () => {
      if (!terminalRef.current) {
        setStatus("disconnected");
      }
    };

    return () => {
      ws.close();
    };
  }, [id]);

  // Auto-scroll to bottom on new events
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [events]);

  const statusBadge = useCallback(() => {
    const base = "inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-xs font-medium";
    switch (status) {
      case "connecting":
        return <span className={`${base} bg-zinc-700 text-zinc-300`}>Connecting...</span>;
      case "live":
        return (
          <span className={`${base} bg-indigo-950 text-indigo-300`}>
            <span className="relative flex h-2 w-2">
              <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-indigo-400 opacity-75"></span>
              <span className="relative inline-flex h-2 w-2 rounded-full bg-indigo-500"></span>
            </span>
            Live
          </span>
        );
      case "completed":
        return <span className={`${base} bg-emerald-950 text-emerald-300`}>Completed</span>;
      case "failed":
        return <span className={`${base} bg-red-950 text-red-300`}>Failed</span>;
      case "disconnected":
        return <span className={`${base} bg-amber-950 text-amber-300`}>Disconnected</span>;
    }
  }, [status]);

  return (
    <div className="min-h-screen px-4 py-8">
      <div className="mx-auto max-w-3xl">
        {/* Header */}
        <div className="mb-8 flex items-start justify-between gap-4">
          <div className="min-w-0 flex-1">
            <Link
              href="/"
              className="mb-3 inline-flex items-center gap-1 text-sm text-zinc-500 transition-colors hover:text-zinc-300"
            >
              &larr; Back to Karuna
            </Link>
            <h1 className="text-2xl font-bold text-zinc-50 break-words">
              {task?.goal ?? "Loading..."}
            </h1>
            <p className="mt-1 text-xs text-zinc-500 font-mono">{id}</p>
          </div>
          <div className="flex-shrink-0 pt-8">{statusBadge()}</div>
        </div>

        {/* Steps progress */}
        {steps.length > 0 && (
          <div className="mb-8 rounded-lg border border-zinc-800 bg-zinc-800/50 p-4">
            <h2 className="mb-3 text-sm font-semibold text-zinc-300 uppercase tracking-wide">
              Steps
            </h2>
            <div className="flex flex-col gap-2">
              {steps.map((step, i) => (
                <div
                  key={step.id}
                  className="flex items-center gap-3 text-sm"
                >
                  <StepIcon status={step.status} />
                  <span className="text-zinc-400 font-mono text-xs">
                    {i + 1}/{steps.length}
                  </span>
                  <span className="text-zinc-200">{step.skill}</span>
                  <span className="text-zinc-500">&mdash;</span>
                  <span className="text-zinc-400 truncate">{step.description}</span>
                  <span className="ml-auto text-xs text-zinc-500 capitalize">
                    {step.status.toLowerCase()}
                  </span>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Event timeline */}
        <div className="mb-8">
          <h2 className="mb-4 text-sm font-semibold text-zinc-300 uppercase tracking-wide">
            Events
          </h2>
          <TaskTimeline events={events} />
          <div ref={bottomRef} />
        </div>

        {/* Result */}
        {status === "completed" && task?.result && (
          <div className="rounded-lg border border-emerald-800 bg-emerald-950/30 p-4">
            <h2 className="mb-2 text-sm font-semibold text-emerald-300 uppercase tracking-wide">
              Result
            </h2>
            <pre className="max-h-64 overflow-auto text-sm text-emerald-200 font-mono whitespace-pre-wrap break-words">
              {JSON.stringify(task.result, null, 2)}
            </pre>
          </div>
        )}

        {/* Error */}
        {status === "failed" && task?.error && (
          <div className="rounded-lg border border-red-800 bg-red-950/30 p-4">
            <h2 className="mb-2 text-sm font-semibold text-red-300 uppercase tracking-wide">
              Error
            </h2>
            <pre className="max-h-64 overflow-auto text-sm text-red-200 font-mono whitespace-pre-wrap break-words">
              {task.error}
            </pre>
          </div>
        )}
      </div>
    </div>
  );
}

function StepIcon({ status }: { status: string }) {
  const s = status.toLowerCase();
  if (s === "completed") {
    return (
      <svg className="h-4 w-4 text-emerald-400" fill="none" viewBox="0 0 24 24" strokeWidth={2.5} stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12.75l6 6 9-13.5" />
      </svg>
    );
  }
  if (s === "failed") {
    return (
      <svg className="h-4 w-4 text-red-400" fill="none" viewBox="0 0 24 24" strokeWidth={2.5} stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
      </svg>
    );
  }
  if (s === "running") {
    return (
      <span className="relative flex h-4 w-4 items-center justify-center">
        <span className="absolute inline-flex h-3 w-3 animate-ping rounded-full bg-indigo-400 opacity-75"></span>
        <span className="relative inline-flex h-2 w-2 rounded-full bg-indigo-500"></span>
      </span>
    );
  }
  return <div className="h-4 w-4 rounded-full border-2 border-zinc-600" />;
}
