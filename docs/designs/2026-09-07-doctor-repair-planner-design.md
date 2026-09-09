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
}
```

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
2. Transform: `FsArtifactReader` supplies optional artifact contents to `doctor::inspect`, which evaluates repository state and all supported client integrations.
3. Sink: the CLI renders findings and advisory repair actions, then exits nonzero only when the report contains errors.

## Hexagonal Boundaries

- **Port**: `ArtifactReader` in `opavs::doctor` abstracts read-only artifact access.
- **Adapter**: `FsArtifactReader` in `opavs::adapters` reads filesystem artifacts.
- **Domain service**: `doctor::inspect` classifies snapshots without mutating files or launching commands.

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
