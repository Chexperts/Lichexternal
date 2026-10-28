//! Strongly typed UCI outgoing command representations and formatters.
//!
//! Covers all GUI-to-Engine commands defined in the official Universal Chess Interface protocol.

extern crate alloc;

use alloc::borrow::Cow;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// Starting position specification for the `position` command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PositionInitial<'a> {
    /// Standard starting chess position (`startpos`).
    Startpos,
    /// Custom FEN string.
    Fen(Cow<'a, str>),
}

impl<'a> PositionInitial<'a> {
    /// Create a starting position from a borrowed string slice.
    #[must_use]
    pub const fn fen(fen: &'a str) -> Self {
        Self::Fen(Cow::Borrowed(fen))
    }

    /// Create a starting position from an owned String.
    #[must_use]
    pub fn fen_owned(fen: String) -> Self {
        Self::Fen(Cow::Owned(fen))
    }
}

/// Search constraints parameters for the `go` command.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GoParams<'a> {
    /// Restrict search to these moves only (`searchmoves <m1> ... <mn>`).
    pub searchmoves: Vec<Cow<'a, str>>,
    /// Search in pondering mode (`ponder`).
    pub ponder: bool,
    /// White time left in milliseconds (`wtime <x>`).
    pub wtime: Option<u64>,
    /// Black time left in milliseconds (`btime <x>`).
    pub btime: Option<u64>,
    /// White increment per move in milliseconds (`winc <x>`).
    pub winc: Option<u64>,
    /// Black increment per move in milliseconds (`binc <x>`).
    pub binc: Option<u64>,
    /// Moves to the next time control (`movestogo <x>`).
    pub movestogo: Option<u32>,
    /// Search exact depth in plies (`depth <x>`).
    pub depth: Option<u32>,
    /// Search exact number of nodes (`nodes <x>`).
    pub nodes: Option<u64>,
    /// Search for mate in N moves (`mate <x>`).
    pub mate: Option<i32>,
    /// Search exact duration in milliseconds (`movetime <x>`).
    pub movetime: Option<u64>,
    /// Search until explicit `stop` command (`infinite`).
    pub infinite: bool,
}

impl<'a> GoParams<'a> {
    /// Formats the search arguments into a space-separated string.
    #[must_use]
    pub fn to_uci_args(&self) -> String {
        let mut parts = Vec::new();

        if !self.searchmoves.is_empty() {
            let moves_joined = self
                .searchmoves
                .iter()
                .map(|s| s.as_ref())
                .collect::<Vec<_>>()
                .join(" ");
            parts.push(format!("searchmoves {moves_joined}"));
        }
        if self.ponder {
            parts.push(String::from("ponder"));
        }
        if let Some(wtime) = self.wtime {
            parts.push(format!("wtime {wtime}"));
        }
        if let Some(btime) = self.btime {
            parts.push(format!("btime {btime}"));
        }
        if let Some(winc) = self.winc {
            parts.push(format!("winc {winc}"));
        }
        if let Some(binc) = self.binc {
            parts.push(format!("binc {binc}"));
        }
        if let Some(movestogo) = self.movestogo {
            parts.push(format!("movestogo {movestogo}"));
        }
        if let Some(depth) = self.depth {
            parts.push(format!("depth {depth}"));
        }
        if let Some(nodes) = self.nodes {
            parts.push(format!("nodes {nodes}"));
        }
        if let Some(mate) = self.mate {
            parts.push(format!("mate {mate}"));
        }
        if let Some(movetime) = self.movetime {
            parts.push(format!("movetime {movetime}"));
        }
        if self.infinite || parts.is_empty() {
            parts.push(String::from("infinite"));
        }

        parts.join(" ")
    }
}

/// Registration parameters for protected engines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegisterCommand<'a> {
    /// Postpone registration (`register later`).
    Later,
    /// Register with name and license code (`register name <name> code <code>`).
    Credentials {
        /// Registrant name.
        name: Cow<'a, str>,
        /// Registration code / key.
        code: Cow<'a, str>,
    },
}

/// Strongly typed outgoing UCI commands sent from a GUI (or bridge) to a chess engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UciCommand<'a> {
    /// Tell engine to use the UCI protocol (`uci`).
    Uci,
    /// Toggle debug mode (`debug [on | off]`).
    Debug(bool),
    /// Synchronize engine status (`isready`).
    IsReady,
    /// Set an engine option (`setoption name <name> [value <value>]`).
    SetOption {
        /// Option identifier.
        name: Cow<'a, str>,
        /// Option value (if any).
        value: Option<Cow<'a, str>>,
    },
    /// Provide engine registration info (`register ...`).
    Register(RegisterCommand<'a>),
    /// Inform engine that the next search will be from a new game (`ucinewgame`).
    UciNewGame,
    /// Set up position on internal board (`position [startpos|fen <fen>] [moves <m1> <m2> ...]`).
    Position {
        /// Initial position setup.
        initial: PositionInitial<'a>,
        /// Moves played from the initial setup.
        moves: Vec<Cow<'a, str>>,
    },
    /// Start engine calculation (`go ...`).
    Go(GoParams<'a>),
    /// Stop engine calculation immediately (`stop`).
    Stop,
    /// Inform engine that the opponent played the expected ponder move (`ponderhit`).
    PonderHit,
    /// Quit engine process (`quit`).
    Quit,
}

impl<'a> UciCommand<'a> {
    /// Helper to construct a SetOption with borrowed string slices.
    #[must_use]
    pub fn set_option(name: &'a str, value: Option<&'a str>) -> Self {
        Self::SetOption {
            name: Cow::Borrowed(name),
            value: value.map(Cow::Borrowed),
        }
    }

    /// Helper to construct a SetOption with owned Strings.
    #[must_use]
    pub fn set_option_owned(name: String, value: Option<String>) -> Self {
        Self::SetOption {
            name: Cow::Owned(name),
            value: value.map(Cow::Owned),
        }
    }

    /// Format the command into a UCI protocol line (without trailing newline).
    #[must_use]
    pub fn to_uci_line(&self) -> String {
        match self {
            Self::Uci => String::from("uci"),
            Self::Debug(true) => String::from("debug on"),
            Self::Debug(false) => String::from("debug off"),
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
            Self::Register(RegisterCommand::Later) => String::from("register later"),
            Self::Register(RegisterCommand::Credentials { name, code }) => {
                format!("register name {name} code {code}")
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
                    let move_str = moves
                        .iter()
                        .map(|s| s.as_ref())
                        .collect::<Vec<_>>()
                        .join(" ");
                    format!("position {init_part} moves {move_str}")
                }
            }
            Self::Go(params) => {
                let args = params.to_uci_args();
                format!("go {args}")
            }
            Self::Stop => String::from("stop"),
            Self::PonderHit => String::from("ponderhit"),
            Self::Quit => String::from("quit"),
        }
    }
}

impl<'a> fmt::Display for UciCommand<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_uci_line())
    }
}
