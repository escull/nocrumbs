// SPDX-License-Identifier: GPL-3.0-or-later
//! Parsing `:colon-commands` (brief §8, §8.1).
//!
//! Arguments are whitespace-separated; double quotes group (`:mkdir "my folder"`) and `\"`
//! escapes a quote. Any other backslash is literal, so Windows paths need no escaping. A command
//! that takes exactly one free-text argument (`:cd`, `:mkdir`, `:touch`, `:rename`) takes the
//! **rest of the line**, so unquoted spaces work for those too.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Name,
    Size,
    Date,
}

impl SortKey {
    pub fn word(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Size => "size",
            Self::Date => "date",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDir {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Sort {
        key: SortKey,
        dir: Option<SortDir>,
    },
    Asc,
    Desc,
    Hidden,
    Cd(String),
    Back,
    Fwd,
    Mkdir(String),
    Touch(String),
    Rename(String),
    Rm {
        permanent: bool,
    },
    Copy,
    Cut,
    Paste,
    Open,
    Undo,
    Redo,
    /// `:alias NAME PATH`, or `:alias` alone to list them.
    Alias(Option<(String, String)>),
    Unalias(String),
    Help,
    Log,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// Nothing after the colon.
    Empty,
    Unknown(String),
    Usage {
        usage: &'static str,
    },
    TakesNoArguments(String),
    UnterminatedQuote,
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "Type a command after the colon. Try :help"),
            Self::Unknown(c) => write!(f, "Unknown command :{c}. Try :help"),
            Self::Usage { usage } => write!(f, "Usage: {usage}"),
            Self::TakesNoArguments(c) => write!(f, ":{c} takes no arguments"),
            Self::UnterminatedQuote => write!(f, "Unterminated quote: add the closing \""),
        }
    }
}

impl std::error::Error for CommandError {}

/// One-line reminder of everything (brief §8, `:help`).
pub const HELP: &str = "Paths: ~ .. proj/sub  Filter: *.pdf  Aliases: :alias NAME PATH, :unalias NAME  \
Navigate: :cd :back :fwd  View: :sort name|size|date [asc|desc], :asc, :desc, :hidden  \
Files: :mkdir :touch :rename :rm :copy :cut :paste :open :undo :redo";

/// Parse a command line that starts with `:`.
pub fn parse(input: &str) -> Result<Command, CommandError> {
    let body = input.strip_prefix(':').unwrap_or(input).trim_start();
    if body.is_empty() {
        return Err(CommandError::Empty);
    }
    let (word, rest) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
    let word_lc = word.to_lowercase();
    let rest = rest.trim();

    let no_args = |cmd: Command| {
        if rest.is_empty() { Ok(cmd) } else { Err(CommandError::TakesNoArguments(word_lc.clone())) }
    };
    let free = |usage: &'static str| -> Result<String, CommandError> {
        let t = free_text(rest)?;
        if t.is_empty() { Err(CommandError::Usage { usage }) } else { Ok(t) }
    };

    match word_lc.as_str() {
        "sort" => parse_sort(rest),
        "asc" => no_args(Command::Asc),
        "desc" => no_args(Command::Desc),
        "hidden" => no_args(Command::Hidden),
        "back" => no_args(Command::Back),
        "fwd" => no_args(Command::Fwd),
        "rm" => no_args(Command::Rm { permanent: false }),
        "rm!" => no_args(Command::Rm { permanent: true }),
        "copy" => no_args(Command::Copy),
        "cut" => no_args(Command::Cut),
        "paste" => no_args(Command::Paste),
        "open" => no_args(Command::Open),
        "undo" => no_args(Command::Undo),
        "redo" => no_args(Command::Redo),
        "help" => no_args(Command::Help),
        "log" => no_args(Command::Log),
        "cd" => free(":cd PATH").map(Command::Cd),
        "mkdir" => free(":mkdir NAME").map(Command::Mkdir),
        "touch" => free(":touch NAME").map(Command::Touch),
        "rename" => free(":rename NEWNAME").map(Command::Rename),
        "unalias" => {
            let toks = tokens(rest)?;
            match toks.as_slice() {
                [name] => Ok(Command::Unalias(name.clone())),
                _ => Err(CommandError::Usage { usage: ":unalias NAME" }),
            }
        }
        "alias" => {
            if rest.is_empty() {
                return Ok(Command::Alias(None));
            }
            let (name, tail) = first_token(rest)?;
            let path = free_text(tail.trim())?;
            if path.is_empty() {
                return Err(CommandError::Usage { usage: ":alias NAME PATH" });
            }
            Ok(Command::Alias(Some((name, path))))
        }
        other => Err(CommandError::Unknown(other.to_string())),
    }
}

