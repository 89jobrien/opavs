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
        "git" => git_command_allowed(&words[1..]),
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

fn git_command_allowed(args: &[&str]) -> bool {
    let args = if matches!(args.first(), Some(&"-C")) && args.len() >= 3 {
        &args[2..]
    } else {
        args
    };

    matches!(
        args,
        ["status", ..]
            | ["diff", ..]
            | ["log", ..]
            | ["show", ..]
            | ["rev-parse", ..]
            | ["branch"]
            | ["branch", "--show-current"]
            | ["remote", "-v"]
    )
}

fn cargo_command_allowed(args: &[&str], phase: Phase) -> bool {
    if phase != Phase::Verify && phase != Phase::Ship {
        return matches!(args, ["metadata", ..]);
    }

    match args {
        ["check", ..] | ["clippy", ..] | ["test", ..] | ["nextest", "run", ..] => true,
        ["fmt", rest @ ..] => rest.contains(&"--check"),
        _ => false,
    }
}

/// Mirrors the guard's regex: matches `git ... commit` or `git ... push` as a
/// whole word, optionally preceded by `-C <dir>`, anywhere in a compound command.
pub fn command_touches_commit_or_push(cmd: &str) -> bool {
    let re_words: Vec<&str> = cmd.split_whitespace().collect();
    // find any "git" token followed later (same segment) by "commit" or "push"
    for segment in cmd.split([';', '&', '|']) {
        let words: Vec<&str> = segment.split_whitespace().collect();
        if let Some(git_pos) = words.iter().position(|w| *w == "git")
            && words[git_pos..]
                .iter()
                .any(|w| *w == "commit" || *w == "push")
        {
            return true;
        }
    }
    let _ = re_words;
    false
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
