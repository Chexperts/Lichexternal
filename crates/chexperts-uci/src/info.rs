//! Structured calculation and search progress report types (`info ...`).

extern crate alloc;

use alloc::vec::Vec;
use core::str::FromStr;

/// Evaluation score reported by the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UciScore {
    /// Centipawn evaluation (positive = advantage for side to move).
    Cp(i32),
    /// Forced mate in N moves (positive = side to move mates, negative = gets mated).
    Mate(i32),
}

/// Score bounds reported during search (for selective depth and aspiration windows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreBound {
    /// Fail-low: value is an upper bound (`upperbound`).
    UpperBound,
    /// Fail-high: value is a lower bound (`lowerbound`).
    LowerBound,
}

/// Structured calculation information parsed from an `info` line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UciInfo<'a> {
    /// Search depth in plies (`depth <x>`).
    pub depth: Option<u32>,
    /// Selective search depth (`seldepth <x>`).
    pub seldepth: Option<u32>,
    /// Multi-PV index (1-based, default 1) (`multipv <x>`).
    pub multipv: Option<u32>,
    /// Numerical score (`score cp <x>` or `score mate <x>`).
    pub score: Option<UciScore>,
    /// Bound on the score (`lowerbound` / `upperbound`).
    pub score_bound: Option<ScoreBound>,
    /// Total positions evaluated (`nodes <x>`).
    pub nodes: Option<u64>,
    /// Nodes calculated per second (`nps <x>`).
    pub nps: Option<u64>,
    /// Elapsed search time in milliseconds (`time <x>`).
    pub time_ms: Option<u64>,
    /// Hash table fullness (per mille, 0..1000) (`hashfull <x>`).
    pub hashfull: Option<u32>,
    /// Tablebase hits (`tbhits <x>`).
    pub tbhits: Option<u64>,
    /// Shredder endgame tablebase hits (`sbhits <x>`).
    pub sbhits: Option<u64>,
    /// CPU load of engine process in per mille (0..1000) (`cpuload <x>`).
    pub cpuload: Option<u32>,
    /// Currently searched move (`currmove <m>`).
    pub currmove: Option<&'a str>,
    /// Number of the currently searched move (1..N) (`currmovenumber <x>`).
    pub currmovenumber: Option<u32>,
    /// Principal variation moves in UCI notation (`pv <m1> ... <mn>`).
    pub pv: Vec<&'a str>,
    /// Any arbitrary text sent with `string <str>`.
    pub string: Option<&'a str>,
    /// Original line slice for transparent forwarding.
    pub raw: &'a str,
}

/// Parse an `info` line body into structured `UciInfo`.
#[must_use]
pub fn parse_info<'a>(rest: &'a str, raw: &'a str) -> UciInfo<'a> {
    let mut info = UciInfo {
        raw,
        ..Default::default()
    };

    let mut tokens = rest.split_ascii_whitespace();

    while let Some(token) = tokens.next() {
        match token {
            "depth" => {
                if let Some(val) = tokens.next().and_then(|v| u32::from_str(v).ok()) {
                    info.depth = Some(val);
                }
            }
            "seldepth" => {
                if let Some(val) = tokens.next().and_then(|v| u32::from_str(v).ok()) {
                    info.seldepth = Some(val);
                }
            }
            "multipv" => {
                if let Some(val) = tokens.next().and_then(|v| u32::from_str(v).ok()) {
                    info.multipv = Some(val);
                }
            }
            "score" => {
                if let Some(score_type) = tokens.next() {
                    match score_type {
                        "cp" => {
                            if let Some(val) = tokens.next().and_then(|v| i32::from_str(v).ok()) {
                                info.score = Some(UciScore::Cp(val));
                            }
                        }
                        "mate" => {
                            if let Some(val) = tokens.next().and_then(|v| i32::from_str(v).ok()) {
                                info.score = Some(UciScore::Mate(val));
                            }
                        }
                        _ => {}
                    }
                }
            }
            "lowerbound" => {
                info.score_bound = Some(ScoreBound::LowerBound);
            }
            "upperbound" => {
                info.score_bound = Some(ScoreBound::UpperBound);
            }
            "nodes" => {
                if let Some(val) = tokens.next().and_then(|v| u64::from_str(v).ok()) {
                    info.nodes = Some(val);
                }
            }
            "nps" => {
                if let Some(val) = tokens.next().and_then(|v| u64::from_str(v).ok()) {
                    info.nps = Some(val);
                }
            }
            "time" => {
                if let Some(val) = tokens.next().and_then(|v| u64::from_str(v).ok()) {
                    info.time_ms = Some(val);
                }
            }
            "hashfull" => {
                if let Some(val) = tokens.next().and_then(|v| u32::from_str(v).ok()) {
                    info.hashfull = Some(val);
                }
            }
            "tbhits" => {
                if let Some(val) = tokens.next().and_then(|v| u64::from_str(v).ok()) {
                    info.tbhits = Some(val);
                }
            }
            "sbhits" => {
                if let Some(val) = tokens.next().and_then(|v| u64::from_str(v).ok()) {
                    info.sbhits = Some(val);
                }
            }
            "cpuload" => {
                if let Some(val) = tokens.next().and_then(|v| u32::from_str(v).ok()) {
                    info.cpuload = Some(val);
                }
            }
            "currmove" => {
                info.currmove = tokens.next();
            }
            "currmovenumber" => {
                if let Some(val) = tokens.next().and_then(|v| u32::from_str(v).ok()) {
                    info.currmovenumber = Some(val);
                }
            }
            "pv" => {
                // All remaining tokens belong to the principal variation sequence
                for move_tok in tokens.by_ref() {
                    info.pv.push(move_tok);
                }
                break;
            }
            "string" => {
                // Info string follows; remainder of rest line is string payload
                if let Some(idx) = rest.find("string") {
                    let str_val = rest[idx + "string".len()..].trim_start();
                    info.string = Some(str_val);
                }
                break;
            }
            _ => {}
        }
    }

    info
}
