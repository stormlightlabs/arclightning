use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

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
        /// Each task is blocked by the next. [`validate_blocker_edges`]
        /// starts it at the smallest task ID, so the same cycle always reads
        /// the same; [`EdgeViolation::starting_at`] rotates it to an edge.
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

/// Describes a violation that already exists in the graph, with the same
/// path or chain [`EdgeViolation::to_error`] reports when refusing an edge.
impl fmt::Display for EdgeViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let join = |ids: &[TaskId]| ids.iter().map(ToString::to_string).collect::<Vec<_>>().join(" -> ");
        match self {
            Self::Cycle { path } => write!(f, "blocker cycle: {}; each task is blocked by the next", join(path)),
            Self::Ancestry { edge, kinship, chain } => write!(
                f,
                "task `{}` is blocked by `{}`, its {}; parent chain: {}",
                edge.task_id,
                edge.blocker_id,
                kinship.as_str(),
                join(chain)
            ),
        }
    }
}

/// A directed blocking relationship between two tasks.
///
/// `task_id` is the task that cannot proceed, and `blocker_id` is the task
/// whose completion satisfies the relationship. [`TaskDependency::new`]
/// refuses a task that blocks itself, but an edge read from a hand-edited
/// database is built as stored and can be one.
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
/// holds every blocker edge. The result follows edge order. Each edge on a
/// cycle that no earlier reported cycle holds adds an
/// [`EdgeViolation::Cycle`] for the shortest cycle through it, starting at
/// the cycle's smallest task ID whatever the edge order. So every edge on a
/// cycle appears in at least one, and the first edge on a cycle gets the
/// shortest one through it; a caller refusing new edges lists them first.
/// Each edge that joins a task to its own ancestor or descendant adds one
/// [`EdgeViolation::Ancestry`]. An empty result means the graph is valid.
pub fn validate_blocker_edges(parents: &HashMap<TaskId, TaskId>, edges: &[TaskDependency]) -> Vec<EdgeViolation> {
    let mut blockers: HashMap<TaskId, Vec<TaskId>> = HashMap::new();
    for edge in edges {
        blockers.entry(edge.task_id).or_default().push(edge.blocker_id);
    }
    // An edge lies on a cycle exactly when both its ends share a strongly
    // connected component, and every path from its blocker back to its task
    // stays inside that component. One linear pass finds the components, so
    // the shortest-path search runs only for edges on a cycle. Searching from
    // every edge would cost O(E * (V + E)) on each read, which a long acyclic
    // blocker chain makes quadratic.
    let component = strong_components(&blockers);
    let mut violations = Vec::new();
    // Edges on a cycle reported so far. An edge already on one needs no
    // search of its own, so one long stored cycle costs a single search.
    let mut covered: HashSet<(TaskId, TaskId)> = HashSet::new();
    for edge in edges {
        let on_cycle = component
            .get(&edge.task_id)
            .is_some_and(|own| component.get(&edge.blocker_id) == Some(own))
            && !covered.contains(&(edge.task_id, edge.blocker_id));
        if let Some(path) = on_cycle
            .then(|| blocker_path(&blockers, edge.blocker_id, edge.task_id))
            .flatten()
        {
            // The cycle holds `edge`, which no reported cycle holds, so it is
            // new.
            let mut cycle = vec![edge.task_id];
            cycle.extend(&path[..path.len() - 1]);
            let lowest = (0..cycle.len()).min_by_key(|&index| cycle[index]).unwrap_or(0);
            cycle.rotate_left(lowest);
            cycle.push(cycle[0]);
            covered.extend(cycle.windows(2).map(|pair| (pair[0], pair[1])));
            violations.push(EdgeViolation::Cycle { path: cycle });
        }
        if let Some(chain) = parent_chain(parents, edge.task_id, edge.blocker_id) {
            violations.push(EdgeViolation::Ancestry { edge: *edge, kinship: Kinship::Ancestor, chain });
        } else if let Some(chain) = parent_chain(parents, edge.blocker_id, edge.task_id) {
            violations.push(EdgeViolation::Ancestry { edge: *edge, kinship: Kinship::Descendant, chain });
        }
    }
    violations
}

