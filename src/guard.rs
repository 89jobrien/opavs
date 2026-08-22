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
        "Edit" | "Write" => {
            if current_phase == Phase::Act {
                Verdict::Allow
            } else {
                Verdict::Deny(format!(
                    "opavs ({repo_root}): repo is in the {current_phase} phase. Edits are only allowed in ACT -- run `opavs phase set ACT` (in {repo_root}) once the user has actually approved that transition."
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
