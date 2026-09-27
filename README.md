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

```text
git clone https://github.com/89jobrien/opavs.git
cargo install --path opavs
opavs plugin install all
```

## Commands

### Phase discipline (core)

```text
opavs init [repo_root]     # scaffold OPAVS state and update instruction files
opavs phase get            # print current phase (defaults to ORIENT)
opavs phase set <PHASE>    # ORIENT | PLAN | ACT | VERIFY | SHIP

opavs guard                # PreToolUse hook entrypoint: reads Claude Code hook
                            # JSON on stdin, emits an allow/deny
                            # permissionDecision on stdout

opavs plugin install <target> [--home /path/to/home]
                            # install OPAVS integration for one target:
                            # claude | codex | gemini | opencode | all

opavs doctor [repo_root] [--home /path/to/home]
                            # diagnose repository and client integration state

opavs upgrade               # download and install the newest GitHub release
```

### Doctor

`opavs doctor` performs a read-only diagnosis of the repository scaffold, task
graph, ephemeral phase state, and supported client integrations. Its filesystem-
and Git-backed adapter reads configuration and uses read-only Git inspection to
check whether phase state is ignored.

Each check is reported as `Pass`, `Warning`, or `Error`. A warning identifies a
condition worth correcting that does not by itself make enforcement untrustworthy;
an error means OPAVS should not be relied on until it is repaired. Printed repair
steps are advisory: doctor does not apply them or otherwise mutate inspected files.

The command exits nonzero after rendering any `Error` finding. Setup failures,
such as an unavailable home directory, and inspection failures from unreadable or
non-UTF-8 artifacts or a failed Git process also exit nonzero and may prevent a
report from being rendered. Malformed inspected content is normally reported as
an `Error` finding with an advisory repair.

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
`opavs uninstall` reverses it.

### Uninstall

`opavs uninstall` removes what OPAVS installed, and is driven by the same artifact
list `opavs plugin install` uses, so the two cannot drift.

```bash
opavs uninstall --dry-run                 # report everything, change nothing
opavs uninstall                            # remove every client integration
opavs uninstall --target claude           # remove one integration
opavs uninstall --purge-repo --repo .     # also strip this repo's scaffolding
```

Two rules keep it from destroying anything it did not create:

- **Owned** artifacts — the skill, phase commands, hook manifest, plugin
  package — are deleted only when their contents still match what OPAVS wrote.
  A file you have edited is reported and left in place.
- **Shared** configuration is never deleted. The OPAVS entry is excised from
  `~/.codex/hooks.json`, `~/.gemini/extensions/extension-enablement.json`, and
  `~/.config/opencode/opencode.json`, and everything around it is left as it was.
  A file OPAVS never touched comes out byte-identical.

`--purge-repo` requires an explicit `--repo` path rather than resolving one from
the working directory, because it deletes the task graph. It removes
`.ctx/opavs/`, `OPAVS.md`, the appended workflow block in `AGENTS.md`/`CLAUDE.md`,
and the `.gitignore` line `init` added. Memory-bank files survive if you have
written into them, and are removed only while they still hold the generated
template.

Uninstall is gated to the `SHIP` phase, since it deletes files and takes work off
the machine. `--dry-run` only reports, so it stays available in every phase.

The command removes OPAVS's artifacts but not the executable itself; remove that
with `cargo install --uninstall --name opavs`.
when nothing changed, the target reports that it is already up to date.

`opavs guard` is meant to be wired as a `PreToolUse` hook. Claude and Codex use
the matcher `Edit|Write|Bash`; OpenCode also routes `apply_patch` through the
guard. Inside an OPAVS-enabled repository, edit, write, patch, and arbitrary
shell mutations are allowed only in `ACT`, while `git commit` and `git push`
are allowed only in `SHIP`. Outside `ACT`, Bash fails closed to an allowlist of
OPAVS phase/task queries, read-only Git and discovery commands, Cargo metadata,
and phase-appropriate verification or handoff commands. Unknown commands are
denied.

Within that allowlist, reading a file (`< file`) and duplicating a file descriptor
(`2>&1`) are permitted in every phase: neither mutates anything. Writing to a path
(`> file`, `>> file`, `&> file`) and command substitution (`$(...)`, backticks, and
the process substitutions `<(...)` and `>(...)`) are permitted only in `ACT`, as is
`tee`, which writes files by design.

**Fail-open by design outside opavs-enabled repos.** Resolution walks upward
from the target directory but stops at the nearest Git repository or worktree
boundary. If no `.ctx/opavs/tasks.yaml` is found before that boundary (or the
hook payload has no target directory), `opavs guard` allows the call. This
makes global installation safe, but a repository that should be gated and is
not initialized will silently allow everything. Verify its own
`.ctx/opavs/tasks.yaml` exists before assuming it is under phase discipline.

### Task graph (optional companion)

```text
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
ports, and pure task-graph logic with zero I/O. `doctor` is the mutation-free
diagnosis and repair-planning service. `adapters` implements state ports against
the filesystem and backs doctor with filesystem reads plus read-only Git ignore
queries. `guard` owns pure policy decisions and shell classification. `init`,
`plugin`, and `upgrade` are filesystem/network-facing adapters, while plugin
installation, doctor, and `uninstall` all read the same client-artifact
expectations from `plugin`, so install, diagnosis, and removal cannot disagree
about what OPAVS owns. `main.rs` is the composition root wiring clap subcommands
to them.

## Build

```text
cargo check
cargo clippy --all-targets
cargo test
```
