use crate::domain::TaskGraph;
use anyhow::Result;
use std::path::Path;

/// Parse a GODMODE.tasks.yaml file (same schema as our own TaskGraph) from disk.
pub fn read_external_graph(path: &Path) -> Result<TaskGraph> {
    let contents = std::fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&contents)?)
}

/// Pure domain logic: merge `incoming` into `base`, upserting by task id.
/// A task present in both keeps `base`'s status (in-progress work isn't
/// silently reset by an import) but takes `incoming`'s description/depends_on.
pub fn merge(base: &TaskGraph, incoming: &TaskGraph) -> TaskGraph {
    let mut merged = base.clone();

    for incoming_task in &incoming.tasks {
        if let Some(existing) = merged.tasks.iter_mut().find(|t| t.id == incoming_task.id) {
            existing.description = incoming_task.description.clone();
            existing.depends_on = incoming_task.depends_on.clone();
        } else {
            merged.tasks.push(incoming_task.clone());
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Task, TaskStatus};

    fn task(id: &str, status: TaskStatus, desc: &str, deps: &[&str]) -> Task {
        Task {
            id: id.to_string(),
            description: desc.to_string(),
            status,
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn merge_adds_new_tasks() {
        let base = TaskGraph { tasks: vec![] };
        let incoming = TaskGraph {
            tasks: vec![task("a", TaskStatus::Todo, "do a", &[])],
        };
        let merged = merge(&base, &incoming);
        assert_eq!(merged.tasks.len(), 1);
        assert_eq!(merged.tasks[0].id, "a");
    }

    #[test]
    fn merge_preserves_existing_status_on_upsert() {
        let base = TaskGraph {
            tasks: vec![task("a", TaskStatus::InProgress, "old desc", &[])],
        };
        let incoming = TaskGraph {
            tasks: vec![task("a", TaskStatus::Todo, "new desc", &["b"])],
        };
        let merged = merge(&base, &incoming);
        assert_eq!(merged.tasks.len(), 1);
        assert_eq!(merged.tasks[0].status, TaskStatus::InProgress);
        assert_eq!(merged.tasks[0].description, "new desc");
        assert_eq!(merged.tasks[0].depends_on, vec!["b".to_string()]);
    }

    #[test]
    fn merge_leaves_untouched_base_tasks_alone() {
        let base = TaskGraph {
            tasks: vec![task("a", TaskStatus::Done, "a", &[])],
        };
        let incoming = TaskGraph {
            tasks: vec![task("b", TaskStatus::Todo, "b", &[])],
        };
        let merged = merge(&base, &incoming);
        assert_eq!(merged.tasks.len(), 2);
        assert!(
            merged
                .tasks
                .iter()
                .any(|t| t.id == "a" && t.status == TaskStatus::Done)
        );
    }

    #[test]
    fn read_external_graph_parses_godmode_yaml() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("GODMODE.tasks.yaml");
        std::fs::write(
            &path,
            "tasks:\n  - id: a\n    description: do a\n    status: todo\n    depends_on: []\n",
        )
        .unwrap();
        let graph = read_external_graph(&path).unwrap();
        assert_eq!(graph.tasks.len(), 1);
        assert_eq!(graph.tasks[0].id, "a");
    }
}
