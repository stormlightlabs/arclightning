use std::collections::{HashMap, HashSet, VecDeque};

use serde::Serialize;

use super::{DomainError, TaskId};

/// Where a blocker sits in the parent chain of the task it blocks.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kinship {
    /// The blocker is a parent, grandparent, or further ancestor of the task.
    Ancestor,
    /// The blocker is a child, grandchild, or further descendant of the task.
    Descendant,
}

impl Kinship {
    /// The lowercase name used in messages.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ancestor => "ancestor",
            Self::Descendant => "descendant",
        }
    }
}

/// A blocker edge, or a set of them, that breaks the blocking rules.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EdgeViolation {
    /// Blocker edges form a cycle.
    Cycle {
        /// The tasks along the cycle, closed by repeating the first task.
        /// Each task is blocked by the next.
        path: Vec<TaskId>,
    },
    /// A task is blocked by its own ancestor or descendant.
    Ancestry {
        /// The edge that joins the two tasks.
        edge: TaskDependency,
        /// Where the blocker sits relative to the task.
        kinship: Kinship,
        /// The parent chain from the descendant up to the ancestor.
        chain: Vec<TaskId>,
    },
}

impl EdgeViolation {
    /// Whether `edge` is one of the edges that forms this violation.
    pub fn involves(&self, edge: &TaskDependency) -> bool {
        match self {
            Self::Cycle { path } => path
                .windows(2)
                .any(|pair| pair[0] == edge.task_id && pair[1] == edge.blocker_id),
            Self::Ancestry { edge: own, .. } => own == edge,
        }
    }

    /// Whether the parent chain of an ancestry violation steps from `child` up
    /// to `parent`. A reparent that sets that link causes every violation that
    /// crosses it; a violation that does not cross it existed before the move.
    /// A cycle never crosses a parent link.
    pub fn crosses_parent_link(&self, child: TaskId, parent: TaskId) -> bool {
        match self {
            Self::Cycle { .. } => false,
            Self::Ancestry { chain, .. } => chain.windows(2).any(|pair| pair[0] == child && pair[1] == parent),
        }
    }

    /// Rotate a cycle so its path starts with `edge`. Ancestry violations, and
    /// cycles that do not contain `edge`, are returned unchanged.
    pub fn starting_at(self, edge: &TaskDependency) -> Self {
        let Self::Cycle { path } = self else {
            return self;
        };
        let open = &path[..path.len().saturating_sub(1)];
        let start = (0..open.len())
            .find(|&index| open[index] == edge.task_id && open[(index + 1) % open.len()] == edge.blocker_id);
        let Some(start) = start else {
            return Self::Cycle { path };
        };
        let mut rotated: Vec<TaskId> = open[start..].iter().chain(&open[..start]).copied().collect();
        rotated.push(edge.task_id);
        Self::Cycle { path: rotated }
    }

    /// The domain error that reports this violation.
    pub fn to_error(&self) -> DomainError {
        match self {
            Self::Cycle { path } => DomainError::BlockerCycle { path: path.iter().map(ToString::to_string).collect() },
            Self::Ancestry { edge, kinship, chain } => DomainError::BlockerAncestry {
                task: edge.task_id.to_string(),
                blocker: edge.blocker_id.to_string(),
                kinship: kinship.as_str(),
                chain: chain.iter().map(ToString::to_string).collect(),
            },
        }
    }
}

/// A directed blocking relationship between two tasks.
///
/// `task_id` is the task that cannot proceed, and `blocker_id` is the task
/// whose completion satisfies the relationship.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct TaskDependency {
    /// The task whose readiness is gated.
    pub task_id: TaskId,
    /// The task whose completed status satisfies the dependency.
    pub blocker_id: TaskId,
}

impl TaskDependency {
    /// Construct a dependency after rejecting a task that blocks itself.
    pub fn new(task_id: TaskId, blocker_id: TaskId) -> Result<Self, DomainError> {
        if task_id == blocker_id {
            return Err(DomainError::SelfDependency { task: task_id.to_string() });
        }
        Ok(Self { task_id, blocker_id })
    }
}

/// Check a whole task graph against the blocker edge rules and report every
/// violation in it.
///
/// `parents` maps each task that has a parent to that parent, and `edges`
/// holds every blocker edge. The result follows edge order. It holds one
/// [`EdgeViolation::Cycle`] for each distinct cycle found from an edge, so
/// every edge on a cycle appears in at least one, and one
/// [`EdgeViolation::Ancestry`] for each edge that joins a task to its own
/// ancestor or descendant. An empty result means the graph is valid.
pub fn validate_blocker_edges(parents: &HashMap<TaskId, TaskId>, edges: &[TaskDependency]) -> Vec<EdgeViolation> {
    let mut blockers: HashMap<TaskId, Vec<TaskId>> = HashMap::new();
    for edge in edges {
        blockers.entry(edge.task_id).or_default().push(edge.blocker_id);
    }
    let mut violations = Vec::new();
    let mut seen_cycles = HashSet::new();
    for edge in edges {
        if let Some(path) = blocker_path(&blockers, edge.blocker_id, edge.task_id) {
            let mut cycle = vec![edge.task_id];
            cycle.extend(path);
            let mut key = cycle[..cycle.len() - 1].to_vec();
            let lowest = (0..key.len()).min_by_key(|&index| key[index]).unwrap_or(0);
            key.rotate_left(lowest);
            if seen_cycles.insert(key) {
                violations.push(EdgeViolation::Cycle { path: cycle });
            }
        }
        if let Some(chain) = parent_chain(parents, edge.task_id, edge.blocker_id) {
            violations.push(EdgeViolation::Ancestry { edge: *edge, kinship: Kinship::Ancestor, chain });
        } else if let Some(chain) = parent_chain(parents, edge.blocker_id, edge.task_id) {
            violations.push(EdgeViolation::Ancestry { edge: *edge, kinship: Kinship::Descendant, chain });
        }
    }
    violations
}

