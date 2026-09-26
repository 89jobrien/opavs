//! Pure policy decisions for phase-gated tool and shell-command execution.

use crate::domain::Phase;

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny(String),
}

/// Pure domain logic mirroring opavs-guard.sh: given the tool being invoked,
/// what it targets, and the repo's current phase, decide allow/deny.
/// Kept free of I/O so it's exhaustively unit-testable.
pub fn decide(
    tool: &str,
    is_commit_or_push: bool,
    current_phase: Phase,
    repo_root: &str,
) -> Verdict {
    match tool {
        "Edit" | "Write" | "apply_patch" | "BashMutation" => {
            if current_phase == Phase::Act {
                Verdict::Allow
            } else {
                Verdict::Deny(format!(
                    "opavs ({repo_root}): repo is in the {current_phase} phase. File mutations are only allowed in ACT -- run `opavs phase set ACT` (in {repo_root}) once the user has actually approved that transition."
                ))
            }
        }
        "Bash" if is_commit_or_push => {
            if current_phase == Phase::Ship {
                Verdict::Allow
            } else {
                Verdict::Deny(format!(
                    "opavs ({repo_root}): repo is in the {current_phase} phase. git commit/push are only allowed in SHIP -- run `opavs phase set SHIP` (in {repo_root}) once the user has actually approved that transition."
                ))
            }
        }
        _ => Verdict::Allow,
    }
}

/// The side effect a command has, from the gate's point of view.
///
/// This is the vocabulary the phase policy below is written in. It exists
/// because policy used to be implicit in three separate places — a blanket
/// short-circuit for ACT, one read-only allowlist shared by every other phase,
/// and ad-hoc phase checks inside each command's matcher. Any capability a
/// phase needed but nobody had written down was simply absent, which is how
/// SHIP ended up unable to stage, run `gh`, or upgrade the tool itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation {
    /// Read-only inspection of the repository, working tree, or manifest graph.
    Inspect,
    /// Runs the project's checks without changing the working tree.
    Verify,
    /// Edits working-tree files.
    Mutate,
    /// Edits the git index only, leaving working-tree content untouched.
    Stage,
    /// Moves work off the machine: commit, push, PR, release, self-upgrade.
    Publish,
    /// Reads or writes the current phase.
    PhaseState,
    /// Edits the task graph.
    TaskState,
    /// Records an end-of-session handoff.
    Handoff,
}

/// Every operation, in a fixed order so the policy test can state each phase's
/// permitted set as a plain list. Test-only: production code asks `permits` about
/// one operation at a time, but the test needs the full set to prove the table has
/// no operation silently falling through to a default arm.
#[cfg(test)]
const ALL_OPERATIONS: [Operation; 8] = [
    Operation::Inspect,
    Operation::Verify,
    Operation::Mutate,
    Operation::Stage,
    Operation::Publish,
    Operation::PhaseState,
    Operation::TaskState,
    Operation::Handoff,
];

/// Which operations each phase permits.
///
/// This table is the single declaration of phase policy. `shell_command_allowed`
/// classifies a command into an `Operation` and asks this function, so granting a
/// capability is a one-line change here rather than a discovery that the gate is
/// missing something.
///
/// TODO(verification-policy): load validated per-repository gates so non-Rust
/// projects can declare their own Verify commands instead of relying on cargo.
fn permits(phase: Phase, op: Operation) -> bool {
    use Operation::*;

    // ACT is the working phase and imposes no command-level restriction at all.
    //
    // The one thing refused in ACT is commit and push, and that is enforced by
    // the dedicated classifier in `command_touches_commit_or_push` before this
    // table is consulted -- a stricter rule layered on top, not an operation the
    // table withholds. `permits` must not claim otherwise, or the two mechanisms
    // disagree and the looser one wins: an operation refused only here reaches
    // `decide` as a generic `BashMutation`, which ACT allows.
    if phase == Phase::Act {
        return true;
    }

    match op {
        // Reading things, and reading or setting the phase itself, is what every
        // phase is for.
        Inspect | PhaseState => true,
        // Planning is when the task graph is edited.
        TaskState => phase == Phase::Plan,
        // Verification is the purpose of VERIFY, and must stay available in SHIP
        // so the gates can be re-run immediately before publishing.
        Verify => matches!(phase, Phase::Verify | Phase::Ship),
        // Staging touches the index but never working-tree content, and that
        // content can only have been changed in ACT. So allowing it in SHIP grants
        // no capability the gate has not already issued, and without it SHIP blocks
        // the step immediately preceding the commit it exists to authorize.
        Stage => phase == Phase::Ship,
        // Publishing is SHIP, and only SHIP. This covers publish operations that
        // are not commit or push -- currently `opavs upgrade`, which replaces the
        // installed executable and so previously could not be run from any phase.
        Publish => phase == Phase::Ship,
        Handoff => phase == Phase::Ship,
        // No other phase edits the working tree.
        Mutate => false,
    }
}

