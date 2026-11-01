//! Shader families (`.shaderbin`, in a track's `bin.zip` under `shaders/`, in `media/Cars/_library/shaders/*.zip` and
//! in the particle and library zips): the tables a material's parameters resolve through. A shaderbin is a
//! [Grub bundle](crate::bundle) of many blobs; this module reads the six that name a family's parameters and say
//! where each lands, each blob exactly once:
//!
//! - `PARM` 1.2, the parameters the family declares ([`ParameterName`]): a u32 count, then per parameter the CRC-32
//!   of its name (u32), its name (a string as [`crate::material`] reads them), its hash (u32: the hash material
//!   parameters carry), a u8 count and that many bytes (kept as stored).
//! - `CBMP`, `TXMP` and `SPMP` 3.1 ([`MapEntry`]): a u16 count, then per entry a parameter hash (u32), a u16 and a
//!   16-byte GUID; the u16 is the parameter's byte offset in the material constants (`CBMP`), its texture slot
//!   (`TXMP`) or its sampler slot (`SPMP`).
//! - `CBEX` 1.0: a u16, the material constants' size in bytes.
//! - `DFPR` 2.1: the family's default parameter values ([`crate::material::Parameters`]).
//!
//! The other blobs (`VARS`, the variant table; `LSCE` and `DBLS`, the light scenarios and their compiled programs;
//! `VERT`, `TXRF`, `VERS`, `TRGT`, `SHUD`) are not read here; [`crate::bundle`] gives their bytes.
//!
//! Refused, never skipped: a missing or repeated one of the six blobs, another version, bytes left over after the
//! records or short of them, a `CBEX` of another size, and strings cut short or not UTF-8.
//!
//! What has been checked, and how: see `tests/material.rs`.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::bundle::{Blob, Bundle, Tag, Version};
use crate::material::{Parameters, string};
use crate::{Error, Result};
use primase_foundations::bytes::{Cursor, Endian::Little as LE};
use std::fmt;

/// The parameter names.
pub const PARM: Tag = Tag::new(b"PARM");
/// The constant offsets.
pub const CBMP: Tag = Tag::new(b"CBMP");
/// The texture slots.
pub const TXMP: Tag = Tag::new(b"TXMP");
/// The sampler slots.
pub const SPMP: Tag = Tag::new(b"SPMP");
/// The constants' size.
pub const CBEX: Tag = Tag::new(b"CBEX");
/// The default parameter values.
pub const DFPR: Tag = Tag::new(b"DFPR");

const PARM_VERSION: Version = Version { major: 1, minor: 2 };
const MAP_VERSION: Version = Version { major: 3, minor: 1 };
const CBEX_VERSION: Version = Version { major: 1, minor: 0 };
const DFPR_VERSION: Version = Version { major: 2, minor: 1 };

fn fail(what: impl fmt::Display) -> Error {
    Error::Shader(what.to_string())
}

/// A parameter a family declares (a `PARM` record).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParameterName<'a> {
    /// The first u32: the CRC-32 of the name.
    pub name_hash: u32,
    /// The name.
    pub name: &'a str,
    /// The hash material parameters carry for it.
    pub hash: u32,
    /// The bytes after the hash's count byte, as stored.
    pub extra: &'a [u8],
}

/// A `CBMP`, `TXMP` or `SPMP` entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapEntry {
    /// The parameter's hash.
    pub hash: u32,
    /// The constant byte offset, texture slot or sampler slot.
    pub value: u16,
    /// The GUID, as stored.
    pub guid: [u8; 16],
}

/// A shader family's parameter tables.
#[derive(Clone, Debug, PartialEq)]
pub struct Shader<'a> {
    /// `PARM`: the parameters it declares, in stored order.
    pub names: Vec<ParameterName<'a>>,
    /// `CBMP`: parameter hash to byte offset in the material constants.
    pub constants: Vec<MapEntry>,
    /// `TXMP`: parameter hash to texture slot.
    pub textures: Vec<MapEntry>,
    /// `SPMP`: parameter hash to sampler slot.
    pub samplers: Vec<MapEntry>,
    /// `CBEX`: the material constants' size in bytes.
    pub constants_size: u16,
    /// `DFPR`: the default values.
    pub defaults: Parameters<'a>,
}

fn one<'b, 'a>(bundle: &'b Bundle<'a>, tag: Tag, version: Version) -> Result<&'b Blob<'a>> {
    let mut found = bundle.tagged(tag);
    let blob = found.next().ok_or_else(|| fail(format!("no {tag} blob")))?;
    if found.next().is_some() {
        return Err(fail(format!("more than one {tag} blob")));
    }
    if blob.version != version {
        return Err(fail(format!("{tag} version {}, not {version}", blob.version)));
    }
    Ok(blob)
}

