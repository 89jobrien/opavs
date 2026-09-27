//! Black-box tests for `opavs uninstall`: the command must invert exactly what
//! `opavs plugin install` and `opavs init` put on disk, and must never destroy
//! configuration it does not own.

use assert_cmd::Command;
use std::fs;
use std::path::Path;

fn opavs() -> Command {
    Command::cargo_bin("opavs").expect("binary builds")
}

fn install_plugin(target: &str, home: &Path) {
    opavs()
        .arg("plugin")
        .arg("install")
        .arg(target)
        .arg("--home")
        .arg(home)
        .assert()
        .success();
}

fn uninstall(extra: &[&str], home: &Path) -> Command {
    let mut command = opavs();
    command.arg("uninstall").arg("--home").arg(home);
    for arg in extra {
        command.arg(arg);
    }
    command
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read json")).expect("valid json")
}

// --- client integrations ---

#[test]
fn uninstall_all_removes_every_owned_client_artifact() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path();
    install_plugin("all", home);

    uninstall(&[], home).assert().success();

    for stale in [
        ".claude/plugins/local-marketplace/plugins/opavs",
        ".codex/commands/opavs-orient.md",
        ".agents/skills/opavs/SKILL.md",
        ".gemini/extensions/opavs",
        ".config/opencode/plugins/opavs",
        ".config/opencode/agents/opavs-orient.md",
        ".config/opencode/commands/opavs-ship.md",
    ] {
        assert!(!home.join(stale).exists(), "still present: {stale}");
    }
}

#[test]
fn uninstall_is_idempotent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path();
    install_plugin("all", home);

    uninstall(&[], home).assert().success();
    // A second run has nothing to do and must not fail.
    uninstall(&[], home).assert().success();
}

#[test]
fn uninstall_target_leaves_other_targets_installed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path();
    install_plugin("all", home);

    uninstall(&["--target", "claude"], home).assert().success();

    assert!(
        !home
            .join(".claude/plugins/local-marketplace/plugins/opavs")
            .exists()
    );
    assert!(home.join(".gemini/extensions/opavs").exists());
    assert!(home.join(".config/opencode/plugins/opavs").exists());
}

#[test]
fn uninstall_leaves_user_edited_artifacts_in_place() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path();
    install_plugin("claude", home);

    let edited = home.join(".claude/plugins/local-marketplace/plugins/opavs/skills/opavs/SKILL.md");
    fs::write(&edited, "# my own notes\n").expect("edit artifact");

    uninstall(&["--target", "claude"], home).assert().success();

    assert_eq!(
        fs::read_to_string(&edited).unwrap(),
        "# my own notes\n",
        "a user-edited artifact must survive uninstall"
    );
    assert!(
        !home
            .join(".claude/plugins/local-marketplace/plugins/opavs/hooks/hooks.json")
            .exists(),
        "untouched siblings are still removed"
    );
}

// --- shared configuration is edited, never deleted ---

#[test]
fn uninstall_strips_only_the_opavs_codex_hook() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path();
    let hooks = home.join(".codex/hooks.json");
    fs::create_dir_all(hooks.parent().expect("parent")).expect("create parent");
    fs::write(
        &hooks,
        r#"{"hooks":{"PreToolUse":[{"matcher":"Edit|Write|Bash","hooks":[{"command":"other-tool guard"}]}]}}"#,
    )
    .expect("seed shared hooks");

    install_plugin("codex", home);
    assert_eq!(
        read_json(&hooks)
            .pointer("/hooks/PreToolUse")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2,
        "install adds alongside the existing entry"
    );

    uninstall(&["--target", "codex"], home).assert().success();

    let parsed = read_json(&hooks);
    let remaining = parsed
        .pointer("/hooks/PreToolUse")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(remaining.len(), 1, "only the opavs entry is removed");
    assert_eq!(
        remaining[0].pointer("/hooks/0/command").unwrap().as_str(),
        Some("other-tool guard")
    );
}

#[test]
fn uninstall_strips_only_the_opavs_opencode_plugin_entry() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path();
    let config = home.join(".config/opencode/opencode.json");
    fs::create_dir_all(config.parent().expect("parent")).expect("create parent");
    fs::write(&config, "{\"plugin\":[\"other-plugin\"],\"mcp\":{}}\n").expect("seed config");

    install_plugin("opencode", home);
    uninstall(&["--target", "opencode"], home)
        .assert()
        .success();

    let parsed = read_json(&config);
    assert_eq!(
        parsed.get("plugin").and_then(|p| p.as_array()),
        Some(&vec![serde_json::json!("other-plugin")])
    );
    assert!(
        parsed.get("mcp").is_some(),
        "unrelated configuration survives"
    );
}

#[test]
fn uninstall_leaves_a_shared_config_untouched_when_opavs_is_absent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path();
    let config = home.join(".config/opencode/opencode.json");
    fs::create_dir_all(config.parent().expect("parent")).expect("create parent");
    let original = "{\"plugin\":[\"other-plugin\"],\"mcp\":{}}\n";
    fs::write(&config, original).expect("seed config");

    uninstall(&["--target", "opencode"], home)
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&config).unwrap(),
        original,
        "a config opavs never touched must be byte-identical afterwards"
    );
}

