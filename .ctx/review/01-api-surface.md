# opavs API Surface Review (v0.0.1)

## Public items inventory

- domain.rs: Phase (enum), Phase::parse, Phase Display impl, PhaseStore (trait), TaskStatus (enum), Task (struct, all fields pub), TaskGraph (struct, pub tasks), TaskStore (trait), GraphError (enum) + Display, validate(), runnable_tasks()
- adapters.rs: FsPhaseStore + new, FsTaskStore + new
- guard.rs: Verdict (enum), decide(), command_touches_commit_or_push()
- import.rs: read_external_graph(), merge()
- init.rs: scaffold()
- repo.rs: resolve_repo_root()
- main.rs: no pub items (binary crate) -- Cli, Command, etc. are private, correct.

## Findings

### Over-exposed (should be pub(crate))

- adapters.rs:5,41 FsPhaseStore/FsTaskStore are pub even though this is a bin-only crate with no lib.rs -- nothing external can ever see them; first candidates for pub(crate) if a lib target is added later.
- guard.rs:43 command_touches_commit_or_push is a guard-hook implementation detail; only main.rs calls it. Should be pub(crate).
- repo.rs:5 resolve_repo_root is only consumed by main.rs -- reasonable as pub(crate).
- Note: with no src/lib.rs, all pub markers here are equivalent to pub(crate) today. Flagging for awareness only, not urgent.

### API surface inconsistencies

- domain.rs:15 Phase::parse returns anyhow::Result<Phase>, but validate() (domain.rs:99) returns Result<(), GraphError> -- mixed error-handling contract within the same domain module. Consider a typed PhaseParseError for symmetry.
- domain.rs:41-44,76-79 PhaseStore/TaskStore ports use anyhow::Result too, reinforcing the inconsistency with GraphErrors typed style in the same file.
- import.rs:14 merge() returns a bare TaskGraph (infallible) while read_external_graph() (import.rs:6) returns Result<TaskGraph> -- sensible but undocumented contract.
- init.rs:20 scaffold() returns Result<Vec<String>> (paths as strings) instead of Result<Vec<PathBuf>>, despite operating entirely on PathBuf internally -- loses type information for callers.

### Awkward to change later

- guard.rs:12-17 decide() takes repo_root: &str while everything else (resolve_repo_root, FsPhaseStore::new) uses &Path/PathBuf. Only used for string interpolation today; prefer &Path with .display() at the call site.
- domain.rs:41-44,76-79 PhaseStore/TaskStore set/save take &self not &mut self. Works for FS adapters via std::fs::write, but forces future in-memory/test adapters into interior mutability instead of natural &mut self.
- domain.rs:56-62 Task has all-public fields with no constructor -- any future invariant (non-empty id, deduped depends_on) requires a breaking change to add a constructor and privatize fields.
- guard.rs:4-7 Verdict::Deny(String) embeds a pre-formatted human-readable message directly in the enum, freezing reason as String -- harder to add structured fields later without breaking match arms.
