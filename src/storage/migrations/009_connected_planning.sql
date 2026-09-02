-- Expand the v1 tracker into the connected planning model. The v1 tables
-- remain readable until the contract migration; every migrated relationship is
-- copied rather than inferred by later application code.

CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0)
);

INSERT OR IGNORE INTO projects (id, name)
VALUES ('arcl-pj-00000000000000000000000000', 'Project');

ALTER TABLE releases ADD COLUMN project_id TEXT NOT NULL DEFAULT 'arcl-pj-00000000000000000000000000';

CREATE INDEX IF NOT EXISTS releases_project_idx ON releases(project_id);
CREATE UNIQUE INDEX IF NOT EXISTS releases_project_id_unique ON releases(project_id, id);

CREATE TABLE IF NOT EXISTS captures (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    body TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('captured', 'promoted', 'discarded')),
    created_at TEXT NOT NULL,
    UNIQUE (project_id, id)
);

CREATE INDEX IF NOT EXISTS captures_project_created_idx ON captures(project_id, created_at, id);

CREATE TABLE IF NOT EXISTS specs (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    body TEXT NOT NULL DEFAULT '',
    acceptance_criteria TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('open', 'completed', 'cancelled')),
    source_capture_id TEXT,
    legacy_description TEXT NOT NULL DEFAULT '',
    legacy_spec_path TEXT,
    legacy_body_imported INTEGER NOT NULL DEFAULT 1 CHECK (legacy_body_imported IN (0, 1)),
    UNIQUE (project_id, id),
    UNIQUE (project_id, source_capture_id),
    FOREIGN KEY (project_id, source_capture_id) REFERENCES captures(project_id, id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS specs_project_idx ON specs(project_id, id);

CREATE TABLE IF NOT EXISTS plans (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    spec_id TEXT NOT NULL,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    body TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('open', 'completed', 'cancelled')),
    UNIQUE (project_id, id),
    FOREIGN KEY (project_id, spec_id) REFERENCES specs(project_id, id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS plans_spec_idx ON plans(project_id, spec_id, id);

CREATE TABLE IF NOT EXISTS phases (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    plan_id TEXT NOT NULL,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    body TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('open', 'completed', 'cancelled')),
    position INTEGER NOT NULL CHECK (position >= 0),
    UNIQUE (project_id, id),
    UNIQUE (project_id, plan_id, position, id),
    FOREIGN KEY (project_id, plan_id) REFERENCES plans(project_id, id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS phases_plan_position_idx ON phases(project_id, plan_id, position, id);

CREATE TABLE IF NOT EXISTS planning_tasks (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    spec_id TEXT,
    plan_id TEXT,
    phase_id TEXT,
    parent_id TEXT,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    body TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('pending', 'in_progress', 'parked', 'completed', 'cancelled')),
    priority TEXT NOT NULL CHECK (priority IN ('critical', 'high', 'normal', 'low')),
    position INTEGER NOT NULL CHECK (position >= 0),
    handoff TEXT NOT NULL DEFAULT '',
    evidence TEXT NOT NULL DEFAULT '',
    UNIQUE (project_id, id),
    FOREIGN KEY (project_id, spec_id) REFERENCES specs(project_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (project_id, plan_id) REFERENCES plans(project_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (project_id, phase_id) REFERENCES phases(project_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (project_id, parent_id) REFERENCES planning_tasks(project_id, id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS planning_tasks_placement_idx
    ON planning_tasks(project_id, phase_id, plan_id, spec_id, parent_id, position, id);

CREATE TABLE IF NOT EXISTS planning_task_dependencies (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    task_id TEXT NOT NULL,
    blocker_id TEXT NOT NULL,
    PRIMARY KEY (project_id, task_id, blocker_id),
    FOREIGN KEY (project_id, task_id) REFERENCES planning_tasks(project_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (project_id, blocker_id) REFERENCES planning_tasks(project_id, id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS planning_task_dependencies_blocker_idx
    ON planning_task_dependencies(project_id, blocker_id, task_id);

CREATE TABLE IF NOT EXISTS notes (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    body TEXT NOT NULL DEFAULT '',
    UNIQUE (project_id, id)
);

CREATE INDEX IF NOT EXISTS notes_project_idx ON notes(project_id, id);

CREATE TABLE IF NOT EXISTS capture_promotions (
    capture_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('spec', 'task', 'note')),
    target_id TEXT NOT NULL,
    UNIQUE (project_id, target_kind, target_id),
    FOREIGN KEY (project_id, capture_id) REFERENCES captures(project_id, id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS release_memberships (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    release_id TEXT NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('spec', 'plan', 'task', 'note')),
    record_id TEXT NOT NULL,
    PRIMARY KEY (project_id, release_id, record_kind, record_id),
    FOREIGN KEY (project_id, release_id) REFERENCES releases(project_id, id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS release_memberships_record_idx
    ON release_memberships(project_id, record_kind, record_id);

CREATE TABLE IF NOT EXISTS record_links (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('capture', 'spec', 'plan', 'phase', 'task', 'note', 'release')),
    source_id TEXT NOT NULL,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('capture', 'spec', 'plan', 'phase', 'task', 'note', 'release')),
    target_id TEXT NOT NULL,
    PRIMARY KEY (project_id, source_kind, source_id, target_kind, target_id)
);

CREATE INDEX IF NOT EXISTS record_links_target_idx
    ON record_links(project_id, target_kind, target_id);

CREATE TABLE IF NOT EXISTS legacy_id_mappings (
    legacy_kind TEXT NOT NULL,
    legacy_id TEXT NOT NULL,
    current_kind TEXT NOT NULL,
    current_id TEXT NOT NULL,
    PRIMARY KEY (legacy_kind, legacy_id),
    UNIQUE (current_kind, current_id)
);

-- The old release-to-epic edge is now an explicit release-to-spec edge. A
-- descendant plan is not implicitly made a release member.
INSERT INTO captures (id, project_id, title, body, status, created_at)
SELECT 'arcl-c-' || substr(id, 8), 'arcl-pj-00000000000000000000000000', title, description, status,
       datetime('now')
FROM ideas;

INSERT INTO specs (
    id, project_id, title, body, acceptance_criteria, status, source_capture_id,
    legacy_description, legacy_spec_path, legacy_body_imported
)
SELECT
    'arcl-s-' || substr(e.id, 8),
    'arcl-pj-00000000000000000000000000',
    e.title,
    e.description,
    '',
    e.status,
    CASE WHEN p.idea_id IS NULL THEN NULL ELSE 'arcl-c-' || substr(p.idea_id, 8) END,
    e.description,
    e.spec_path,
    0
FROM epics e
LEFT JOIN idea_promotions p ON p.epic_id = e.id;

INSERT INTO plans (id, project_id, spec_id, title, body, status)
SELECT
    'arcl-pl-' || substr(e.id, 8),
    'arcl-pj-00000000000000000000000000',
    'arcl-s-' || substr(e.id, 8),
    e.title || ' implementation plan',
    '',
    e.status
FROM epics e;

INSERT INTO phases (id, project_id, plan_id, title, body, status, position)
SELECT
    'arcl-ph-' || substr(m.id, 8),
    'arcl-pj-00000000000000000000000000',
    'arcl-pl-' || substr(m.epic_id, 8),
    m.title,
    m.description,
    m.status,
    m.position
FROM milestones m;

-- Parent edges are added after all task rows exist so an arbitrary row order in
-- the old database cannot prevent a valid migration.
INSERT INTO planning_tasks (
    id, project_id, spec_id, plan_id, phase_id, parent_id, title, body, status,
    priority, position, handoff, evidence
)
SELECT
    t.id,
    'arcl-pj-00000000000000000000000000',
    'arcl-s-' || substr(m.epic_id, 8),
    'arcl-pl-' || substr(m.epic_id, 8),
    'arcl-ph-' || substr(t.milestone_id, 8),
    NULL,
    t.title,
    t.description,
    t.status,
    t.priority,
    t.position,
    t.handoff,
    t.evidence
FROM tasks t
JOIN milestones m ON m.id = t.milestone_id;

UPDATE planning_tasks
SET parent_id = (
    SELECT old.parent_id
    FROM tasks old
    WHERE old.id = planning_tasks.id
)
WHERE EXISTS (SELECT 1 FROM tasks old WHERE old.id = planning_tasks.id AND old.parent_id IS NOT NULL);

INSERT INTO planning_task_dependencies (project_id, task_id, blocker_id)
SELECT 'arcl-pj-00000000000000000000000000', task_id, blocker_id
FROM task_dependencies;

INSERT INTO release_memberships (project_id, release_id, record_kind, record_id)
SELECT
    'arcl-pj-00000000000000000000000000',
    e.release_id,
    'spec',
    'arcl-s-' || substr(e.id, 8)
FROM epics e
WHERE e.release_id IS NOT NULL;

INSERT INTO capture_promotions (capture_id, project_id, target_kind, target_id)
SELECT
    'arcl-c-' || substr(p.idea_id, 8),
    'arcl-pj-00000000000000000000000000',
    'spec',
    'arcl-s-' || substr(p.epic_id, 8)
FROM idea_promotions p;

INSERT INTO legacy_id_mappings (legacy_kind, legacy_id, current_kind, current_id)
SELECT 'idea', id, 'capture', 'arcl-c-' || substr(id, 8) FROM ideas
UNION ALL
SELECT 'epic', id, 'spec', 'arcl-s-' || substr(id, 8) FROM epics
UNION ALL
SELECT 'milestone', id, 'phase', 'arcl-ph-' || substr(id, 8) FROM milestones
UNION ALL
SELECT 'task', id, 'task', id FROM tasks
UNION ALL
SELECT 'release', id, 'release', id FROM releases;

CREATE TRIGGER capture_promotions_target_exists
BEFORE INSERT ON capture_promotions
WHEN NOT (
    (NEW.target_kind = 'spec' AND EXISTS (SELECT 1 FROM specs WHERE project_id = NEW.project_id AND id = NEW.target_id))
 OR (NEW.target_kind = 'task' AND EXISTS (SELECT 1 FROM planning_tasks WHERE project_id = NEW.project_id AND id = NEW.target_id))
 OR (NEW.target_kind = 'note' AND EXISTS (SELECT 1 FROM notes WHERE project_id = NEW.project_id AND id = NEW.target_id))
)
BEGIN
    SELECT RAISE(ABORT, 'capture promotion target does not exist in project');
END;

-- A task may specify several ancestry fields, but each explicit field must
-- describe the same chain. Cross-project references are already rejected by
-- the composite foreign keys above.
CREATE TRIGGER planning_tasks_ancestry_insert
BEFORE INSERT ON planning_tasks
WHEN (NEW.phase_id IS NOT NULL AND NEW.plan_id IS NOT NULL AND NOT EXISTS (
         SELECT 1 FROM phases WHERE project_id = NEW.project_id AND id = NEW.phase_id AND plan_id = NEW.plan_id
     ))
  OR (NEW.plan_id IS NOT NULL AND NEW.spec_id IS NOT NULL AND NOT EXISTS (
         SELECT 1 FROM plans WHERE project_id = NEW.project_id AND id = NEW.plan_id AND spec_id = NEW.spec_id
     ))
  OR (NEW.parent_id IS NOT NULL AND EXISTS (
         SELECT 1 FROM planning_tasks parent
         WHERE parent.project_id = NEW.project_id AND parent.id = NEW.parent_id
           AND ((NEW.spec_id IS NOT NULL AND parent.spec_id IS NOT NULL AND NEW.spec_id <> parent.spec_id)
             OR (NEW.plan_id IS NOT NULL AND parent.plan_id IS NOT NULL AND NEW.plan_id <> parent.plan_id)
             OR (NEW.phase_id IS NOT NULL AND parent.phase_id IS NOT NULL AND NEW.phase_id <> parent.phase_id))
     ))
BEGIN
    SELECT RAISE(ABORT, 'planning task has contradictory ancestry');
END;

CREATE TRIGGER planning_tasks_ancestry_update
BEFORE UPDATE OF project_id, spec_id, plan_id, phase_id, parent_id ON planning_tasks
WHEN (NEW.phase_id IS NOT NULL AND NEW.plan_id IS NOT NULL AND NOT EXISTS (
         SELECT 1 FROM phases WHERE project_id = NEW.project_id AND id = NEW.phase_id AND plan_id = NEW.plan_id
     ))
  OR (NEW.plan_id IS NOT NULL AND NEW.spec_id IS NOT NULL AND NOT EXISTS (
         SELECT 1 FROM plans WHERE project_id = NEW.project_id AND id = NEW.plan_id AND spec_id = NEW.spec_id
     ))
  OR (NEW.parent_id IS NOT NULL AND EXISTS (
         SELECT 1 FROM planning_tasks parent
         WHERE parent.project_id = NEW.project_id AND parent.id = NEW.parent_id
           AND ((NEW.spec_id IS NOT NULL AND parent.spec_id IS NOT NULL AND NEW.spec_id <> parent.spec_id)
             OR (NEW.plan_id IS NOT NULL AND parent.plan_id IS NOT NULL AND NEW.plan_id <> parent.plan_id)
             OR (NEW.phase_id IS NOT NULL AND parent.phase_id IS NOT NULL AND NEW.phase_id <> parent.phase_id))
     ))
BEGIN
    SELECT RAISE(ABORT, 'planning task has contradictory ancestry');
END;

CREATE TRIGGER release_memberships_record_exists
BEFORE INSERT ON release_memberships
WHEN NOT (
    (NEW.record_kind = 'spec' AND EXISTS (SELECT 1 FROM specs WHERE project_id = NEW.project_id AND id = NEW.record_id))
 OR (NEW.record_kind = 'plan' AND EXISTS (SELECT 1 FROM plans WHERE project_id = NEW.project_id AND id = NEW.record_id))
 OR (NEW.record_kind = 'task' AND EXISTS (SELECT 1 FROM planning_tasks WHERE project_id = NEW.project_id AND id = NEW.record_id))
 OR (NEW.record_kind = 'note' AND EXISTS (SELECT 1 FROM notes WHERE project_id = NEW.project_id AND id = NEW.record_id))
)
BEGIN
    SELECT RAISE(ABORT, 'release member does not exist in project');
END;

CREATE TRIGGER record_links_source_exists
BEFORE INSERT ON record_links
WHEN NOT (
    (NEW.source_kind = 'capture' AND EXISTS (SELECT 1 FROM captures WHERE project_id = NEW.project_id AND id = NEW.source_id))
 OR (NEW.source_kind = 'spec' AND EXISTS (SELECT 1 FROM specs WHERE project_id = NEW.project_id AND id = NEW.source_id))
 OR (NEW.source_kind = 'plan' AND EXISTS (SELECT 1 FROM plans WHERE project_id = NEW.project_id AND id = NEW.source_id))
 OR (NEW.source_kind = 'phase' AND EXISTS (SELECT 1 FROM phases WHERE project_id = NEW.project_id AND id = NEW.source_id))
 OR (NEW.source_kind = 'task' AND EXISTS (SELECT 1 FROM planning_tasks WHERE project_id = NEW.project_id AND id = NEW.source_id))
 OR (NEW.source_kind = 'note' AND EXISTS (SELECT 1 FROM notes WHERE project_id = NEW.project_id AND id = NEW.source_id))
 OR (NEW.source_kind = 'release' AND EXISTS (SELECT 1 FROM releases WHERE project_id = NEW.project_id AND id = NEW.source_id))
)
BEGIN
    SELECT RAISE(ABORT, 'linked source does not exist in project');
END;

CREATE TRIGGER record_links_target_exists
BEFORE INSERT ON record_links
WHEN NOT (
    (NEW.target_kind = 'capture' AND EXISTS (SELECT 1 FROM captures WHERE project_id = NEW.project_id AND id = NEW.target_id))
 OR (NEW.target_kind = 'spec' AND EXISTS (SELECT 1 FROM specs WHERE project_id = NEW.project_id AND id = NEW.target_id))
 OR (NEW.target_kind = 'plan' AND EXISTS (SELECT 1 FROM plans WHERE project_id = NEW.project_id AND id = NEW.target_id))
 OR (NEW.target_kind = 'phase' AND EXISTS (SELECT 1 FROM phases WHERE project_id = NEW.project_id AND id = NEW.target_id))
 OR (NEW.target_kind = 'task' AND EXISTS (SELECT 1 FROM planning_tasks WHERE project_id = NEW.project_id AND id = NEW.target_id))
 OR (NEW.target_kind = 'note' AND EXISTS (SELECT 1 FROM notes WHERE project_id = NEW.project_id AND id = NEW.target_id))
 OR (NEW.target_kind = 'release' AND EXISTS (SELECT 1 FROM releases WHERE project_id = NEW.project_id AND id = NEW.target_id))
)
BEGIN
    SELECT RAISE(ABORT, 'linked target does not exist in project');
END;

INSERT INTO meta (key, value)
VALUES ('database-format-version', '9')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
