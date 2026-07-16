CREATE TABLE IF NOT EXISTS task_dependencies (
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
    blocker_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
    PRIMARY KEY (task_id, blocker_id)
);

INSERT INTO meta (key, value)
VALUES ('database-format-version', '6')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
