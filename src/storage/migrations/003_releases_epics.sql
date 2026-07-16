CREATE TABLE IF NOT EXISTS releases (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('open', 'completed', 'cancelled'))
);

CREATE TABLE IF NOT EXISTS epics (
    id TEXT PRIMARY KEY,
    release_id TEXT REFERENCES releases(id) ON DELETE RESTRICT,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    description TEXT NOT NULL DEFAULT '',
    spec_path TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL CHECK (status IN ('open', 'completed', 'cancelled'))
);

INSERT INTO meta (key, value)
VALUES ('database-format-version', '3')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
