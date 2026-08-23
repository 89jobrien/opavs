# CLAUDE.md

`opavs` — Rust CLI implementing the Orient-Plan-Act-Verify-Ship workflow
phasing system: it gates what's allowed (edits, commits) based on which
phase a repo is in, enforced via a PreToolUse guard hook rather than
convention alone. Originally prototyped as shell scripts in the `opavs` Claude Code plugin
(`~/.claude/plugins/local-marketplace/plugins/opavs/`); that plugin's
`hooks/hooks.json` now shells out to this compiled binary (`opavs guard`)
instead of running its own scripts.

The task graph (`domain::Task`/`TaskGraph`, `src/import.rs`, `opavs tasks
*`) is a secondary, optional companion for repos that want to track
in-phase work — it is not the core of this tool. Don't grow it at the
expense of the phase-gating path; the guard/phase/init trio is what opavs
is for.

## Architecture

Hexagonal (see `~/.claude/skills/writing-solid-rust`):

- `src/domain.rs` — `Phase`, `Task`/`TaskGraph`, `PhaseStore`/`TaskStore`
  ports (traits), and pure graph logic (validate, runnable_tasks). Zero I/O.
- `src/adapters.rs` — `FsPhaseStore`/`FsTaskStore`: filesystem implementations
  of the ports.
- `src/guard.rs` — pure `decide()` allow/deny logic for the PreToolUse hook.
- `src/repo.rs` — repo-root resolution (walk up for `.ctx/opavs/tasks.yaml`).
- `src/init.rs` — scaffolds `.ctx/opavs/tasks.yaml`, `.ctx/opavs/memory-bank/`,
  `AGENTS.md` in a target repo.
- `src/import.rs` — reads an external `GODMODE.tasks.yaml` (same schema) and
  merges it into the repo's graph by id, preserving existing task status.
- `src/main.rs` — composition root: clap CLI wiring subcommands to adapters.
- `src/lib.rs` — re-exports the above modules so `tests/` and `fuzz/` can
  link against the crate as a library; `main.rs` is a thin binary over it.

## Build & Test

```
cargo check
cargo clippy --all-targets
cargo test
```

Per `~/.claude/skills/testing-philosophy`, all seven dimensions are in use:

- Unit: pure domain/guard logic tested inline in `#[cfg(test)]` modules.
- Property (`proptest`, dev-dep): `guard::command_touches_commit_or_push`
  and `main::extract_dash_c_target`/`parse_guard_request` — hand-rolled
  string/JSON parsers over inputs with no example-based test can fully cover.
- Fuzz (`cargo-fuzz`, `fuzz/`): `fuzz/fuzz_targets/fuzz_import_yaml.rs`
  drives `serde_yaml::from_str::<TaskGraph>` directly — `import::read_
external_graph` parses a user-supplied `GODMODE.tasks.yaml` path, i.e.
  untrusted bytes not authored by this tool. Run with
  `cargo +nightly fuzz run fuzz_import_yaml -- -max_total_time=30`.
  Seed corpus lives in `fuzz/corpus/fuzz_import_yaml/` — never delete it.
- Conformance: `domain::conformance::{assert_phase_store_contract,
assert_task_store_contract}` — shared suites run against every
  `PhaseStore`/`TaskStore` impl (currently `FsPhaseStore`/`FsTaskStore` in
  `adapters.rs`'s test module).
- Integration (`assert_cmd`, dev-dep): `tests/cli.rs` exercises the
  compiled binary end to end (init → phase → tasks → guard over real
  stdin/stdout), catching composition-root wiring bugs unit tests can't see.
- Regression: adapters.rs and import.rs each carry a malformed-YAML test —
  keep adding one per bug fixed against either YAML parse path.

## Conventions

Edition 2024. License: MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`).
Version starts at 0.0.1; do not bump to 1.0.0 without being told.
