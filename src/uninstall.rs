//! Removes what `opavs init` and `opavs plugin install` put on disk.
//!
//! Removal is driven by the same artifact expectations installation uses
//! ([`crate::plugin::artifact_expectations`]), so the two cannot drift: an
//! artifact added to install is automatically known to uninstall.
//!
//! Two rules keep this from destroying anything it did not create:
//!
//! * **Owned** artifacts are deleted only when their contents still match what
//!   OPAVS would have written. A file the user has edited is preserved and
//!   reported, because it is no longer ours to remove.
//! * **Shared** configuration is never deleted. The OPAVS entry is excised from
//!   the surrounding JSON and everything else is left exactly as it was.

use crate::init;
use crate::integration::{ArtifactExpectation, ArtifactMatch, Target, is_opavs_guard_command};
use crate::plugin;
use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// What a removal pass did, or would do under `--dry-run`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// OPAVS-owned files deleted.
    pub removed: Vec<String>,
    /// Shared configuration files rewritten to drop the OPAVS entry.
    pub edited: Vec<String>,
    /// OPAVS-owned files the user had modified, and so were left in place.
    pub preserved: Vec<String>,
}

impl Report {
    /// Fold another pass's findings into this report.
    pub fn merge(&mut self, other: Report) {
        self.removed.extend(other.removed);
        self.edited.extend(other.edited);
        self.preserved.extend(other.preserved);
    }

    /// Whether the pass would change anything on disk.
    pub fn is_empty(&self) -> bool {
        self.removed.is_empty() && self.edited.is_empty() && self.preserved.is_empty()
    }
}

/// Remove the client integration for `target` from `home`.
///
/// # Errors
///
/// Propagates filesystem failures from owned-artifact deletion. Shared
/// configuration that cannot be parsed is reported through the error rather than
/// being rewritten, so malformed user configuration is never clobbered.
pub fn remove_target(target: Target, home: &Path, apply: bool) -> Result<Report> {
    let root = owned_root(target, home);
    let mut report = Report::default();

    for artifact in plugin::artifact_expectations(target, home) {
        if artifact.owned {
            remove_owned_artifact(&artifact, &root, apply, &mut report)?;
        } else {
            remove_from_shared(&artifact, apply, &mut report)?;
        }
    }
    Ok(report)
}

/// Remove the repository scaffolding `opavs init` created.
///
/// Memory-bank files are removed only while they still hold the template `init`
/// wrote. The task graph is always removed: the caller asked for it explicitly
/// with `--purge-repo`.
///
/// # Errors
///
/// Propagates filesystem failures encountered while removing scaffolding.
pub fn purge_repo(repo_root: &Path, apply: bool) -> Result<Report> {
    let mut report = Report::default();

    remove_file_if_present(&repo_root.join(".ctx/opavs/tasks.yaml"), apply, &mut report);
    remove_file_if_present(&repo_root.join(".ctx/opavs/phase"), apply, &mut report);

    remove_if_untouched(
        &repo_root.join(".ctx/memory-bank/active-context.md"),
        init::ACTIVE_CONTEXT_TEMPLATE,
        apply,
        &mut report,
    );
    remove_if_untouched(
        &repo_root.join(".ctx/memory-bank/progress.md"),
        init::PROGRESS_TEMPLATE,
        apply,
        &mut report,
    );

    remove_if_marked(&repo_root.join("OPAVS.md"), apply, &mut report);
    strip_workflow_block(&repo_root.join("AGENTS.md"), apply, &mut report)?;
    strip_workflow_block(&repo_root.join("CLAUDE.md"), apply, &mut report)?;
    remove_gitignore_entry(repo_root, apply, &mut report);

    prune_repo_dirs(repo_root, apply);
    Ok(report)
}

/// The directory a target's plugin lives in, which OPAVS owns outright.
fn owned_root(target: Target, home: &Path) -> PathBuf {
    match target {
        Target::Claude => home.join(".claude/plugins/local-marketplace/plugins/opavs"),
        Target::Codex => home.join(".agents/skills/opavs"),
        Target::Gemini => home.join(".gemini/extensions/opavs"),
        Target::Opencode => home.join(".config/opencode/plugins/opavs"),
    }
}

