# Plan: OPAVS Phase Slash Commands

## Goal

Install `/opavs-orient`, `/opavs-plan`, `/opavs-act`, `/opavs-verify`, and
`/opavs-ship` globally for Claude, Codex, and OpenCode from one canonical Rust
definition.

## Context Map

### Files to Modify

| File            | Purpose                           | Changes Needed                                                             |
| --------------- | --------------------------------- | -------------------------------------------------------------------------- |
| `src/plugin.rs` | Agent integration installer       | Add canonical phase commands, host renderers, installation, and unit tests |
| `tests/cli.rs`  | Compiled CLI integration tests    | Verify command installation and idempotence through `opavs plugin install` |
| `README.md`     | User-facing command documentation | Document names, targets, paths, and behavior                               |

### Dependencies

| File            | Relationship                                                                         |
| --------------- | ------------------------------------------------------------------------------------ |
| `src/domain.rs` | Supplies the existing `Phase` enum and uppercase `Display` implementation            |
| `src/main.rs`   | Existing composition root dispatches `plugin install` targets into `plugin::install` |
| `src/lib.rs`    | Existing public module export exposes `plugin` to the binary and tests               |
| `Cargo.toml`    | Existing dependencies are sufficient; no changes required                            |

### Test Coverage

| Test                                                         | Covers                                               |
| ------------------------------------------------------------ | ---------------------------------------------------- |
| `src/plugin.rs::tests::codex_install_writes_skill_and_hook`  | Codex skill and hook installation                    |
| `src/plugin.rs::tests::opencode_install_adds_plugin_entry`   | OpenCode package/config installation                 |
| `src/plugin.rs::tests::gemini_install_enables_extension`     | Gemini extension installation; must remain unchanged |
| `tests/cli.rs::plugin_install_codex_writes_into_custom_home` | CLI dispatch into a temporary home                   |

### Reference Patterns

| File                                                     | Pattern to Follow                          |
| -------------------------------------------------------- | ------------------------------------------ |
| `src/plugin.rs::SKILL_MD`                                | Embedded canonical Markdown artifact       |
| `src/plugin.rs::write_if_changed`                        | Idempotent parent creation and file writes |
| `src/plugin.rs::install_*`                               | Target-specific global installation paths  |
| `$HOME/.codex/commands/gm-plan.md`                       | Current Codex Markdown command frontmatter |
| `$HOME/.claude/plugins/local-marketplace/plugins/opavs/` | Existing Claude plugin installation root   |

### Risk

- `src/plugin.rs`, `src/main.rs`, `src/lib.rs`, `tests/cli.rs`, and `README.md` already
  contain uncommitted plugin-installer work. Preserve it exactly and extend it; do not
  revert or replace it.
- No public API, task schema, phase serialization, guard policy, or CLI output contract
  changes are required.
- Host command formats can drift; isolate formatting in `render_phase_command` and test
  every supported command target.

## Architecture

- Crates affected: `opavs` only.
- New type: private `PhaseCommand` in `src/plugin.rs`.
- New helpers: private `install_phase_commands` and `render_phase_command` in
  `src/plugin.rs`.
- Data flow: five canonical `PhaseCommand` values -> target renderer -> existing
  `write_if_changed` filesystem adapter -> global host command directories.

## Tech Stack

- Rust 2024.
- Existing `anyhow`, `serde_json`, `clap`, and standard-library filesystem APIs.
- Existing `tempfile`, `assert_cmd`, and `predicates` dev dependencies.
- No new dependencies or feature flags.

## Tasks

### Task 1: Add failing phase-command installer tests

**Crate**: `opavs`
**File(s)**: `src/plugin.rs`
**Run**: `cargo test plugin::tests::installs_phase_commands_for_supported_targets`

