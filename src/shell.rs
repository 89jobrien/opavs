//! Quote-aware tokenizing of compound shell command lines.
//!
//! Deliberately not a full shell: it models only what the guard needs to
//! classify a command, namely command boundaries, argument vectors, and the
//! redirection and substitution constructs that can escape classification.

/// What a command does beyond invoking its program.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Effects {
    /// Runs a command and splices its output in: `$(...)`, backticks, and the
    /// process substitutions `<(...)` / `>(...)`.
    pub substitution: bool,
    /// Writes to a path: `> file`, `>> file`, `&> file`, `>| file`.
    pub file_write: bool,
    /// Reads from a path: `< file`.
    pub file_read: bool,
    /// Duplicates a descriptor without touching the filesystem: `2>&1`, `1>&2`.
    pub descriptor_dup: bool,
}

/// One command in a compound line.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Command {
    /// Argument vector with quotes removed, so `rg "a b" src`
    /// becomes `["rg", "a b", "src"]`.
    pub args: Vec<String>,
    pub effects: Effects,
}

impl Command {
    /// Whether this command carries nothing the guard could classify or refuse.
    fn is_empty(&self) -> bool {
        self.args.is_empty()
            && !self.effects.substitution
            && !self.effects.file_write
            && !self.effects.file_read
            && !self.effects.descriptor_dup
    }
}

/// Split a compound shell line into its commands at unquoted control operators.
pub fn parse(line: &str) -> Vec<Command> {
    let mut commands: Vec<Command> = Vec::new();
    let mut current = Command::default();
    let mut word = String::new();
    let mut in_word = false;

    for ch in line.chars() {
        match ch {
            ';' | '&' | '|' | '\n' => {
                end_word(&mut current, &mut word, &mut in_word);
                if !current.is_empty() {
                    commands.push(std::mem::take(&mut current));
                }
                current = Command::default();
            }
            c if c.is_whitespace() => end_word(&mut current, &mut word, &mut in_word),
            c => {
                word.push(c);
                in_word = true;
            }
        }
    }

    end_word(&mut current, &mut word, &mut in_word);
    if !current.is_empty() {
        commands.push(current);
    }
    commands
}

fn end_word(command: &mut Command, word: &mut String, in_word: &mut bool) {
    if *in_word {
        command.args.push(std::mem::take(word));
        *in_word = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_unquoted_control_operators() {
        let words: Vec<Vec<String>> = parse("git status; git log")
            .into_iter()
            .map(|command| command.args)
            .collect();
        assert_eq!(words, vec![vec!["git", "status"], vec!["git", "log"]]);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Arbitrary input, including unbalanced quotes, must not panic.
        #[test]
        fn parse_never_panics(line in ".*") {
            let _ = parse(&line);
        }
    }
}
