# Design: Doctor Repair Planner

## Goal

Add a read-only `opavs doctor` repair planner that diagnoses repository state and client integration drift before users rely on phase enforcement.

## Approved Approach

Build the approved full repair planner across repository state and all supported client integrations without applying repairs.

## Crate Ownership

- **Owner crate**: `opavs` -- the existing library owns phase state, plugin artifacts, and CLI composition.
- **Affected crates**: none; this repository contains one package with library and binary targets.

## Public API

### Traits

```rust
pub trait ArtifactReader {
    fn read(&self, path: &Path) -> Result<Option<String>>;

    fn is_ignored(
        &self,
        repo_root: &Path,
        relative_path: &Path,
    ) -> Result<Option<bool>> {
        Ok(None)
    }
}
```

`read` returns `Ok(None)` only when an artifact does not exist, returns UTF-8 text
for an existing artifact, and propagates read or decoding failures. The ignore-query
capability returns `Some(true)` for ignored paths, `Some(false)` for paths Git confirms
are not ignored, and `None` when no Git decision is available. Its default makes the
capability optional for readers that cannot query Git; doctor then falls back to reading
`.gitignore`. Process-launch failures propagate as inspection failures.

The reviewed implementation temporarily combines these capabilities on
`ArtifactReader`. If the architecture work splits the focused ports, the equivalent
ignore contract is:

```rust
pub trait IgnoreQuery {
    fn is_ignored(
        &self,
        repo_root: &Path,
        relative_path: &Path,
    ) -> Result<Option<bool>>;
}
```

Splitting the ports does not change either contract: `inspect` receives both capabilities,
while filesystem reads and Git ignore queries remain independently replaceable.

### Types

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingLevel {
    Pass,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairAction {
    RunInit { repo_root: PathBuf },
    InstallPlugin { target: Target, home: PathBuf },
    Manual { description: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorFinding {
    pub code: String,
    pub level: FindingLevel,
    pub message: String,
    pub repair: Option<RepairAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReport {
    pub findings: Vec<DoctorFinding>,
}

pub struct FsArtifactReader;
```

### Functions

```rust
pub fn inspect(
    reader: &impl ArtifactReader,
    repo_root: &Path,
    home: &Path,
) -> Result<DoctorReport>;

impl DoctorReport {
    pub fn has_errors(&self) -> bool;
}
```

## Data Flow

1. Source: the CLI resolves the requested repository path and home directory without requiring an existing OPAVS marker.
2. Transform: `FsArtifactReader` supplies optional artifact contents and Git ignore decisions to `doctor::inspect`, which evaluates repository state and all supported client integrations.
3. Sink: the CLI renders findings and advisory repair actions. A rendered `Error` finding produces a nonzero exit after the report is shown. Setup failures, including unresolved home-directory configuration, and inspection failures from artifact reads or Git process execution also exit nonzero through the CLI error path and may occur before a report is available. Malformed inspected content is normally represented by an `Error` finding in the report.

## Hexagonal Boundaries

- **Ports**: the reader and ignore-query contracts in `opavs::doctor` abstract text reads and Git ignore decisions. They are currently methods on `ArtifactReader`; a focused split names the second port `IgnoreQuery` without changing its behavior.
- **Adapter**: `FsArtifactReader` in `opavs::adapters` reads filesystem artifacts and may launch `git check-ignore --quiet --no-index` as a read-only inspection. Exit codes zero and one become ignored/not-ignored decisions; unavailable repository context or an indeterminate Git result yields no decision, while launch failures propagate.
- **Domain service**: `doctor::inspect` is mutation-free and command-agnostic through its injected ports. It requests observations but neither knows nor controls whether an adapter satisfies them with direct I/O or a read-only subprocess.

## Integration Points

- `src/lib.rs` exports the new `doctor` module.
- `src/main.rs` adds the `doctor` command and renders its report.
- `src/plugin.rs` provides the shared expected-artifact model used by installation and diagnosis.
- `tests/cli.rs` verifies healthy and unhealthy command behavior end to end.

## Out of Scope

- Applying repairs or changing files.
- JSON output.
- `opavs init --repair`.
- Configurable verification policies or enforced phase transitions.
- New external dependencies.

## Risk

- [x] Breaking API changes: no; all APIs are additive.
- [x] Persisted schema changes: no.
- [x] New external dependency: no.
- [x] Feature flag required: no.
- [x] Filesystem mutation: no; the command and port are read-only.