fn parse_sort(rest: &str) -> Result<Command, CommandError> {
    const USAGE: CommandError = CommandError::Usage { usage: ":sort name|size|date [asc|desc]" };
    let toks = tokens(rest)?;
    let key = match toks.first().map(|s| s.to_lowercase()).as_deref() {
        Some("name") => SortKey::Name,
        Some("size") => SortKey::Size,
        Some("date") => SortKey::Date,
        _ => return Err(USAGE),
    };
    let dir = match toks.get(1).map(|s| s.to_lowercase()).as_deref() {
        None => None,
        Some("asc") => Some(SortDir::Asc),
        Some("desc") => Some(SortDir::Desc),
        Some(_) => return Err(USAGE),
    };
    if toks.len() > 2 {
        return Err(USAGE);
    }
    Ok(Command::Sort { key, dir })
}

/// Split into whitespace-separated tokens; `"` groups, `\"` is a literal quote.
fn tokens(s: &str) -> Result<Vec<String>, CommandError> {
    let mut out = Vec::new();
    let mut rest = s.trim_start();
    while !rest.is_empty() {
        let (tok, tail) = first_token(rest)?;
        out.push(tok);
        rest = tail.trim_start();
    }
    Ok(out)
}

/// The first token of `s` and the text after it.
fn first_token(s: &str) -> Result<(String, &str), CommandError> {
    let s = s.trim_start();
    let mut tok = String::new();
    let mut in_quotes = false;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' if chars.peek().is_some_and(|&(_, n)| n == '"') => {
                tok.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => return Ok((tok, &s[i..])),
            c => tok.push(c),
        }
    }
    if in_quotes { Err(CommandError::UnterminatedQuote) } else { Ok((tok, "")) }
}

