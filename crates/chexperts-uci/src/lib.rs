//! Universal Chess Interface (UCI) protocol parser and serializer.
//!
//! Provides zero-copy, `#![no_std]` compliant types for serializing GUI-to-Engine commands
//! and parsing Engine-to-GUI responses according to the official UCI specification.

#![no_std]

extern crate alloc;

pub mod command;
pub mod info;
pub mod output;

pub use command::{GoParams, PositionInitial, RegisterCommand, UciCommand};
pub use info::{parse_info, ScoreBound, UciInfo, UciScore};
pub use output::{
    parse_uci_line, CopyProtectionStatus, OptionType, RegistrationStatus, UciOptionDeclaration,
    UciOutputLine,
};
