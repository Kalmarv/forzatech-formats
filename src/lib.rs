//! Read ForzaTech files, as Forza Horizon 6 ships them.
//!
//! This crate reads, from bytes or a reader you give it (it opens no file itself):
//!
//! - [`asset_manifest`]: the asset manifests (`AssetManifest.xml`): which entry of a `bin.zip` holds each shader and
//!   material, by the source path it was built from, and a track's textures and models with the instances that use
//!   them.
//! - [`bundle`]: the Grub bundles models, textures, materials and shaders are stored in: their blobs, metadata
//!   records and data, as stored.
//! - [`container`]: the encrypted container (the Arxan TransformIT envelope) that wraps Forza Horizon 6's game
//!   database, its settings archives' method-22 zip entries, `sfsdata`, `NerdData.json` and the player's profile: the
//!   36-byte header, its blocks and their MACs, and decryption with keys you supply.
//! - [`gameobjs`]: the named places gameplay refers to (`GameObjs.xml`): each object's name, position and axes.
//! - [`keys`]: those keys, parsed from a small text file you keep. No key ships with this crate.
//! - [`gamedb`]: the second layer of the game database (`gamedbRC.slt`), a keystream that turns the decrypted
//!   container into an SQLite image, and each car's stock tyre and rim sizes read from it.
//! - [`modelbin`]: models (`.modelbin`): their header, skeleton, material instances (as bundles), meshes, vertex
//!   layouts and buffers, as stored, and one mesh's indices and vertices decoded.
//! - [`minizip`]: the PGZP streaming archives a track's world is stored in (`GeoChunkN.minizip`), their entries
//!   decoded, and the chunk map and contents list beside each that type and name the entries.
//! - [`particles`]: the particle instances (`.particlebin`) a track and its routes place: effect, position, rotation
//!   and the strings that gate them.
//! - [`pgeo`]: the placement files (`.pgeo`) of a track's procedural world: every instance's position, rotation and
//!   scale, its spline-deform curve, its season and variant data, and the template its block names.
//! - [`plib`]: the template libraries (`.plib`) those blocks name: draw behaviours, model templates with the model paths
//!   they list, placement templates, and the group libraries of the `Placement` folders.
//! - [`pvs`]: the hand-placed instance file (`.pvs`): the instances not placed by a rule, with their transforms, LOD
//!   rows, models, textures and clips.
//! - [`settings`]: the `DatabaseFile` settings XML of the encrypted render and game settings entries, read as
//!   stored.
//! - [`track`]: the index of a track's GeoChunk entries by chunk map reference and by model path, which the placement
//!   files' model references resolve through.
//! - [`world`]: the world frame the placement files share: fixed-point positions, half floats, x/z boxes, the cell
//!   grid and the seasons.
//! - [`zip`]: the zip archives the settings, cars, shaders and libraries are kept in, their entries decoded and, with
//!   keys you supply, their encrypted (method 22) entries decrypted.
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

pub mod asset_manifest;
pub mod bundle;
pub mod container;
pub mod gamedb;
pub mod gameobjs;
pub mod keys;
pub mod minizip;
pub mod modelbin;
pub mod particles;
pub mod pgeo;
pub mod plib;
pub mod pvs;
mod read;
pub mod settings;
pub mod track;
pub mod world;
mod xml;
pub mod zip;

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
    /// The bytes are not a Grub bundle this crate reads.
    #[error("bundle: {0}")]
    Bundle(String),
    /// The bytes are not a model this crate reads, or a mesh does not decode.
    #[error("modelbin: {0}")]
    ModelBin(String),
    /// The bytes are not a PGZP archive's directory, a chunk map or a contents list, or an entry does not decode.
    #[error("minizip: {0}")]
    MiniZip(String),
    /// The bytes are not a `.pgeo` placement file this crate reads.
    #[error("pgeo: {0}")]
    Pgeo(String),
    /// The bytes are not an `AssetManifest.xml` asset manifest this crate reads.
    #[error("asset manifest: {0}")]
    AssetManifest(String),
    /// The bytes are not a `GameObjs.xml` game objects file this crate reads.
    #[error("game objects: {0}")]
    GameObjs(String),
    /// The bytes are not a `.particlebin` particle instance file this crate reads.
    #[error("particles: {0}")]
    Particles(String),
    /// The bytes are not a `.plib` template library this crate reads.
    #[error("plib: {0}")]
    Plib(String),
    /// The bytes are not a `.pvs` instance file this crate reads.
    #[error("pvs: {0}")]
    Pvs(String),
    /// A track's chunk maps and contents lists do not index its entries as this crate reads them.
    #[error("track: {0}")]
    Track(String),
    /// The bytes are not a zip archive this crate reads, or an entry does not decode.
    #[error("zip: {0}")]
    Zip(String),
    /// The game database lacks a table or column a reader needs, or holds a value it does not read.
    #[error("game database: {0}")]
    GameDb(String),
    /// The bytes are not a `DatabaseFile` settings file this crate reads.
    #[error("settings: {0}")]
    Settings(String),
    /// Reading from the reader the caller gave failed.
    #[error("I/O: {0}")]
    Io(String),
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
