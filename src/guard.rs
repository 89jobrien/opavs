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

/// Return whether a shell command is safe to run in a phase that does not
/// permit arbitrary file mutations. Unknown commands fail closed.
pub fn shell_command_allowed(cmd: &str, phase: Phase) -> bool {
    if phase == Phase::Act {
        return true;
    }

    // TODO(shell-parser): Replace delimiter splitting with quote-aware shell analysis.
    cmd.split([';', '&', '|'])
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .all(|segment| shell_segment_allowed(segment, phase))
}

fn shell_segment_allowed(segment: &str, phase: Phase) -> bool {
    if segment.contains(['>', '<', '`']) || segment.contains("$(") {
        return false;
    }

    let words: Vec<&str> = segment.split_whitespace().collect();
    let Some(program) = words.first().copied() else {
        return true;
    };
    let program = program.rsplit('/').next().unwrap_or(program);

    match program {
        "opavs" => opavs_command_allowed(&words[1..], phase),
        "git" => git_command_allowed(&words, phase),
        "cargo" => cargo_command_allowed(&words[1..], phase),
        "pwd" | "ls" | "rg" | "fd" | "file" | "which" => true,
        "nu" => words
            .get(1)
            .is_some_and(|path| path.ends_with(".claude/skills/run-opavs/smoke.nu")),
        "hj" | "godmode" if phase == Phase::Ship => {
            words.get(1).is_some_and(|command| *command == "handoff")
        }
        _ => false,
    }
}

fn opavs_command_allowed(args: &[&str], phase: Phase) -> bool {
    match args {
        ["phase", "get"] | ["phase", "set", _] => true,
        ["tasks", "list"] | ["tasks", "runnable"] | ["tasks", "validate"] => true,
        ["tasks", "set-status", ..] | ["tasks", "import", ..] => phase == Phase::Plan,
        _ => false,
    }
}

fn git_command_allowed(words: &[&str], phase: Phase) -> bool {
    let Some(git) = git_invocation(words) else {
        // A bare `git`, or a program that is not git at all.
        return false;
    };
    let args = &words[git.index + 1..];

    match git.subcommand {
        // Read-only inspection: safe in any phase that forbids mutations.
        "status" | "diff" | "log" | "show" | "rev-parse" => true,

        // Listing is read-only but the mutating forms are not, so these two stay
        // argument-sensitive: `git branch -d` deletes a branch and
        // `git remote add` rewrites config.
        "branch" => args.is_empty() || args == ["--show-current"],
        "remote" => args == ["-v"],

        // Staging mutates the index but never working-tree content, and that
        // content can only have been changed in ACT. So staging in SHIP grants
        // no capability the gate has not already handed out, and without it
        // SHIP blocks the step immediately preceding the commit it exists to
        // authorize. `git commit -am` happens to work only because the commit
        // itself is allowlisted and stages tracked files implicitly.
        "add" if phase == Phase::Ship => true,

        // Unstaging is the inverse of `add` and equally index-only. The
        // `--staged` form is required: bare `git restore` overwrites working
        // tree content from the index and would destroy uncommitted work.
        "restore" if phase == Phase::Ship => args.first() == Some(&"--staged"),

        _ => false,
    }
}

fn cargo_command_allowed(args: &[&str], phase: Phase) -> bool {
    // TODO(verification-policy): Load validated per-repository gates for non-Rust projects.
    if phase != Phase::Verify && phase != Phase::Ship {
        return matches!(args, ["metadata", ..]);
    }

    match args {
        ["check", ..] | ["clippy", ..] | ["test", ..] | ["nextest", "run", ..] => true,
        ["fmt", rest @ ..] => rest.contains(&"--check"),
        _ => false,
    }
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
