# Design: Quote-Aware Shell Command Parsing

## Goal

Stop denying valid shell commands. The `PreToolUse` guard splits a compound
command line on every `;`, `&`, and `|` it contains, so a delimiter inside a quoted
argument tears one command into two and the remainder names no known program. A
read-only search such as `rg "foo;bar" src` is refused in `VERIFY` today. Alongside
that, the handling of redirection and command substitution is a substring scan
outside the policy table, and the read-only pipe filters have no allowlist entry at
all, so `cargo test 2>&1 | tail -30` is refused for a reason unrelated to safety.

## Approved Approach

Approach B, a full tokenizer, in a new `src/shell.rs` module. Commands are parsed
into typed `Command` values carrying real argument vectors and an explicit set of
effects. `Operation` and `permits` stay in `src/guard.rs` and are not moved; effects
are a precondition checked ahead of classification rather than a ninth `Operation`.

Both `shell_command_allowed` and `command_touches_commit_or_push` call the same
`parse`, which removes the duplicated split that let the original misclassification
bug have two independent halves. `main.rs`'s third parser of the same string,
`extract_dash_c_target`, is replaced by a `guard::git_dash_c_target` built on that
same `parse`, so all three consumers read a line the same way.

## Crate Ownership

- **Owner crate**: `opavs` -- the existing library owns phase policy and guard
  classification.
- **Affected crates**: none. This repository contains one package with library and
  binary targets.
- **New dependencies**: none. `shell-words` and `shlex` were considered and rejected:
  neither exposes command-boundary detection, so the hard half stays hand-written
  while the easy half would be delegated.

## Context Map

### Files to Modify

| File           | Purpose                         | Changes Needed                                                                                                                                 |
| -------------- | ------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/shell.rs` | new pure parse layer            | `Effects`, `Command`, `parse`                                                                                                                  |
| `src/lib.rs`   | module declarations             | add `pub(crate) mod shell;` between `repo` and `upgrade`                                                                                       |
| `src/guard.rs` | phase policy and classification | consume `shell::parse`; delete both naive splits; add filter entries; add `effects_require_act` and `command_allowed`; add `git_dash_c_target` |
| `src/main.rs`  | guard composition root          | delete `extract_dash_c_target` and its two tests; call `guard::git_dash_c_target`                                                              |
| `tests/cli.rs` | end-to-end guard tests          | integration coverage through the real binary                                                                                                   |
| `README.md`    | guard contract                  | one sentence recording the effects rule                                                                                                        |

### Dependencies (may need updates)

| File          | Relationship                                                                                                                                                                                                                                                                                                    |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/main.rs` | sole external consumer of the guard API: `command_touches_commit_or_push` (line 346), `shell_command_allowed` (line 385), `decide` (line 392), `Verdict` (lines 398-399). The first two keep their signatures, so those call sites are untouched; the `extract_dash_c_target` call at line 342 is the one edit. |

### Third parser, now in scope

`src/main.rs:431` `extract_dash_c_target` parses the same command string a third time,
with a whole-line `split_whitespace` and no segment awareness, duplicating the `-C`
handling that `guard::git_invocation` already owns. It is corrected here rather than
left in place, because leaving a known-broken parser beside a correct one in the same
change is a defect that outlives the change. See Risk for the mechanism.

### Test Coverage

| Test                           | Covers                                                                             | Affected?                                                       |
| ------------------------------ | ---------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| `src/guard.rs` `mod tests`     | ~30 inline tests over `shell_command_allowed` and `command_touches_commit_or_push` | yes -- all route through the new parser                         |
| `src/guard.rs` `mod proptests` | panic-freedom and verdict stability                                                | yes -- new tokenizer properties added                           |
| `src/main.rs` `mod tests`      | `parse_guard_request_*`, `evaluate_guard_*`, `extract_dash_c_target_*`             | yes -- the two `extract_dash_c_target` tests move to `guard.rs` |
| `tests/cli.rs`                 | 10 `guard_*` end-to-end tests                                                      | yes -- all traverse the parser                                  |
| `src/plugin.rs:622`            | `verify_and_ship_match_the_non_act_shell_allowlist`                                | no -- asserts only the phrase "non-ACT safe allowlist"          |
| `tests/documentation.rs`       | doctor and CLAUDE.md contracts                                                     | no                                                              |

**Coverage gap to close:** no test currently asserts that a quoted delimiter does not
split a command, nor that redirection is distinguished by kind. Both are added.

### Reference Patterns

| File                          | Pattern to Follow                                                                                                         |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `src/repo.rs`                 | shape for a small pure module: one-line `//!` doc, public items with a "why" doc comment, inline `#[cfg(test)] mod tests` |
| `src/integration.rs`          | `pub(crate)` visibility for internals no consumer outside the library needs                                               |
| `src/guard.rs` `classify_git` | argument-sensitive matching for `git branch` / `git remote`, which is the precedent for the `sort` and `uniq` conditions  |

