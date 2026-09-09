//! Black-box integration tests: exercise the compiled `opavs` binary end to
//! end, the way Claude Code's PreToolUse hook and a human operator actually
//! invoke it. Unit tests cover the pure logic; these cover the wiring.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use std::fs;
use std::path::Path;

fn opavs() -> Command {
    Command::cargo_bin("opavs").expect("binary builds")
}

fn doctor(repo: &Path, home: &Path) -> Command {
    let mut command = opavs();
    command.arg("doctor").arg(repo).arg("--home").arg(home);
    command
}

#[test]
fn help_lists_upgrade_command() {
    opavs()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("upgrade"));
}

#[test]
fn doctor_missing_state_recommends_init() {
    let tmp = tempfile::tempdir().expect("tempdir");

    doctor(tmp.path(), tmp.path())
        .assert()
        .failure()
        .stdout(predicates::str::contains("repair: opavs init '"));
}

#[test]
fn doctor_partial_state_does_not_recommend_failing_init() {
    let tmp = tempfile::tempdir().expect("tempdir");
    fs::write(tmp.path().join("OPAVS.md"), "# OPAVS\n").expect("write partial state");

    doctor(tmp.path(), tmp.path())
        .assert()
        .failure()
        .stdout(predicates::str::contains("repair missing task graph"))
        .stdout(predicates::str::contains("opavs init").not());
}

#[test]
fn doctor_allows_init_when_only_agent_instructions_exist() {
    let tmp = tempfile::tempdir().expect("tempdir");
    fs::write(tmp.path().join("AGENTS.md"), "# Existing instructions\n")
        .expect("write instructions");

    doctor(tmp.path(), tmp.path())
        .assert()
        .failure()
        .stdout(predicates::str::contains("repair: opavs init '"));
}

#[test]
fn doctor_partial_state_reports_missing_instructions() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let tasks = tmp.path().join(".ctx/opavs/tasks.yaml");
    fs::create_dir_all(tasks.parent().expect("tasks parent")).expect("create state dir");
    fs::write(tasks, "tasks: []\n").expect("write tasks");

    doctor(tmp.path(), tmp.path())
        .assert()
        .failure()
        .stdout(predicates::str::contains("repo.instructions.missing"));
}

#[test]
fn doctor_reports_initialized_repository_as_healthy() {
    let tmp = tempfile::tempdir().expect("tempdir");
    opavs().arg("init").arg(tmp.path()).assert().success();

    doctor(tmp.path(), tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("repo.tasks.valid"))
        .stdout(predicates::str::contains("repo.instructions.linked"));
}

#[test]
fn doctor_accepts_rooted_phase_gitignore_pattern() {
    let tmp = tempfile::tempdir().expect("tempdir");
    opavs().arg("init").arg(tmp.path()).assert().success();
    fs::write(tmp.path().join(".gitignore"), "/.ctx/opavs/phase\n").expect("write gitignore");

    doctor(tmp.path(), tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("repo.phase.ignored"));
}

#[test]
fn doctor_reports_partial_plugin_install_as_drift() {
    let repo = tempfile::tempdir().expect("repo tempdir");
    let home = tempfile::tempdir().expect("home tempdir");
    opavs().arg("init").arg(repo.path()).assert().success();
    opavs()
        .args(["plugin", "install", "codex", "--home"])
        .arg(home.path())
        .assert()
        .success();
    fs::remove_file(home.path().join(".codex/hooks.json")).expect("remove Codex hook");

    doctor(repo.path(), home.path())
        .assert()
        .failure()
        .stdout(predicates::str::contains("plugin.codex.drift"))
        .stdout(predicates::str::contains("opavs plugin install codex"));
}

#[test]
fn doctor_reports_all_installed_plugins_as_current() {
    let repo = tempfile::tempdir().expect("repo tempdir");
    let home = tempfile::tempdir().expect("home tempdir");
    opavs().arg("init").arg(repo.path()).assert().success();
    opavs()
        .args(["plugin", "install", "all", "--home"])
        .arg(home.path())
        .assert()
        .success();

    let mut assertion = doctor(repo.path(), home.path()).assert().success();
    for target in ["claude", "codex", "gemini", "opencode"] {
        assertion = assertion.stdout(predicates::str::contains(format!(
            "plugin.{target}.current"
        )));
    }
}

#[test]
fn doctor_ignores_unrelated_shared_client_config() {
    let repo = tempfile::tempdir().expect("repo tempdir");
    let home = tempfile::tempdir().expect("home tempdir");
    opavs().arg("init").arg(repo.path()).assert().success();
    fs::create_dir_all(home.path().join(".codex")).expect("create Codex config dir");
    fs::write(home.path().join(".codex/hooks.json"), "{\"hooks\":{}}\n")
        .expect("write unrelated Codex hooks");
    fs::create_dir_all(home.path().join(".config/opencode")).expect("create OpenCode config dir");
    fs::write(
        home.path().join(".config/opencode/opencode.json"),
        "{\"plugin\":[]}\n",
    )
    .expect("write unrelated OpenCode config");

    doctor(repo.path(), home.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("plugin.codex.missing"))
        .stdout(predicates::str::contains("plugin.opencode.missing"));
}

#[test]
fn doctor_treats_malformed_opavs_shared_config_as_drift() {
    let repo = tempfile::tempdir().expect("repo tempdir");
    let home = tempfile::tempdir().expect("home tempdir");
    opavs().arg("init").arg(repo.path()).assert().success();
    fs::create_dir_all(home.path().join(".codex")).expect("create Codex config dir");
    fs::write(home.path().join(".codex/hooks.json"), "opavs guard\n")
        .expect("write malformed OPAVS hook config");

    doctor(repo.path(), home.path())
        .assert()
        .failure()
        .stdout(predicates::str::contains("plugin.codex.drift"));
}

