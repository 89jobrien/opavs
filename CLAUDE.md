# CLAUDE.md

`opavs` — Rust CLI implementing the Orient-Plan-Act-Verify-Ship workflow
phasing system: it gates what's allowed (edits, commits) based on which
phase a repo is in, enforced via a PreToolUse guard hook rather than
convention alone. Originally defined as shell scripts in the `opavs` Claude
Code plugin (`~/.claude/plugins/local-marketplace/plugins/opavs/`).

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

## Build & Test

```
cargo check
cargo clippy --all-targets
cargo test
```

Per `~/.claude/skills/testing-philosophy`: pure domain/guard logic is
unit-tested inline; adapters are unit-tested against `tempfile::tempdir()`
(fast, no shared state). No property/fuzz tier yet — nothing here parses
untrusted bytes or does unchecked arithmetic; add fuzzing if a
`serde_yaml` parse path or raw-byte handling is introduced later.

## Conventions

Edition 2024. License: MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`).
Version starts at 0.0.1; do not bump to 1.0.0 without being told.
