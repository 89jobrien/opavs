use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Orient,
    Plan,
    Act,
    Verify,
    Ship,
}

impl Phase {
    pub fn parse(s: &str) -> Result<Phase> {
        match s {
            "ORIENT" => Ok(Phase::Orient),
            "PLAN" => Ok(Phase::Plan),
            "ACT" => Ok(Phase::Act),
            "VERIFY" => Ok(Phase::Verify),
            "SHIP" => Ok(Phase::Ship),
            other => bail!("invalid phase: {other} (expected ORIENT|PLAN|ACT|VERIFY|SHIP)"),
        }
    }
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Phase::Orient => "ORIENT",
            Phase::Plan => "PLAN",
            Phase::Act => "ACT",
            Phase::Verify => "VERIFY",
            Phase::Ship => "SHIP",
        };
        write!(f, "{s}")
    }
}

/// Port: persist and retrieve the current opavs phase for a repo.
pub trait PhaseStore {
    fn get(&self) -> Result<Phase>;
    fn set(&self, phase: Phase) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Todo,
    InProgress,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_status")]
    pub status: TaskStatus,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

fn default_status() -> TaskStatus {
    TaskStatus::Todo
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskGraph {
    #[serde(default)]
    pub tasks: Vec<Task>,
}

/// Port: persist and retrieve the task graph for a repo.
pub trait TaskStore {
    fn load(&self) -> Result<TaskGraph>;
    fn save(&self, graph: &TaskGraph) -> Result<()>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum GraphError {
    UnknownDependency { task: String, depends_on: String },
    Cycle(Vec<String>),
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GraphError::UnknownDependency { task, depends_on } => {
                write!(f, "task '{task}' depends on unknown task '{depends_on}'")
            }
            GraphError::Cycle(path) => write!(f, "dependency cycle: {}", path.join(" -> ")),
        }
    }
}

/// Pure domain logic: validate a task graph (unknown deps, cycles).
pub fn validate(graph: &TaskGraph) -> Result<(), GraphError> {
    let ids: std::collections::HashSet<&str> = graph.tasks.iter().map(|t| t.id.as_str()).collect();

    for task in &graph.tasks {
        for dep in &task.depends_on {
            if !ids.contains(dep.as_str()) {
                return Err(GraphError::UnknownDependency {
                    task: task.id.clone(),
                    depends_on: dep.clone(),
                });
            }
        }
    }

    // DFS cycle detection.
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Unvisited,
        Visiting,
        Done,
    }
    use std::collections::HashMap;
    let mut marks: HashMap<&str, Mark> = ids.iter().map(|id| (*id, Mark::Unvisited)).collect();
    let by_id: HashMap<&str, &Task> = graph.tasks.iter().map(|t| (t.id.as_str(), t)).collect();

    fn visit<'a>(
        id: &'a str,
        by_id: &HashMap<&'a str, &'a Task>,
        marks: &mut HashMap<&'a str, Mark>,
        path: &mut Vec<String>,
    ) -> Result<(), GraphError> {
        match marks.get(id) {
            Some(Mark::Done) => return Ok(()),
            Some(Mark::Visiting) => {
                path.push(id.to_string());
                return Err(GraphError::Cycle(path.clone()));
            }
            _ => {}
        }
        marks.insert(id, Mark::Visiting);
        path.push(id.to_string());
        if let Some(task) = by_id.get(id) {
            for dep in &task.depends_on {
                visit(dep, by_id, marks, path)?;
            }
        }
        path.pop();
        marks.insert(id, Mark::Done);
        Ok(())
    }

    for id in ids.iter() {
        let mut path = Vec::new();
        visit(id, &by_id, &mut marks, &mut path)?;
    }

    Ok(())
}

/// Pure domain logic: a task is runnable when it is not done and every
/// dependency has status `Done`.
pub fn runnable_tasks(graph: &TaskGraph) -> Vec<&Task> {
    let done: std::collections::HashSet<&str> = graph
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Done)
        .map(|t| t.id.as_str())
        .collect();

    graph
        .tasks
        .iter()
        .filter(|t| t.status != TaskStatus::Done)
        .filter(|t| t.depends_on.iter().all(|d| done.contains(d.as_str())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, status: TaskStatus, deps: &[&str]) -> Task {
        Task {
            id: id.to_string(),
            description: String::new(),
            status,
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn phase_parse_roundtrips_all_variants() {
        for s in ["ORIENT", "PLAN", "ACT", "VERIFY", "SHIP"] {
            assert_eq!(Phase::parse(s).unwrap().to_string(), s);
        }
    }

    #[test]
    fn phase_parse_rejects_unknown() {
        assert!(Phase::parse("BOGUS").is_err());
    }

    #[test]
    fn runnable_tasks_excludes_done_and_blocked() {
        let graph = TaskGraph {
            tasks: vec![
                task("a", TaskStatus::Done, &[]),
                task("b", TaskStatus::Todo, &["a"]),
                task("c", TaskStatus::Todo, &["b"]),
                task("d", TaskStatus::InProgress, &[]),
            ],
        };
        let runnable: Vec<&str> = runnable_tasks(&graph)
            .iter()
            .map(|t| t.id.as_str())
            .collect();
        assert_eq!(runnable, vec!["b", "d"]);
    }

    #[test]
    fn runnable_tasks_empty_graph_is_empty() {
        let graph = TaskGraph::default();
        assert!(runnable_tasks(&graph).is_empty());
    }

    #[test]
    fn validate_detects_unknown_dependency() {
        let graph = TaskGraph {
            tasks: vec![task("a", TaskStatus::Todo, &["ghost"])],
        };
        let err = validate(&graph).unwrap_err();
        assert_eq!(
            err,
            GraphError::UnknownDependency {
                task: "a".into(),
                depends_on: "ghost".into()
            }
        );
    }

    #[test]
    fn validate_detects_direct_cycle() {
        let graph = TaskGraph {
            tasks: vec![
                task("a", TaskStatus::Todo, &["b"]),
                task("b", TaskStatus::Todo, &["a"]),
            ],
        };
        assert!(matches!(validate(&graph), Err(GraphError::Cycle(_))));
    }

    #[test]
    fn validate_accepts_diamond_dependency() {
        let graph = TaskGraph {
            tasks: vec![
                task("a", TaskStatus::Done, &[]),
                task("b", TaskStatus::Todo, &["a"]),
                task("c", TaskStatus::Todo, &["a"]),
                task("d", TaskStatus::Todo, &["b", "c"]),
            ],
        };
        assert!(validate(&graph).is_ok());
    }
}
