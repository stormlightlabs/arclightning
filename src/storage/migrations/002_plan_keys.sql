ALTER TABLE phases ADD COLUMN plan_key TEXT;
ALTER TABLE planning_tasks ADD COLUMN plan_key TEXT;

CREATE UNIQUE INDEX phases_plan_key_idx
    ON phases(project_id, plan_id, plan_key)
    WHERE plan_key IS NOT NULL;

CREATE UNIQUE INDEX planning_tasks_plan_key_idx
    ON planning_tasks(project_id, plan_id, plan_key)
    WHERE plan_key IS NOT NULL;

UPDATE meta SET value = '2' WHERE key = 'database-format-version';