fn end(c: &Cursor<'_>, tag: Tag, count: usize) -> Result<()> {
    if c.remaining() != 0 {
        return Err(fail(format!(
            "{tag}: {} bytes left at {:#x} after {count} records",
            c.remaining(),
            c.position()
        )));
    }
    Ok(())
}

fn map(blob: &Blob<'_>) -> Result<Vec<MapEntry>> {
    let tag = blob.tag;
    let mut c = Cursor::new(blob.data);
    let count = usize::from(c.u16(LE).map_err(|e| fail(format!("{tag}'s count: {e}")))?);
    let mut entries = Vec::with_capacity(count);
    for k in 0..count {
        let e = |e| fail(format!("{tag} entry {k}: {e}"));
        entries.push(MapEntry {
            hash: c.u32(LE).map_err(e)?,
            value: c.u16(LE).map_err(e)?,
            guid: c.array().map_err(e)?,
        });
    }
    end(&c, tag, count)?;
    Ok(entries)
}

impl<'a> Shader<'a> {
    /// The tables of a shaderbin's bytes.
    pub fn parse(bytes: &'a [u8]) -> Result<Shader<'a>> {
        Shader::from_bundle(&Bundle::parse(bytes)?)
    }

    /// The tables of a bundle already read. Refused: anything the module docs list.
    pub fn from_bundle(bundle: &Bundle<'a>) -> Result<Shader<'a>> {
        let parm = one(bundle, PARM, PARM_VERSION)?;
        let mut c = Cursor::new(parm.data);
        let count = c.u32(LE).map_err(|e| fail(format!("PARM's count: {e}")))?;
        let count = usize::try_from(count).map_err(|_| fail("PARM's count past a usize"))?;
        let mut names = Vec::with_capacity(count.min(parm.data.len()));
        for k in 0..count {
            let e = |e: String| fail(format!("PARM record {k}: {e}"));
            let name_hash = c.u32(LE).map_err(|x| e(x.to_string()))?;
            let name = string(&mut c, "the name").map_err(e)?;
            let hash = c.u32(LE).map_err(|x| e(x.to_string()))?;
            let n = c.u8().map_err(|x| e(x.to_string()))?;
            let extra = c.take(usize::from(n)).map_err(|x| e(x.to_string()))?;
            names.push(ParameterName {
                name_hash,
                name,
                hash,
                extra,
            });
        }
        end(&c, PARM, count)?;
        let constants = map(one(bundle, CBMP, MAP_VERSION)?)?;
        let textures = map(one(bundle, TXMP, MAP_VERSION)?)?;
        let samplers = map(one(bundle, SPMP, MAP_VERSION)?)?;
        let cbex = one(bundle, CBEX, CBEX_VERSION)?;
        let constants_size = match cbex.data {
            [a, b] => u16::from_le_bytes([*a, *b]),
            d => return Err(fail(format!("CBEX is {} bytes, not 2", d.len()))),
        };
        let dfpr = one(bundle, DFPR, DFPR_VERSION)?;
        let defaults = Parameters::parse(dfpr.data, dfpr.version)?;
        Ok(Shader {
            names,
            constants,
            textures,
            samplers,
            constants_size,
            defaults,
        })
    }

    /// The declared parameter with a hash (the first, if the table repeats one).
    pub fn name_of(&self, hash: u32) -> Option<&ParameterName<'a>> {
        self.names.iter().find(|n| n.hash == hash)
    }
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    reason = "tests"
)]
mod tests {
    use super::*;
    use crate::material::tests::{param, text};
    use crate::test_bundle::{BlobSpec, bundle};

    fn parm(records: &[(&str, u32, &[u8])]) -> Vec<u8> {
        let mut d = (records.len() as u32).to_le_bytes().to_vec();
        for (name, hash, extra) in records {
            d.extend_from_slice(&primase_foundations::hash::crc32(name.as_bytes()).to_le_bytes());
            d.extend_from_slice(&text(name));
            d.extend_from_slice(&hash.to_le_bytes());
            d.push(extra.len() as u8);
            d.extend_from_slice(extra);
        }
        d
    }

    fn map(entries: &[(u32, u16)]) -> Vec<u8> {
        let mut d = (entries.len() as u16).to_le_bytes().to_vec();
        for (hash, value) in entries {
            d.extend_from_slice(&hash.to_le_bytes());
            d.extend_from_slice(&value.to_le_bytes());
            d.extend_from_slice(&[*value as u8; 16]);
        }
        d
    }

