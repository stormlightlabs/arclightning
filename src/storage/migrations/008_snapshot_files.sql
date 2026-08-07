CREATE TABLE IF NOT EXISTS snapshot_files (
    path TEXT PRIMARY KEY,
    content BLOB NOT NULL
);

INSERT INTO meta (key, value)
VALUES ('database-format-version', '8')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
