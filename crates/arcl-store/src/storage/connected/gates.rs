use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::{Result, StorageError, planning_task};
use arcl_core::domain::{
    DomainError, Gate, GateId, GateKind, GateStatus, ProjectId, TaskId, TaskStatus, validate_gate_target,
};

const GATE_COLUMNS: &str = "id, task_id, kind, target, status, created_at, met_at";

/// Read every gate in the project, grouped by task in task ID order, each
/// task's gates in the order they were added. Insertion order, not the
/// timestamp, decides, since two gates can share a millisecond.
pub fn gates(connection: &Connection, project_id: ProjectId) -> Result<Vec<Gate>> {
    let mut statement = connection.prepare(&format!(
        "SELECT {GATE_COLUMNS} FROM task_gates WHERE project_id = ?1 ORDER BY task_id, rowid"
    ))?;
    let rows = statement.query_map([project_id.to_string()], raw_gate)?;
    rows.map(|row| decode_gate(row?)).collect()
}

/// Read the gates on task `id` in the order they were added.
pub fn gates_for_task(connection: &Connection, id: TaskId) -> Result<Vec<Gate>> {
    let mut statement = connection.prepare(&format!(
        "SELECT {GATE_COLUMNS} FROM task_gates WHERE task_id = ?1 ORDER BY rowid"
    ))?;
    let rows = statement.query_map([id.to_string()], raw_gate)?;
    rows.map(|row| decode_gate(row?)).collect()
}

/// Read one gate by its ID.
pub fn gate(connection: &Connection, id: GateId) -> Result<Option<Gate>> {
    connection
        .query_row(
            &format!("SELECT {GATE_COLUMNS} FROM task_gates WHERE id = ?1"),
            [id.to_string()],
            raw_gate,
        )
        .optional()?
        .map(decode_gate)
        .transpose()
}

/// Add an open gate of `kind` on task `task_id`.
///
/// `target` is checked by [`validate_gate_target`] and stored in its
/// normalized form. A completed or cancelled task is refused with
/// [`StorageError::GateOnClosedTask`]; any other status takes a gate.
pub fn add_gate(
    connection: &mut Connection, project_id: ProjectId, task_id: TaskId, kind: GateKind, target: &str,
) -> Result<Gate> {
    let target = validate_gate_target(kind, target).map_err(StorageError::InvalidGate)?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let task = planning_task(&tx, task_id)?
        .filter(|task| task.project_id == project_id)
        .ok_or_else(|| StorageError::PlanningTaskNotFound { id: task_id.to_string() })?;
    if matches!(task.status, TaskStatus::Completed | TaskStatus::Cancelled) {
        return Err(StorageError::GateOnClosedTask {
            id: task_id.to_string(),
            status: task.status.as_str().to_owned(),
        });
    }
    let created_at: String = tx.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| row.get(0))?;
    let gate = Gate { id: GateId::new(), task_id, kind, target, status: GateStatus::Open, created_at, met_at: None };
    tx.execute(
        "INSERT INTO task_gates (id, project_id, task_id, kind, target, status, created_at, met_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
        params![
            gate.id.to_string(),
            project_id.to_string(),
            task_id.to_string(),
            kind.as_str(),
            gate.target,
            gate.status.as_str(),
            gate.created_at
        ],
    )?;
    tx.commit()?;
    Ok(gate)
}

/// Remove gate `id`, open or met, and return it as it was.
pub fn remove_gate(connection: &mut Connection, project_id: ProjectId, id: GateId) -> Result<Gate> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let gate = project_gate(&tx, project_id, id)?;
    tx.execute("DELETE FROM task_gates WHERE id = ?1", [id.to_string()])?;
    tx.commit()?;
    Ok(gate)
}

/// Mark gate `id` met at the current UTC time and return it. A gate that is
/// already met comes back unchanged.
pub fn meet_gate(connection: &mut Connection, project_id: ProjectId, id: GateId) -> Result<Gate> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let gate = project_gate(&tx, project_id, id)?;
    if gate.status == GateStatus::Met {
        return Ok(gate);
    }
    let met_at: String = tx.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| row.get(0))?;
    tx.execute(
        "UPDATE task_gates SET status = 'met', met_at = ?1 WHERE id = ?2",
        params![met_at, id.to_string()],
    )?;
    tx.commit()?;
    Ok(Gate { status: GateStatus::Met, met_at: Some(met_at), ..gate })
}

/// Mark each gate in `ids` met in one immediate transaction, for a check
/// that probed them outside any transaction. Each gate is read again first:
/// one still open is marked met at the current UTC time, one met since the
/// probe comes back unchanged, and one removed since the probe, or in
/// another project, comes back as `None`. The result has one entry per ID,
/// in order.
pub fn meet_gates(connection: &mut Connection, project_id: ProjectId, ids: &[GateId]) -> Result<Vec<Option<Gate>>> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let met_at: String = tx.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| row.get(0))?;
    let mut gates = Vec::with_capacity(ids.len());
    for &id in ids {
        let gate = match project_gate(&tx, project_id, id) {
            Ok(gate) => gate,
            Err(StorageError::GateNotFound { .. }) => {
                gates.push(None);
                continue;
            }
            Err(error) => return Err(error),
        };
        if gate.status == GateStatus::Met {
            gates.push(Some(gate));
            continue;
        }
        tx.execute(
            "UPDATE task_gates SET status = 'met', met_at = ?1 WHERE id = ?2",
            params![met_at, id.to_string()],
        )?;
        gates.push(Some(Gate {
            status: GateStatus::Met,
            met_at: Some(met_at.clone()),
            ..gate
        }));
    }
    tx.commit()?;
    Ok(gates)
}

