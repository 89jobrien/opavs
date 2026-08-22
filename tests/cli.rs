//! Black-box integration tests: exercise the compiled `opavs` binary end to
//! end, the way Claude Code's PreToolUse hook and a human operator actually
//! invoke it. Unit tests cover the pure logic; these cover the wiring.

use assert_cmd::Command;
use std::fs;

fn opavs() -> Command {
    Command::cargo_bin("opavs").expect("binary builds")
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
