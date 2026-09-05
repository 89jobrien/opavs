# Design: OPAVS Phase Slash Commands

## Goal

Install one global slash command for each OPAVS phase across Claude, Codex, and OpenCode,
with a shared canonical workflow that switches phase and orchestrates the matching work.

## Approved Approach

Use the **Shared Phase Playbook** approach: define five workflows once in the `opavs`
crate and render host-specific command files through `opavs plugin install`.

## Crate Ownership

- **Owner crate**: `opavs` — the existing `plugin` adapter already installs global agent
  integrations and owns target-specific paths and file rendering.
- **Affected crates**: none; this is a single-crate project.

## Public API

No new public API is required. The existing entry point remains:

```rust
pub fn install(target: Target, home: &Path) -> Result<Vec<String>>;
```

The implementation adds one private canonical command descriptor:

```rust
struct PhaseCommand {
    name: &'static str,
    phase: Phase,
    description: &'static str,
    workflow: &'static str,
}
```

and private rendering/installation helpers:

```rust
fn install_phase_commands(target: Target, home: &Path) -> Result<Vec<String>>;

fn render_phase_command(target: Target, command: &PhaseCommand) -> String;
```

The canonical collection contains exactly five entries named `opavs-orient`,
`opavs-plan`, `opavs-act`, `opavs-verify`, and `opavs-ship`.

## Command Behavior

Every command:

1. Runs `opavs phase get` to verify the current directory belongs to an initialized OPAVS
   repository.
2. Stops with `opavs init` guidance if repository resolution fails; it never initializes
   a repository implicitly.
3. Runs `opavs phase set <PHASE>` using the matching uppercase phase.
4. Treats optional user arguments as workflow context, never as shell input.
5. Executes the canonical workflow for that phase.

Phase workflows:

- **ORIENT**: read project guidance, repository state, memory-bank context, task graph,
  and runnable tasks; remain read-only.
- **PLAN**: clarify scope, obtain design approval, produce the plan in conversation, and
  optionally maintain the graph through `opavs tasks`; do not use Edit/Write.
- **ACT**: select runnable work, update task status, implement only approved scope, and
  preserve unrelated changes.
- **VERIFY**: run change-appropriate checks without editing; on failure report evidence
  and set the phase back to ACT.
- **SHIP**: rerun verification, then commit and push because invoking the slash command is
  explicit shipping authorization; update memory/handoff state after success.

## Installation Paths

- Claude:
  `$HOME/.claude/plugins/local-marketplace/plugins/opavs/commands/opavs-<phase>.md`
- Codex: `$HOME/.codex/commands/opavs-<phase>.md`
- OpenCode: `$HOME/.config/opencode/commands/opavs-<phase>.md`

Claude and Codex receive their native command frontmatter. OpenCode receives Markdown
command frontmatter with `description`; all three bodies preserve the same semantics and
argument placeholder.

Gemini remains unchanged because the approved command targets are Claude, Codex, and
OpenCode.

## Data Flow

1. **Source**: five `PhaseCommand` values in `opavs::plugin` hold canonical workflow text.
2. **Transform**: `render_phase_command` adds target-specific frontmatter and argument
   syntax.
3. **Sink**: `install_phase_commands` writes files through the existing idempotent
   `write_if_changed` adapter and returns changed paths to the CLI.

## Hexagonal Boundaries

- **Domain**: existing `Phase` values define valid phase identity; no new domain port is
  needed for static command templates.
- **Adapter**: `plugin.rs` maps canonical commands to host filesystem conventions.
- **Composition root**: the existing `opavs plugin install <target>` dispatch remains
  unchanged.

## Tests

- Extend plugin unit tests to assert each supported target installs all five commands at
  the correct paths.
- Assert each file contains its matching uppercase `opavs phase set` command and workflow
  marker.
- Assert a second install reports no command changes.
- Extend CLI integration coverage for `plugin install ... --home` without mutating the
  real user home.
- Preserve existing hook, skill, plugin-package, and config assertions.

## Documentation

Update `README.md` with the five slash command names, supported targets, installation
command, and phase-transition behavior.

## Out of Scope

- A single `/opavs <phase>` dispatcher.
- Project-local command installation.
- Gemini slash commands.
- Automatic `opavs init`.
- Changes to guard permissions, phase parsing, task schema, or public API.
- Executing user-supplied command arguments as shell input.

## Risk

- [x] Breaking API changes: no.
- [x] Serialization changes: no.
- [x] New external dependency: no.
- [x] Feature flag required: no.
- [ ] Host format drift: contained in one renderer and covered by per-target tests.
- [ ] Existing dirty plugin work: preserve and extend the current uncommitted
      `src/plugin.rs`, `src/main.rs`, `src/lib.rs`, `tests/cli.rs`, and `README.md` changes;
      do not revert or overwrite them.
