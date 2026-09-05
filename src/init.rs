use anyhow::{Result, bail};
use std::path::Path;

const TASKS_TEMPLATE: &str = "tasks: []\n";

const ACTIVE_CONTEXT_TEMPLATE: &str = "# Active Context\n\n\
    _Updated at the end of ACT or after SHIP. What's in flight, what's next._\n";

const PROGRESS_TEMPLATE: &str = "# Progress\n\n\
    _Milestones as they land. Append, don't rewrite history._\n";

const OPAVS_TEMPLATE: &str = "# OPAVS\n\n\
    This repo uses the opavs (Orient-Plan-Act-Verify-Ship) phase discipline.\n\n\
    - Task graph: `.ctx/opavs/tasks.yaml` (managed via `opavs tasks`)\n\
    - Memory bank: `.ctx/opavs/memory-bank/` (`active-context.md`, `progress.md`)\n\
    - Current phase: `.ctx/opavs/phase` (managed via `opavs phase`, not committed)\n";

const OPAVS_LINK: &str = "@OPAVS.md";

/// Scaffold the files opavs requires in a target repo: task graph, memory
/// bank, canonical instructions, and instruction-file links. Refuses to
/// overwrite generated state or an existing OPAVS.md.
pub fn scaffold(repo_root: &Path) -> Result<Vec<String>> {
    let mut created = Vec::new();

    let opavs_dir = repo_root.join(".ctx").join("opavs");
    let tasks_file = opavs_dir.join("tasks.yaml");
    let memory_bank = opavs_dir.join("memory-bank");
    let active_context = memory_bank.join("active-context.md");
    let progress = memory_bank.join("progress.md");
    let opavs = repo_root.join("OPAVS.md");
    let agents = repo_root.join("AGENTS.md");
    let claude = repo_root.join("CLAUDE.md");

    for existing in [&tasks_file, &active_context, &progress, &opavs] {
        if existing.exists() {
            bail!(
                "refusing to scaffold: {} already exists",
                existing.display()
            );
        }
    }

    std::fs::create_dir_all(&memory_bank)?;
    std::fs::write(&tasks_file, TASKS_TEMPLATE)?;
    created.push(tasks_file.display().to_string());
    std::fs::write(&active_context, ACTIVE_CONTEXT_TEMPLATE)?;
    created.push(active_context.display().to_string());
    std::fs::write(&progress, PROGRESS_TEMPLATE)?;
    created.push(progress.display().to_string());
    std::fs::write(&opavs, OPAVS_TEMPLATE)?;
    created.push(opavs.display().to_string());

    let has_instruction_file = agents.exists() || claude.exists();
    if agents.exists() {
        append_instruction_block(&agents, OPAVS_TEMPLATE)?;
    }
    if claude.exists() {
        append_instruction_block(&claude, OPAVS_LINK)?;
    }
    if !has_instruction_file {
        std::fs::write(&agents, OPAVS_TEMPLATE)?;
        created.push(agents.display().to_string());
    }

    append_gitignore_entry(repo_root, ".ctx/opavs/phase")?;

    Ok(created)
}

fn append_instruction_block(path: &Path, block: &str) -> Result<()> {
    let existing = std::fs::read_to_string(path)?;
    if existing.contains(block) {
        return Ok(());
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    if !updated.is_empty() {
        updated.push('\n');
    }
    updated.push_str(block);
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    std::fs::write(path, updated)?;
    Ok(())
}

fn append_gitignore_entry(repo_root: &Path, entry: &str) -> Result<()> {
    let gitignore = repo_root.join(".gitignore");
    let existing = if gitignore.exists() {
        std::fs::read_to_string(&gitignore)?
    } else {
        String::new()
    };
    if existing.lines().any(|l| l.trim() == entry) {
        return Ok(());
    }
    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(entry);
    updated.push('\n');
    std::fs::write(&gitignore, updated)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_creates_all_required_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let created = scaffold(tmp.path()).unwrap();
        assert_eq!(created.len(), 5);
        assert!(tmp.path().join(".ctx/opavs/tasks.yaml").is_file());
        assert!(
            tmp.path()
                .join(".ctx/opavs/memory-bank/active-context.md")
                .is_file()
        );
        assert!(
            tmp.path()
                .join(".ctx/opavs/memory-bank/progress.md")
                .is_file()
        );
        assert!(tmp.path().join("AGENTS.md").is_file());
        assert!(tmp.path().join("OPAVS.md").is_file());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("AGENTS.md")).unwrap(),
            OPAVS_TEMPLATE
        );
    }

    #[test]
    fn scaffold_links_existing_instruction_files_without_overwriting_them() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("AGENTS.md"), "# Agents\n").unwrap();
        std::fs::write(tmp.path().join("CLAUDE.md"), "# Claude\n").unwrap();

        scaffold(tmp.path()).unwrap();

        assert_eq!(
            std::fs::read_to_string(tmp.path().join("AGENTS.md")).unwrap(),
            format!("# Agents\n\n{OPAVS_TEMPLATE}")
        );
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("CLAUDE.md")).unwrap(),
            "# Claude\n\n@OPAVS.md\n"
        );
    }

    #[test]
    fn scaffold_uses_existing_claude_without_creating_agents() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("CLAUDE.md"), "# Claude\n").unwrap();

        scaffold(tmp.path()).unwrap();

        assert!(!tmp.path().join("AGENTS.md").exists());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("CLAUDE.md")).unwrap(),
            "# Claude\n\n@OPAVS.md\n"
        );
    }

    #[test]
    fn scaffold_adds_gitignore_entry() {
        let tmp = tempfile::tempdir().expect("tempdir");
        scaffold(tmp.path()).unwrap();
        let gitignore = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(gitignore.lines().any(|l| l == ".ctx/opavs/phase"));
    }

    #[test]
    fn scaffold_refuses_to_overwrite_existing_tasks_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join(".ctx/opavs")).unwrap();
        std::fs::write(tmp.path().join(".ctx/opavs/tasks.yaml"), "tasks: []").unwrap();
        assert!(scaffold(tmp.path()).is_err());
    }

    #[test]
    fn scaffold_does_not_duplicate_existing_gitignore_entry() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join(".gitignore"), ".ctx/opavs/phase\n").unwrap();
        scaffold(tmp.path()).unwrap();
        let gitignore = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert_eq!(gitignore.matches(".ctx/opavs/phase").count(), 1);
    }
}
