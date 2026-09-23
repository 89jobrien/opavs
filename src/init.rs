//! Scaffolds OPAVS state, memory-bank, instruction, and ignore files.

use anyhow::{Result, bail};
use std::path::Path;

const TASKS_TEMPLATE: &str = "tasks: []\n";

const ACTIVE_CONTEXT_TEMPLATE: &str = "# Active Context\n\n\
    _Updated at the end of ACT or after SHIP. What's in flight, what's next._\n";

const PROGRESS_TEMPLATE: &str = "# Progress\n\n\
    _Milestones as they land. Append, don't rewrite history._\n";

pub(crate) const OPAVS_TEMPLATE: &str = r##"<!-- opavs-workflow:begin -->

# Phased workflow

Unless the user clearly opts out (for example, **"skip plan, just fix it"**), every
non-trivial task progresses through five phases. Short confirmations such as
**"do it"**, **"act"**, and **"go"** advance to the next phase.

## Phases

<opavs-phase name="ORIENT" mode="read-only" response-header="# Phase: ORIENT" skills="opavs">
Default phase. Read files, search code, run `opavs phase get`, and check the task
graph with `opavs tasks list`. Summarize the branch, dirty files, and relevant
context. Do not modify the repository. End by stating what you found and which
phase comes next.
</opavs-phase>

<opavs-phase name="PLAN" mode="read-only" response-header="# Phase: PLAN" skills="brainstorm, writing-plans">
Produce a written plan covering files to touch, approach, tests, and risks. Stay
read-only: no edits and no builds that write output. For complex work, invoke an
applicable brainstorming or planning skill. End with "Type ACT to proceed" or
suggest refinements.
</opavs-phase>

<opavs-phase name="ACT" mode="read-write" response-header="# Phase: ACT" skills="task-driven-development, parallel-agents">
Enter when the user approves with "act", "go ahead", or "do it". Set the phase
with `opavs phase set ACT`, edit files, run commands, and dispatch subagents. Use
the task graph for multi-step work and parallel agents for independent tasks.
After finishing, transition to VERIFY automatically.
</opavs-phase>

<opavs-phase name="VERIFY" mode="read + test" response-header="# Phase: VERIFY" skills="verification-before-completion">
Set the phase with `opavs phase set VERIFY`. Run the relevant checks, including
`cargo check`, `cargo clippy`, and `cargo test` for Rust changes. Report results.
If failures exist, return to ACT to fix them. When green, state readiness and ask
the user to SHIP.
</opavs-phase>

<opavs-phase name="SHIP" mode="commit/push" response-header="# Phase: SHIP" skills="cap, handoff">
Enter only with explicit user approval. Set the phase with `opavs phase set SHIP`,
commit, push, and update the handoff or memory bank. After shipping, return to
ORIENT for the next task.
</opavs-phase>

## Phase transitions

- **Users can skip phases**: "skip plan, implement now" jumps to ACT. "Just fix
  it" implies ORIENT -> ACT -> VERIFY in one pass; SHIP still requires explicit
  approval.
- **After each ACT turn**, default to VERIFY unless the user says otherwise.
- **Multiple ACT turns** are allowed; the user can keep approving refinements.
- When the user gives a lettered choice or short confirmation, advance to the
  most obvious next phase without asking again.

## Skill invocation rule

Before responding in any phase, check whether an available skill applies. Invoke
process skills such as brainstorming or systematic debugging before implementation
skills such as task-driven development or parallel agents.

## Task graph

Tasks live in `.ctx/opavs/tasks.yaml`. Use the `opavs tasks` CLI for state
transitions. Independent chains can run in parallel. A task is runnable when all
items in `depends_on` are done.

## Memory bank

- Persistent context lives in `.ctx/opavs/memory-bank/`.
- Read `active-context.md` and `progress.md` before substantive work.
- Update the memory bank after milestones and after shipping.
- See `AGENTS.md` for repository-specific guidance.

## Agent-specific guidance

For subagent conventions and repository-specific instructions, see `AGENTS.md`.

<!-- opavs-workflow:end -->
"##;

pub(crate) const OPAVS_LINK: &str = "@OPAVS.md";

/// Scaffold the files opavs requires in a target repo: task graph, memory
/// bank, canonical instructions, and instruction-file links. Refuses to
/// overwrite generated state or an existing OPAVS.md.
pub fn scaffold(repo_root: &Path) -> Result<Vec<String>> {
    // TODO(init-repair): Add a repair/refresh mode for partial or stale scaffolds.
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
        let opavs = std::fs::read_to_string(tmp.path().join("OPAVS.md")).unwrap();
        assert!(opavs.contains("<!-- opavs-workflow:begin -->"));
        assert!(opavs.contains("<opavs-phase name=\"ORIENT\""));
        assert!(opavs.contains("## Phase transitions"));
        assert!(opavs.contains("## Task graph"));
        assert!(opavs.contains("## Memory bank"));
        assert!(opavs.contains("<!-- opavs-workflow:end -->"));
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
