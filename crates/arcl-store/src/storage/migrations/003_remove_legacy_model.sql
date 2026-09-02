-- Copy any records that still exist only in the superseded tracker model into
-- the connected planning model before removing the old tables. The transformed
-- identifiers retain each record's ULID while adopting the current type prefix.
INSERT OR IGNORE INTO captures (id, project_id, title, body, status, created_at)
SELECT
    'arcl-c-' || substr(id, 8),
    'arcl-pj-00000000000000000000000000',
    title,
    description,
    status,
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
FROM ideas;

INSERT OR IGNORE INTO specs (
    id, project_id, title, body, acceptance_criteria, status, source_capture_id
)
SELECT
    'arcl-s-' || substr(e.id, 8),
    'arcl-pj-00000000000000000000000000',
    e.title,
    e.description,
    '',
    e.status,
    CASE WHEN p.idea_id IS NULL THEN NULL ELSE 'arcl-c-' || substr(p.idea_id, 8) END
FROM epics e
LEFT JOIN idea_promotions p ON p.epic_id = e.id;

INSERT OR IGNORE INTO plans (id, project_id, spec_id, title, body, status)
SELECT
    'arcl-pl-' || substr(id, 8),
    'arcl-pj-00000000000000000000000000',
    'arcl-s-' || substr(id, 8),
    title || ' implementation plan',
    '',
    status
FROM epics;

INSERT OR IGNORE INTO phases (id, project_id, plan_id, plan_key, title, body, status, position)
SELECT
    'arcl-ph-' || substr(id, 8),
    'arcl-pj-00000000000000000000000000',
    'arcl-pl-' || substr(epic_id, 8),
    plan_key,
    title,
    description,
    status,
    position
FROM milestones;

-- Add parent relationships only after every task has been copied.
INSERT OR IGNORE INTO planning_tasks (
    id, project_id, spec_id, plan_id, phase_id, parent_id, plan_key, title, body,
    status, priority, position, handoff, evidence
)
SELECT
    t.id,
    'arcl-pj-00000000000000000000000000',
    'arcl-s-' || substr(m.epic_id, 8),
    'arcl-pl-' || substr(m.epic_id, 8),
    'arcl-ph-' || substr(t.milestone_id, 8),
    NULL,
    t.plan_key,
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
SET parent_id = (SELECT old.parent_id FROM tasks old WHERE old.id = planning_tasks.id)
WHERE parent_id IS NULL
  AND EXISTS (SELECT 1 FROM tasks old WHERE old.id = planning_tasks.id AND old.parent_id IS NOT NULL);

INSERT OR IGNORE INTO planning_task_dependencies (project_id, task_id, blocker_id)
SELECT 'arcl-pj-00000000000000000000000000', task_id, blocker_id
FROM task_dependencies;

INSERT OR IGNORE INTO release_memberships (project_id, release_id, record_kind, record_id)
SELECT
    'arcl-pj-00000000000000000000000000',
    release_id,
    'spec',
    'arcl-s-' || substr(id, 8)
FROM epics
WHERE release_id IS NOT NULL;

INSERT OR IGNORE INTO capture_promotions (capture_id, project_id, target_kind, target_id)
SELECT
    'arcl-c-' || substr(idea_id, 8),
    'arcl-pj-00000000000000000000000000',
    'spec',
    'arcl-s-' || substr(epic_id, 8)
FROM idea_promotions;

DROP TABLE task_dependencies;
DROP TABLE idea_promotions;
UPDATE tasks SET parent_id = NULL;
DROP TABLE tasks;
DROP TABLE milestones;
DROP TABLE epics;
DROP TABLE ideas;

UPDATE meta SET value = '3' WHERE key = 'database-format-version';
