"use client";

import { useState } from "react";

interface GoalInputProps {
  onSubmit: (goal: string) => void;
  isLoading: boolean;
}

export default function GoalInput({ onSubmit, isLoading }: GoalInputProps) {
  const [goal, setGoal] = useState("");

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = goal.trim();
    if (!trimmed || isLoading) return;
    onSubmit(trimmed);
  };

  return (
    <form onSubmit={handleSubmit} className="w-full max-w-2xl flex flex-col gap-4">
      <textarea
        value={goal}
        onChange={(e) => setGoal(e.target.value)}
        placeholder="Describe what you want Karuna to do..."
        rows={5}
        className="w-full rounded-lg border border-zinc-700 bg-zinc-800 px-4 py-3 text-zinc-100 placeholder-zinc-500 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500 resize-none text-base"
        disabled={isLoading}
      />
      <button
        type="submit"
        disabled={!goal.trim() || isLoading}
        className="self-end rounded-lg bg-indigo-600 px-6 py-2.5 text-sm font-medium text-white transition-colors hover:bg-indigo-500 disabled:opacity-40 disabled:cursor-not-allowed"
      >
        {isLoading ? "Submitting..." : "Run Task"}
      </button>
    </form>
  );
}