fn remove_owned_artifact(
    artifact: &ArtifactExpectation,
    root: &Path,
    apply: bool,
    report: &mut Report,
) -> Result<()> {
    let Ok(contents) = fs::read_to_string(&artifact.path) else {
        return Ok(());
    };

    if !artifact.is_current(&contents) {
        report.preserved.push(artifact.path.display().to_string());
        return Ok(());
    }

    if apply {
        fs::remove_file(&artifact.path)
            .with_context(|| format!("remove file {}", artifact.path.display()))?;
    }
    report.removed.push(artifact.path.display().to_string());

    // Collapse the plugin directory itself, and stop there. Directories that
    // merely held a command file are shared with the client's other content, and
    // an empty one is cheaper to leave than to guess about.
    if artifact.path.starts_with(root)
        && let Some(parent) = artifact.path.parent()
    {
        prune_empty_dirs(parent, root);
    }
    Ok(())
}

/// Remove empty directories from `start` upward, through `owned` itself.
///
/// `owned` is included so a plugin directory OPAVS created outright disappears
/// rather than lingering as an empty shell. The walk cannot escape it: the first
/// directory above `owned` is not itself prefixed by `owned`, so the loop ends.
fn prune_empty_dirs(start: &Path, owned: &Path) {
    let mut cursor = start.to_path_buf();
    while cursor.starts_with(owned) {
        let empty = fs::read_dir(&cursor)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        if !empty || fs::remove_dir(&cursor).is_err() {
            return;
        }
        match cursor.parent() {
            Some(parent) => cursor = parent.to_path_buf(),
            None => return,
        }
    }
}

fn remove_from_shared(
    artifact: &ArtifactExpectation,
    apply: bool,
    report: &mut Report,
) -> Result<()> {
    let Some(contents) = read_optional(&artifact.path)? else {
        return Ok(());
    };
    // A config opavs cannot parse is left strictly alone: rewriting it would
    // either fail or discard whatever the user meant by it.
    let Ok(mut root) = serde_json::from_str::<Value>(&contents) else {
        return Ok(());
    };

    let changed = match &artifact.expected {
        ArtifactMatch::CodexHook => remove_codex_hook(&mut root),
        ArtifactMatch::GeminiEnablement { .. } => remove_gemini_enablement(&mut root),
        ArtifactMatch::OpencodePlugin { plugin_ref } => {
            remove_opencode_plugin_entry(&mut root, plugin_ref)
        }
        ArtifactMatch::Exact(_) => false,
    };
    if !changed {
        return Ok(());
    }

    if apply {
        // An enablement or hooks file that held nothing but OPAVS has no reason
        // to outlive it.
        if is_empty_container(&root) {
            fs::remove_file(&artifact.path)
                .with_context(|| format!("remove file {}", artifact.path.display()))?;
        } else {
            plugin::write_json_if_changed(&artifact.path, &root)?;
        }
    }
    report.edited.push(artifact.path.display().to_string());
    Ok(())
}

fn remove_codex_hook(root: &mut Value) -> bool {
    let Some(pretool) = root
        .pointer_mut("/hooks/PreToolUse")
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let before = pretool.len();
    pretool.retain(|entry| !is_opavs_hook_entry(entry));
    if pretool.len() == before {
        return false;
    }

    // Collapse the containers install created so a hooks file that held only the
    // OPAVS hook does not keep an empty shell behind.
    if pretool.is_empty()
        && let Some(hooks) = root.pointer_mut("/hooks").and_then(Value::as_object_mut)
    {
        hooks.remove("PreToolUse");
        if hooks.is_empty() {
            root.as_object_mut().expect("object root").remove("hooks");
        }
    }
    true
}