### Risk

- [ ] **The parser becomes a bypass surface.** A bug that under-reports a hazard
      permits a file write outside `ACT`. This inverts the cost of a defect relative
      to the status quo, where a bug causes a wrong denial. Mitigated by property
      tests plus explicit per-hazard assertions; the top risk in this change.
- [ ] **Allowlist additions are a policy loosening.** Nine programs newly permitted in
      every non-`ACT` phase. If any has an output-file flag that was missed, the gate
      permits a file write. `sort` and `uniq` are handled explicitly; the other seven
      must be confirmed to have no output-file flag.
- [ ] **Regression surface is the 10 existing `guard_*` integration tests**, all of
      which traverse the new parser and would catch most breakage.
- [ ] `parse` runs three times per `Bash` hook call, once per consumer function.
      Negligible for a `PreToolUse` hook; accepted to keep each consumer a single
      self-contained call.
- [ ] Additive public API only: `git_dash_c_target` is new, and no existing
      signature changes. No serialization change. No CLI output change. Single crate.

## Public API

### Types

```rust
// src/shell.rs -- pub(crate): no consumer outside the library needs these.

/// What a command does beyond invoking its program.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Effects {
    /// Runs a command and splices its output in: `$(...)`, backticks, and the
    /// process substitutions `<(...)` / `>(...)`.
    pub(crate) substitution: bool,
    /// Writes to a path: `> file`, `>> file`, `&> file`, `>| file`.
    pub(crate) file_write: bool,
    /// Reads from a path: `< file`.
    pub(crate) file_read: bool,
    /// Duplicates a descriptor without touching the filesystem: `2>&1`, `1>&2`.
    pub(crate) descriptor_dup: bool,
}

/// One command in a compound line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Command {
    /// Argument vector with quotes removed, so `rg "a b" src`
    /// becomes `["rg", "a b", "src"]`.
    pub(crate) args: Vec<String>,
    pub(crate) effects: Effects,
}
```

### Functions

```rust
/// Split a compound shell line into its commands, honouring quoting.
pub(crate) fn parse(line: &str) -> Vec<Command>
```

Unchanged public signatures, in `src/guard.rs`:

```rust
pub fn shell_command_allowed(cmd: &str, phase: Phase) -> bool
pub fn command_touches_commit_or_push(cmd: &str) -> bool
```

New public function replacing `main.rs`'s private `extract_dash_c_target`:

```rust
/// The path argument to a `git -C <path>` flag, if any command in `line` is a
/// git invocation carrying one.
///
/// Segment-aware by construction: a `-C` belonging to a non-git command is not a
/// directory flag. `rg -C 3 pattern` is grep's context flag, and treating it as
/// git's would hand the resolver a repo root of `3`.
pub fn git_dash_c_target(line: &str) -> Option<String>
```

Implemented with `shell::parse` plus the existing git-program resolution in
`git_invocation`, so the two never disagree about which command is a git command.

`Operation` and `permits` are unchanged. `Operation` keeps its eight variants, and
`each_phase_permits_exactly_its_declared_operations` must continue to pass **unmodified**
-- that test is the regression proving the policy layer needed no new concepts.

New internal helpers in `src/guard.rs`:

```rust
/// Constructs that write files or run an extra command. ACT is the only phase that
/// permits them, and ACT already permits everything else too. Reading a file and
/// duplicating a descriptor are not mutations: the gate governs changing the
/// repository, and every Inspect command already reads the filesystem.
fn effects_require_act(effects: Effects) -> bool {
    effects.substitution || effects.file_write
}

fn command_allowed(command: &Command, phase: Phase) -> bool {
    if effects_require_act(command.effects) && phase != Phase::Act {
        return false;
    }
    match classify(&command.args) {
        Some(operation) => permits(phase, operation),
        // ACT permits commands the gate has no policy for; every other phase
        // fails closed on an unrecognised program.
        None => phase == Phase::Act,
    }
}
```

### Allowlist additions

| Programs                                        | Classification                                                                                            |
| ----------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `head`, `tail`, `grep`, `wc`, `cut`, `tr`, `jq` | `Inspect`, unconditionally                                                                                |
| `sort`                                          | `Inspect` unless `-o` or `--output` appears, which writes a file with no shell redirect involved          |
| `uniq`                                          | `Mutate` when exactly one non-flag operand follows, because `uniq sorted.txt` rewrites that file in place |
| `tee`                                           | excluded -- writing files is its entire purpose                                                           |

`uniq`'s count must discount the value-taking flags `-f`, `-s`, and `-w` and their
long forms, so that `uniq -f 2 sorted.txt` is correctly seen as one operand.

## Data Flow

```
hook JSON
  -> parse_guard_request            (main.rs)
  -> git_dash_c_target              (guard) -> shell::parse
  -> command_touches_commit_or_push (guard) -> shell::parse
  -> resolve(target_dir) -> (repo_root, phase)
  -> shell_command_allowed          (guard) -> shell::parse
  -> decide(effective_tool, is_commit_or_push, phase, repo_root)
  -> Verdict -> JSON
```