/// The rest of the line as one argument: either a quoted string, or literal text.
fn free_text(s: &str) -> Result<String, CommandError> {
    let s = s.trim();
    if !s.starts_with('"') {
        return Ok(s.to_string());
    }
    let (tok, tail) = first_token(s)?;
    // `"a b" extra` is not one quoted argument: keep the line literal rather than guess.
    if tail.trim().is_empty() { Ok(tok) } else { Ok(s.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Command::*;

    fn p(s: &str) -> Result<Command, CommandError> {
        parse(s)
    }

    #[test]
    fn no_argument_commands() {
        assert_eq!(p(":asc"), Ok(Asc));
        assert_eq!(p(":DESC"), Ok(Desc), "command words are case-insensitive");
        assert_eq!(p(":hidden"), Ok(Hidden));
        assert_eq!(p(":rm"), Ok(Rm { permanent: false }));
        assert_eq!(p(":rm!"), Ok(Rm { permanent: true }));
        for (s, c) in [(":copy", Copy), (":cut", Cut), (":paste", Paste), (":open", Open), (":undo", Undo)] {
            assert_eq!(p(s), Ok(c));
        }
        assert_eq!(p(":back"), Ok(Back));
        assert_eq!(p(":fwd"), Ok(Fwd));
        assert_eq!(p(":help"), Ok(Help));
        assert_eq!(p(":asc now"), Err(CommandError::TakesNoArguments("asc".into())));
    }

    #[test]
    fn sort() {
        assert_eq!(p(":sort size"), Ok(Sort { key: SortKey::Size, dir: None }));
        assert_eq!(p(":sort date desc"), Ok(Sort { key: SortKey::Date, dir: Some(SortDir::Desc) }));
        assert_eq!(p(":Sort NAME ASC"), Ok(Sort { key: SortKey::Name, dir: Some(SortDir::Asc) }));
        for bad in [":sort", ":sort color", ":sort size sideways", ":sort size asc extra"] {
            assert!(matches!(p(bad), Err(CommandError::Usage { .. })), "{bad}");
        }
    }

    #[test]
    fn free_text_commands_take_the_rest_of_the_line() {
        assert_eq!(p(":mkdir my folder"), Ok(Mkdir("my folder".into())));
        assert_eq!(p(":cd  C:\\Program Files\\App "), Ok(Cd("C:\\Program Files\\App".into())));
        assert_eq!(p(":rename holiday-2026 (1).jpg"), Ok(Rename("holiday-2026 (1).jpg".into())));
        assert_eq!(p(":touch a.txt"), Ok(Touch("a.txt".into())));
    }

    #[test]
    fn quotes_group_and_escape() {
        assert_eq!(p(r#":mkdir "my folder""#), Ok(Mkdir("my folder".into())));
        assert_eq!(p(r#":touch "say \"hi\".txt""#), Ok(Touch("say \"hi\".txt".into())));
        assert_eq!(p(r#":cd "C:\Users\Ed Scull""#), Ok(Cd(r"C:\Users\Ed Scull".into())));
        // Quotes in the middle of an unquoted argument are literal.
        assert_eq!(p(r#":mkdir a"b"#), Ok(Mkdir(r#"a"b"#.into())));
        // A quoted prefix followed by more text is left literal, not guessed at.
        assert_eq!(p(r#":mkdir "a b" c"#), Ok(Mkdir(r#""a b" c"#.into())));
        assert_eq!(p(r#":mkdir "open"#), Err(CommandError::UnterminatedQuote));
    }

    #[test]
    fn backslashes_are_literal_except_before_a_quote() {
        assert_eq!(p(r":cd C:\Users\ed"), Ok(Cd(r"C:\Users\ed".into())));
        assert_eq!(p(r":cd \\server\share"), Ok(Cd(r"\\server\share".into())));
    }

    #[test]
    fn alias_commands() {
        assert_eq!(p(":alias"), Ok(Alias(None)));
        assert_eq!(p(":alias proj ~/projects"), Ok(Alias(Some(("proj".into(), "~/projects".into())))));
        assert_eq!(p(":alias proj ~/my projects"), Ok(Alias(Some(("proj".into(), "~/my projects".into())))));
        assert_eq!(p(r#":alias "odd name" "~/a b""#), Ok(Alias(Some(("odd name".into(), "~/a b".into())))));
        assert!(matches!(p(":alias proj"), Err(CommandError::Usage { .. })));
        assert_eq!(p(":unalias proj"), Ok(Unalias("proj".into())));
        assert!(matches!(p(":unalias"), Err(CommandError::Usage { .. })));
        assert!(matches!(p(":unalias a b"), Err(CommandError::Usage { .. })));
    }

    #[test]
    fn missing_arguments_are_usage_errors() {
        for bad in [":cd", ":mkdir", ":touch   ", ":rename"] {
            assert!(matches!(p(bad), Err(CommandError::Usage { .. })), "{bad}");
        }
        assert_eq!(p(":cd").unwrap_err().to_string(), "Usage: :cd PATH");
    }

    #[test]
    fn unknown_and_empty() {
        assert_eq!(p(":"), Err(CommandError::Empty));
        assert_eq!(p(":   "), Err(CommandError::Empty));
        assert_eq!(p(":bogus x"), Err(CommandError::Unknown("bogus".into())));
        assert_eq!(p(":bogus").unwrap_err().to_string(), "Unknown command :bogus. Try :help");
    }

    #[test]
    fn never_panics_on_odd_input() {
        for s in
            [":\"", ":\\", ":mkdir \\", ":alias \"", ":sort \"x", ":\u{0}", ":日本語 x", ":cd \"\"", ":alias a \"\""]
        {
            let _ = parse(s);
        }
    }
}