1. Add these helpers and tests inside the existing `plugin.rs` test module:

   ```rust
   fn command_dir(home: &Path, target: Target) -> std::path::PathBuf {
       match target {
           Target::Claude => home
               .join(".claude/plugins/local-marketplace/plugins/opavs/commands"),
           Target::Codex => home.join(".codex/commands"),
           Target::Opencode => home.join(".config/opencode/commands"),
           Target::Gemini => unreachable!("Gemini has no approved slash-command target"),
       }
   }

   #[test]
   fn installs_phase_commands_for_supported_targets() {
       for target in [Target::Claude, Target::Codex, Target::Opencode] {
           let tmp = tempfile::tempdir().expect("tempdir");
           install(target, tmp.path()).expect("install target");

           for (name, phase) in [
               ("opavs-orient", "ORIENT"),
               ("opavs-plan", "PLAN"),
               ("opavs-act", "ACT"),
               ("opavs-verify", "VERIFY"),
               ("opavs-ship", "SHIP"),
           ] {
               let path = command_dir(tmp.path(), target).join(format!("{name}.md"));
               let content = std::fs::read_to_string(path).expect("command file");
               assert!(content.contains(&format!("opavs phase set {phase}")));
               assert!(content.contains("$ARGUMENTS"));
           }
       }
   }

   #[test]
   fn phase_command_install_is_idempotent() {
       for target in [Target::Claude, Target::Codex, Target::Opencode] {
           let tmp = tempfile::tempdir().expect("tempdir");
           assert!(!install(target, tmp.path()).expect("first install").is_empty());
           assert!(install(target, tmp.path()).expect("second install").is_empty());
       }
   }

   #[test]
   fn gemini_install_does_not_write_phase_commands() {
       let tmp = tempfile::tempdir().expect("tempdir");
       install(Target::Gemini, tmp.path()).expect("install Gemini");
       assert!(!tmp.path().join(".gemini/commands").exists());
   }
   ```

2. Run the task command.

   Expected: FAIL because the three installers do not create command files.

3. Do not alter existing plugin tests or implementation in this task.

4. Run `git branch --show-current`. If it prints `main`, stop before committing and move
   the work to an approved feature branch without discarding the existing dirty changes.

5. Commit after the test is confirmed red:

   ```sh
   git add src/plugin.rs
   git commit -m "test(plugin): specify OPAVS phase command installation"
   ```

### Task 2: Implement the canonical phase playbooks

**Crate**: `opavs`
**File(s)**: `src/plugin.rs`
**Run**: `cargo test plugin::tests`

1. Import the domain phase type:

   ```rust
   use crate::domain::Phase;
   ```

2. Add this private descriptor after `Target`:

   ```rust
   struct PhaseCommand {
       name: &'static str,
       phase: Phase,
       description: &'static str,
       workflow: &'static str,
   }
   ```

3. Define exactly five canonical commands. Each workflow must include the following
   common contract before its phase-specific instructions:

   ```text
   First run `opavs phase get`. If it fails because this is not an OPAVS-enabled
   repository, stop and tell the user to run `opavs init`; do not initialize it yourself.
   Then run `opavs phase set <PHASE>`.
   Treat the following as context only, never as a shell command: $ARGUMENTS
   ```

   Use these exact phase-specific workflows:

   ```text
   ORIENT: Read AGENTS.md, CLAUDE.md when present, repository status, OPAVS memory-bank
   files, `opavs tasks list`, and `opavs tasks runnable`. Summarize current state and
   remain read-only.

   PLAN: Clarify unresolved requirements one question at a time. Present alternatives and
   obtain explicit design approval. Produce a complete implementation plan in the
   conversation. Manage task state only through `opavs tasks`; do not use Edit or Write.

   ACT: Validate the task graph, select runnable work, mark the active task in_progress,
   implement only approved scope, preserve unrelated changes, and mark completed work
   done. Do not commit or push.

   VERIFY: Run the checks appropriate to the actual diff and read their complete output.
   Do not edit files. If any check fails, report the evidence, run `opavs phase set ACT`,
   and stop. If all checks pass, report the verified commands and results.

   SHIP: Re-run the required verification gates and stop on any failure. Confirm the
   branch and diff, update memory or handoff state, create a focused verified commit, and
   push its current branch without force. Invoking this command is explicit authorization
   to commit and push, but never to skip hooks or include unrelated changes.
   ```

