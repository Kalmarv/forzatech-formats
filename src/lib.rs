//! Read ForzaTech files, as Forza Horizon 6 ships them.
//!
//! This crate reads, from bytes you give it, without file access:
//!
//! - [`container`]: the encrypted container (the Arxan TransformIT envelope) that wraps Forza Horizon 6's game
//!   database, its settings archives' method-22 zip entries, `sfsdata`, `NerdData.json` and the player's profile: the
//!   36-byte header, its blocks and their MACs, and decryption with keys you supply.
//! - [`keys`]: those keys, parsed from a small text file you keep. No key ships with this crate.
//! - [`gamedb`]: the second layer of the game database (`gamedbRC.slt`), a keystream that turns the decrypted
//!   container into an SQLite image.
//!
//! Every read is checked: untrusted bytes give an [`Error`] naming what was expected, never a panic.
//!
//! # Example
//!
//! ```
//! use forzatech_formats::container::{BlockSize, Container};
//!
//! # fn main() -> Result<(), forzatech_formats::Error> {
//! // A container header and one 512-byte block with its MAC, built by hand: the header states 12 bytes of padding.
//! let mut file = vec![0u8; 36 + 512 + 16];
//! file[16..20].copy_from_slice(&12u32.to_le_bytes());
//! let container = Container::parse(&file, BlockSize::Small)?;
//! assert_eq!(container.block_count(), 1);
//! assert_eq!(container.plaintext_len(), 500);
//! # Ok(())
//! # }
//! ```
#![deny(clippy::disallowed_methods, clippy::disallowed_types)]
#![deny(missing_docs)]

pub mod container;
pub mod gamedb;
pub mod keys;

#[cfg(test)]
#[path = "../tests/inputs/mod.rs"]
mod inputs;

/// What went wrong reading a file or a keys text.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The bytes are not a container of the stated block size, or are cut short.
    #[error("container: {0}")]
    Container(String),
    /// A MAC did not match: the wrong key, or bytes that are not this kind of container.
    #[error("container: the MAC of {what} does not match (a wrong key, or not a container of this kind)")]
    Mac {
        /// The header, or the block at an index.
        what: String,
    },
    /// The keys text is malformed: the line (counted from 1) and what is wrong with it.
    #[error("keys, line {line}: {detail}")]
    Keys {
        /// The line, counted from 1 (0 for what concerns the whole text).
        line: usize,
        /// What is wrong.
        detail: String,
    },
}

/// A result whose error is this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
