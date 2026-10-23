-- Name the stored synchronization base after the repository-native workspace.
ALTER TABLE snapshot_files RENAME TO workspace_files;

UPDATE meta SET key = 'workspace-sync-state' WHERE key = 'snapshot-sync-state';

UPDATE meta SET value = '4' WHERE key = 'database-format-version';
