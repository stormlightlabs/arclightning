CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR IGNORE INTO meta (key, value)
VALUES
    ('database-format-version', '1'),
    ('snapshot-sync-state', 'clean');