fn is_opavs_hook_entry(entry: &Value) -> bool {
    entry.get("matcher").and_then(Value::as_str) == Some("Edit|Write|Bash")
        && entry
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|hooks| {
                hooks.iter().any(|hook| {
                    hook.get("command")
                        .and_then(Value::as_str)
                        .is_some_and(is_opavs_guard_command)
                })
            })
}

fn remove_gemini_enablement(root: &mut Value) -> bool {
    root.as_object_mut()
        .is_some_and(|object| object.remove("opavs").is_some())
}

fn remove_opencode_plugin_entry(root: &mut Value, plugin_ref: &str) -> bool {
    let Some(plugins) = root.get_mut("plugin").and_then(Value::as_array_mut) else {
        return false;
    };
    let before = plugins.len();
    plugins.retain(|entry| entry.as_str() != Some(plugin_ref));
    if plugins.len() == before {
        return false;
    }

    if plugins.is_empty()
        && let Some(object) = root.as_object_mut()
    {
        object.remove("plugin");
    }
    true
}

fn is_empty_container(value: &Value) -> bool {
    value.as_object().is_some_and(Map::is_empty)
}

fn read_optional(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("read file {}", path.display())),
    }
}

fn remove_file_if_present(path: &Path, apply: bool, report: &mut Report) {
    if !path.exists() {
        return;
    }
    if apply {
        let _ = fs::remove_file(path);
    }
    report.removed.push(path.display().to_string());
}

/// Remove a file only while it still holds the content `init` generated, so a
/// memory bank the user has since written into survives.
fn remove_if_untouched(path: &Path, template: &str, apply: bool, report: &mut Report) {
    match read_optional(path) {
        Ok(Some(contents)) if contents == template => remove_file_if_present(path, apply, report),
        Ok(_) => report.preserved.push(path.display().to_string()),
        Err(_) => {}
    }
}

/// Remove a file only if it carries OPAVS's own markers, so a hand-written
/// `OPAVS.md` is not treated as scaffolding.
fn remove_if_marked(path: &Path, apply: bool, report: &mut Report) {
    match read_optional(path) {
        Ok(Some(contents)) if contents.contains(init::WORKFLOW_BEGIN) => {
            remove_file_if_present(path, apply, report);
        }
        Ok(_) => report.preserved.push(path.display().to_string()),
        Err(_) => {}
    }
}

/// Remove the appended workflow block and `@OPAVS.md` link from an instruction
/// file, and delete the file if the block was all it held.
fn strip_workflow_block(path: &Path, apply: bool, report: &mut Report) -> Result<()> {
    let Some(contents) = read_optional(path)? else {
        return Ok(());
    };
    let stripped = strip_opavs_text(&contents);
    if stripped == contents {
        return Ok(());
    }

    if apply {
        if stripped.trim().is_empty() {
            fs::remove_file(path).with_context(|| format!("remove file {}", path.display()))?;
        } else {
            fs::write(path, &stripped).with_context(|| format!("write file {}", path.display()))?;
        }
    }
    report.removed.push(path.display().to_string());
    Ok(())
}

/// The OPAVS text `init` appends to an instruction file, removed from whatever
/// the user wrote around it.
fn strip_opavs_text(contents: &str) -> String {
    let mut kept = String::with_capacity(contents.len());
    let mut rest = contents;

    loop {
        let Some(start) = rest.find(init::WORKFLOW_BEGIN) else {
            kept.push_str(rest);
            break;
        };
        let spanned = &rest[start..];
        let Some(end) = spanned.find(init::WORKFLOW_END) else {
            // An unterminated marker is not ours to interpret; keep the rest.
            kept.push_str(rest);
            break;
        };
        kept.push_str(&rest[..start]);
        let after = &spanned[end + init::WORKFLOW_END.len()..];
        rest = after.strip_prefix('\n').unwrap_or(after);
    }

    let body = kept
        .lines()
        .filter(|line| line.trim() != init::OPAVS_LINK)
        .collect::<Vec<_>>()
        .join("\n");
    let body = body.trim_end();

    if body.is_empty() {
        String::new()
    } else {
        format!("{body}\n")
    }
}