#[test]
fn uninstall_removes_the_opavs_gemini_enablement_only() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path();
    let enablement = home.join(".gemini/extensions/extension-enablement.json");
    fs::create_dir_all(enablement.parent().expect("parent")).expect("create parent");
    fs::write(&enablement, "{\"other\":{\"overrides\":[\"/x\"]}}\n").expect("seed");

    install_plugin("gemini", home);
    uninstall(&["--target", "gemini"], home).assert().success();

    let parsed = read_json(&enablement);
    assert!(parsed.get("opavs").is_none());
    assert!(parsed.get("other").is_some());
}

// --- dry run ---

#[test]
fn uninstall_dry_run_reports_without_removing_anything() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path();
    install_plugin("all", home);

    uninstall(&["--dry-run"], home)
        .assert()
        .success()
        .stdout(predicates::str::contains("opavs-orient.md"));

    assert!(
        home.join(".config/opencode/plugins/opavs").exists(),
        "dry run must not delete"
    );
    assert_eq!(
        read_json(&home.join(".codex/hooks.json"))
            .pointer("/hooks/PreToolUse")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1,
        "dry run must not edit shared config"
    );
}

// --- repository purge is opt-in, explicit, and conservative ---

/// A temp repo with `init` already run, plus a separate home for the client
/// artifacts. Keeping the two apart is what stops a purge test from ever
/// resolving against the working directory this test binary was launched in.
fn scaffolded_repo() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    let home = tmp.path().join("home");
    fs::create_dir_all(&repo).expect("create repo");
    fs::create_dir_all(&home).expect("create home");
    (tmp, repo, home)
}

fn purge(repo: &Path, home: &Path, extra: &[&str]) -> Command {
    let mut args = vec!["--purge-repo", "--repo", repo.to_str().expect("utf-8 path")];
    args.extend_from_slice(extra);
    uninstall(&args, home)
}

#[test]
fn uninstall_leaves_the_repository_alone_by_default() {
    let (_tmp, repo, home) = scaffolded_repo();
    opavs().arg("init").arg(&repo).assert().success();

    uninstall(&[], &home).assert().success();

    assert!(repo.join(".ctx/opavs/tasks.yaml").exists());
    assert!(repo.join("OPAVS.md").exists());
}

/// Deleting a task graph must never happen because nothing was named.
#[test]
fn purge_repo_without_an_explicit_repo_is_refused() {
    let (_tmp, _repo, home) = scaffolded_repo();

    opavs()
        .arg("uninstall")
        .arg("--home")
        .arg(&home)
        .arg("--purge-repo")
        .assert()
        .failure()
        .stderr(predicates::str::contains("--repo"));
}

#[test]
fn uninstall_purge_repo_removes_scaffolding_and_instruction_block() {
    let (_tmp, repo, home) = scaffolded_repo();
    fs::write(repo.join("AGENTS.md"), "# House rules\n").expect("seed instructions");
    opavs().arg("init").arg(&repo).assert().success();

    purge(&repo, &home, &[]).assert().success();

    assert!(!repo.join(".ctx/opavs/tasks.yaml").exists());
    assert!(!repo.join(".ctx/opavs").exists());
    assert!(!repo.join("OPAVS.md").exists());

    let agents = fs::read_to_string(repo.join("AGENTS.md")).expect("instructions survive");
    assert_eq!(
        agents, "# House rules\n",
        "only the opavs block is stripped"
    );
    assert!(!agents.contains("opavs-workflow"));

    let gitignore = fs::read_to_string(repo.join(".gitignore")).unwrap_or_default();
    assert!(!gitignore.contains(".ctx/opavs/phase"));
}

/// A `.gitignore` the user authored keeps everything but the line `init` added.
#[test]
fn uninstall_purge_repo_keeps_other_gitignore_entries() {
    let (_tmp, repo, home) = scaffolded_repo();
    fs::write(repo.join(".gitignore"), "/target\n").expect("seed gitignore");
    opavs().arg("init").arg(&repo).assert().success();

    purge(&repo, &home, &[]).assert().success();

    let gitignore = fs::read_to_string(repo.join(".gitignore")).expect("gitignore survives");
    assert_eq!(gitignore, "/target\n");
}

#[test]
fn uninstall_purge_repo_keeps_a_written_memory_bank() {
    let (_tmp, repo, home) = scaffolded_repo();
    opavs().arg("init").arg(&repo).assert().success();
    let progress = repo.join(".ctx/memory-bank/progress.md");
    fs::write(&progress, "# Progress\n\nshipped the parser\n").expect("write memory bank");

    purge(&repo, &home, &[]).assert().success();

    assert_eq!(
        fs::read_to_string(&progress).unwrap(),
        "# Progress\n\nshipped the parser\n",
        "a memory bank the user wrote into is not disposable"
    );
    assert!(!repo.join(".ctx/opavs/tasks.yaml").exists());
}

#[test]
fn uninstall_purge_repo_removes_an_untouched_memory_bank() {
    let (_tmp, repo, home) = scaffolded_repo();
    opavs().arg("init").arg(&repo).assert().success();

    purge(&repo, &home, &[]).assert().success();

    assert!(!repo.join(".ctx/memory-bank").exists());
}

#[test]
fn uninstall_purge_repo_dry_run_changes_nothing() {
    let (_tmp, repo, home) = scaffolded_repo();
    opavs().arg("init").arg(&repo).assert().success();

    purge(&repo, &home, &["--dry-run"]).assert().success();

    assert!(repo.join(".ctx/opavs/tasks.yaml").exists());
    assert!(repo.join("OPAVS.md").exists());
}

// --- help surface ---

#[test]
fn help_lists_uninstall_command() {
    opavs()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("uninstall"));
}
