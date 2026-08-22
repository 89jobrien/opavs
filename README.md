# opavs

Orient → Plan → Act → Verify → Ship. A workflow-phasing CLI: it gates what an
agent (or a person) is allowed to do based on which phase a repo is in —
read-only exploration, then a written plan, then edits, then verification,
then commit/push — each with a wider blast radius than the last. The point is
enforcement, not ceremony: a `PreToolUse` hook denies `Edit`/`Write` outside
`ACT` and `git commit`/`git push` outside `SHIP`, so the discipline is real
state on disk, not just a convention someone has to remember to follow.

Reimplements (in Rust) the shell-based `opavs-phase.sh` / `opavs-guard.sh`
pair from the `opavs` Claude Code plugin, for any repo using the
`.ctx/opavs/` state directory convention.

A lightweight task-graph companion rides alongside the phase machinery, for
repos that want to track what's runnable inside a phase — but the phase gate
is what this tool is _for_.

## Commands

### Phase discipline (core)

```
opavs init [repo_root]     # scaffold .ctx/opavs/{tasks.yaml, memory-bank/}, AGENTS.md
opavs phase get            # print current phase (defaults to ORIENT)
opavs phase set <PHASE>    # ORIENT | PLAN | ACT | VERIFY | SHIP

opavs guard                # PreToolUse hook entrypoint: reads Claude Code hook
                            # JSON on stdin, emits an allow/deny
                            # permissionDecision on stdout
```

`opavs guard` is meant to be wired as a `PreToolUse` hook (matcher
`Edit|Write|Bash`) in a plugin's `hooks.json`, denying `Edit`/`Write` outside
the `ACT` phase and `git commit`/`git push` outside `SHIP`, scoped to any repo
containing `.ctx/opavs/tasks.yaml`.

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
ports, and pure logic (guard decisions, graph validation, cycle detection,
runnable-task resolution) with zero I/O. `adapters` implements those ports
against the filesystem. `main.rs` is the composition root wiring clap
subcommands to the adapters.

## Build

```
cargo check
cargo clippy --all-targets
cargo test
```
