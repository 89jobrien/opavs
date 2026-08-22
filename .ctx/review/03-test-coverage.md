# Lens 3 — Test Coverage Gaps

## Blocking

- `src/main.rs:169` `run_guard()` — zero coverage. No test drives it end-to-end
  (stdin JSON -> stdout verdict) for Edit/Write/Bash, missing `tool_name`, missing
  `cwd`, or an unresolvable target dir.
- `src/main.rs:83` `main()` / clap `Command` wiring — no integration test exercises
  the CLI enum end-to-end (`opavs tasks list`, `opavs tasks import <path>`,
  `opavs guard`, etc). Only free functions have tests.
- `src/main.rs:147` `TasksAction::Import` handler — untested: no case merges then
  hits `domain::validate` failure (unknown dep/cycle from import), and the printed
  "N new, M total" counts aren't asserted.
- `src/main.rs:130` `TasksAction::SetStatus` handler — untested happy path, invalid
  status string (`bail!`), and unknown task id (`ok_or_else`).
- `src/main.rs:121` `TasksAction::Validate` handler — untested (ok and bail branches).
- `src/main.rs:115` `TasksAction::Runnable`, `src/main.rs:109` `TasksAction::List` —
  untested output paths.
- `src/main.rs:93` `Command::Phase` (`Get`/`Set`) — untested; the CLI-level "invalid
  phase string" path (via `PhaseAction::Set` -> `Phase::parse`) is only exercised at
  the domain level, not through the CLI.
- `src/main.rs:87` `Command::Init` handler — untested via CLI wiring (only
  `init::scaffold` itself is tested directly).

## Suggestions

- `src/main.rs:70` `find_repo_root()` — untested: no case for the "no tasks.yaml
  found" error message, or the `explicit` Some/None branches.
- `src/main.rs:174-221` guard tool dispatch — untested branches: `Edit`/`Write` with
  empty `file_path`, unrecognized tool name, `Bash` with a non-commit/push command,
  `resolve_repo_root` returning `None`.
- `src/main.rs:248` `allow_and_exit()` calls `std::process::exit(0)` directly, which
  makes these branches fundamentally hard to unit-test (any hit kills the test
  process). Recommend refactoring `run_guard()` to return a `Verdict`-like value and
  pushing the actual exit to a thin `main()` wrapper — mirrors the pure/impure split
  already used for `guard::decide`.
- `src/domain.rs` — `#[serde(default)]` fields on `Task` (`description`,
  `depends_on`, `default_status`) are untested: no test deserializes a minimal YAML
  task missing those fields and checks defaults apply.
- `src/adapters.rs:22-29` `FsPhaseStore::get()` — corrupt/invalid phase-file content
  (`Phase::parse` failure) is untested.
- `src/adapters.rs:58-59` `FsTaskStore::load()` — malformed-YAML error path
  (`serde_yaml::from_str` failure) is untested.
- `src/import.rs:7` `read_external_graph()` — only the happy path is tested; missing
  file and malformed-YAML paths are untested.

## Observations (well covered)

- `src/domain.rs` `validate()`/`runnable_tasks()`, `src/guard.rs` `decide()`/
  `command_touches_commit_or_push()`, `src/init.rs` `scaffold()`, `src/repo.rs`
  `resolve_repo_root()`, `src/import.rs` `merge()` all have solid inline coverage
  including edge cases.
