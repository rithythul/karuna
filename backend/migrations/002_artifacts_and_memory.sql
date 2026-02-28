-- Artifacts produced by skills (files, screenshots, reports, etc.)
CREATE TABLE artifacts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id UUID NOT NULL REFERENCES tasks(id),
    step_id UUID REFERENCES task_steps(id),
    name TEXT NOT NULL,
    artifact_type TEXT NOT NULL,  -- 'file', 'screenshot', 'report', 'code', 'data', 'deployment'
    mime_type TEXT,
    path_in_sandbox TEXT,         -- path inside the container
    content TEXT,                 -- inline content for small artifacts (code, text)
    metadata JSONB DEFAULT '{}',
    size_bytes BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_artifacts_task_id ON artifacts(task_id);
CREATE INDEX idx_artifacts_step_id ON artifacts(step_id);

-- Task memory: persistent context across steps within a task
CREATE TABLE task_memory (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id UUID NOT NULL REFERENCES tasks(id),
    key TEXT NOT NULL,
    value JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(task_id, key)
);
CREATE INDEX idx_task_memory_task_id ON task_memory(task_id);

-- Add columns to task_steps for richer tracking
ALTER TABLE task_steps ADD COLUMN IF NOT EXISTS retry_count INT NOT NULL DEFAULT 0;
ALTER TABLE task_steps ADD COLUMN IF NOT EXISTS reflection TEXT;

-- Add token usage tracking to tasks
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS token_usage JSONB DEFAULT '{}';
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS total_duration_ms BIGINT;
