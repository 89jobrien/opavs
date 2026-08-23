---
name: run-opavs
description: Build, run, and drive the opavs CLI (phase gating, task graph, PreToolUse guard). Use to run opavs, test the guard hook, or verify a change with a real init/phase/tasks/guard flow.
---

Paths below are relative to the opavs repo root (`/Users/joe/dev/opavs`), not to
this skill directory.

opavs is a CLI, not a GUI/server — there's no window to screenshot. The agent
path is the driver script, which exercises every subcommand (`init`, `phase
get/set`, `tasks list/runnable/validate/set-status/import`, `guard`) against a
disposable scratch repo and prints each command's real output.

## Build

```
cargo build
```

Produces `target/debug/opavs`.

## Run (agent path)

```
nu .claude/skills/run-opavs/smoke.nu
```

Optionally pass an explicit binary path as the first arg (defaults to
`target/debug/opavs`, building it first if missing). The script:

1. Creates a scratch repo in a temp dir and runs `opavs init` on it.
2. Confirms `opavs phase get` defaults to `ORIENT`.
3. Writes a sample `GODMODE.tasks.yaml` (two tasks, one dependent) and runs
   `opavs tasks import`, `list`, `runnable`, `validate`, `set-status`.
4. Walks phases `PLAN -> ACT -> SHIP`, piping synthetic PreToolUse hook JSON
   into `opavs guard` at each stage to confirm: edits denied in `PLAN`,
   allowed in `ACT`; `git commit` denied in `ACT`, allowed in `SHIP`.
5. Cleans up the scratch repo.

Every line printed is real command output — read it, don't just check the
exit code, to confirm allow/deny verdicts landed where expected.

## Run (human path)

Same subcommands, run directly against a real repo:

```
opavs init .
opavs phase get
opavs phase set ACT
opavs tasks list
```

`opavs guard` is normally invoked by Claude Code's PreToolUse hook, not by
hand — it reads hook JSON on stdin and writes a `permissionDecision` verdict
to stdout. To drive it manually, pipe JSON in:

```
'{"tool_name": "Edit", "tool_input": {"file_path": "/repo/src/main.rs"}, "cwd": "/repo"}' | opavs guard
```

## Test

```
cargo test
```

## Gotchas

- **Phases are uppercase and unforgiving.** `opavs phase set plan` fails with
  `invalid phase: plan (expected ORIENT|PLAN|ACT|VERIFY|SHIP)` — it does not
  lowercase or fuzzy-match. Always pass `ORIENT`/`PLAN`/`ACT`/`VERIFY`/`SHIP`.
- **`opavs tasks *` and `opavs phase *` resolve the repo root by walking up
  from the current directory** looking for `.ctx/opavs/tasks.yaml` — they
  ignore `$PWD`-independent flags entirely. Running them outside an
  `opavs init`-ed tree (or a subdirectory of one) fails with `no
.ctx/opavs/tasks.yaml found ... run 'opavs init' first`. `cd` into the
  scratch repo before calling anything but `init`/`guard`.
- **`opavs guard` needs `cwd` in the hook JSON** (or falls back to the
  `$PWD` env var) to resolve which repo's phase gates the decision — a
  hand-built hook payload missing `cwd` while your shell's cwd isn't the
  target repo silently resolves to the wrong (or no) repo and always allows.
- **A repo with no `.ctx/opavs/tasks.yaml` guards nothing.** `evaluate_guard`
  treats "no repo root found" as an unconditional allow, not a deny — so
  running `guard` against a plain, non-opavs repo is a silent no-op.
