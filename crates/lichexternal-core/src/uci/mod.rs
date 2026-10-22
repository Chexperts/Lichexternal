//! UCI (Universal Chess Interface) command generation and zero-copy output parsing.

pub mod command;
pub mod output;

pub use command::{PositionInitial, UciCommand};
pub use output::{parse_uci_line, UciInfo, UciOutputLine, UciScore};
