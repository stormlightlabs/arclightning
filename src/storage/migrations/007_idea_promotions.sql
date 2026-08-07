CREATE TABLE IF NOT EXISTS idea_promotions (
    idea_id TEXT PRIMARY KEY REFERENCES ideas(id) ON DELETE RESTRICT,
    epic_id TEXT NOT NULL UNIQUE REFERENCES epics(id) ON DELETE RESTRICT
);

ALTER TABLE tasks ADD COLUMN handoff TEXT NOT NULL DEFAULT '';
ALTER TABLE tasks ADD COLUMN evidence TEXT NOT NULL DEFAULT '';

INSERT INTO meta (key, value)
VALUES ('database-format-version', '7')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
