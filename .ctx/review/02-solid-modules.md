# Lens 2 — SOLID / Module Boundaries

## Blocking

None.

## Suggestions

- `src/main.rs:169-246` — `run_guard()` mixes hook-JSON parsing, path extraction, adapter
  construction, and I/O (stdin read, println) in one ~80-line function. Extract the
  "parse hook -> (tool, target_dir, is_commit_or_push)" step into a small pure function
  so `run_guard` stays glue, matching `guard::decide`'s existing pure/testable style.
- `src/main.rs:152` — `merged.tasks.len() - base.tasks.len()` computes "added" via
  subtraction inside main.rs (business logic in the composition root; could underflow
  in edge cases). Belongs in `import.rs`, not `main.rs`.
- `src/guard.rs:44` — `let re_words: Vec<&str> = cmd.split_whitespace().collect();` is
  computed then discarded via `let _ = re_words;` — dead code, drop it.

## Observations (no action needed)

- `src/domain.rs` — correctly pure: no `std::fs`, no I/O; only `PhaseStore`/`TaskStore`
  traits (ports) plus `validate`/`runnable_tasks`. Dependency direction correct.
- `src/adapters.rs:1-70` — all filesystem access confined here, implementing the domain
  ports. No `Path`/`PathBuf` leakage into `domain.rs` signatures — DIP respected.
- `src/guard.rs:12-39` — `decide()` is genuinely I/O-free and the most cleanly tested
  module in the crate.
- `src/import.rs:1-27` — clean separation: `read_external_graph()` (I/O) is a thin
  wrapper; `merge()` is pure and independently unit-tested.
- `src/init.rs:20-72` — `scaffold()`/`append_gitignore_entry()` correctly confine all
  writes to this file, consistent with CLAUDE.md's architecture notes.
- `src/repo.rs` was not included in this pass — flag for follow-up if repo-root
  resolution logic needs a separate audit.
