use crate::doctor::{ArtifactReader, IgnoreQuery};
use crate::domain::{Phase, PhaseStore, TaskGraph, TaskStore};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Filesystem-backed reader for read-only doctor checks.
#[derive(Debug, Clone, Copy, Default)]
pub struct FsArtifactReader;

impl ArtifactReader for FsArtifactReader {
    fn read(&self, path: &Path) -> Result<Option<String>> {
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(std::fs::read_to_string(path)?))
    }
}

/// Git-backed adapter for read-only ignore decisions.
#[derive(Debug, Clone, Copy, Default)]
pub struct GitIgnoreQuery;

impl IgnoreQuery for GitIgnoreQuery {
    fn is_ignored(&self, repo_root: &Path, relative_path: &Path) -> Result<Option<bool>> {
        if !repo_root.join(".git").exists() {
            return Ok(None);
        }
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["check-ignore", "--quiet", "--no-index"])
            .arg(relative_path)
            .output()
            .with_context(|| format!("run git check-ignore in {}", repo_root.display()))?;
        let ignored = match output.status.code() {
            Some(0) => true,
            Some(1) => false,
            code => bail!(
                "git check-ignore failed in {} with status {:?}: {}",
                repo_root.display(),
                code,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        };
        Ok(Some(ignored))
    }
}

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
        // TODO(atomic-state): Replace direct state overwrites with atomic write-and-rename.
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

    fn git(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .status()
            .expect("run git");
        assert!(status.success(), "git command failed: {args:?}");
    }

    #[test]
    fn fs_artifact_reader_reads_present_and_missing_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let present = tmp.path().join("present.txt");
        std::fs::write(&present, "present\n").expect("write text");

        assert_eq!(
            FsArtifactReader.read(&present).expect("read present"),
            Some("present\n".to_string())
        );
        assert_eq!(
            FsArtifactReader
                .read(&tmp.path().join("missing.txt"))
                .expect("read missing"),
            None
        );
    }

    #[test]
    fn fs_artifact_reader_rejects_non_utf8_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("binary");
        std::fs::write(&path, [0xff, 0xfe]).expect("write binary");

        assert!(FsArtifactReader.read(&path).is_err());
    }

    #[test]
    fn git_ignore_query_reports_true_and_false() {
        let tmp = tempfile::tempdir().expect("tempdir");
        git(tmp.path(), &["init", "--quiet"]);
        std::fs::write(tmp.path().join(".gitignore"), "ignored.txt\n").expect("write gitignore");

        assert_eq!(
            GitIgnoreQuery
                .is_ignored(tmp.path(), Path::new("ignored.txt"))
                .expect("ignored query"),
            Some(true)
        );
        assert_eq!(
            GitIgnoreQuery
                .is_ignored(tmp.path(), Path::new("visible.txt"))
                .expect("visible query"),
            Some(false)
        );
    }

    #[test]
    fn git_ignore_query_propagates_git_errors() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join(".git"), "gitdir: /missing/opavs-git-dir\n")
            .expect("write invalid git file");

        let error = GitIgnoreQuery
            .is_ignored(tmp.path(), Path::new("anything"))
            .expect_err("invalid Git metadata should fail");
        assert!(error.to_string().contains("git check-ignore"));
    }

    #[test]
    fn git_ignore_query_supports_worktree_git_file() {
        let root = tempfile::tempdir().expect("root tempdir");
        let repo = root.path().join("repo");
        let worktree = root.path().join("worktree");
        std::fs::create_dir(&repo).expect("create repo");
        git(&repo, &["init", "--quiet"]);
        std::fs::write(repo.join(".gitignore"), "ignored.txt\n").expect("write gitignore");
        std::fs::write(repo.join("tracked.txt"), "tracked\n").expect("write tracked file");
        git(&repo, &["add", "."]);
        git(
            &repo,
            &[
                "-c",
                "user.name=OPAVS Test",
                "-c",
                "user.email=opavs@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "fixture",
            ],
        );
        git(
            &repo,
            &[
                "worktree",
                "add",
                "--quiet",
                "--detach",
                worktree.to_str().expect("UTF-8 path"),
            ],
        );

        assert!(worktree.join(".git").is_file());
        assert_eq!(
            GitIgnoreQuery
                .is_ignored(&worktree, Path::new("ignored.txt"))
                .expect("worktree ignore query"),
            Some(true)
        );
    }

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

    #[test]
    fn task_store_malformed_yaml_errors() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = FsTaskStore::new(tmp.path());
        let path = tmp.path().join(".ctx").join("opavs").join("tasks.yaml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not: [valid, task, graph").unwrap();
        assert!(store.load().is_err());
    }

    #[test]
    fn fs_phase_store_satisfies_port_contract() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::domain::conformance::assert_phase_store_contract(FsPhaseStore::new(tmp.path()));
    }

    #[test]
    fn fs_task_store_satisfies_port_contract() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::domain::conformance::assert_task_store_contract(FsTaskStore::new(tmp.path()));
    }
}