/// Read gate `id` in project `project_id`, or fail with
/// [`StorageError::GateNotFound`].
pub fn project_gate(connection: &Connection, project_id: ProjectId, id: GateId) -> Result<Gate> {
    let owner: Option<String> = connection
        .query_row(
            "SELECT project_id FROM task_gates WHERE id = ?1",
            [id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    if owner.as_deref() != Some(project_id.to_string().as_str()) {
        return Err(StorageError::GateNotFound { id: id.to_string() });
    }
    gate(connection, id)?.ok_or_else(|| StorageError::GateNotFound { id: id.to_string() })
}

type RawGate = (String, String, String, String, String, String, Option<String>);

fn raw_gate(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawGate> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}

fn decode_gate((id, task, kind, target, status, created_at, met_at): RawGate) -> Result<Gate> {
    let invalid = |error: DomainError| StorageError::InvalidGate(error);
    Ok(Gate {
        id: GateId::parse(&id).map_err(DomainError::from).map_err(invalid)?,
        task_id: TaskId::parse(&task).map_err(DomainError::from).map_err(invalid)?,
        kind: GateKind::parse(&kind).map_err(invalid)?,
        target,
        status: GateStatus::parse(&status).map_err(invalid)?,
        created_at,
        met_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{Database, OpenSubtasks, PlanningTaskCreate, ReadinessReasonKind};
    use arcl_core::domain::TaskPriority;

    fn task(database: &mut Database, title: &str) -> TaskId {
        let project_id = database.project().expect("project exists").id;
        database
            .create_planning_task(PlanningTaskCreate {
                project_id,
                spec_id: None,
                plan_id: None,
                phase_id: None,
                parent_id: None,
                title: title.to_owned(),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: 0,
            })
            .expect("task creates")
            .id
    }

    #[test]
    fn an_open_gate_keeps_a_task_from_being_ready_until_met() {
        let mut database = Database::open_in_memory().expect("database opens");
        let work = task(&mut database, "Work");
        let gate = database
            .add_gate(work, GateKind::Date, "2026-11-01")
            .expect("gate adds");
        assert_eq!(gate.status, GateStatus::Open);
        assert!(gate.id.to_string().starts_with("arcl-g-"));

        let view = database.planning_task_view(work).expect("view reads");
        assert!(!view.readiness.ready);
        assert!(view.readiness.blocked);
        let reason = &view.readiness.reasons[0];
        assert_eq!(reason.kind, ReadinessReasonKind::OpenGate);
        assert_eq!(reason.gate.as_ref().map(|gate| gate.id), Some(gate.id));
        assert_eq!(reason.message, format!("gate `{}` waits until `2026-11-01`", gate.id));
        assert_eq!(view.gates, vec![gate.clone()]);
        assert!(database.ready_planning_tasks().expect("ready reads").is_empty());
        assert!(database.start_planning_task(work, false).is_err());

        let met = database.meet_gate(gate.id).expect("gate meets");
        assert_eq!(met.status, GateStatus::Met);
        assert!(met.met_at.is_some());
        assert_eq!(database.meet_gate(gate.id).expect("meeting again changes nothing"), met);
        assert!(database.planning_task_view(work).expect("view reads").readiness.ready);

        let removed = database.remove_gate(gate.id).expect("gate removes");
        assert_eq!(removed, met);
        assert!(matches!(
            database.remove_gate(gate.id),
            Err(StorageError::GateNotFound { .. })
        ));
        assert!(database.gates(Some(work)).expect("gates read").is_empty());
    }

    #[test]
    fn gates_refuse_closed_tasks_and_invalid_targets() {
        let mut database = Database::open_in_memory().expect("database opens");
        let work = task(&mut database, "Work");
        assert!(matches!(
            database.add_gate(work, GateKind::Pr, "https://github.com/o/r/issues/1"),
            Err(StorageError::InvalidGate(DomainError::InvalidGateUrl { .. }))
        ));
        assert!(matches!(
            database.add_gate(work, GateKind::Date, "2026-02-30"),
            Err(StorageError::InvalidGate(DomainError::InvalidGateDate { .. }))
        ));
        database
            .complete_planning_task(work, OpenSubtasks::Refuse, None)
            .expect("task completes");
        assert!(matches!(
            database.add_gate(work, GateKind::Date, "2026-11-01"),
            Err(StorageError::GateOnClosedTask { .. })
        ));
    }

    #[test]
    fn gate_short_ids_resolve_with_the_g_kind() {
        let mut database = Database::open_in_memory().expect("database opens");
        let work = task(&mut database, "Work");
        let gate = database
            .add_gate(work, GateKind::Run, "https://github.com/o/r/actions/runs/7")
            .expect("gate adds");
        let id = gate.id.to_string();
        let suffix = &id[id.len() - 8..];
        assert_eq!(
            database
                .resolve_record_id(&format!("g-{suffix}"), None)
                .expect("short ID resolves"),
            id
        );
        assert_eq!(
            database
                .resolve_record_id(suffix, Some(GateId::PREFIX))
                .expect("short ID resolves"),
            id
        );
    }
}
