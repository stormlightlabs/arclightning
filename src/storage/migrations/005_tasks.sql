CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    milestone_id TEXT NOT NULL REFERENCES milestones(id) ON DELETE RESTRICT,
    parent_id TEXT REFERENCES tasks(id) ON DELETE RESTRICT,
    plan_key TEXT,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('pending', 'in_progress', 'parked', 'completed', 'cancelled')),
    priority TEXT NOT NULL CHECK (priority IN ('critical', 'high', 'normal', 'low')),
    position INTEGER NOT NULL CHECK (position >= 0),
    UNIQUE (milestone_id, plan_key)
);

INSERT INTO meta (key, value)
VALUES ('database-format-version', '5')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
