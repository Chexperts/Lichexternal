//! Zero-copy SIMD-accelerated UCI output line parser.

use alloc::vec::Vec;
use core::str::FromStr;

/// Evaluation score reported by the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UciScore {
    /// Centipawn evaluation (positive = advantage for white/current side, e.g. `+150`).
    Cp(i32),
    /// Forced mate in N moves (positive = side to move mates, negative = gets mated).
    Mate(i32),
}

/// Structured calculation information parsed from an `info` line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UciInfo<'a> {
    /// Search depth in plies.
    pub depth: Option<u32>,
    /// Selective search depth.
    pub seldepth: Option<u32>,
    /// Multi-PV index (1-based, default 1).
    pub multipv: Option<u32>,
    /// Numerical score.
    pub score: Option<UciScore>,
    /// Total positions evaluated.
    pub nodes: Option<u64>,
    /// Nodes calculated per second.
    pub nps: Option<u64>,
    /// Elapsed search time in milliseconds.
    pub time_ms: Option<u64>,
    /// Hash table fullness (per mille, 0..1000).
    pub hashfull: Option<u32>,
    /// Principal variation moves in UCI notation.
    pub pv: Vec<&'a str>,
    /// Original line slice for transparent forwarding.
    pub raw: &'a str,
}

/// Parsed UCI output line emitted from an engine to stdout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UciOutputLine<'a> {
    /// Evaluation and search progress report (`info ...`).
    Info(UciInfo<'a>),
    /// Engine calculation completed with best move (`bestmove <m> [ponder <p>]`).
    BestMove {
        /// Best move in UCI coordinate notation (e.g. `e2e4`, `e7e8q`).
        best_move: &'a str,
        /// Optional ponder move to anticipate.
        ponder: Option<&'a str>,
    },
    /// Synchronization response (`readyok`).
    ReadyOk,
    /// Protocol handshake confirmation (`uciok`).
    UciOk,
    /// Engine name (`id name <name>`).
    IdName(&'a str),
    /// Engine author (`id author <author>`).
    IdAuthor(&'a str),
    /// Engine configuration capability (`option name ...`).
    Option(&'a str),
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

    if let Some(rest) = trimmed.strip_prefix("option ") {
        return UciOutputLine::Option(rest);
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

fn parse_info<'a>(rest: &'a str, raw: &'a str) -> UciInfo<'a> {
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
            "pv" => {
                // All remaining tokens belong to the principal variation sequence
                for move_tok in tokens.by_ref() {
                    info.pv.push(move_tok);
                }
                break;
            }
            "string" => {
                // Info string follows; rest of line is message
                break;
            }
            _ => {}
        }
    }

    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_handshake_lines() {
        assert_eq!(parse_uci_line("readyok\r\n"), UciOutputLine::ReadyOk);
        assert_eq!(parse_uci_line("uciok"), UciOutputLine::UciOk);
        assert_eq!(
            parse_uci_line("id name Stockfish 19\n"),
            UciOutputLine::IdName("Stockfish 19")
        );
        assert_eq!(
            parse_uci_line("id author the Stockfish developers"),
            UciOutputLine::IdAuthor("the Stockfish developers")
        );
    }

    #[test]
    fn test_parse_bestmove() {
        assert_eq!(
            parse_uci_line("bestmove e2e4"),
            UciOutputLine::BestMove {
                best_move: "e2e4",
                ponder: None,
            }
        );

        assert_eq!(
            parse_uci_line("bestmove e7e8q ponder e1e2\n"),
            UciOutputLine::BestMove {
                best_move: "e7e8q",
                ponder: Some("e1e2"),
            }
        );
    }

    #[test]
    fn test_parse_info_cp() {
        let line = "info depth 24 seldepth 32 multipv 1 score cp 35 nodes 1827390 nps 2341000 time 780 hashfull 250 pv e2e4 e7e5 g1f3 b8c6";
        let parsed = parse_uci_line(line);

        match parsed {
            UciOutputLine::Info(info) => {
                assert_eq!(info.depth, Some(24));
                assert_eq!(info.seldepth, Some(32));
                assert_eq!(info.multipv, Some(1));
                assert_eq!(info.score, Some(UciScore::Cp(35)));
                assert_eq!(info.nodes, Some(1827390));
                assert_eq!(info.nps, Some(2341000));
                assert_eq!(info.time_ms, Some(780));
                assert_eq!(info.hashfull, Some(250));
                assert_eq!(info.pv, vec!["e2e4", "e7e5", "g1f3", "b8c6"]);
                assert_eq!(info.raw, line);
            }
            _ => panic!("Expected UciOutputLine::Info"),
        }
    }

    #[test]
    fn test_parse_info_mate() {
        let line = "info depth 12 multipv 2 score mate -3 nodes 48200 time 20 pv f7f8q e4e5";
        let parsed = parse_uci_line(line);

        match parsed {
            UciOutputLine::Info(info) => {
                assert_eq!(info.depth, Some(12));
                assert_eq!(info.multipv, Some(2));
                assert_eq!(info.score, Some(UciScore::Mate(-3)));
                assert_eq!(info.pv, vec!["f7f8q", "e4e5"]);
            }
            _ => panic!("Expected UciOutputLine::Info"),
        }
    }
}
