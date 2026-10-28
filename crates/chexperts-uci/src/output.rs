//! Zero-copy UCI engine output parser and representations.
//!
//! Covers all Engine-to-GUI lines defined in the official Universal Chess Interface protocol.

extern crate alloc;

use crate::info::{parse_info, UciInfo};
use alloc::vec::Vec;

/// Copy protection status reported by copy-protected engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyProtectionStatus {
    /// Engine is performing copy protection check (`copyprotection checking`).
    Checking,
    /// Copy protection check succeeded (`copyprotection ok`).
    Ok,
    /// Copy protection check failed (`copyprotection error`).
    Error,
}

/// Registration status reported by registered engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationStatus {
    /// Engine is verifying registration (`registration checking`).
    Checking,
    /// Engine is correctly registered (`registration ok`).
    Ok,
    /// Engine is not registered or registration failed (`registration error`).
    Error,
}

/// Supported UCI option types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionType {
    /// Boolean checkbox option (`type check`).
    Check,
    /// Integer slider option (`type spin`).
    Spin,
    /// Multiple choice dropdown (`type combo`).
    Combo,
    /// Trigger button (`type button`).
    Button,
    /// Text field (`type string`).
    String,
}

/// Engine configuration capability declaration (`option name <id> type <t> ...`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UciOptionDeclaration<'a> {
    /// Option identifier.
    pub name: &'a str,
    /// Option type.
    pub opt_type: OptionType,
    /// Default value (if any).
    pub default: Option<&'a str>,
    /// Minimum value for `spin` options.
    pub min: Option<i64>,
    /// Maximum value for `spin` options.
    pub max: Option<i64>,
    /// Supported values for `combo` options.
    pub vars: Vec<&'a str>,
}

/// Parsed UCI output line emitted from an engine to stdout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UciOutputLine<'a> {
    /// Protocol handshake confirmation (`uciok`).
    UciOk,
    /// Synchronization response (`readyok`).
    ReadyOk,
    /// Engine name (`id name <name>`).
    IdName(&'a str),
    /// Engine author (`id author <author>`).
    IdAuthor(&'a str),
    /// Copy protection report (`copyprotection ...`).
    CopyProtection(CopyProtectionStatus),
    /// Registration status report (`registration ...`).
    Registration(RegistrationStatus),
    /// Evaluation and search progress report (`info ...`).
    Info(UciInfo<'a>),
    /// Engine configuration capability (`option name ...`).
    Option(UciOptionDeclaration<'a>),
    /// Raw unparsed option line if declaration structure was irregular.
    RawOption(&'a str),
    /// Engine calculation completed with best move (`bestmove <m> [ponder <p>]`).
    BestMove {
        /// Best move in UCI coordinate notation (e.g. `e2e4`, `e7e8q`).
        best_move: &'a str,
        /// Optional ponder move to anticipate.
        ponder: Option<&'a str>,
    },
    /// Any other line emitted by the engine (e.g. comments, unknown lines).
    Other(&'a str),
}

/// Parse a raw UCI line into a zero-copy structured `UciOutputLine`.
#[must_use]
pub fn parse_uci_line(line: &str) -> UciOutputLine<'_> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return UciOutputLine::Other(trimmed);
    }

    if trimmed == "readyok" {
        return UciOutputLine::ReadyOk;
    }
    if trimmed == "uciok" {
        return UciOutputLine::UciOk;
    }

    if let Some(rest) = trimmed.strip_prefix("id ") {
        if let Some(name) = rest.strip_prefix("name ") {
            return UciOutputLine::IdName(name.trim());
        }
        if let Some(author) = rest.strip_prefix("author ") {
            return UciOutputLine::IdAuthor(author.trim());
        }
    }

    if let Some(rest) = trimmed.strip_prefix("copyprotection ") {
        match rest.trim() {
            "checking" => return UciOutputLine::CopyProtection(CopyProtectionStatus::Checking),
            "ok" => return UciOutputLine::CopyProtection(CopyProtectionStatus::Ok),
            "error" => return UciOutputLine::CopyProtection(CopyProtectionStatus::Error),
            _ => return UciOutputLine::Other(trimmed),
        }
    }

    if let Some(rest) = trimmed.strip_prefix("registration ") {
        match rest.trim() {
            "checking" => return UciOutputLine::Registration(RegistrationStatus::Checking),
            "ok" => return UciOutputLine::Registration(RegistrationStatus::Ok),
            "error" => return UciOutputLine::Registration(RegistrationStatus::Error),
            _ => return UciOutputLine::Other(trimmed),
        }
    }

    if let Some(rest) = trimmed.strip_prefix("option ") {
        if let Some(decl) = parse_option_declaration(rest.trim()) {
            return UciOutputLine::Option(decl);
        }
        return UciOutputLine::RawOption(rest);
    }

    if let Some(rest) = trimmed.strip_prefix("bestmove") {
        return parse_bestmove(rest.trim());
    }

    if let Some(rest) = trimmed.strip_prefix("info") {
        return UciOutputLine::Info(parse_info(rest.trim(), trimmed));
    }

    UciOutputLine::Other(trimmed)
}

fn parse_bestmove(rest: &str) -> UciOutputLine<'_> {
    let mut tokens = rest.split_ascii_whitespace();
    let best_move = match tokens.next() {
        Some(bm) => bm,
        None => return UciOutputLine::Other(rest),
    };

    let mut ponder = None;
    while let Some(tok) = tokens.next() {
        if tok == "ponder" {
            ponder = tokens.next();
            break;
        }
    }

    UciOutputLine::BestMove { best_move, ponder }
}

fn parse_option_declaration(rest: &str) -> Option<UciOptionDeclaration<'_>> {
    // Syntax: option name <id> type <t> [default <d>] [min <m>] [max <m>] [var <v>]...
    let name_prefix = rest.strip_prefix("name ")?;
    let type_idx = name_prefix.find(" type ")?;
    let name = &name_prefix[..type_idx];
    let after_type = &name_prefix[type_idx + " type ".len()..];

    let mut tokens = after_type.split_ascii_whitespace();
    let type_str = tokens.next()?;

    let opt_type = match type_str {
        "check" => OptionType::Check,
        "spin" => OptionType::Spin,
        "combo" => OptionType::Combo,
        "button" => OptionType::Button,
        "string" => OptionType::String,
        _ => return None,
    };

    let mut default = None;
    let mut min = None;
    let mut max = None;
    let mut vars = Vec::new();

    while let Some(token) = tokens.next() {
        match token {
            "default" => {
                default = tokens.next();
            }
            "min" => {
                if let Some(val) = tokens.next().and_then(|v| v.parse::<i64>().ok()) {
                    min = Some(val);
                }
            }
            "max" => {
                if let Some(val) = tokens.next().and_then(|v| v.parse::<i64>().ok()) {
                    max = Some(val);
                }
            }
            "var" => {
                if let Some(val) = tokens.next() {
                    vars.push(val);
                }
            }
            _ => {}
        }
    }

    Some(UciOptionDeclaration {
        name,
        opt_type,
        default,
        min,
        max,
        vars,
    })
}
