"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import GoalInput from "@/components/GoalInput";

export default function Home() {
  const router = useRouter();
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (goal: string) => {
    setIsLoading(true);
    setError(null);

    try {
      const res = await fetch("/api/tasks", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ goal }),
      });

      if (!res.ok) {
        const text = await res.text();
        throw new Error(text || `Request failed with status ${res.status}`);
      }

      const data = await res.json();
      router.push(`/tasks/${data.task_id}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Something went wrong");
      setIsLoading(false);
    }
  };

  return (
    <div className="flex min-h-screen flex-col items-center justify-center px-4">
      <div className="w-full max-w-2xl flex flex-col items-center gap-8">
        {/* Branding */}
        <div className="text-center">
          <h1 className="text-4xl font-bold tracking-tight text-zinc-50">
            Karuna
          </h1>
          <p className="mt-2 text-lg text-zinc-400">
            Autonomous AI agent platform
          </p>
        </div>

        {/* Goal input */}
        <GoalInput onSubmit={handleSubmit} isLoading={isLoading} />

        {/* Error display */}
        {error && (
          <div className="w-full max-w-2xl rounded-lg border border-red-800 bg-red-950/50 px-4 py-3 text-sm text-red-300">
            {error}
          </div>
        )}
      </div>
    </div>
  );
}
