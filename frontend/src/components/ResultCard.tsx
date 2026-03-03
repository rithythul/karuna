"use client";

import { useState, useEffect } from "react";
import { type Artifact } from "@/components/ArtifactViewer";

interface ResultData {
  summary?: string;
  key_outputs?: string[];
  artifacts?: string[];
  next_steps?: string[];
}

interface ResultCardProps {
  result: ResultData;
  artifacts: Artifact[];
  taskId: string;
}

function contentUrl(taskId: string, artifactId: string): string {
  return `/api/tasks/${taskId}/artifacts/${artifactId}/content`;
}

function ArtifactPreview({ artifact, taskId }: { artifact: Artifact; taskId: string }) {
  const [textContent, setTextContent] = useState<string | null>(null);
  const [fetchError, setFetchError] = useState(false);
  const url = contentUrl(taskId, artifact.id);
  const mime = artifact.mime_type ?? "";

  useEffect(() => {
    const isText =
      (mime.startsWith("text/") && mime !== "text/html") ||
      mime === "application/json";
    if (isText) {
      fetch(url)
        .then((r) => {
          if (!r.ok) {
            setFetchError(true);
            return;
          }
          return r.text().then(setTextContent);
        })
        .catch(() => {
          setFetchError(true);
        });
    }
  }, [url, mime]);

  if (mime === "text/html") {
    return (
      <iframe
        src={url}
        className="w-full rounded-lg"
        style={{ height: 480, border: "1px solid var(--border-subtle)" }}
        sandbox="allow-scripts"
        title={artifact.name}
      />
    );
  }

  if (mime.startsWith("image/")) {
    return (
      <img
        src={url}
        alt={artifact.name}
        className="w-full rounded-lg object-contain"
        style={{ maxHeight: 480, border: "1px solid var(--border-subtle)" }}
      />
    );
  }

  if (textContent !== null) {
    return (
      <pre
        className="w-full rounded-lg p-4 overflow-auto text-[12px] leading-relaxed"
        style={{
          maxHeight: 480,
          background: "var(--bg-elevated)",
          border: "1px solid var(--border-subtle)",
          color: "var(--text-primary)",
          fontFamily: "monospace",
        }}
      >
        {textContent}
      </pre>
    );
  }

  if (fetchError) {
    return (
      <p
        className="text-[12px]"
        style={{ color: "var(--text-tertiary)" }}
      >
        Preview unavailable
      </p>
    );
  }

  // Fallback: download button
  return (
    <a
      href={url}
      download={artifact.name}
      className="inline-flex items-center gap-2 rounded-lg px-4 py-2.5 text-[13px] font-medium"
      style={{ background: "var(--accent)", color: "var(--text-on-accent)" }}
    >
      <svg width="14" height="14" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
        <path strokeLinecap="round" strokeLinejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5M16.5 12L12 16.5m0 0L7.5 12m4.5 4.5V3" />
      </svg>
      Download {artifact.name}
    </a>
  );
}

export default function ResultCard({ result, artifacts, taskId }: ResultCardProps) {
  // Filter out input artifacts (uploaded by user)
  const outputArtifacts = artifacts.filter((a) => a.artifact_type !== "input");
  const [selectedId, setSelectedId] = useState<string | null>(
    outputArtifacts.length > 0 ? outputArtifacts[0].id : null
  );

  useEffect(() => {
    setSelectedId(outputArtifacts.length > 0 ? outputArtifacts[0].id : null);
  }, [outputArtifacts.length, outputArtifacts[0]?.id]);

  const selected = outputArtifacts.find((a) => a.id === selectedId) ?? null;

  return (
    <div
      className="rounded-2xl p-6 animate-fade-in-up"
      style={{
        background: "var(--bg-raised)",
        border: "1px solid var(--border-subtle)",
      }}
    >
      {/* Summary */}
      {result.summary && (
        <p
          className="text-[15px] leading-relaxed mb-5"
          style={{ color: "var(--text-primary)" }}
        >
          {result.summary}
        </p>
      )}

      {/* Key outputs */}
      {result.key_outputs && result.key_outputs.length > 0 && (
        <ul className="flex flex-col gap-1.5 mb-5">
          {result.key_outputs.map((output, i) => (
            <li
              key={`output-${i}`}
              className="flex items-start gap-2 text-[13px]"
              style={{ color: "var(--text-secondary)" }}
            >
              <span style={{ color: "var(--status-success)", marginTop: 2 }}>✓</span>
              {output}
            </li>
          ))}
        </ul>
      )}

      {/* Artifact preview */}
      {outputArtifacts.length > 0 && (
        <div>
          {/* Tabs for multiple artifacts */}
          {outputArtifacts.length > 1 && (
            <div className="flex gap-2 mb-3 overflow-x-auto pb-1">
              {outputArtifacts.map((a) => (
                <button
                  key={a.id}
                  onClick={() => setSelectedId(a.id)}
                  className="flex-shrink-0 rounded-lg px-3 py-1.5 text-[12px] font-medium transition-all cursor-pointer"
                  style={{
                    background:
                      selectedId === a.id ? "var(--accent)" : "var(--bg-elevated)",
                    color:
                      selectedId === a.id
                        ? "var(--text-on-accent)"
                        : "var(--text-secondary)",
                  }}
                >
                  {a.name}
                </button>
              ))}
            </div>
          )}

          {selected && <ArtifactPreview artifact={selected} taskId={taskId} />}
        </div>
      )}

      {/* Next steps */}
      {result.next_steps && result.next_steps.length > 0 && (
        <div
          className="mt-5 pt-4"
          style={{ borderTop: "1px solid var(--border-subtle)" }}
        >
          <p
            className="text-[11px] font-semibold uppercase tracking-wider mb-2"
            style={{ color: "var(--text-tertiary)" }}
          >
            Suggested Next Steps
          </p>
          <ul className="flex flex-col gap-1">
            {result.next_steps.map((step, i) => (
              <li
                key={`step-${i}`}
                className="text-[12px]"
                style={{ color: "var(--text-secondary)" }}
              >
                → {step}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
