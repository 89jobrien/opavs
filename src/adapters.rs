use crate::domain::{Phase, PhaseStore, TaskGraph, TaskStore};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct FsPhaseStore {
    repo_root: PathBuf,
}

impl FsPhaseStore {
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
        }
    }

    fn phase_file(&self) -> PathBuf {
        self.repo_root.join(".ctx").join("opavs").join("phase")
    }
}

impl PhaseStore for FsPhaseStore {
    fn get(&self) -> Result<Phase> {
        let file = self.phase_file();
        if !file.exists() {
            return Ok(Phase::Orient);
        }
        let contents = std::fs::read_to_string(&file)?;
        Phase::parse(contents.trim())
    }

    fn set(&self, phase: Phase) -> Result<()> {
        let file = self.phase_file();
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file, phase.to_string())?;
        Ok(())
    }
}

pub struct FsTaskStore {
    path: PathBuf,
}

impl FsTaskStore {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            path: repo_root.join(".ctx").join("opavs").join("tasks.yaml"),
        }
    }
}

impl TaskStore for FsTaskStore {
    fn load(&self) -> Result<TaskGraph> {
        if !self.path.exists() {
            return Ok(TaskGraph::default());
        }
        let contents = std::fs::read_to_string(&self.path)?;
        Ok(serde_yaml::from_str(&contents)?)
    }

    fn save(&self, graph: &TaskGraph) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let yaml = serde_yaml::to_string(graph)?;
        std::fs::write(&self.path, yaml)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Task, TaskStatus};

    #[test]
    fn phase_store_defaults_to_orient_when_unset() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = FsPhaseStore::new(tmp.path());
        assert_eq!(store.get().unwrap(), Phase::Orient);
    }

    #[test]
    fn phase_store_roundtrips_set_get() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = FsPhaseStore::new(tmp.path());
        store.set(Phase::Act).unwrap();
        assert_eq!(store.get().unwrap(), Phase::Act);
    }

    #[test]
    fn task_store_roundtrips_graph() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = FsTaskStore::new(tmp.path());
        let graph = TaskGraph {
            tasks: vec![Task {
                id: "a".into(),
                description: "do the thing".into(),
                status: TaskStatus::Todo,
                depends_on: vec![],
            }],
        };
        store.save(&graph).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.tasks[0].id, "a");
    }

    #[test]
    fn task_store_missing_file_yields_empty_graph() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = FsTaskStore::new(tmp.path());
        assert!(store.load().unwrap().tasks.is_empty());
    }
}
