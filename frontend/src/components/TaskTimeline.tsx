"use client";

export interface TaskEvent {
  id?: string;
  task_id?: string;
  event_type: string;
  data: Record<string, unknown>;
  created_at: string;
}

interface TaskTimelineProps {
  events: TaskEvent[];
}

function statusColor(eventType: string): string {
  switch (eventType) {
    case "task_completed":
      return "bg-emerald-500";
    case "task_failed":
    case "step_failed":
      return "bg-red-500";
    case "step_started":
    case "task_started":
      return "bg-indigo-500";
    case "step_completed":
      return "bg-emerald-400";
    case "planning_started":
    case "planning_completed":
      return "bg-amber-400";
    default:
      return "bg-zinc-500";
  }
}

function formatEventType(eventType: string): string {
  return eventType
    .split("_")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

function formatTime(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  } catch {
    return iso;
  }
}

function renderData(data: Record<string, unknown>): React.ReactNode {
  if (!data || Object.keys(data).length === 0) return null;

  return (
    <pre className="mt-2 max-h-48 overflow-auto rounded bg-zinc-900 p-3 text-xs text-zinc-400 font-mono whitespace-pre-wrap break-words">
      {JSON.stringify(data, null, 2)}
    </pre>
  );
}

export default function TaskTimeline({ events }: TaskTimelineProps) {
  if (events.length === 0) {
    return (
      <div className="flex items-center gap-3 text-zinc-500">
        <span className="relative flex h-3 w-3">
          <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-indigo-400 opacity-75"></span>
          <span className="relative inline-flex h-3 w-3 rounded-full bg-indigo-500"></span>
        </span>
        Waiting for events...
      </div>
    );
  }

  return (
    <div className="relative flex flex-col gap-0">
      {/* Vertical line */}
      <div className="absolute left-[7px] top-2 bottom-2 w-px bg-zinc-700" />

      {events.map((event, i) => (
        <div key={event.id ?? i} className="relative flex gap-4 pb-6 last:pb-0">
          {/* Dot */}
          <div className="relative z-10 mt-1.5 flex-shrink-0">
            <div className={`h-[15px] w-[15px] rounded-full ${statusColor(event.event_type)} ring-4 ring-zinc-800`} />
          </div>

          {/* Content */}
          <div className="flex-1 min-w-0">
            <div className="flex items-baseline gap-3 flex-wrap">
              <span className="text-sm font-semibold text-zinc-100">
                {formatEventType(event.event_type)}
              </span>
              <span className="text-xs text-zinc-500">
                {formatTime(event.created_at)}
              </span>
            </div>
            {renderData(event.data)}
          </div>
        </div>
      ))}
    </div>
  );
}