`parse` is pure and total: it returns a `Vec<Command>` for any input, including
malformed quoting, because the parser's job is to describe what the line contains and
leave the judgement to policy.

## Hexagonal Boundaries

`src/shell.rs` is pure domain logic with no I/O and no adapter dependency, so it
belongs in core under the crate-per-concern layout. It depends only on `std`. The
guard remains the policy port; no adapter, store, or client-integration code changes.
`src/repo.rs` is the shape reference: a small pure module with inline tests.

## Tests

**Unit, in `src/shell.rs`** -- word extraction with quote stripping across single,
double, and escaped forms; command boundaries at unquoted `;`, `&&`, `||`, `|`, and
newline; no boundary inside quotes; hazard detection; the distinctions that carry the
design, namely `<(cmd)` setting `substitution` rather than `file_read`, `&>` setting
`file_write`, and `2>&1` setting `descriptor_dup` rather than `file_write`; and edges
-- unterminated quote, trailing backslash, empty line, operators only, non-ASCII
arguments.

**Property tests** -- required by the project constitution for hand-rolled string
parsers, and load-bearing rather than ceremonial because the scanner walks a byte
cursor: `parse` never panics on arbitrary UTF-8; quoting a word never changes the
segment count; a quoted `;` or `|` never creates a boundary; `parse("git status")`
yields exactly `["git", "status"]`.

**Unit, in `src/guard.rs`** -- the `effects_require_act` truth table; the
eight-operation phase matrix unchanged; each new filter permitted; `sort` refused with
`-o`; `uniq` refused with one operand; and the headline accuracy cases
`cargo test 2>&1 | tail -30` and `rg "foo;bar" src` permitted in `VERIFY`.

**Integration, in `tests/cli.rs`** -- through the real binary: a quoted-delimiter
command allowed in `VERIFY`; substitution refused in `VERIFY` yet allowed in `ACT`; and
`2>&1` allowed while `> file` is refused in the same phase.

**`-C` resolution** -- `git_dash_c_target` returns the path for `git -C /repo push`;
returns `None` for `rg -C 3 pattern`; returns the git command's path for
`echo hi; git -C /repo push` rather than the first `-C` anywhere; and returns `None`
for a quoted `"-C"` in a non-git command. Its panic-freedom property test moves from
`main.rs` to `guard.rs` along with the function.

**Gates** -- `cargo fmt --all`, `cargo clippy --workspace -- -D warnings`,
`cargo nextest run --workspace`.

## Documentation

One sentence added to `README.md` at the guard contract, recording that reads and
descriptor duplication are permitted outside `ACT` while file writes and command
substitution require it. That rule is user-visible and currently undocumented.

`src/plugin.rs` VERIFY workflow prose is left alone: "basic discovery" is not wrong,
and naming seven filters in agent instructions creates a second place to keep current.

The existing design-doc contract test in `tests/documentation.rs` asserts concepts for
the doctor design specifically; no equivalent test is added for this document.

## Out of Scope

- Heredocs (`<<EOF`, `<<<`), brace expansion, variable expansion, `eval`, and
  `sh -c "..."`. The last two remain unclassified programs, so they are refused
  outside `ACT` by falling through to fail-closed -- the correct outcome reached by
  accident rather than by design, and recorded as such.
- Reason plumbing for `TODO(guard-explain)`. The typed parse result is the seam; a
  `Reason` type is not added speculatively.
- `TODO(verification-policy)`: per-repository gates for non-Rust projects.
- `gh` classification.
- A general no-write-flag invariant across every allowlisted program.

## Risk

The dominant risk is that this change converts the guard's failure mode. Today a
parsing bug causes a wrong denial, which is annoying but safe. After this change, a
parsing bug that under-reports a hazard causes a permitted file write outside `ACT`,
which is a bypass. The property tests and per-hazard assertions are the mitigation,
and they are a required part of the deliverable rather than an optional extra.

Second, the nine newly allowed programs are a genuine policy loosening. The seven
unconditional entries must be confirmed to have no output-file flag before landing;
`sort` and `uniq` are handled explicitly because they do.

Third, the `-C` fix removes a live fail-open path. `extract_dash_c_target` split the
whole line on whitespace, so `rg -C 3 pattern` -- grep's context flag, not git's
directory flag -- yielded a repo root of `3`. `resolve_repo_root` then walked a
relative path and either found the marker by accident, degrading `repo_root` to an
empty string and corrupting deny messages, or returned `None`, in which case
`evaluate_guard` returns `{"continue": true}` and the gate allows the call. Which
branch is reached depends on the hook process's working directory, which was not
measured: the mechanism is verified, the exposure is not. Folding the fix in here
means the parser lands with a live correctness bug removed rather than immediately
after it, at the cost of one extra concern in a change that already has several.
