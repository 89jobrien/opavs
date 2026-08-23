use std::path::{Path, PathBuf};

/// Walk up from `start` looking for a directory containing .ctx/opavs/tasks.yaml.
/// This is the on-disk marker for "an opavs-enabled repo".
///
/// Stops at the first git repo/worktree boundary (a directory containing `.git`,
/// file or dir) rather than climbing past it. `.ctx/` is gitignored in most repo
/// conventions, so a linked git worktree (e.g. `.worktrees/issue-42/`, which lives
/// *inside* the main repo's directory tree) never gets its own copy of
/// `.ctx/opavs/tasks.yaml`. Without this boundary check, resolving from inside such
/// a worktree would silently climb past the worktree's own `.git` and pick up the
/// unrelated main repo's phase state -- gating (or ungating) commits based on a
/// phase the worktree's own work has no connection to. A worktree that wants opavs
/// gating needs its own `.ctx/opavs/tasks.yaml` (e.g. copied or symlinked in
/// explicitly); it does not inherit one from a parent directory.
pub fn resolve_repo_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join(".ctx").join("opavs").join("tasks.yaml").is_file() {
            return Some(dir);
        }
        if dir.join(".git").exists() {
            return None;
        }
        if !dir.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_root_from_nested_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let ctx = tmp.path().join(".ctx").join("opavs");
        fs::create_dir_all(&ctx).expect("mkdir");
        fs::write(ctx.join("tasks.yaml"), "tasks: []").expect("write");
        let nested = tmp.path().join("src").join("deep");
        fs::create_dir_all(&nested).expect("mkdir nested");

        let found = resolve_repo_root(&nested).expect("should find root");
        assert_eq!(found, tmp.path());
    }

    #[test]
    fn returns_none_when_no_marker_present() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(resolve_repo_root(tmp.path()).is_none());
    }

    #[test]
    fn does_not_climb_past_a_git_worktree_into_the_main_repo() {
        let tmp = tempfile::tempdir().expect("tempdir");

        // Main repo root: has its own opavs marker.
        let main_ctx = tmp.path().join(".ctx").join("opavs");
        fs::create_dir_all(&main_ctx).expect("mkdir main .ctx/opavs");
        fs::write(main_ctx.join("tasks.yaml"), "tasks: []").expect("write main tasks.yaml");
        fs::create_dir_all(tmp.path().join(".git")).expect("mkdir main .git");

        // A linked worktree living inside the main repo's tree, with its own .git
        // (a file, as real worktrees have) but no .ctx/opavs/tasks.yaml of its own.
        let worktree = tmp.path().join(".worktrees").join("issue-42");
        fs::create_dir_all(&worktree).expect("mkdir worktree");
        fs::write(
            worktree.join(".git"),
            "gitdir: ../../.git/worktrees/issue-42",
        )
        .expect("write worktree .git file");
        let nested = worktree.join("src").join("deep");
        fs::create_dir_all(&nested).expect("mkdir nested in worktree");

        // Resolving from inside the worktree must NOT find the main repo's marker.
        assert!(
            resolve_repo_root(&nested).is_none(),
            "must not silently inherit the main repo's opavs marker from inside a worktree"
        );
    }

    #[test]
    fn finds_worktrees_own_marker_when_present() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let worktree = tmp.path().join(".worktrees").join("issue-42");
        let ctx = worktree.join(".ctx").join("opavs");
        fs::create_dir_all(&ctx).expect("mkdir worktree .ctx/opavs");
        fs::write(ctx.join("tasks.yaml"), "tasks: []").expect("write worktree tasks.yaml");
        fs::write(
            worktree.join(".git"),
            "gitdir: ../../.git/worktrees/issue-42",
        )
        .expect("write worktree .git file");
        let nested = worktree.join("src");
        fs::create_dir_all(&nested).expect("mkdir nested in worktree");

        let found = resolve_repo_root(&nested).expect("should find the worktree's own marker");
        assert_eq!(found, worktree);
    }
}
