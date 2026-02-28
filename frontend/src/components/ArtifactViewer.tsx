"use client";

interface Artifact {
  id: string;
  name: string;
  artifact_type: string;
  mime_type: string | null;
  path_in_sandbox: string | null;
  content: string | null;
  size_bytes: number | null;
  created_at: string;
}

interface ArtifactViewerProps {
  artifacts: Artifact[];
}

function artifactIcon(type: string): string {
  switch (type) {
    case "screenshot": return "🖼️";
    case "report": return "📄";
    case "code": return "💻";
    case "data": return "📊";
    case "deployment": return "📦";
    default: return "📎";
  }
}

function artifactColor(type: string): string {
  switch (type) {
    case "screenshot": return "var(--status-info)";
    case "report": return "var(--accent)";
    case "code": return "var(--status-success)";
    case "data": return "var(--status-info)";
    case "deployment": return "#a78bfa";
    default: return "var(--text-tertiary)";
  }
}

function formatSize(bytes: number | null): string {
  if (!bytes) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1048576).toFixed(1)} MB`;
}

export type { Artifact };

export default function ArtifactViewer({ artifacts }: ArtifactViewerProps) {
  if (artifacts.length === 0) return null;

  return (
    <div className="rounded-xl p-4" style={{ background: "var(--bg-raised)", border: "1px solid var(--border-subtle)" }}>
      <h2 className="mb-3 text-[12px] font-semibold uppercase tracking-wider" style={{ color: "var(--text-secondary)" }}>
        Artifacts ({artifacts.length})
      </h2>
      <div className="flex flex-col gap-2">
        {artifacts.map((artifact) => (
          <div
            key={artifact.id}
            className="flex items-center gap-3 rounded-lg p-3 transition-colors"
            style={{ background: "var(--bg-elevated)", border: "1px solid var(--border-subtle)" }}
          >
            <span className="text-lg flex-shrink-0">{artifactIcon(artifact.artifact_type)}</span>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="text-[13px] font-medium truncate" style={{ color: "var(--text-primary)" }}>
                  {artifact.name}
                </span>
                <span
                  className="rounded-full px-2 py-0.5 text-[10px] font-medium flex-shrink-0"
                  style={{
                    background: `color-mix(in srgb, ${artifactColor(artifact.artifact_type)} 15%, transparent)`,
                    color: artifactColor(artifact.artifact_type),
                  }}
                >
                  {artifact.artifact_type}
                </span>
              </div>
              <div className="flex items-center gap-2 mt-0.5">
                {artifact.path_in_sandbox && (
                  <span className="text-[11px] font-mono truncate" style={{ color: "var(--text-tertiary)" }}>
                    {artifact.path_in_sandbox}
                  </span>
                )}
                {artifact.size_bytes && (
                  <span className="text-[10px] flex-shrink-0" style={{ color: "var(--text-tertiary)" }}>
                    {formatSize(artifact.size_bytes)}
                  </span>
                )}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
