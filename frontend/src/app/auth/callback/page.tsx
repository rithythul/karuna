"use client";

import { Suspense, useEffect, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";

function CallbackHandler() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const code = searchParams.get("code");
    const state = searchParams.get("state");

    if (!code || !state) {
      setError("Missing authorization code or state parameter");
      return;
    }

    fetch("/api/auth/token", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ code, state }),
    })
      .then(async (res) => {
        if (!res.ok) {
          const body = await res.json().catch(() => ({}));
          throw new Error(body.error || `Authentication failed: ${res.status}`);
        }
        return res.json();
      })
      .then(() => {
        // Session cookie is set by the response. Redirect to home.
        router.replace("/");
      })
      .catch((err) => {
        console.error("Auth callback error:", err);
        setError(err instanceof Error ? err.message : "Authentication failed");
      });
  }, [searchParams, router]);

  if (error) {
    return (
      <div className="flex min-h-screen items-center justify-center px-6">
        <div
          className="w-full max-w-md rounded-xl p-6 text-center"
          style={{ background: "var(--bg-raised)", border: "1px solid var(--border-subtle)" }}
        >
          <div className="text-2xl mb-3">:(</div>
          <h2
            className="text-[15px] font-medium mb-2"
            style={{ color: "var(--text-primary)" }}
          >
            Authentication Failed
          </h2>
          <p
            className="text-[13px] mb-4"
            style={{ color: "var(--status-error)" }}
          >
            {error}
          </p>
          <button
            onClick={() => router.replace("/")}
            className="rounded-lg px-4 py-2 text-[13px] font-medium cursor-pointer transition-colors"
            style={{
              background: "var(--bg-elevated)",
              color: "var(--text-primary)",
              border: "1px solid var(--border-subtle)",
            }}
          >
            Back to Home
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex min-h-screen items-center justify-center">
      <div className="flex flex-col items-center gap-3">
        <svg className="animate-spin" width="24" height="24" viewBox="0 0 24 24" fill="none">
          <circle
            cx="12" cy="12" r="10"
            stroke="var(--accent)"
            strokeWidth="2.5"
            strokeDasharray="31.416"
            strokeDashoffset="10"
            strokeLinecap="round"
          />
        </svg>
        <span className="text-[13px]" style={{ color: "var(--text-tertiary)" }}>
          Signing you in...
        </span>
      </div>
    </div>
  );
}

export default function AuthCallbackPage() {
  return (
    <Suspense
      fallback={
        <div className="flex min-h-screen items-center justify-center">
          <svg className="animate-spin" width="24" height="24" viewBox="0 0 24 24" fill="none">
            <circle
              cx="12" cy="12" r="10"
              stroke="var(--accent)"
              strokeWidth="2.5"
              strokeDasharray="31.416"
              strokeDashoffset="10"
              strokeLinecap="round"
            />
          </svg>
        </div>
      }
    >
      <CallbackHandler />
    </Suspense>
  );
}
