use std::path::{Path, PathBuf};

/// Walk up from `start` looking for a directory containing .ctx/opavs/tasks.yaml.
/// This is the on-disk marker for "an opavs-enabled repo".
pub fn resolve_repo_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join(".ctx").join("opavs").join("tasks.yaml").is_file() {
            return Some(dir);
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
}
