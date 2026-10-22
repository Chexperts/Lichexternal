//! Strongly typed UCI outgoing command representations and formatters.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::models::SearchLimit;

/// Starting position specification for the `position` command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PositionInitial {
    /// Standard starting chess position (`startpos`).
    Startpos,
    /// Custom FEN string.
    Fen(String),
}

/// Strongly typed outgoing UCI commands sent from the bridge to an engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UciCommand {
    /// Tell engine to use the UCI protocol (`uci`).
    Uci,
    /// Synchronize engine status (`isready`).
    IsReady,
    /// Set an engine option (`setoption name <name> [value <value>]`).
    SetOption { name: String, value: Option<String> },
    /// Inform engine that the next search will be from a new game (`ucinewgame`).
    UciNewGame,
    /// Set up position on internal board (`position [startpos|fen <fen>] [moves <m1> <m2> ...]`).
    Position {
        initial: PositionInitial,
        moves: Vec<String>,
    },
    /// Start engine calculation (`go ...`).
    Go(SearchLimit),
    /// Stop engine calculation immediately (`stop`).
    Stop,
    /// Inform engine that the user played the expected ponder move (`ponderhit`).
    PonderHit,
    /// Quit engine process (`quit`).
    Quit,
}

impl UciCommand {
    /// Format the command into a UCI protocol line (without trailing newline).
    #[must_use]
    pub fn to_uci_line(&self) -> String {
        match self {
            Self::Uci => String::from("uci"),
            Self::IsReady => String::from("isready"),
            Self::SetOption {
                name,
                value: Some(val),
            } => {
                format!("setoption name {name} value {val}")
            }
            Self::SetOption { name, value: None } => {
                format!("setoption name {name}")
            }
            Self::UciNewGame => String::from("ucinewgame"),
            Self::Position { initial, moves } => {
                let init_part = match initial {
                    PositionInitial::Startpos => String::from("startpos"),
                    PositionInitial::Fen(fen) => format!("fen {fen}"),
                };

                if moves.is_empty() {
                    format!("position {init_part}")
                } else {
                    let move_str = moves.join(" ");
                    format!("position {init_part} moves {move_str}")
                }
            }
            Self::Go(limits) => {
                let args = limits.to_uci_go_args();
                format!("go {args}")
            }
            Self::Stop => String::from("stop"),
            Self::PonderHit => String::from("ponderhit"),
            Self::Quit => String::from("quit"),
        }
    }
}

impl fmt::Display for UciCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_uci_line())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uci_command_formatting() {
        assert_eq!(UciCommand::Uci.to_uci_line(), "uci");
        assert_eq!(UciCommand::IsReady.to_uci_line(), "isready");
        assert_eq!(UciCommand::UciNewGame.to_uci_line(), "ucinewgame");
        assert_eq!(UciCommand::Stop.to_uci_line(), "stop");
        assert_eq!(UciCommand::Quit.to_uci_line(), "quit");

        assert_eq!(
            UciCommand::SetOption {
                name: "Threads".into(),
                value: Some("4".into()),
            }
            .to_uci_line(),
            "setoption name Threads value 4"
        );

        assert_eq!(
            UciCommand::SetOption {
                name: "Clear Hash".into(),
                value: None,
            }
            .to_uci_line(),
            "setoption name Clear Hash"
        );

        assert_eq!(
            UciCommand::Position {
                initial: PositionInitial::Startpos,
                moves: vec![],
            }
            .to_uci_line(),
            "position startpos"
        );

        assert_eq!(
            UciCommand::Position {
                initial: PositionInitial::Startpos,
                moves: vec!["e2e4".into(), "e7e5".into()],
            }
            .to_uci_line(),
            "position startpos moves e2e4 e7e5"
        );

        assert_eq!(
            UciCommand::Position {
                initial: PositionInitial::Fen("8/8/8/8/8/8/8/8 w - - 0 1".into()),
                moves: vec!["a2a4".into()],
            }
            .to_uci_line(),
            "position fen 8/8/8/8/8/8/8/8 w - - 0 1 moves a2a4"
        );

        assert_eq!(
            UciCommand::Go(SearchLimit {
                depth: Some(18),
                movetime: None,
                nodes: None,
            })
            .to_uci_line(),
            "go depth 18"
        );
    }
}