/// Map each task in `blockers` to the index of its strongly connected
/// component, using Tarjan's algorithm.
///
/// The depth-first search keeps its own stack of frames instead of
/// recursing, so a blocker chain thousands of tasks deep cannot overflow the
/// thread's stack.
fn strong_components(blockers: &HashMap<TaskId, Vec<TaskId>>) -> HashMap<TaskId, usize> {
    let mut order: HashMap<TaskId, usize> = HashMap::new();
    let mut low: Vec<usize> = Vec::new();
    let mut on_stack: Vec<bool> = Vec::new();
    let mut stack: Vec<TaskId> = Vec::new();
    let mut component: HashMap<TaskId, usize> = HashMap::new();
    let mut count = 0;
    let none: &[TaskId] = &[];
    for &root in blockers.keys() {
        if order.contains_key(&root) {
            continue;
        }
        // Each frame holds a task and how many of its blockers it has visited.
        let mut frames: Vec<(TaskId, usize)> = Vec::new();
        let mut arrive = Some(root);
        loop {
            if let Some(task) = arrive.take() {
                order.insert(task, low.len());
                low.push(low.len());
                on_stack.push(true);
                stack.push(task);
                frames.push((task, 0));
            }
            let Some(frame) = frames.last_mut() else {
                break;
            };
            let (task, next) = *frame;
            let at = order[&task];
            let next_blockers = blockers.get(&task).map_or(none, Vec::as_slice);
            if let Some(&blocker) = next_blockers.get(next) {
                frame.1 += 1;
                match order.get(&blocker) {
                    None => arrive = Some(blocker),
                    Some(&seen) if on_stack[seen] => low[at] = low[at].min(seen),
                    Some(_) => {}
                }
                continue;
            }
            frames.pop();
            if let Some(&(parent, _)) = frames.last() {
                let parent_at = order[&parent];
                low[parent_at] = low[parent_at].min(low[at]);
            }
            if low[at] == at {
                while let Some(member) = stack.pop() {
                    on_stack[order[&member]] = false;
                    component.insert(member, count);
                    if member == task {
                        break;
                    }
                }
                count += 1;
            }
        }
    }
    component
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
        let EdgeViolation::Cycle { path } = &violations[0] else {
            panic!("expected a cycle, got {violations:?}");
        };
        let start = [a, b, c].into_iter().min().expect("three tasks");
        let expected = if start == a {
            vec![a, b, c, a]
        } else if start == b {
            vec![b, c, a, b]
        } else {
            vec![c, a, b, c]
        };
        assert_eq!(violations.len(), 1);
        assert_eq!(path, &expected);

        let closing = edge(c, a);
        assert!(violations[0].involves(&closing));
        let rotated = violations[0].clone().starting_at(&closing);
        assert_eq!(rotated, EdgeViolation::Cycle { path: vec![c, a, b, c] });
        assert_eq!(
            rotated.to_error().to_string(),
            format!("blocker edges would form a cycle: {c} -> {a} -> {b} -> {c}; each task is blocked by the next")
        );
        assert_eq!(
            rotated.to_string(),
            format!("blocker cycle: {c} -> {a} -> {b} -> {c}; each task is blocked by the next")
        );
    }

    #[test]
    fn cycle_starts_at_its_smallest_task_whatever_the_edge_order() {
        let mut ids = [TaskId::new(), TaskId::new(), TaskId::new()];
        ids.sort();
        let [low, mid, high] = ids;
        let expected = format!("blocker cycle: {low} -> {high} -> {mid} -> {low}; each task is blocked by the next");
        let (first, second, third) = (edge(low, high), edge(high, mid), edge(mid, low));
        for edges in [[first, second, third], [second, third, first], [third, first, second]] {
            let violations = validate_blocker_edges(&HashMap::new(), &edges);
            assert_eq!(
                violations,
                vec![EdgeViolation::Cycle { path: vec![low, high, mid, low] }]
            );
            assert_eq!(violations[0].to_string(), expected);
        }
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
        assert_eq!(
            violations[0].to_string(),
            format!("task `{leaf}` is blocked by `{root}`, its ancestor; parent chain: {leaf} -> {middle} -> {root}")
        );
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
    fn long_acyclic_chain_is_valid() {
        let tasks: Vec<TaskId> = (0..5_000).map(|_| TaskId::new()).collect();
        let edges: Vec<TaskDependency> = tasks.windows(2).map(|pair| edge(pair[0], pair[1])).collect();
        assert!(validate_blocker_edges(&HashMap::new(), &edges).is_empty());
    }

    #[test]
    fn chain_closed_into_a_cycle_reports_it_once() {
        let mut tasks: Vec<TaskId> = (0..300).map(|_| TaskId::new()).collect();
        tasks.sort();
        let mut edges: Vec<TaskDependency> = tasks.windows(2).map(|pair| edge(pair[0], pair[1])).collect();
        edges.push(edge(tasks[tasks.len() - 1], tasks[0]));
        let violations = validate_blocker_edges(&HashMap::new(), &edges);
        let mut path = tasks.clone();
        path.push(tasks[0]);
        assert_eq!(violations, vec![EdgeViolation::Cycle { path }]);
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

    /// The validator before it skipped covered edges: a shortest-cycle search
    /// from every edge, keeping each distinct cycle once.
    fn reference(parents: &HashMap<TaskId, TaskId>, edges: &[TaskDependency]) -> Vec<EdgeViolation> {
        let mut blockers: HashMap<TaskId, Vec<TaskId>> = HashMap::new();
        for edge in edges {
            blockers.entry(edge.task_id).or_default().push(edge.blocker_id);
        }
        let mut violations = Vec::new();
        let mut seen = HashSet::new();
        for edge in edges {
            if let Some(path) = blocker_path(&blockers, edge.blocker_id, edge.task_id) {
                let mut cycle = vec![edge.task_id];
                cycle.extend(&path[..path.len() - 1]);
                let lowest = (0..cycle.len()).min_by_key(|&index| cycle[index]).unwrap_or(0);
                cycle.rotate_left(lowest);
                if seen.insert(cycle.clone()) {
                    cycle.push(cycle[0]);
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

    /// A seeded xorshift generator, so a failing round can be replayed.
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % n as u64) as usize
        }
    }

    #[test]
    fn random_graphs_agree_with_the_search_from_every_edge() {
        let is_cycle = |violation: &&EdgeViolation| matches!(violation, EdgeViolation::Cycle { .. });
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for round in 0..20_000 {
            let n = 1 + rng.below(12);
            let ids: Vec<TaskId> = (0..n).map(|_| TaskId::new()).collect();
            // Parents point at earlier tasks, with an occasional parent cycle.
            let mut parents = HashMap::new();
            for child in 1..n {
                if rng.below(3) == 0 {
                    let parent = if rng.below(20) == 0 { rng.below(n) } else { rng.below(child) };
                    if parent != child {
                        parents.insert(ids[child], ids[parent]);
                    }
                }
            }
            // Random edges, with duplicates and an occasional self-loop, as a
            // hand-edited database can hold.
            let mut edges = Vec::new();
            for _ in 0..rng.below(n * 3 + 1) {
                let (task, blocker) = (rng.below(n), rng.below(n));
                if task != blocker || rng.below(15) == 0 {
                    edges.push(TaskDependency { task_id: ids[task], blocker_id: ids[blocker] });
                    if rng.below(10) == 0 {
                        edges.push(TaskDependency { task_id: ids[task], blocker_id: ids[blocker] });
                    }
                }
            }
            let expected = reference(&parents, &edges);
            let actual = validate_blocker_edges(&parents, &edges);
            let context = format!("round {round}: edges {edges:?} parents {parents:?}");

            let ancestry = |violations: &[EdgeViolation]| -> Vec<EdgeViolation> {
                violations.iter().filter(|v| !is_cycle(v)).cloned().collect()
            };
            assert_eq!(ancestry(&actual), ancestry(&expected), "{context}");
            let cycles: Vec<&EdgeViolation> = actual.iter().filter(is_cycle).collect();
            for (index, cycle) in cycles.iter().enumerate() {
                assert!(expected.contains(cycle), "{context}: {cycle:?} is not a shortest cycle");
                assert!(!cycles[..index].contains(cycle), "{context}: {cycle:?} repeats");
            }
            for edge in &edges {
                if expected.iter().filter(is_cycle).any(|cycle| cycle.involves(edge)) {
                    assert!(
                        cycles.iter().any(|cycle| cycle.involves(edge)),
                        "{context}: {edge:?} unreported"
                    );
                }
            }
            // The first edge, as a refused new edge is, gets the shortest
            // cycle through it.
            if let Some(first) = edges.first()
                && let Some(cycle) = expected.iter().find(is_cycle)
                && cycle.involves(first)
            {
                assert_eq!(cycles.first(), Some(&cycle), "{context}");
            }
        }
    }
}
