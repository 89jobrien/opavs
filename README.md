# opavs

Orient → Plan → Act → Verify → Ship. A workflow-phasing CLI: it gates what an
agent (or a person) is allowed to do based on which phase a repo is in —
read-only exploration, then a written plan, then edits, then verification,
then commit/push — each with a wider blast radius than the last. The point is
enforcement, not ceremony: a `PreToolUse` hook denies file mutations and
unapproved shell commands outside `ACT`, and denies `git commit`/`git push`
outside `SHIP`. The discipline is real state on disk, not just a convention.

Reimplements (in Rust) the shell-based `opavs-phase.sh` / `opavs-guard.sh`
pair from the `opavs` Claude Code plugin, for any repo using the
`.ctx/opavs/` state directory convention.

A lightweight task-graph companion rides alongside the phase machinery, for
repos that want to track what's runnable inside a phase — but the phase gate
is what this tool is _for_.

## Install

Install from a source checkout, then install the agent integrations you use:

```
git clone https://github.com/89jobrien/opavs.git
cargo install --path opavs
opavs plugin install all
```

## Commands

### Phase discipline (core)

```
opavs init [repo_root]     # scaffold OPAVS state and update instruction files
opavs phase get            # print current phase (defaults to ORIENT)
opavs phase set <PHASE>    # ORIENT | PLAN | ACT | VERIFY | SHIP

opavs guard                # PreToolUse hook entrypoint: reads Claude Code hook
                            # JSON on stdin, emits an allow/deny
                            # permissionDecision on stdout

opavs plugin install <target> [--home /path/to/home]
                            # install OPAVS integration for one target:
                            # claude | codex | gemini | opencode | all

opavs upgrade               # download and install the newest GitHub release
```

`opavs upgrade` checks the latest `89jobrien/opavs` GitHub Release, downloads
the archive matching the current platform, and replaces the running executable.
It exits without changing the binary when the installed version is current.

### Plugin install notes

- `opavs plugin install opencode` installs a local OpenCode plugin package at
  `~/.config/opencode/plugins/opavs/` and adds a `file://` plugin source to
  `~/.config/opencode/opencode.json`.
- This is intentional: OpenCode accepts package spec strings in `plugin`, and
  OPAVS uses a local file plugin spec (`opavs@file://...`) so install works
  immediately without publishing to a registry.

### Phase slash commands

Run `opavs plugin install all` to install the global integrations. Claude,
Codex, and OpenCode receive five commands:

- `/opavs-orient`
- `/opavs-plan`
- `/opavs-act`
- `/opavs-verify`
- `/opavs-ship`

Each command verifies that the current repository is OPAVS-enabled, sets the
matching uppercase phase, and orchestrates that phase's workflow. Arguments are
treated as additional context and are never executed as shell commands.

Gemini retains its extension and context integration but does not receive the
phase slash commands. Re-running plugin installation updates changed artifacts;
when nothing changed, the target reports that it is already up to date.

`opavs guard` is meant to be wired as a `PreToolUse` hook. Claude and Codex use
the matcher `Edit|Write|Bash`; OpenCode also routes `apply_patch` through the
guard. Inside an OPAVS-enabled repository, edit, write, patch, and arbitrary
shell mutations are allowed only in `ACT`, while `git commit` and `git push`
are allowed only in `SHIP`. Outside `ACT`, Bash fails closed to an allowlist of
OPAVS phase/task queries, read-only Git and discovery commands, Cargo metadata,
and phase-appropriate verification or handoff commands. Unknown commands are
denied.

**Fail-open by design outside opavs-enabled repos.** Resolution walks upward
from the target directory but stops at the nearest Git repository or worktree
boundary. If no `.ctx/opavs/tasks.yaml` is found before that boundary (or the
hook payload has no target directory), `opavs guard` allows the call. This
makes global installation safe, but a repository that should be gated and is
not initialized will silently allow everything. Verify its own
`.ctx/opavs/tasks.yaml` exists before assuming it is under phase discipline.

### Task graph (optional companion)

```
opavs tasks list                      # list all tasks with status
opavs tasks runnable                  # tasks not done, with all deps done
opavs tasks validate                  # unknown-dependency and cycle detection
opavs tasks set-status <id> <status>  # todo | in_progress | done
opavs tasks import <path>             # merge an external GODMODE.tasks.yaml
                                       # into this repo's graph (upsert by id,
                                       # existing task status is preserved)
```

## Architecture

Hexagonal: `domain` holds `Phase`/`Task` types, the `PhaseStore`/`TaskStore`
ports, and pure task-graph logic with zero I/O. `adapters` implements those
ports against the filesystem; `guard` owns pure policy decisions and shell
classification. `init`, `plugin`, and `upgrade` are filesystem/network-facing
adapters. `main.rs` is the composition root wiring clap subcommands to them.

## Build

```
cargo check
cargo clippy --all-targets
cargo test
```
