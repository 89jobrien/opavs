<!-- opavs-workflow:begin -->

# Phased workflow

Unless the user clearly opts out (for example, **"skip plan, just fix it"**), every
non-trivial task progresses through five phases. Short confirmations such as
**"do it"**, **"act"**, and **"go"** advance to the next phase.

## Phases

<opavs-phase name="ORIENT" mode="read-only" response-header="# Phase: ORIENT" skills="opavs">
Default phase. Read files, search code, run `opavs phase get`, and check the task
graph with `opavs tasks list`. Summarize the branch, dirty files, and relevant
context. Do not modify the repository. End by stating what you found and which
phase comes next.
</opavs-phase>

<opavs-phase name="PLAN" mode="read-only" response-header="# Phase: PLAN" skills="brainstorm, writing-plans">
Produce a written plan covering files to touch, approach, tests, and risks. Stay
read-only: no edits and no builds that write output. For complex work, invoke an
applicable brainstorming or planning skill. End with "Type ACT to proceed" or
suggest refinements.
</opavs-phase>

<opavs-phase name="ACT" mode="read-write" response-header="# Phase: ACT" skills="task-driven-development, parallel-agents">
Enter when the user approves with "act", "go ahead", or "do it". Set the phase
with `opavs phase set ACT`, edit files, run commands, and dispatch subagents. Use
the task graph for multi-step work and parallel agents for independent tasks.
After finishing, transition to VERIFY automatically.
</opavs-phase>

<opavs-phase name="VERIFY" mode="read + test" response-header="# Phase: VERIFY" skills="verification-before-completion">
Set the phase with `opavs phase set VERIFY`. Run the relevant checks, including
`cargo check`, `cargo clippy`, and `cargo test` for Rust changes. Report results.
If failures exist, return to ACT to fix them. When green, state readiness and ask
the user to SHIP.
</opavs-phase>

<opavs-phase name="SHIP" mode="commit/push" response-header="# Phase: SHIP" skills="cap, handoff">
Enter only with explicit user approval. Set the phase with `opavs phase set SHIP`,
commit, push, and update the handoff or memory bank. After shipping, return to
ORIENT for the next task.
</opavs-phase>

## Phase transitions

- **Users can skip phases**: "skip plan, implement now" jumps to ACT. "Just fix
  it" implies ORIENT -> ACT -> VERIFY in one pass; SHIP still requires explicit
  approval.
- **After each ACT turn**, default to VERIFY unless the user says otherwise.
- **Multiple ACT turns** are allowed; the user can keep approving refinements.
- When the user gives a lettered choice or short confirmation, advance to the
  most obvious next phase without asking again.

## Skill invocation rule

Before responding in any phase, check whether an available skill applies. Invoke
process skills such as brainstorming or systematic debugging before implementation
skills such as task-driven development or parallel agents.

## Task graph

Tasks live in `.ctx/opavs/tasks.yaml`. Use the `opavs tasks` CLI for state
transitions. Independent chains can run in parallel. A task is runnable when all
items in `depends_on` are done.

## Memory bank

- Persistent context lives in `.ctx/opavs/memory-bank/`.
- Read `active-context.md` and `progress.md` before substantive work.
- Update the memory bank after milestones and after shipping.
- See `AGENTS.md` for repository-specific guidance.

## Agent-specific guidance

For subagent conventions and repository-specific instructions, see `AGENTS.md`.

<!-- opavs-workflow:end -->
