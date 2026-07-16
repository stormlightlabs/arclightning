CREATE TABLE IF NOT EXISTS milestones (
    id TEXT PRIMARY KEY,
    epic_id TEXT NOT NULL REFERENCES epics(id) ON DELETE RESTRICT,
    plan_key TEXT,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('open', 'completed', 'cancelled')),
    position INTEGER NOT NULL CHECK (position >= 0),
    UNIQUE (epic_id, plan_key)
);

INSERT INTO meta (key, value)
VALUES ('database-format-version', '4')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