/// The shortest path of blocker edges from `from` to `to`, both included.
fn blocker_path(blockers: &HashMap<TaskId, Vec<TaskId>>, from: TaskId, to: TaskId) -> Option<Vec<TaskId>> {
    let mut previous: HashMap<TaskId, TaskId> = HashMap::new();
    let mut queue = VecDeque::from([from]);
    let mut visited = HashSet::from([from]);
    while let Some(current) = queue.pop_front() {
        if current == to {
            let mut path = vec![current];
            let mut step = current;
            while let Some(before) = previous.get(&step) {
                path.push(*before);
                step = *before;
            }
            path.reverse();
            return Some(path);
        }
        for next in blockers.get(&current).into_iter().flatten() {
            if visited.insert(*next) {
                previous.insert(*next, current);
                queue.push_back(*next);
            }
        }
    }
    None
}

/// The parent chain from `descendant` up to `ancestor`, both included, when
/// `ancestor` is a proper ancestor of `descendant`.
fn parent_chain(parents: &HashMap<TaskId, TaskId>, descendant: TaskId, ancestor: TaskId) -> Option<Vec<TaskId>> {
    let mut chain = vec![descendant];
    let mut visited = HashSet::from([descendant]);
    let mut current = descendant;
    while let Some(parent) = parents.get(&current) {
        if !visited.insert(*parent) {
            return None;
        }
        chain.push(*parent);
        if *parent == ancestor {
            return Some(chain);
        }
        current = *parent;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(task: TaskId, blocker: TaskId) -> TaskDependency {
        TaskDependency::new(task, blocker).expect("edge is not a self-dependency")
    }

    #[test]
    fn valid_graph_reports_nothing() {
        let (parent, child, other) = (TaskId::new(), TaskId::new(), TaskId::new());
        let parents = HashMap::from([(child, parent)]);
        let edges = [edge(child, other), edge(parent, other)];
        assert!(validate_blocker_edges(&parents, &edges).is_empty());
    }

    #[test]
    fn cycle_lists_every_task_along_it() {
        let (a, b, c) = (TaskId::new(), TaskId::new(), TaskId::new());
        let edges = [edge(a, b), edge(b, c), edge(c, a)];
        let violations = validate_blocker_edges(&HashMap::new(), &edges);
        assert_eq!(violations, vec![EdgeViolation::Cycle { path: vec![a, b, c, a] }]);

        let closing = edge(c, a);
        assert!(violations[0].involves(&closing));
        let rotated = violations[0].clone().starting_at(&closing);
        assert_eq!(rotated, EdgeViolation::Cycle { path: vec![c, a, b, c] });
        assert_eq!(
            rotated.to_error().to_string(),
            format!("blocker edges would form a cycle: {c} -> {a} -> {b} -> {c}; each task is blocked by the next")
        );
    }

    #[test]
    fn ancestor_edge_names_the_parent_chain() {
        let (root, middle, leaf) = (TaskId::new(), TaskId::new(), TaskId::new());
        let parents = HashMap::from([(middle, root), (leaf, middle)]);
        let violations = validate_blocker_edges(&parents, &[edge(leaf, root)]);
        assert_eq!(
            violations,
            vec![EdgeViolation::Ancestry {
                edge: edge(leaf, root),
                kinship: Kinship::Ancestor,
                chain: vec![leaf, middle, root],
            }]
        );
        assert_eq!(
            violations[0].to_error().to_string(),
            format!(
                "task `{leaf}` cannot be blocked by `{root}`, its ancestor; parent chain: {leaf} -> {middle} -> {root}"
            )
        );
        assert!(violations[0].crosses_parent_link(middle, root));
        assert!(violations[0].crosses_parent_link(leaf, middle));
        assert!(!violations[0].crosses_parent_link(root, middle));
    }

    #[test]
    fn descendant_edge_names_the_parent_chain() {
        let (root, middle, leaf) = (TaskId::new(), TaskId::new(), TaskId::new());
        let parents = HashMap::from([(middle, root), (leaf, middle)]);
        let violations = validate_blocker_edges(&parents, &[edge(root, leaf)]);
        assert_eq!(
            violations,
            vec![EdgeViolation::Ancestry {
                edge: edge(root, leaf),
                kinship: Kinship::Descendant,
                chain: vec![leaf, middle, root],
            }]
        );
    }

    #[test]
    fn every_violation_in_the_graph_is_reported() {
        let (a, b, c, d) = (TaskId::new(), TaskId::new(), TaskId::new(), TaskId::new());
        let (parent, child) = (TaskId::new(), TaskId::new());
        let parents = HashMap::from([(child, parent)]);
        let edges = [edge(a, b), edge(b, a), edge(c, d), edge(d, c), edge(child, parent)];
        let violations = validate_blocker_edges(&parents, &edges);
        assert_eq!(violations.len(), 3);
        assert!(violations[0].involves(&edge(a, b)) && violations[0].involves(&edge(b, a)));
        assert!(violations[1].involves(&edge(c, d)) && violations[1].involves(&edge(d, c)));
        assert!(violations[2].involves(&edge(child, parent)));
        assert!(!violations[2].involves(&edge(a, b)));
    }
}