/// Return whether every segment of `cmd` performs an operation `phase` permits.
///
/// Unknown programs fail closed, except in ACT, which is the working phase.
pub fn shell_command_allowed(cmd: &str, phase: Phase) -> bool {
    // TODO(shell-parser): Replace delimiter splitting with quote-aware shell analysis.
    cmd.split([';', '&', '|'])
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .all(|segment| {
            if segment.contains(['>', '<', '`']) || segment.contains("$(") {
                return false;
            }
            let words: Vec<&str> = segment.split_whitespace().collect();
            match classify(&words) {
                Some(op) => permits(phase, op),
                // ACT permits commands the gate has no policy for; every other
                // phase fails closed on an unrecognised program.
                None => phase == Phase::Act,
            }
        })
}

/// Classify one shell segment into the operation it performs.
///
/// Purely a function of the command: nothing here consults the phase, so the
/// per-phase policy lives in exactly one place (`permits`). `None` means the gate
/// has no policy for this program, which callers treat as fail-closed.
fn classify(words: &[&str]) -> Option<Operation> {
    let program = words.first()?;
    let program = program.rsplit('/').next().unwrap_or(program);
    let args = &words[1..];

    match program {
        "git" => classify_git(words),
        "cargo" => classify_cargo(args),
        "opavs" => classify_opavs(args),
        "hj" | "godmode" => (args.first() == Some(&"handoff")).then_some(Operation::Handoff),
        // Reading the filesystem and locating binaries changes nothing.
        "pwd" | "ls" | "rg" | "fd" | "file" | "which" => Some(Operation::Inspect),
        // The project's own smoke driver builds a throwaway repo in a temp dir
        // and touches nothing in the working tree, so it stays available in every
        // phase as it was before this table existed.
        "nu" => args
            .first()
            .is_some_and(|path| path.ends_with(".claude/skills/run-opavs/smoke.nu"))
            .then_some(Operation::Inspect),
        _ => None,
    }
}

fn classify_git(words: &[&str]) -> Option<Operation> {
    let git = git_invocation(words)?;
    let args = &words[git.index + 1..];

    Some(match git.subcommand {
        "status" | "diff" | "log" | "show" | "rev-parse" => Operation::Inspect,

        // Listing is read-only, but the mutating forms of these two verbs are
        // not, so they stay argument-sensitive: `git branch -d` deletes a branch
        // and `git remote add` rewrites config.
        "branch" if args.is_empty() || args == ["--show-current"] => Operation::Inspect,
        "remote" if args == ["-v"] => Operation::Inspect,

        "add" => Operation::Stage,
        // The `--staged` form only rewrites the index. Bare `git restore`
        // overwrites working-tree content from the index, which is why it falls
        // through to Mutate below.
        "restore" if args.first() == Some(&"--staged") => Operation::Stage,

        "commit" | "push" => Operation::Publish,

        // Ref-, config-, and working-tree-rewriting verbs.
        "restore" | "branch" | "remote" | "reset" | "checkout" | "switch" | "stash" | "rebase"
        | "merge" | "cherry-pick" | "revert" | "config" | "clean" | "apply" | "am" | "tag" => {
            Operation::Mutate
        }

        // Anything unrecognised fails closed rather than being assumed read-only.
        _ => return None,
    })
}