    fn dfpr() -> Vec<u8> {
        let mut d = 1u16.to_le_bytes().to_vec();
        d.extend_from_slice(&param(
            0x10,
            Some(primase_foundations::hash::crc32(b"Gloss")),
            2,
            &1f32.to_le_bytes(),
        ));
        d
    }

    struct Parts {
        parm: Vec<u8>,
        cbmp: Vec<u8>,
        txmp: Vec<u8>,
        spmp: Vec<u8>,
        cbex: Vec<u8>,
        dfpr: Vec<u8>,
    }

    fn parts() -> Parts {
        Parts {
            parm: parm(&[
                ("Gloss", 0x10, &[]),
                ("Diffuse", 0x20, &[1, 2, 3]),
                ("Wrap", 0x30, &[]),
            ]),
            cbmp: map(&[(0x10, 16)]),
            txmp: map(&[(0x20, 0)]),
            spmp: map(&[(0x30, 1)]),
            cbex: 48u16.to_le_bytes().to_vec(),
            dfpr: dfpr(),
        }
    }

    fn build(p: &Parts) -> Vec<u8> {
        let blobs: Vec<BlobSpec<'_>> = vec![
            (b"PARM", (1, 2), vec![], &p.parm),
            (b"CBMP", (3, 1), vec![], &p.cbmp),
            (b"TXMP", (3, 1), vec![], &p.txmp),
            (b"SPMP", (3, 1), vec![], &p.spmp),
            (b"CBEX", (1, 0), vec![], &p.cbex),
            (b"VARS", (1, 8), vec![], &[1, 2, 3, 4]),
            (b"DFPR", (2, 1), vec![], &p.dfpr),
        ];
        bundle(&blobs)
    }

    #[test]
    fn reads_the_parameter_tables() {
        let bytes = build(&parts());
        let s = Shader::parse(&bytes).unwrap();
        assert_eq!(s.names.len(), 3);
        assert_eq!(s.names[1].name, "Diffuse");
        assert_eq!((s.names[1].hash, s.names[1].extra), (0x20, &[1u8, 2, 3][..]));
        assert_eq!(s.names[0].name_hash, primase_foundations::hash::crc32(b"Gloss"));
        assert_eq!(
            (s.constants[0].hash, s.constants[0].value, s.constants[0].guid),
            (0x10, 16, [16; 16])
        );
        assert_eq!(s.textures[0].value, 0);
        assert_eq!(s.samplers[0].value, 1);
        assert_eq!(s.constants_size, 48);
        assert_eq!(s.defaults.list.len(), 1);
        assert_eq!(
            s.defaults.list[0].flag_hash,
            Some(primase_foundations::hash::crc32(b"Gloss"))
        );
        assert_eq!(s.name_of(0x30).unwrap().name, "Wrap");
        assert!(s.name_of(0x99).is_none());
    }

    #[test]
    fn refuses_what_it_does_not_account_for() {
        let refused = |p: &Parts, expect: &str| match Shader::parse(&build(p)) {
            Err(Error::Shader(m)) => assert!(m.contains(expect), "{m:?} lacks {expect:?}"),
            other => panic!("expected a refusal naming {expect:?}, got {other:?}"),
        };
        let mut p = parts();
        p.parm.push(0);
        refused(&p, "PARM: 1 bytes left");
        let mut p = parts();
        p.parm.truncate(p.parm.len() - 1);
        refused(&p, "PARM record 2");
        let mut p = parts();
        p.cbmp.push(0);
        refused(&p, "CBMP: 1 bytes left");
        let mut p = parts();
        p.txmp.truncate(p.txmp.len() - 3);
        refused(&p, "TXMP entry 0");
        let mut p = parts();
        p.cbex.push(0);
        refused(&p, "CBEX is 3 bytes");
        let mut p = parts();
        p.dfpr.push(0);
        // the default values are parameters, read (and refused) as a material's are
        assert!(matches!(Shader::parse(&build(&p)), Err(Error::Material(m)) if m.contains("bytes left")));

        let p = parts();
        let without = bundle(&[(b"PARM", (1, 2), vec![], &p.parm)]);
        assert!(matches!(Shader::parse(&without), Err(Error::Shader(m)) if m.contains("no CBMP")));
        let twice = bundle(&[
            (b"PARM", (1, 2), vec![], &p.parm),
            (b"PARM", (1, 2), vec![], &p.parm),
        ]);
        assert!(matches!(Shader::parse(&twice), Err(Error::Shader(m)) if m.contains("more than one PARM")));
        let other = bundle(&[(b"PARM", (1, 3), vec![], &p.parm)]);
        assert!(matches!(Shader::parse(&other), Err(Error::Shader(m)) if m.contains("PARM version 1.3")));
    }
}
