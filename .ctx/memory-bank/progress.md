# Progress

## 2026-09-05

- Added global OPAVS phase command installation for Claude, Codex, and OpenCode.
- Added canonical OPAVS instructions and instruction-file integration during `opavs init`.
- Added fail-closed non-ACT shell policy and Git worktree boundary handling.
- Added self-upgrade and release workflow support.
- Reconciled generated and static documentation with the implemented guard, repository,
  initializer, plugin, upgrade, testing, and smoke-test behavior.
- Passed formatting, compilation, clippy, 102 tests, the end-to-end smoke flow, diff checks,
  and a specialist documentation re-review with no remaining findings.

## 2026-10-01

Session ran entirely in sibling repos; no opavs source changed.

- Audited all Rust CLI binaries under `~/dev`: 103 of 127 user-facing binaries lacked
  `--version`. Fixed 63, each verified by building and running the binary — never by
  static scan, which was blind to every shared-`Parser` fix and misreported coverage
  twice (18/127 before, 51/127 after, versus 58 then 63 actual).
- Landed ~48 commits across 34 repos via 8 parallel subagent batches. Most were a
  one-line `#[command(..., version)]`; 7 `hj` aliases and 4 `warpx` bins were each
  covered by a single edit to a shared struct; 5 MCP servers got a guard clause before
  server startup instead of a new clap dependency.
- `surgeon` (`b523638`): slash had deleted `ExecutionPlan`/`ExecutionStep` in `21457e0`,
  so `surgeon-agents` had not compiled since 2026-09-18. Restoring was barred by slash's
  own architecture test, so ported to the slash-lang `Program` AST and added 4 tests,
  mutation-checked to confirm they fail when the `&&` chaining is wrong.
- `neusym` (`5408566`): crux moved `Crux<T>` to a new `crux-schema` crate rather than
  deleting it. Repointed 4 imports and added 5 newly-required fields at 10 struct-literal
  sites, using `--all-targets` after plain `cargo check` missed 2 test-only sites.
- Fixed 4 binaries that silently discarded `--version`: `testx` printed its whole report
  and exited 0, `parsex` never terminated, `insightx` errored, warpx's
  `generate_settings_schema` exited 1. Excluded 2 more as misclassified non-CLIs
  (`echo-plugin` is a JSON-RPC test fixture, `e2e-placeholder` is `fn main() {}`).
- Re-anchored 5 commits stranded on throwaway topic branches onto `chore/version-flags`
  in per-repo worktrees. Verification caught that notfiles had two separate version
  commits and only the tip one had been rescued — 3 binaries would have shipped broken.
- Wrote the mistake ledger (`.ctx/memory-bank/mistakes.md`) and the reflection
  (`.ctx/godmode/reports/reflect/reflect-2026-10-01.md`).

Open: 5 rescue branches unmerged, `groovenance`'s change is in a vendored third-party
tree and will not survive a re-vendor, 4 warpx bins unverified, no `--version`
regression gate, and `checkup::git_health::tests::format_clean_report` is red on `main`
(pre-existing, stale assertion).
