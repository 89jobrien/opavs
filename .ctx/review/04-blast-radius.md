# Lens 4 — Blast Radius

## Blocking

- **Path convention mismatch: Rust crate vs. shell-script plugin.**
  `~/.claude/plugins/local-marketplace/plugins/opavs/scripts/opavs-guard.sh:24` and
  `opavs-phase.sh:8` still gate on `.ctx/GODMODE.tasks.yaml` (repo-root marker) and
  read/write phase at `.ctx/.opavs-phase`. This crate uses `.ctx/opavs/tasks.yaml`
  (`src/repo.rs:8`) and `.ctx/opavs/phase` (`src/adapters.rs:17`).

  If `opavs` (the binary) is wired into `hooks.json` as a drop-in replacement without
  updating the scripts/paths, both directions **fail silently open**, not loud:
  1. A repo scaffolded by the old scripts (`.ctx/GODMODE.tasks.yaml` +
     `.ctx/.opavs-phase`) is invisible to `opavs::repo::resolve_repo_root` — the new
     guard falls through to `allow_and_exit()` (`main.rs:213-221`) with zero
     enforcement and zero error.
  2. A repo scaffolded by `opavs init` (`.ctx/opavs/*`) is invisible to the old shell
     guard if it's still what `hooks.json` invokes — same silent no-op.

  This is exactly the failure mode an approval gate must not have. Action before
  wiring the binary in: either (a) update the shell scripts' path constants to match,
  or (b) fully swap `hooks.json` to call `opavs guard`/`opavs phase` and migrate any
  already-scaffolded `.ctx/GODMODE.tasks.yaml`/`.ctx/.opavs-phase` repos to the new
  paths. Never run both guards concurrently against different conventions.

## Suggestions

- `src/main.rs:65-67` — `TasksAction::Import`'s doc comment still says "Merge an
  external GODMODE.tasks.yaml file" — harmless (schema-compatible, not path-coupled)
  but could read as implying guard-path compatibility. Clarify it refers to schema
  only.
- `src/init.rs:12-16` — `AGENTS_TEMPLATE` correctly documents the new `.ctx/opavs/*`
  paths, so freshly-scaffolded repos are internally consistent. The risk is entirely
  in pre-existing repos + the not-yet-updated plugin scripts.

## Observations

- Internal wiring in `src/main.rs` calls every public domain/adapter/guard/import
  function — no dead public items found.
- `cargo check` and `cargo clippy --all-targets` both clean, no errors/warnings.
- Hexagonal boundaries respected: `guard::decide()`/`repo::resolve_repo_root()` are
  pure and unit-tested; `adapters.rs` is the sole filesystem touchpoint for
  phase/task persistence; `main.rs` stays a thin composition root.
