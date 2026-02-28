CREATE TYPE task_status AS ENUM ('pending', 'planning', 'running', 'paused', 'completed', 'failed');

CREATE TABLE tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    goal TEXT NOT NULL,
    status task_status NOT NULL DEFAULT 'pending',
    plan JSONB,
    result JSONB,
    error TEXT,
    sandbox_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_tasks_user_id ON tasks(user_id);

CREATE TABLE task_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id UUID NOT NULL REFERENCES tasks(id),
    skill TEXT NOT NULL,
    description TEXT NOT NULL,
    step_order INT NOT NULL DEFAULT 0,
    status task_status NOT NULL DEFAULT 'pending',
    input_data JSONB,
    output_data JSONB,
    error TEXT,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ
);
CREATE INDEX idx_task_steps_task_id ON task_steps(task_id);

CREATE TABLE task_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id UUID NOT NULL REFERENCES tasks(id),
    event_type TEXT NOT NULL,
    data JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_task_events_task_id ON task_events(task_id);
