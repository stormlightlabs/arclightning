-- Record gates: a pull request, CI run or date a task waits on.
CREATE TABLE IF NOT EXISTS task_gates (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    task_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('pr', 'run', 'date')),
    target TEXT NOT NULL CHECK (length(trim(target)) > 0),
    status TEXT NOT NULL CHECK (status IN ('open', 'met')),
    created_at TEXT NOT NULL CHECK (length(trim(created_at)) > 0),
    met_at TEXT,
    CHECK ((status = 'met') = (met_at IS NOT NULL)),
    FOREIGN KEY (project_id, task_id) REFERENCES planning_tasks(project_id, id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS task_gates_task ON task_gates(task_id);

UPDATE meta SET value = '7' WHERE key = 'database-format-version';