fn classify_cargo(args: &[&str]) -> Option<Operation> {
    Some(match *args.first()? {
        // Reading the manifest graph changes nothing.
        "metadata" => Operation::Inspect,
        "check" | "clippy" | "test" => Operation::Verify,
        "nextest" if args.get(1) == Some(&"run") => Operation::Verify,
        // `cargo fmt --check` only reports; without `--check` it rewrites files.
        "fmt" if args.contains(&"--check") => Operation::Verify,
        "fmt" => Operation::Mutate,
        // `build`, `doc`, `install` and friends stay unclassified: ACT permits
        // them, and no other phase has a reason to.
        _ => return None,
    })
}

fn classify_opavs(args: &[&str]) -> Option<Operation> {
    Some(match (*args.first()?, args.get(1).copied()) {
        ("phase", Some("get" | "set")) => Operation::PhaseState,
        ("tasks", Some("list" | "runnable" | "validate")) => Operation::PhaseState,
        ("tasks", Some("set-status" | "import")) => Operation::TaskState,
        ("init", _) => Operation::Mutate,
        // Replacing the installed executable is a publish action, and SHIP is
        // the only phase that permits one.
        ("upgrade", _) => Operation::Publish,
        _ => return None,
    })
}

/// A `git` program token found at the head of a shell segment, resolved to the
/// subcommand it will actually run.
struct GitInvocation<'a> {
    /// Index of the subcommand token within the source `words` slice, so
    /// callers can recover the arguments that follow it.
    index: usize,
    subcommand: &'a str,
}

/// Recognize a git program at the head of `words` and resolve its subcommand.
///
/// Tolerates a path prefix (`/usr/bin/git`) by comparing the final path
/// component, exactly as `shell_segment_allowed` does — if the two classifiers
/// disagree about what "a git command" is, a real `git push` can fall through
/// to the ACT-only mutation rule and a user who obeys the resulting message is
/// bounced into the phase that triggers the other one.
///
/// Skips git global options, and the argument to `-C`/`-c`, so
/// `git -C /repo push` and `git --no-pager push` both resolve to `push`.
///
/// Returns `None` for a non-git program, or a bare `git` with no subcommand.
fn git_invocation<'a>(words: &[&'a str]) -> Option<GitInvocation<'a>> {
    let program = words.first()?;
    if program.rsplit('/').next().unwrap_or(program) != "git" {
        return None;
    }

    let mut i = 1;
    while i < words.len() {
        let word = words[i];
        // `-C <dir>` and `-c <str>` consume the following word.
        if matches!(word, "-C" | "-c") {
            i += 2;
            continue;
        }
        // Any other leading option is global; the subcommand follows it.
        if word.starts_with('-') {
            i += 1;
            continue;
        }
        return Some(GitInvocation {
            index: i,
            subcommand: word,
        });
    }
    None
}

