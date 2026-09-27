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

/// Split a compound shell line into its commands, honouring quoting.
///
/// A delimiter only separates commands when the shell would treat it as one, so
/// this tracks single-quoted, double-quoted, and escaped state. Single quotes make
/// everything literal. Double quotes still expand `$`, backtick, and `\`, which
/// later tasks rely on.
pub fn parse(line: &str) -> Vec<Command> {
    let mut commands: Vec<Command> = Vec::new();
    let mut current = Command::default();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote = '\0';
    let mut escaped = false;

    let mut chars = line.chars().peekable();
    // `while let` rather than `for` because the loop body needs `peek` and
    // `next` to recognise two-character operators such as `>&` and `$(`, which
    // clippy's `while_let_on_iterator` would otherwise forbid.
    #[allow(clippy::while_let_on_iterator)]
    while let Some(ch) = chars.next() {
        // A backslash quotes the next character, except inside single quotes
        // where it is literal.
        if escaped {
            word.push(ch);
            in_word = true;
            escaped = false;
            continue;
        }

        match quote {
            // Single quotes make everything literal, metacharacters included.
            '\'' => {
                if ch == '\'' {
                    quote = '\0';
                } else {
                    word.push(ch);
                    in_word = true;
                }
            }
            '"' => match ch {
                '"' => quote = '\0',
                '\\' => escaped = true,
                // `$`, backtick, and `\` still expand inside double quotes.
                '`' => current.effects.substitution = true,
                '$' if chars.peek() == Some(&'(') => current.effects.substitution = true,
                _ => {
                    word.push(ch);
                    in_word = true;
                }
            },
            _ => match ch {
                '\\' => escaped = true,
                '\'' | '"' => {
                    quote = ch;
                    in_word = true;
                }
                '`' => {
                    current.effects.substitution = true;
                    end_word(&mut current, &mut word, &mut in_word);
                }
                '$' if chars.peek() == Some(&'(') => {
                    current.effects.substitution = true;
                    end_word(&mut current, &mut word, &mut in_word);
                }
                '<' => {
                    // `<(cmd)` runs a command to produce a path, so it is a
                    // substitution rather than a read.
                    if chars.peek() == Some(&'(') {
                        current.effects.substitution = true;
                    } else {
                        current.effects.file_read = true;
                    }
                    end_word(&mut current, &mut word, &mut in_word);
                }
                // Must precede the plain `>` arm, which is the fallback.
                '>' if chars.peek() == Some(&'(') => {
                    current.effects.substitution = true;
                    end_word(&mut current, &mut word, &mut in_word);
                }
                '&' if chars.peek() == Some(&'>') => {
                    // `&> file` redirects both streams to a file.
                    current.effects.file_write = true;
                    end_word(&mut current, &mut word, &mut in_word);
                }
                '>' => {
                    if chars.peek() == Some(&'&') {
                        // `2>&1` duplicates a descriptor and writes no file. The
                        // `&` belongs to this operator, so it must be consumed
                        // rather than read as a control operator that would end
                        // the command here.
                        chars.next();
                        current.effects.descriptor_dup = true;
                    } else {
                        current.effects.file_write = true;
                    }
                    // Flush so a redirect target stays its own word and cannot
                    // be glued to the subcommand: `git log>out.txt` must still
                    // read as `log`.
                    end_word(&mut current, &mut word, &mut in_word);
                }
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
            },
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

    #[test]
    fn quoting_keeps_delimiters_inside_a_word() {
        let words: Vec<Vec<String>> = parse("rg \"foo;bar\" src")
            .into_iter()
            .map(|command| command.args)
            .collect();
        assert_eq!(words, vec![vec!["rg", "foo;bar", "src"]]);

        let words: Vec<Vec<String>> = parse("rg 'a|b' src")
            .into_iter()
            .map(|command| command.args)
            .collect();
        assert_eq!(words, vec![vec!["rg", "a|b", "src"]]);

        let words: Vec<Vec<String>> = parse("rg foo\\;bar src")
            .into_iter()
            .map(|command| command.args)
            .collect();
        assert_eq!(words, vec![vec!["rg", "foo;bar", "src"]]);
    }

    #[test]
    fn distinguishes_file_read_from_process_substitution() {
        let command = parse("sort < in.txt").remove(0);
        assert!(command.effects.file_read);
        assert!(!command.effects.substitution);

        let command = parse("diff <(a) <(b)").remove(0);
        assert!(command.effects.substitution);
        assert!(!command.effects.file_read);
    }

    #[test]
    fn detects_command_substitution_except_inside_single_quotes() {
        for cmd in ["echo $(whoami)", "echo `whoami`", "echo \"$(whoami)\""] {
            assert!(parse(cmd)[0].effects.substitution, "{cmd}");
        }
        // Single quotes are literal, so this is not a substitution.
        assert!(!parse("echo '$(whoami)'")[0].effects.substitution);
    }

    #[test]
    fn descriptor_duplication_is_not_a_file_write() {
        let command = parse("cargo test 2>&1").remove(0);
        assert_eq!(command.args, vec!["cargo", "test", "2", "1"]);
        assert!(command.effects.descriptor_dup);
        assert!(!command.effects.file_write);
    }

    #[test]
    fn detects_unquoted_file_redirection_only() {
        // The property that matters is that the line performs a file write, not
        // that the first command does: `&>` is still split at the `&` here, so
        // the write lands on a later command.
        let writes = |cmd: &str| parse(cmd).iter().any(|c| c.effects.file_write);

        for cmd in [
            "rg foo > out.txt",
            "rg foo >> out.txt",
            "rg foo &> out.txt",
            "rg foo >| out.txt",
        ] {
            assert!(writes(cmd), "{cmd}");
        }
        // A quoted metacharacter is literal text, not shell syntax.
        for cmd in ["rg 'a > b' src", "rg \"a > b\" src"] {
            assert!(!writes(cmd), "{cmd}");
        }
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

        /// A delimiter inside quotes belongs to the word, never the command line.
        #[test]
        fn quoted_delimiters_never_split(word in "[a-zA-Z0-9]{1,8}") {
            let commands = parse(&format!("'{word};{word}'"));
            prop_assert_eq!(commands.len(), 1);
            prop_assert_eq!(&commands[0].args, &vec![format!("{word};{word}")]);

            let commands = parse(&format!("\"{word}|{word}\""));
            prop_assert_eq!(commands.len(), 1);
            prop_assert_eq!(&commands[0].args, &vec![format!("{word}|{word}")]);
        }

        /// Quoting a bare word cannot change how many commands the line holds.
        #[test]
        fn quoting_never_splits_a_command(word in "[a-zA-Z0-9_./-]{1,12}") {
            let bare = parse(&word);
            let quoted = parse(&format!("'{word}'"));
            prop_assert_eq!(bare.len(), quoted.len());
            prop_assert_eq!(&bare[0].args, &quoted[0].args);
        }
    }
}