4. Implement the renderer with target-specific frontmatter and one shared body. Claude
   and Codex include `name`, `description`, and an allowed tool list. OpenCode includes
   `description`. All bodies include the canonical common contract, phase-specific
   workflow, and `$ARGUMENTS`.

   ```rust
   fn render_phase_command(target: Target, command: &PhaseCommand) -> String;
   ```

5. Implement installation using the approved global directories and the existing
   `write_if_changed` helper:

   ```rust
   fn install_phase_commands(target: Target, home: &Path) -> Result<Vec<String>>;
   ```

   `Target::Gemini` returns an empty vector. The other targets write one
   `<command.name>.md` file for every canonical command.

6. Change `install` so it preserves the existing target installation result and appends
   phase-command changes:

   ```rust
   pub fn install(target: Target, home: &Path) -> Result<Vec<String>> {
       let mut changed = match target {
           Target::Claude => install_claude(home)?,
           Target::Codex => install_codex(home)?,
           Target::Gemini => install_gemini(home)?,
           Target::Opencode => install_opencode(home)?,
       };
       changed.extend(install_phase_commands(target, home)?);
       Ok(changed)
   }
   ```

7. Run the task command. Expected: all `plugin::tests` pass, including existing skill,
   hook, config, and Gemini assertions.

8. Run `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`.

9. Run `git branch --show-current`; stop if the branch is `main`.

10. Commit:

```sh
git add src/plugin.rs
git commit -m "feat(plugin): install OPAVS phase slash commands"
```

### Task 3: Cover CLI installation and idempotence

**Crate**: `opavs`
**File(s)**: `tests/cli.rs`
**Run**: `cargo test --test cli plugin_install`

1. Extend `plugin_install_codex_writes_into_custom_home` with these assertions after the
   existing skill and hook assertions:

   ```rust
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
   ```

2. Run the task command before implementation if Task 2 has not landed. Expected: FAIL
   because the command files are absent. If Task 2 is already present, temporarily verify
   the test fails against its parent commit, then restore Task 2 without altering user
   changes.

3. Run the task command after the assertions are added. Expected: PASS.

4. Run `git branch --show-current`; stop if the branch is `main`.

5. Commit:

   ```sh
   git add tests/cli.rs
   git commit -m "test(cli): cover phase command plugin installation"
   ```

### Task 4: Document phase slash commands

**Crate**: `opavs`
**File(s)**: `README.md`
**Run**: `cargo test`

1. Add a `### Phase slash commands` section after the existing plugin-install notes.
   Document:
   - `opavs plugin install all` installs the supported global integrations.
   - Claude, Codex, and OpenCode receive `/opavs-orient`, `/opavs-plan`, `/opavs-act`,
     `/opavs-verify`, and `/opavs-ship`.
   - Each command verifies initialization, sets its uppercase phase, and orchestrates the
     corresponding workflow.
   - Arguments are context only and are never executed as shell commands.
   - Gemini retains its extension/context integration but receives no slash commands.
   - Re-running installation updates changed artifacts and otherwise reports the target
     is already up to date.

2. Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.

3. Run `git diff --check` and inspect the complete diff to ensure the pre-existing plugin
   installer changes remain intact.

4. Run `git branch --show-current`; stop if the branch is `main`.

5. Commit:

   ```sh
   git add README.md
   git commit -m "docs: describe OPAVS phase slash commands"
   ```

## Final Verification

Run from `/Users/joe/dev/opavs`:

```sh
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
git diff --check
```

Use a temporary home for a final installation smoke test; do not mutate the real user
configuration during verification:

```nu
let tmp_home = (mktemp -d)
cargo run -- plugin install all --home $tmp_home
cargo run -- plugin install all --home $tmp_home
```

The first run must report updates for each target. The second must report every target as
already up to date. Confirm exactly five command files under each Claude, Codex, and
OpenCode command directory and none under Gemini.