fn remove_gitignore_entry(repo_root: &Path, apply: bool, report: &mut Report) {
    let path = repo_root.join(".gitignore");
    let Ok(Some(contents)) = read_optional(&path) else {
        return;
    };
    let kept: Vec<&str> = contents
        .lines()
        .filter(|line| line.trim() != init::GITIGNORE_PHASE_ENTRY)
        .collect();
    if kept.len() == contents.lines().count() {
        return;
    }

    if apply {
        if kept.is_empty() {
            let _ = fs::remove_file(&path);
        } else {
            let _ = fs::write(&path, kept.join("\n") + "\n");
        }
    }
    report.removed.push(path.display().to_string());
}

fn prune_repo_dirs(repo_root: &Path, _apply: bool) {
    // Both directories are created by `init` and hold nothing else, so removing
    // them once empty is safe regardless of whether the removal was applied.
    for relative in [".ctx/opavs", ".ctx/memory-bank"] {
        let dir = repo_root.join(relative);
        if dir.is_dir() {
            let _ = fs::remove_dir(dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install_then_uninstall(target: Target) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tempdir");
        plugin::install(target, tmp.path()).expect("install target");
        tmp
    }

    #[test]
    fn owned_artifacts_are_removed_and_pruned_to_the_plugin_root() {
        let tmp = install_then_uninstall(Target::Gemini);
        let home = tmp.path();

        let report = remove_target(Target::Gemini, home, true).expect("uninstall");

        assert!(!home.join(".gemini/extensions/opavs").exists());
        assert!(!report.removed.is_empty());
    }

    #[test]
    fn uninstall_leaves_the_parent_of_a_command_file_alone() {
        let tmp = install_then_uninstall(Target::Codex);
        let home = tmp.path();

        remove_target(Target::Codex, home, true).expect("uninstall");

        assert!(!home.join(".codex/commands/opavs-orient.md").exists());
        assert!(
            home.join(".codex/commands").is_dir(),
            "a directory shared with the client's other content is not pruned"
        );
        assert!(!home.join(".agents/skills/opavs").exists());
    }

    #[test]
    fn a_dry_run_reports_the_same_work_without_applying_it() {
        let tmp = install_then_uninstall(Target::Opencode);
        let home = tmp.path();

        let planned = remove_target(Target::Opencode, home, false).expect("dry run");
        assert!(!planned.removed.is_empty());
        assert!(home.join(".config/opencode/plugins/opavs").exists());

        remove_target(Target::Opencode, home, true).expect("uninstall");
    }

    #[test]
    fn removing_an_absent_target_is_a_no_op() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let report = remove_target(Target::Claude, tmp.path(), true).expect("uninstall");
        assert!(report.is_empty());
    }

    #[test]
    fn strip_opavs_text_keeps_surrounding_instructions() {
        let contents = format!("# Rules\n\n{}\n", init::OPAVS_TEMPLATE);
        let stripped = strip_opavs_text(&contents);
        assert_eq!(stripped, "# Rules\n");
    }

    #[test]
    fn strip_opavs_text_keeps_instructions_linked_through_opavs_md() {
        let contents = format!("# Rules\n\n{}\n", init::OPAVS_LINK);
        assert_eq!(strip_opavs_text(&contents), "# Rules\n");
    }

    #[test]
    fn strip_opavs_text_leaves_unrelated_files_alone() {
        let contents = "# Rules\n\nBe careful.\n";
        assert_eq!(strip_opavs_text(contents), contents);
    }

    #[test]
    fn strip_opavs_text_empties_a_file_that_held_only_the_block() {
        assert_eq!(strip_opavs_text(init::OPAVS_TEMPLATE), "");
    }

    #[test]
    fn every_target_has_an_owned_root_under_home() {
        let home = Path::new("/home/example");
        for target in Target::ALL {
            assert!(owned_root(target, home).starts_with(home), "{target:?}");
        }
    }
}