/// Whether any segment of `cmd` invokes `git commit` or `git push`.
///
/// Classification is by git *subcommand*, not by the presence of the words
/// "commit" or "push" anywhere in the segment: `git log --grep push` is a
/// read-only query and must not be gated as a push.
pub fn command_touches_commit_or_push(cmd: &str) -> bool {
    cmd.split([';', '&', '|']).any(|segment| {
        let words: Vec<&str> = segment.split_whitespace().collect();
        git_invocation(&words).is_some_and(|git| matches!(git.subcommand, "commit" | "push"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_allowed_in_act() {
        assert_eq!(decide("Edit", false, Phase::Act, "/repo"), Verdict::Allow);
    }

    #[test]
    fn edit_denied_outside_act() {
        for phase in [Phase::Orient, Phase::Plan, Phase::Verify, Phase::Ship] {
            assert!(matches!(
                decide("Edit", false, phase, "/repo"),
                Verdict::Deny(_)
            ));
        }
    }

    #[test]
    fn write_denied_outside_act() {
        assert!(matches!(
            decide("Write", false, Phase::Plan, "/repo"),
            Verdict::Deny(_)
        ));
    }

    #[test]
    fn apply_patch_denied_outside_act() {
        assert!(matches!(
            decide("apply_patch", false, Phase::Verify, "/repo"),
            Verdict::Deny(_)
        ));
    }

    #[test]
    fn mutating_bash_denied_outside_act() {
        assert!(matches!(
            decide("BashMutation", false, Phase::Verify, "/repo"),
            Verdict::Deny(_)
        ));
    }

    #[test]
    fn shell_policy_allows_verification_commands_but_denies_formatting() {
        assert!(shell_command_allowed("cargo test", Phase::Verify));
        assert!(shell_command_allowed(
            "cargo fmt --all --check",
            Phase::Verify
        ));
        assert!(!shell_command_allowed("cargo fmt --all", Phase::Verify));
    }

    #[test]
    fn shell_policy_denies_mutation_hidden_in_a_chain() {
        assert!(!shell_command_allowed(
            "git status | tee status.txt",
            Phase::Orient
        ));
    }

    #[test]
    fn commit_allowed_in_ship() {
        assert_eq!(decide("Bash", true, Phase::Ship, "/repo"), Verdict::Allow);
    }

    #[test]
    fn commit_denied_outside_ship() {
        assert!(matches!(
            decide("Bash", true, Phase::Act, "/repo"),
            Verdict::Deny(_)
        ));
    }

    #[test]
    fn non_commit_bash_always_allowed() {
        for phase in [
            Phase::Orient,
            Phase::Plan,
            Phase::Act,
            Phase::Verify,
            Phase::Ship,
        ] {
            assert_eq!(decide("Bash", false, phase, "/repo"), Verdict::Allow);
        }
    }

    #[test]
    fn unrelated_tool_always_allowed() {
        assert_eq!(
            decide("Read", false, Phase::Orient, "/repo"),
            Verdict::Allow
        );
    }

    #[test]
    fn detects_plain_commit() {
        assert!(command_touches_commit_or_push("git commit -m 'x'"));
    }

    #[test]
    fn detects_push_with_dash_c() {
        assert!(command_touches_commit_or_push(
            "git -C /repo push origin main"
        ));
    }

    #[test]
    fn detects_commit_after_chain() {
        assert!(command_touches_commit_or_push(
            "cargo test; git commit -m x"
        ));
    }

    #[test]
    fn ignores_unrelated_git_commands() {
        assert!(!command_touches_commit_or_push("git status"));
        assert!(!command_touches_commit_or_push("git log --oneline"));
    }

    #[test]
    fn ignores_non_git_commands() {
        assert!(!command_touches_commit_or_push("echo commit push"));
    }

    #[test]
    fn detects_path_prefixed_commit_and_push() {
        assert!(command_touches_commit_or_push(
            "/usr/bin/git push origin main"
        ));
        assert!(command_touches_commit_or_push(
            "/opt/homebrew/bin/git commit -m x"
        ));
        assert!(command_touches_commit_or_push("./scripts/git push"));
    }

    #[test]
    fn detects_commit_and_push_behind_global_options() {
        assert!(command_touches_commit_or_push(
            "git --no-pager push origin main"
        ));
        assert!(command_touches_commit_or_push(
            "git -c core.pager=cat commit -m x"
        ));
    }

    #[test]
    fn ignores_read_only_git_whose_arguments_name_commit_or_push() {
        assert!(!command_touches_commit_or_push(
            "git log --grep push --oneline"
        ));
        assert!(!command_touches_commit_or_push(
            "git show HEAD --stat commit"
        ));
        assert!(!command_touches_commit_or_push("git log push"));
    }

    #[test]
    fn ignores_bare_git_with_no_subcommand() {
        assert!(!command_touches_commit_or_push("git"));
        assert!(!command_touches_commit_or_push("git -C /repo"));
        assert!(!command_touches_commit_or_push("git -C"));
    }

    #[test]
    fn allows_staging_in_ship_phase() {
        assert!(shell_command_allowed("git add -A", Phase::Ship));
        assert!(shell_command_allowed("git add src/guard.rs", Phase::Ship));
        assert!(shell_command_allowed("git add -p", Phase::Ship));
        assert!(shell_command_allowed("git restore --staged .", Phase::Ship));
    }

    #[test]
    fn denies_staging_outside_ship_phase() {
        for phase in [Phase::Orient, Phase::Plan, Phase::Verify] {
            assert!(!shell_command_allowed("git add -A", phase));
            assert!(!shell_command_allowed("git restore --staged .", phase));
        }
        // ACT permits arbitrary mutation, so staging is trivially fine there.
        assert!(shell_command_allowed("git add -A", Phase::Act));
    }

    #[test]
    fn bare_restore_stays_blocked_so_it_cannot_discard_working_tree() {
        assert!(!shell_command_allowed(
            "git restore src/guard.rs",
            Phase::Ship
        ));
        assert!(!shell_command_allowed("git restore .", Phase::Ship));
    }

    #[test]
    fn argument_sensitive_git_verbs_stay_blocked_when_mutating() {
        // Listing is allowed; the destructive forms are not.
        assert!(shell_command_allowed("git branch", Phase::Ship));
        assert!(shell_command_allowed(
            "git branch --show-current",
            Phase::Ship
        ));
        assert!(!shell_command_allowed("git branch -D main", Phase::Ship));
        assert!(!shell_command_allowed("git branch -d main", Phase::Ship));

        assert!(shell_command_allowed("git remote -v", Phase::Ship));
        assert!(!shell_command_allowed(
            "git remote add origin url",
            Phase::Ship
        ));
        assert!(!shell_command_allowed(
            "git remote remove origin",
            Phase::Ship
        ));
    }

    #[test]
    fn read_only_git_resolves_subcommand_past_global_options() {
        // Previously these were denied as mutations because the allowlist
        // matched literal argument shapes rather than the resolved subcommand.
        assert!(shell_command_allowed(
            "git --no-pager log --oneline",
            Phase::Ship
        ));
        assert!(shell_command_allowed(
            "git -c color.ui=always status",
            Phase::Verify
        ));
        assert!(shell_command_allowed(
            "/usr/bin/git log --oneline",
            Phase::Orient
        ));
        assert!(shell_command_allowed("git -C /repo status", Phase::Orient));
    }

    #[test]
    fn other_mutating_git_subcommands_remain_blocked_in_ship_phase() {
        for cmd in [
            "git reset --hard",
            "git checkout main",
            "git stash",
            "git rebase main",
            "git merge main",
            "git config user.name x",
        ] {
            assert!(!shell_command_allowed(cmd, Phase::Ship), "allowed: {cmd}");
        }
    }

    // --- phase policy ---

    /// The whole point of the `Operation` table: each phase's permitted set is
    /// stated here as data, so a change to policy shows up as a failing assertion
    /// naming the phase and the operation, rather than as a capability someone
    /// discovers missing while trying to ship.
    #[test]
    fn each_phase_permits_exactly_its_declared_operations() {
        use Operation::*;

        let permitted = |phase| -> Vec<Operation> {
            ALL_OPERATIONS
                .into_iter()
                .filter(|op| permits(phase, *op))
                .collect()
        };

        assert_eq!(permitted(Phase::Orient), vec![Inspect, PhaseState]);
        assert_eq!(permitted(Phase::Plan), vec![Inspect, PhaseState, TaskState]);
        assert_eq!(permitted(Phase::Act), ALL_OPERATIONS.to_vec());
        assert_eq!(permitted(Phase::Verify), vec![Inspect, Verify, PhaseState]);
        assert_eq!(
            permitted(Phase::Ship),
            vec![Inspect, Verify, Stage, Publish, PhaseState, Handoff]
        );
    }

    #[test]
    fn commands_classify_into_declared_operations() {
        let op = |cmd: &str| -> Option<Operation> {
            let words: Vec<&str> = cmd.split_whitespace().collect();
            classify(&words)
        };

        assert_eq!(op("git status"), Some(Operation::Inspect));
        assert_eq!(op("git log --oneline"), Some(Operation::Inspect));
        assert_eq!(op("git add -A"), Some(Operation::Stage));
        assert_eq!(op("git restore --staged ."), Some(Operation::Stage));
        assert_eq!(op("git commit -m x"), Some(Operation::Publish));
        assert_eq!(op("/usr/bin/git push"), Some(Operation::Publish));
        assert_eq!(op("git branch -D main"), Some(Operation::Mutate));
        assert_eq!(op("git restore ."), Some(Operation::Mutate));
        assert_eq!(op("cargo test"), Some(Operation::Verify));
        assert_eq!(op("cargo fmt --check"), Some(Operation::Verify));
        assert_eq!(op("cargo fmt"), Some(Operation::Mutate));
        assert_eq!(op("opavs phase set ACT"), Some(Operation::PhaseState));
        assert_eq!(
            op("opavs tasks set-status a done"),
            Some(Operation::TaskState)
        );
        assert_eq!(op("opavs upgrade"), Some(Operation::Publish));
        assert_eq!(op("hj handoff"), Some(Operation::Handoff));

        // Unrecognised programs and subcommands classify to nothing, so every
        // phase but ACT fails closed on them.
        assert_eq!(op("rm -rf target"), None);
        assert_eq!(op("git nonsense-subcommand"), None);
        assert_eq!(op("cargo build"), None);
        assert_eq!(op("git"), None);
    }

    #[test]
    fn act_permits_unclassified_commands_and_no_other_phase_does() {
        for cmd in [
            "rm -rf target",
            "gh issue list",
            "npm install",
            "echo commit push",
            "cargo build",
        ] {
            assert!(shell_command_allowed(cmd, Phase::Act), "ACT denied: {cmd}");
        }

        for phase in [Phase::Orient, Phase::Plan, Phase::Verify, Phase::Ship] {
            assert!(!shell_command_allowed("rm -rf target", phase), "{phase:?}");
            assert!(!shell_command_allowed("gh issue list", phase), "{phase:?}");
        }
    }

    #[test]
    fn self_upgrade_is_a_publish_operation() {
        assert!(shell_command_allowed("opavs upgrade", Phase::Ship));
        for phase in [Phase::Orient, Phase::Plan, Phase::Verify] {
            assert!(!shell_command_allowed("opavs upgrade", phase), "{phase:?}");
        }
        // ACT is unrestricted at the command level, so this is allowed there.
        // Commit and push are still refused in ACT, but by the dedicated
        // classifier rather than by this table -- see `permits`.
        assert!(shell_command_allowed("opavs upgrade", Phase::Act));
        assert!(matches!(
            decide("Bash", true, Phase::Act, "/repo"),
            Verdict::Deny(_)
        ));
    }

    #[test]
    fn manifest_metadata_is_inspectable_in_every_phase() {
        for phase in [
            Phase::Orient,
            Phase::Plan,
            Phase::Act,
            Phase::Verify,
            Phase::Ship,
        ] {
            assert!(
                shell_command_allowed("cargo metadata --no-deps", phase),
                "{phase:?}"
            );
        }
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// No arbitrary UTF-8 input may panic the classifier.
        #[test]
        fn command_touches_commit_or_push_never_panics(cmd in ".*") {
            let _ = command_touches_commit_or_push(&cmd);
        }

        /// A command with no "git" token anywhere can never be flagged.
        #[test]
        fn commands_without_git_token_are_never_flagged(
            words in prop::collection::vec("[a-zA-Z0-9_-]{1,10}", 0..8)
        ) {
            let cmd = words.join(" ");
            prop_assume!(!words.iter().any(|w| w == "git"));
            prop_assert!(!command_touches_commit_or_push(&cmd));
        }

        /// Extra leading/trailing whitespace around a flagged command must
        /// not change the verdict.
        #[test]
        fn whitespace_padding_does_not_change_verdict(pad in " {0,5}") {
            let cmd = format!("{pad}git commit -m x{pad}");
            prop_assert!(command_touches_commit_or_push(&cmd));
        }
    }
}
