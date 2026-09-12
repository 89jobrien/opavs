# Changelog

All notable changes to OPAVS are documented here. This initial changelog covers the full
repository history because no release tags exist yet.

## [Unreleased] - 2026-09-12

### Features

- Add read-only `opavs doctor` diagnostics and repair recommendations (`037d5ac`).
- Expand lifecycle support across Claude, Codex, Gemini, and OpenCode integrations (`62422c5`).
- Stop repository-root discovery at Git worktree boundaries (`c7795b0`).

### Fixes

- Harden doctor APIs, module boundaries, plugin repair convergence, and error-path coverage
  following architecture review (`63d6ee9`).
- Correct doctor contracts and usage guidance (`4d74982`, `8640ce7`).
- Address earlier high- and medium-priority architecture review findings (`122cc11`).
- Integrate the earlier MoA review fix branch (`b7cf556`).

### Tests

- Add property, fuzz, conformance, and integration coverage (`80021a4`).

### Documentation

- Clarify that the OPAVS plugin delegates enforcement to the compiled binary (`0501830`).

### Continuous Integration

- Add a Crux-driven GitHub Actions pipeline (`4101c8d`).
- Specify the `crux-cli` package during CI installation (`590350d`).
- Cache the Crux binary by upstream commit (`9a38cf4`).

### Non-Conventional Commits

- `af0a8e0` Initial scaffold: opavs phase-gating CLI with task-graph companion.
- `a89c5a1` integrate(opavs): merge MoA code fixes.
- `de82447` integrate(docs): merge MoA review fixes.