#[test]
fn doctor_rejects_stale_owned_and_malformed_shared_artifacts() {
    let repo = tempfile::tempdir().expect("repo tempdir");
    let home = tempfile::tempdir().expect("home tempdir");
    opavs().arg("init").arg(repo.path()).assert().success();
    opavs()
        .args(["plugin", "install", "claude", "--home"])
        .arg(home.path())
        .assert()
        .success();
    opavs()
        .args(["plugin", "install", "codex", "--home"])
        .arg(home.path())
        .assert()
        .success();

    let claude_skill = home
        .path()
        .join(".claude/plugins/local-marketplace/plugins/opavs/skills/opavs/SKILL.md");
    let mut stale_skill = fs::read_to_string(&claude_skill).expect("read Claude skill");
    stale_skill.push_str("# stale\n");
    fs::write(claude_skill, stale_skill).expect("write stale Claude skill");
    fs::write(home.path().join(".codex/hooks.json"), "not json\n")
        .expect("write malformed Codex hooks");

    doctor(repo.path(), home.path())
        .assert()
        .failure()
        .stdout(predicates::str::contains("plugin.claude.drift"))
        .stdout(predicates::str::contains("plugin.codex.drift"));
}

#[test]
fn init_then_phase_get_defaults_to_orient() {
    let tmp = tempfile::tempdir().expect("tempdir");

    opavs()
        .arg("init")
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("created"));

    assert!(tmp.path().join("OPAVS.md").is_file());
    let agents = fs::read_to_string(tmp.path().join("AGENTS.md")).unwrap();
    assert!(agents.contains("This repo uses the opavs"));
    assert!(!agents.contains("@OPAVS.md"));

    opavs()
        .current_dir(tmp.path())
        .args(["phase", "get"])
        .assert()
        .success()
        .stdout("ORIENT\n");
}

#[test]
fn phase_set_then_get_roundtrips() {
    let tmp = tempfile::tempdir().expect("tempdir");
    opavs().arg("init").arg(tmp.path()).assert().success();

    opavs()
        .current_dir(tmp.path())
        .args(["phase", "set", "ACT"])
        .assert()
        .success();

    opavs()
        .current_dir(tmp.path())
        .args(["phase", "get"])
        .assert()
        .success()
        .stdout("ACT\n");
}

#[test]
fn tasks_import_then_list_shows_imported_task() {
    let tmp = tempfile::tempdir().expect("tempdir");
    opavs().arg("init").arg(tmp.path()).assert().success();

    let external = tmp.path().join("GODMODE.tasks.yaml");
    fs::write(
        &external,
        "tasks:\n  - id: a\n    description: do a\n    status: todo\n    depends_on: []\n",
    )
    .expect("write external graph");

    opavs()
        .current_dir(tmp.path())
        .args(["tasks", "import"])
        .arg(&external)
        .assert()
        .success()
        .stdout(predicates::str::contains("1 new"));

    opavs()
        .current_dir(tmp.path())
        .args(["tasks", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("a"));
}

#[test]
fn guard_denies_edit_outside_act_phase() {
    let tmp = tempfile::tempdir().expect("tempdir");
    opavs().arg("init").arg(tmp.path()).assert().success();

    let file_path = tmp.path().join("src").join("main.rs");
    let hook = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": {"file_path": file_path.display().to_string()},
        "cwd": tmp.path().display().to_string(),
    });

    opavs()
        .args(["guard"])
        .write_stdin(hook.to_string())
        .assert()
        .success()
        .stdout(predicates::str::contains("\"permissionDecision\":\"deny\""));
}

#[test]
fn guard_allows_edit_in_act_phase() {
    let tmp = tempfile::tempdir().expect("tempdir");
    opavs().arg("init").arg(tmp.path()).assert().success();
    opavs()
        .current_dir(tmp.path())
        .args(["phase", "set", "ACT"])
        .assert()
        .success();

    let file_path = tmp.path().join("src").join("main.rs");
    let hook = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": {"file_path": file_path.display().to_string()},
        "cwd": tmp.path().display().to_string(),
    });

    opavs()
        .args(["guard"])
        .write_stdin(hook.to_string())
        .assert()
        .success()
        .stdout("{\"continue\": true}\n");
}

#[test]
fn tasks_validate_reports_cycle() {
    let tmp = tempfile::tempdir().expect("tempdir");
    opavs().arg("init").arg(tmp.path()).assert().success();

    let tasks_path = tmp.path().join(".ctx").join("opavs").join("tasks.yaml");
    fs::write(
        &tasks_path,
        "tasks:\n  - id: a\n    depends_on: [b]\n  - id: b\n    depends_on: [a]\n",
    )
    .expect("write cyclic graph");

    opavs()
        .current_dir(tmp.path())
        .args(["tasks", "validate"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("cycle"));
}

#[test]
fn plugin_install_codex_writes_into_custom_home() {
    let tmp = tempfile::tempdir().expect("tempdir");

    opavs()
        .args(["plugin", "install", "codex", "--home"])
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("codex: updated"));

    assert!(tmp.path().join(".agents/skills/opavs/SKILL.md").exists());
    assert!(tmp.path().join(".codex/hooks.json").exists());

    for phase in ["orient", "plan", "act", "verify", "ship"] {
        assert!(
            tmp.path()
                .join(".codex/commands")
                .join(format!("opavs-{phase}.md"))
                .exists()
        );
    }

    opavs()
        .args(["plugin", "install", "codex", "--home"])
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("codex: already up to date"));
}
