CREATE TABLE IF NOT EXISTS ideas (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('captured', 'promoted', 'discarded'))
);

INSERT INTO meta (key, value)
VALUES ('database-format-version', '2')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
