//! Materials: the parent materials (`.materialbin`, in a track's `bin.zip` under `materials/` and in the car and
//! library zips) and the material instances nested in models (a model's `MatI` blob, whose data is a bundle of its
//! own). Both are [Grub bundles](crate::bundle); what they hold is a path to what they derive from and a list of
//! shader parameters ([`Parameters`], also the shape of a shader's `DFPR` defaults, see [`crate::shader`]).
//!
//! A `.materialbin` ([`Material`]) is three blobs, in this order:
//!
//! - `MATL` 1.2, with the metadata `Name` (version 0: the material's name, its whole payload), `ATST` (version 2,
//!   2 bytes), `BLST` (version 1, 5 bytes) and, on most, `XTRA` (version 4), in that order. Its data is three
//!   strings: the shader (`Game:\Media\...\<family>.shaderbin`), the shader technique (`Standard`, `AlphaTest`,
//!   `Road`, ..., or empty) and a third string (empty, or `UBER_FOLIAGE` on some foliage materials).
//! - `MTUD` 1.0: a u8 count, then that many pairs of strings (a key and its value, such as `FM5Path` and a source
//!   path).
//! - `MTPR` 2.0: the parameters.
//!
//! A material instance ([`Instance`], the bundle in a model's `MatI` blob) is two blobs: `MATI` 1.0 (stored as the
//! bytes `ITAM`), with the metadata `Name` (version 0), `ATST` (version 2, 2 bytes) and `XTRA` (version 5), whose data
//! is one string, the parent material (`Game:\Media\...\<name>.materialbin`); and `MTPR` 2.0. A `MatI` blob can also
//! be empty (no bytes), which [`Instance::from_model_blob`] gives as `None`.
//!
//! A string is a length in 7-bit groups (low group first, the high bit set on every byte but the last) and that
//! many bytes of UTF-8.
//!
//! A parameter list ([`Parameters::parse`]) is a count (u8 in a version 2.0 blob, u16 in 2.1), the parameters, and
//! after them, in a 2.0 blob only, 12 bytes kept as stored. A parameter:
//!
//! | size | field |
//! |---|---|
//! | 1 | version major (3) |
//! | 1 | version minor (4; 3 on some car materials, 1 on a few others) |
//! | 4 | hash (u32): the parameter, as its shader's `PARM` table names it |
//! | 1 | flag (0 or 1) |
//! | 4 | when the flag is 1: a second hash (u32) |
//! | 1 | type |
//! | 16 | GUID |
//! | by type | the value ([`Value`]) |
//!
//! Types: 0 a vector, 1 a colour, 5 a swizzle and 12 four 32-bit words (16 bytes each); 2 a float, 3 a bool and 4
//! an int (4 bytes each); 6 a texture: a string (the texture's game path, often empty) and a u32 ([`path_hash`] of
//! the path); 7 a sampler state, four u32; 8 a gradient: a u32 count and that many 4-byte entries; 11 a vector of
//! two floats.
//!
//! Refused, never skipped: another blob, blob or metadata version, order or count than the above; metadata payloads
//! of another size; bytes left over after the strings, the pairs or the parameters (other than the 2.0 list's 12);
//! a parameter version other than 3.1, 3.3 or 3.4; a flag other than 0 or 1; a type other than those listed; strings
//! that are cut short or not UTF-8.
//!
//! Which value a material ends up with (an instance's parameter, its parent's, or the shader's default) is not
//! decided here.
//!
//! What has been checked, and how: see `tests/material.rs`.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::bundle::{Blob, Bundle, Metadata, Tag, Version};
use crate::{Error, Result};
use primase_foundations::bytes::{Cursor, Endian::Little as LE};
use std::fmt;

/// A `.materialbin`'s first blob.
pub const MATL: Tag = Tag::new(b"MATL");
/// A material instance's first blob (stored as the bytes `ITAM`).
pub const MATI: Tag = Tag::new(b"MATI");
/// The model blob a material instance bundle is nested in.
pub const MAT_I: Tag = Tag::new(b"MatI");
/// A `.materialbin`'s user data blob.
pub const MTUD: Tag = Tag::new(b"MTUD");
/// A material's parameter blob.
pub const MTPR: Tag = Tag::new(b"MTPR");
/// The `Name` metadata.
pub const NAME: Tag = Tag::new(b"Name");
/// The `ATST` metadata.
pub const ATST: Tag = Tag::new(b"ATST");
/// The `BLST` metadata.
pub const BLST: Tag = Tag::new(b"BLST");
/// The `XTRA` metadata.
pub const XTRA: Tag = Tag::new(b"XTRA");

fn fail(what: impl fmt::Display) -> Error {
    Error::Material(what.to_string())
}

/// The hash a texture parameter stores after its path: the CRC-32 of the path with its ASCII letters lowercased
/// (`Game:\Media\...` hashes as `game:\media\...`).
pub fn path_hash(path: &str) -> u32 {
    primase_foundations::hash::crc32(path.to_ascii_lowercase().as_bytes())
}

/// Reads a string: a length in 7-bit groups and that many bytes of UTF-8.
pub(crate) fn string<'a>(c: &mut Cursor<'a>, what: &str) -> std::result::Result<&'a str, String> {
    let at = c.position();
    let mut len = 0usize;
    let mut shift = 0u32;
    loop {
        let b = c.u8().map_err(|e| format!("{what}'s length: {e}"))?;
        let group = usize::from(b & 0x7F)
            .checked_shl(shift)
            .filter(|_| shift < usize::BITS)
            .ok_or_else(|| format!("{what}'s length at {at:#x} runs past a usize"))?;
        len |= group;
        if b & 0x80 == 0 {
            break;
        }
        shift = shift.saturating_add(7);
    }
    let bytes = c.take(len).map_err(|e| format!("{what} ({len} bytes): {e}"))?;
    std::str::from_utf8(bytes).map_err(|e| format!("{what} at {at:#x} is not UTF-8: {e}"))
}

/// A parameter's value, as stored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value<'a> {
    /// Type 0: four floats.
    Vector([f32; 4]),
    /// Type 1: a colour, four floats.
    Colour([f32; 4]),
    /// Type 2: a float.
    Float(f32),
    /// Type 3: a bool, the u32 as stored.
    Bool(u32),
    /// Type 4: an int.
    Int(i32),
    /// Type 5: a swizzle, four floats.
    Swizzle([f32; 4]),
    /// Type 6: a texture.
    Texture {
        /// The texture's game path as stored (often empty).
        path: &'a str,
        /// The u32 after the path: [`path_hash`] of the texture's game path, stored also when the path is empty.
        path_hash: u32,
    },
    /// Type 7: a sampler state, four u32 as stored.
    Sampler([u32; 4]),
    /// Type 8: a gradient: its entries, 4 bytes each, as stored (a u32 count, then the entries).
    Gradient(&'a [u8]),
    /// Type 11: two floats.
    Vector2([f32; 2]),
    /// Type 12: four 32-bit words as stored.
    Type12([u32; 4]),
}

impl Value<'_> {
    /// The stored type number.
    pub fn type_number(&self) -> u8 {
        match self {
            Value::Vector(_) => 0,
            Value::Colour(_) => 1,
            Value::Float(_) => 2,
            Value::Bool(_) => 3,
            Value::Int(_) => 4,
            Value::Swizzle(_) => 5,
            Value::Texture { .. } => 6,
            Value::Sampler(_) => 7,
            Value::Gradient(_) => 8,
            Value::Vector2(_) => 11,
            Value::Type12(_) => 12,
        }
    }
}

/// A shader parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parameter<'a> {
    /// Where it starts in its blob's data.
    pub offset: usize,
    /// Where it ends in its blob's data (the next parameter starts there, or the trailer, or the data ends).
    pub end: usize,
    /// Its version (3.4, 3.3 or 3.1; the layout is the same).
    pub version: Version,
    /// The parameter's hash.
    pub hash: u32,
    /// The second hash, when the flag byte is 1.
    pub flag_hash: Option<u32>,
    /// The GUID, as stored.
    pub guid: [u8; 16],
    /// The value.
    pub value: Value<'a>,
}

/// A parameter list: an `MTPR` (version 2.0) or a shader's `DFPR` (version 2.1).
#[derive(Clone, Debug, PartialEq)]
pub struct Parameters<'a> {
    /// The parameters, in stored order.
    pub list: Vec<Parameter<'a>>,
    /// The 12 bytes after a version 2.0 list, as stored (`None` for 2.1).
    pub trailer: Option<[u8; 12]>,
}

/// The parameter list versions read: 2.0 (a u8 count and a 12-byte trailer) and 2.1 (a u16 count).
pub const PARAMETERS_V2_0: Version = Version { major: 2, minor: 0 };
/// See [`PARAMETERS_V2_0`].
pub const PARAMETERS_V2_1: Version = Version { major: 2, minor: 1 };

impl<'a> Parameters<'a> {
    /// The parameters of a blob's data, its version saying the count's width and whether the trailer is there.
    /// Refused: another version, and anything the module docs list.
    pub fn parse(data: &'a [u8], version: Version) -> Result<Parameters<'a>> {
        let mut c = Cursor::new(data);
        let e = |e| fail(format!("the parameter count: {e}"));
        let count = match version {
            PARAMETERS_V2_0 => usize::from(c.u8().map_err(e)?),
            PARAMETERS_V2_1 => usize::from(c.u16(LE).map_err(e)?),
            v => {
                return Err(fail(format!(
                    "a parameter blob of version {v}, not {PARAMETERS_V2_0} or {PARAMETERS_V2_1}"
                )));
            }
        };
        let mut list = Vec::with_capacity(count.min(data.len()));
        for i in 0..count {
            list.push(parameter(&mut c, i)?);
        }
        let trailer = if version == PARAMETERS_V2_0 {
            Some(
                c.array::<12>()
                    .map_err(|e| fail(format!("the 12 bytes after {count} parameters: {e}")))?,
            )
        } else {
            None
        };
        if c.remaining() != 0 {
            return Err(fail(format!(
                "{} bytes left at {:#x} after {count} parameters{}",
                c.remaining(),
                c.position(),
                if trailer.is_some() {
                    " and the 12-byte trailer"
                } else {
                    ""
                }
            )));
        }
        Ok(Parameters { list, trailer })
    }

    /// The parameter with a hash (the first, if a list repeats one).
    pub fn get(&self, hash: u32) -> Option<&Parameter<'a>> {
        self.list.iter().find(|p| p.hash == hash)
    }
}

fn floats<const N: usize>(c: &mut Cursor<'_>) -> std::result::Result<[f32; N], String> {
    let mut v = [0f32; N];
    for x in &mut v {
        *x = f32::from_bits(c.u32(LE).map_err(|e| e.to_string())?);
    }
    Ok(v)
}

fn words<const N: usize>(c: &mut Cursor<'_>) -> std::result::Result<[u32; N], String> {
    let mut v = [0u32; N];
    for x in &mut v {
        *x = c.u32(LE).map_err(|e| e.to_string())?;
    }
    Ok(v)
}

fn parameter<'a>(c: &mut Cursor<'a>, i: usize) -> Result<Parameter<'a>> {
    let offset = c.position();
    let at = |what: &str, e: String| fail(format!("parameter {i} at {offset:#x}: {what}: {e}"));
    let version = Version {
        major: c.u8().map_err(|e| at("version", e.to_string()))?,
        minor: c.u8().map_err(|e| at("version", e.to_string()))?,
    };
    if version.major != 3 || !matches!(version.minor, 1 | 3 | 4) {
        return Err(fail(format!(
            "parameter {i} at {offset:#x}: version {version}, not 3.4, 3.3 or 3.1"
        )));
    }
    let hash = c.u32(LE).map_err(|e| at("hash", e.to_string()))?;
    let flag = c.u8().map_err(|e| at("flag", e.to_string()))?;
    let flag_hash = match flag {
        0 => None,
        1 => Some(c.u32(LE).map_err(|e| at("flag hash", e.to_string()))?),
        f => {
            return Err(fail(format!(
                "parameter {i} ({hash:#010x}) at {offset:#x}: the flag is {f}, not 0 or 1"
            )));
        }
    };
    let kind = c.u8().map_err(|e| at("type", e.to_string()))?;
    let guid = c.array::<16>().map_err(|e| at("GUID", e.to_string()))?;
    let what = format!("the type {kind} value of {hash:#010x}");
    let v = |r: std::result::Result<Value<'a>, String>| r.map_err(|e| at(&what, e));
    let value = match kind {
        0 => v(floats(c).map(Value::Vector))?,
        1 => v(floats(c).map(Value::Colour))?,
        2 => v(floats::<1>(c).map(|[x]| Value::Float(x)))?,
        3 => v(words::<1>(c).map(|[x]| Value::Bool(x)))?,
        4 => v(words::<1>(c).map(|[x]| Value::Int(i32::from_le_bytes(x.to_le_bytes()))))?,
        5 => v(floats(c).map(Value::Swizzle))?,
        6 => {
            let path = string(c, "the texture path").map_err(|e| at(&what, e))?;
            let path_hash = c
                .u32(LE)
                .map_err(|e| at("the texture path hash", e.to_string()))?;
            Value::Texture { path, path_hash }
        }
        7 => v(words(c).map(Value::Sampler))?,
        8 => {
            let n = c.u32(LE).map_err(|e| at("the gradient's count", e.to_string()))?;
            let len = usize::try_from(n)
                .ok()
                .and_then(|n| n.checked_mul(4))
                .ok_or_else(|| at(&what, format!("{n} entries past a usize")))?;
            let entries = c.take(len).map_err(|e| at(&what, e.to_string()))?;
            Value::Gradient(entries)
        }
        11 => v(floats(c).map(Value::Vector2))?,
        12 => v(words(c).map(Value::Type12))?,
        k => {
            return Err(fail(format!(
                "parameter {i} ({hash:#010x}) at {offset:#x}: type {k}, which is not read (0-8, 11 and 12 are)"
            )));
        }
    };
    Ok(Parameter {
        offset,
        end: c.position(),
        version,
        hash,
        flag_hash,
        guid,
        value,
    })
}

fn check_blob(blob: &Blob<'_>, tag: Tag, version: Version, at: usize) -> Result<()> {
    if blob.tag != tag || blob.version != version {
        return Err(fail(format!(
            "blob {at} is {} {}, expected {tag} {version}",
            blob.tag, blob.version
        )));
    }
    Ok(())
}

fn utf8<'a>(m: &Metadata<'a>, what: &str) -> Result<&'a str> {
    std::str::from_utf8(m.payload).map_err(|e| fail(format!("the {what} metadata is not UTF-8: {e}")))
}

fn fixed<const N: usize>(m: &Metadata<'_>, blob: Tag) -> Result<[u8; N]> {
    <[u8; N]>::try_from(m.payload).map_err(|_| {
        fail(format!(
            "{blob}'s {} metadata is {} bytes, not {N}",
            m.tag,
            m.payload.len()
        ))
    })
}

fn metadata<'b, 'a>(blob: &'b Blob<'a>, expected: &[(Tag, u8)]) -> Result<Vec<&'b Metadata<'a>>> {
    let found: Vec<(Tag, u8)> = blob.metadata.iter().map(|m| (m.tag, m.version)).collect();
    if found != expected {
        let show = |v: &[(Tag, u8)]| {
            v.iter()
                .map(|(t, ver)| format!("{t} v{ver}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        return Err(fail(format!(
            "{}'s metadata is [{}], expected [{}]",
            blob.tag,
            show(&found),
            show(expected)
        )));
    }
    Ok(blob.metadata.iter().collect())
}

fn end(c: &Cursor<'_>, what: &str) -> Result<()> {
    if c.remaining() != 0 {
        return Err(fail(format!(
            "{} bytes left at {:#x} after {what}",
            c.remaining(),
            c.position()
        )));
    }
    Ok(())
}

/// A `.materialbin`: a parent material.
#[derive(Clone, Debug, PartialEq)]
pub struct Material<'a> {
    /// The `Name` metadata: the material's name.
    pub name: &'a str,
    /// The `ATST` metadata, as stored.
    pub atst: [u8; 2],
    /// The `BLST` metadata, as stored.
    pub blst: [u8; 5],
    /// The `XTRA` metadata (version 4), as stored, when there is one.
    pub xtra: Option<&'a [u8]>,
    /// The shader the material belongs to: a `.shaderbin` game path.
    pub shader: &'a str,
    /// The shader technique (`Standard`, `AlphaTest`, ...; may be empty).
    pub technique: &'a str,
    /// The third string of the `MATL` data, as stored (empty, or `UBER_FOLIAGE`).
    pub third: &'a str,
    /// The `MTUD` pairs: a key and its value.
    pub user_data: Vec<(&'a str, &'a str)>,
    /// The `MTPR` parameters.
    pub parameters: Parameters<'a>,
}

const MATL_VERSION: Version = Version { major: 1, minor: 2 };
const MTUD_VERSION: Version = Version { major: 1, minor: 0 };
const MATI_VERSION: Version = Version { major: 1, minor: 0 };

impl<'a> Material<'a> {
    /// The material a `.materialbin`'s bytes hold.
    pub fn parse(bytes: &'a [u8]) -> Result<Material<'a>> {
        Material::from_bundle(&Bundle::parse(bytes)?)
    }

    /// The material of a bundle already read. Refused: anything the module docs list.
    pub fn from_bundle(bundle: &Bundle<'a>) -> Result<Material<'a>> {
        let [matl, mtud, mtpr] = bundle.blobs.as_slice() else {
            return Err(fail(format!(
                "{} blobs, expected MATL, MTUD and MTPR",
                bundle.blobs.len()
            )));
        };
        check_blob(matl, MATL, MATL_VERSION, 0)?;
        check_blob(mtud, MTUD, MTUD_VERSION, 1)?;
        check_blob(mtpr, MTPR, PARAMETERS_V2_0, 2)?;
        let with_xtra = [(NAME, 0), (ATST, 2), (BLST, 1), (XTRA, 4)];
        let meta = match metadata(matl, &with_xtra) {
            Ok(m) => m,
            Err(_) => metadata(matl, &with_xtra[..3])?,
        };
        let (name, atst, blst, xtra) = match meta.as_slice() {
            [n, a, b, rest @ ..] => (
                utf8(n, "Name")?,
                fixed::<2>(a, MATL)?,
                fixed::<5>(b, MATL)?,
                rest.first().map(|x| x.payload),
            ),
            _ => return Err(fail("MATL metadata")),
        };
        let mut c = Cursor::new(matl.data);
        let s = |c: &mut Cursor<'a>, what: &str| string(c, what).map_err(|e| fail(format!("MATL: {e}")));
        let shader = s(&mut c, "the shader")?;
        let technique = s(&mut c, "the technique")?;
        let third = s(&mut c, "the third string")?;
        end(&c, "MATL's three strings")?;
        let mut c = Cursor::new(mtud.data);
        let count = c.u8().map_err(|e| fail(format!("MTUD's count: {e}")))?;
        let mut user_data = Vec::with_capacity(usize::from(count));
        for k in 0..count {
            let key = string(&mut c, &format!("MTUD pair {k}'s key")).map_err(fail)?;
            let value = string(&mut c, &format!("MTUD pair {k}'s value")).map_err(fail)?;
            user_data.push((key, value));
        }
        end(&c, &format!("MTUD's {count} pairs"))?;
        let parameters = Parameters::parse(mtpr.data, mtpr.version)?;
        Ok(Material {
            name,
            atst,
            blst,
            xtra,
            shader,
            technique,
            third,
            user_data,
            parameters,
        })
    }
}

/// A material instance: the bundle in a model's `MatI` blob.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance<'a> {
    /// The `Name` metadata of the `MATI` blob.
    pub name: &'a str,
    /// The `ATST` metadata, as stored.
    pub atst: [u8; 2],
    /// The `XTRA` metadata (version 5), as stored.
    pub xtra: &'a [u8],
    /// The parent material: a `.materialbin` game path.
    pub parent: &'a str,
    /// The `MTPR` parameters.
    pub parameters: Parameters<'a>,
}

impl<'a> Instance<'a> {
    /// The instance a nested bundle's bytes hold.
    pub fn parse(bytes: &'a [u8]) -> Result<Instance<'a>> {
        Instance::from_bundle(&Bundle::parse(bytes)?)
    }

    /// The instance in a model's `MatI` blob: `None` when the blob is empty. Refused: a blob of another tag or
    /// version than `MatI` 1.0, and data that is neither empty nor an instance bundle.
    pub fn from_model_blob(blob: &Blob<'a>) -> Result<Option<Instance<'a>>> {
        check_blob(blob, MAT_I, MATI_VERSION, 0)?;
        if blob.data.is_empty() {
            return Ok(None);
        }
        Instance::parse(blob.data).map(Some)
    }

    /// The instance of a bundle already read. Refused: anything the module docs list.
    pub fn from_bundle(bundle: &Bundle<'a>) -> Result<Instance<'a>> {
        let [mati, mtpr] = bundle.blobs.as_slice() else {
            return Err(fail(format!(
                "{} blobs, expected MATI and MTPR",
                bundle.blobs.len()
            )));
        };
        check_blob(mati, MATI, MATI_VERSION, 0)?;
        check_blob(mtpr, MTPR, PARAMETERS_V2_0, 1)?;
        let meta = metadata(mati, &[(NAME, 0), (ATST, 2), (XTRA, 5)])?;
        let [n, a, x] = meta.as_slice() else {
            return Err(fail("MATI metadata"));
        };
        let mut c = Cursor::new(mati.data);
        let parent = string(&mut c, "the parent").map_err(|e| fail(format!("MATI: {e}")))?;
        end(&c, "MATI's parent path")?;
        Ok(Instance {
            name: utf8(n, "Name")?,
            atst: fixed::<2>(a, MATI)?,
            xtra: x.payload,
            parent,
            parameters: Parameters::parse(mtpr.data, mtpr.version)?,
        })
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
pub(crate) mod tests {
    use super::*;
    use crate::test_bundle::bundle;

    /// A string as the files store it.
    pub(crate) fn text(s: &str) -> Vec<u8> {
        let mut out = Vec::new();
        let mut n = s.len();
        loop {
            let low = (n & 0x7F) as u8;
            n >>= 7;
            if n == 0 {
                out.push(low);
                break;
            }
            out.push(low | 0x80);
        }
        out.extend_from_slice(s.as_bytes());
        out
    }

    /// A parameter of version 3.4: hash, an optional flag hash, type, a GUID of `hash` bytes and the value bytes.
    pub(crate) fn param(hash: u32, flag: Option<u32>, kind: u8, value: &[u8]) -> Vec<u8> {
        let mut p = vec![3, 4];
        p.extend_from_slice(&hash.to_le_bytes());
        match flag {
            Some(f) => {
                p.push(1);
                p.extend_from_slice(&f.to_le_bytes());
            }
            None => p.push(0),
        }
        p.push(kind);
        p.extend_from_slice(&[hash as u8; 16]);
        p.extend_from_slice(value);
        p
    }

    fn words(w: &[u32]) -> Vec<u8> {
        w.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    fn texture_value(path: &str) -> Vec<u8> {
        let mut v = text(path);
        v.extend_from_slice(&path_hash(path).to_le_bytes());
        v
    }

    /// An `MTPR` 2.0 blob's data: a u8 count, the parameters, 12 trailer bytes.
    pub(crate) fn mtpr(params: &[Vec<u8>]) -> Vec<u8> {
        let mut d = vec![params.len() as u8];
        for p in params {
            d.extend_from_slice(p);
        }
        d.extend_from_slice(&[0, 0, 0, 0, 1, 2, 3, 4, 0, 0, 0, 0]);
        d
    }

    fn sample_parameters() -> Vec<Vec<u8>> {
        vec![
            param(1, None, 0, &words(&[0x3F80_0000, 0, 0, 0])),
            param(
                2,
                Some(0xAABB_CCDD),
                1,
                &words(&[0x3F80_0000, 0x4000_0000, 0, 0x3F80_0000]),
            ),
            param(3, None, 2, &0.5f32.to_le_bytes()),
            param(4, None, 3, &words(&[1])),
            param(5, None, 4, &(-7i32).to_le_bytes()),
            param(6, None, 5, &words(&[0, 1, 2, 3])),
            param(7, None, 6, &texture_value("Game:\\Media\\Textures\\A.swatchbin")),
            param(8, None, 6, &texture_value("")),
            param(9, None, 7, &words(&[1, 1, 4, 0])),
            param(10, None, 8, &[words(&[2]), words(&[5, 6])].concat()),
            param(
                11,
                None,
                11,
                &[1.5f32.to_le_bytes(), 2.5f32.to_le_bytes()].concat(),
            ),
            param(12, None, 12, &words(&[9, 8, 7, 6])),
        ]
    }

    #[test]
    fn reads_strings_of_any_length() {
        for len in [0usize, 1, 127, 128, 300, 20_000] {
            let s = "é".repeat(len / 2) + &"x".repeat(len % 2);
            let bytes = text(&s);
            let mut c = Cursor::new(&bytes);
            assert_eq!(string(&mut c, "s").unwrap(), s);
            assert_eq!(c.remaining(), 0);
        }
        let mut c = Cursor::new(&[0x85, 0x01, b'a']);
        assert!(string(&mut c, "s").unwrap_err().contains("133 bytes"));
        let mut c = Cursor::new(&[0x80]);
        assert!(string(&mut c, "s").is_err());
        let mut c = Cursor::new(&[2, 0xFF, 0xFE]);
        assert!(string(&mut c, "s").unwrap_err().contains("UTF-8"));
        let mut c = Cursor::new(&[0xFF; 12]);
        assert!(string(&mut c, "s").unwrap_err().contains("usize"));
    }

    #[test]
    fn hashes_the_lowercased_path() {
        assert_eq!(
            path_hash("Game:\\Media\\A.swatchbin"),
            path_hash("game:\\media\\a.swatchbin")
        );
        assert_ne!(path_hash("a"), path_hash("b"));
        assert_eq!(path_hash(""), 0);
    }

    #[test]
    fn reads_every_parameter_type() {
        let data = mtpr(&sample_parameters());
        let p = Parameters::parse(&data, PARAMETERS_V2_0).unwrap();
        assert_eq!(p.list.len(), 12);
        assert_eq!(p.trailer, Some([0, 0, 0, 0, 1, 2, 3, 4, 0, 0, 0, 0]));
        assert_eq!(
            p.list.iter().map(|q| q.value.type_number()).collect::<Vec<_>>(),
            [0, 1, 2, 3, 4, 5, 6, 6, 7, 8, 11, 12]
        );
        assert_eq!(p.get(2).unwrap().flag_hash, Some(0xAABB_CCDD));
        assert_eq!(p.get(2).unwrap().value, Value::Colour([1.0, 2.0, 0.0, 1.0]));
        assert_eq!(p.get(3).unwrap().value, Value::Float(0.5));
        assert_eq!(p.get(5).unwrap().value, Value::Int(-7));
        assert_eq!(p.get(4).unwrap().guid, [4; 16]);
        assert_eq!(
            p.get(7).unwrap().value,
            Value::Texture {
                path: "Game:\\Media\\Textures\\A.swatchbin",
                path_hash: path_hash("Game:\\Media\\Textures\\A.swatchbin")
            }
        );
        assert_eq!(
            p.get(8).unwrap().value,
            Value::Texture {
                path: "",
                path_hash: 0
            }
        );
        assert_eq!(p.get(9).unwrap().value, Value::Sampler([1, 1, 4, 0]));
        assert_eq!(p.get(10).unwrap().value, Value::Gradient(&words(&[5, 6])));
        assert_eq!(p.get(11).unwrap().value, Value::Vector2([1.5, 2.5]));
        assert_eq!(p.get(12).unwrap().value, Value::Type12([9, 8, 7, 6]));
        // each parameter ends where the next begins, the last where the trailer begins
        for w in p.list.windows(2) {
            assert_eq!(w[0].end, w[1].offset);
        }
        assert_eq!(p.list[11].end, data.len() - 12);
        assert_eq!(p.list[0].offset, 1);
        assert!(p.get(99).is_none());
    }

    #[test]
    fn reads_a_version_2_1_list_without_a_trailer() {
        let mut data = 2u16.to_le_bytes().to_vec();
        data.extend_from_slice(&param(1, None, 3, &words(&[0])));
        data.extend_from_slice(&param(2, None, 3, &words(&[1])));
        let p = Parameters::parse(&data, PARAMETERS_V2_1).unwrap();
        assert_eq!((p.list.len(), p.trailer), (2, None));
    }

    #[test]
    fn refuses_what_it_does_not_read() {
        let refused = |data: &[u8], version: Version, expect: &str| match Parameters::parse(data, version) {
            Err(Error::Material(m)) => assert!(m.contains(expect), "{m:?} lacks {expect:?}"),
            other => panic!("expected a refusal naming {expect:?}, got {other:?}"),
        };
        let good = mtpr(&[param(1, None, 3, &words(&[1]))]);
        assert!(Parameters::parse(&good, PARAMETERS_V2_0).is_ok());
        let mut extra = good.clone();
        extra.push(0);
        refused(&extra, PARAMETERS_V2_0, "bytes left");
        refused(
            &good[..good.len() - 1],
            PARAMETERS_V2_0,
            "the 12 bytes after 1 parameters",
        );
        refused(&good, Version { major: 3, minor: 0 }, "not 2.0 or 2.1");
        let mut flag = good.clone();
        flag[7] = 2;
        refused(&flag, PARAMETERS_V2_0, "the flag is 2");
        let mut version = good.clone();
        version[2] = 2;
        refused(&version, PARAMETERS_V2_0, "version 3.2");
        refused(
            &mtpr(&[param(1, None, 9, &words(&[0, 0, 0, 0]))]),
            PARAMETERS_V2_0,
            "type 9",
        );
        refused(
            &[&[1u8, 0][..], &param(1, None, 6, &[5, b'a'])].concat(),
            PARAMETERS_V2_1,
            "texture path",
        );
        refused(
            &mtpr(&[param(1, None, 8, &words(&[u32::MAX]))]),
            PARAMETERS_V2_0,
            "type 8",
        );
        refused(&[3], PARAMETERS_V2_0, "parameter 0");
    }

    fn materialbin(xtra: bool) -> Vec<u8> {
        let mut meta: Vec<Meta<'_>> = vec![
            (b"Name", 0, b"rock"),
            (b"ATST", 2, &[1, 0]),
            (b"BLST", 1, &[0, 1, 2, 3, 4]),
        ];
        if xtra {
            meta.push((b"XTRA", 4, &[0, 0, 0, 0, 0]));
        }
        let matl = [
            text("Game:\\Media\\shaders\\rock.shaderbin"),
            text("Standard"),
            text(""),
        ]
        .concat();
        let mtud = [vec![1u8], text("FM5Path"), text("D:\\src\\rock.mat")].concat();
        bundle(&[
            (b"MATL", (1, 2), meta, &matl),
            (b"MTUD", (1, 0), vec![], &mtud),
            (
                b"MTPR",
                (2, 0),
                vec![],
                &mtpr(&[param(1, None, 2, &1f32.to_le_bytes())]),
            ),
        ])
    }

    use crate::test_bundle::Meta;

    #[test]
    fn reads_a_materialbin() {
        for xtra in [true, false] {
            let bytes = materialbin(xtra);
            let m = Material::parse(&bytes).unwrap();
            assert_eq!(m.name, "rock");
            assert_eq!((m.atst, m.blst), ([1, 0], [0, 1, 2, 3, 4]));
            assert_eq!(m.xtra.is_some(), xtra);
            assert_eq!(m.shader, "Game:\\Media\\shaders\\rock.shaderbin");
            assert_eq!((m.technique, m.third), ("Standard", ""));
            assert_eq!(m.user_data, [("FM5Path", "D:\\src\\rock.mat")]);
            assert_eq!(m.parameters.list.len(), 1);
        }
    }

    #[test]
    fn refuses_a_materialbin_that_is_not_what_it_lists() {
        let refused = |bytes: &[u8], expect: &str| match Material::parse(bytes) {
            Err(Error::Material(m)) => assert!(m.contains(expect), "{m:?} lacks {expect:?}"),
            other => panic!("expected a refusal naming {expect:?}, got {other:?}"),
        };
        let matl = [text("s"), text("t"), text("")].concat();
        let mtud = vec![0u8];
        let mtpr_data = mtpr(&[]);
        let name: Vec<Meta<'_>> = vec![(b"Name", 0, b"n"), (b"ATST", 2, &[0, 0]), (b"BLST", 1, &[0; 5])];
        let build = |matl_v: (u8, u8), meta: Vec<Meta<'_>>, matl: &[u8], mtud: &[u8]| {
            bundle(&[
                (b"MATL", matl_v, meta, matl),
                (b"MTUD", (1, 0), vec![], mtud),
                (b"MTPR", (2, 0), vec![], &mtpr_data),
            ])
        };
        assert!(Material::parse(&build((1, 2), name.clone(), &matl, &mtud)).is_ok());
        refused(&build((1, 3), name.clone(), &matl, &mtud), "expected MATL 1.2");
        refused(&build((1, 2), name[..2].to_vec(), &matl, &mtud), "metadata is");
        refused(
            &build((1, 2), name.clone(), &[matl.clone(), vec![0]].concat(), &mtud),
            "bytes left",
        );
        refused(&build((1, 2), name.clone(), &matl, &[1]), "MTUD pair 0");
        refused(&build((1, 2), name.clone(), &matl, &[0, 0]), "bytes left");
        let mut odd = name.clone();
        odd[1] = (b"ATST", 2, &[0, 0, 0]);
        refused(&build((1, 2), odd, &matl, &mtud), "bytes, not 2");
        refused(&bundle(&[(b"MATL", (1, 2), name.clone(), &matl)]), "1 blobs");
    }

    fn instance(parent: &str) -> Vec<u8> {
        bundle(&[
            (
                b"MATI",
                (1, 0),
                vec![
                    (b"Name", 0, b"wall"),
                    (b"ATST", 2, &[0, 1]),
                    (b"XTRA", 5, &[0; 6]),
                ],
                &text(parent),
            ),
            (b"MTPR", (2, 0), vec![], &mtpr(&sample_parameters())),
        ])
    }

    #[test]
    fn reads_an_instance_and_an_empty_model_blob() {
        let bytes = instance("Game:\\Media\\materials\\wall.materialbin");
        let i = Instance::parse(&bytes).unwrap();
        assert_eq!((i.name, i.atst, i.xtra), ("wall", [0, 1], &[0u8; 6][..]));
        assert_eq!(i.parent, "Game:\\Media\\materials\\wall.materialbin");
        assert_eq!(i.parameters.list.len(), 12);

        let model = bundle(&[(b"MatI", (1, 0), vec![], &bytes), (b"MatI", (1, 0), vec![], &[])]);
        let b = Bundle::parse(&model).unwrap();
        let [full, empty] = b.blobs.as_slice() else {
            panic!("blobs")
        };
        assert_eq!(Instance::from_model_blob(full).unwrap().unwrap(), i);
        assert!(Instance::from_model_blob(empty).unwrap().is_none());
        let wrong = bundle(&[(b"MatI", (1, 1), vec![], &[])]);
        let b = Bundle::parse(&wrong).unwrap();
        assert!(Instance::from_model_blob(&b.blobs[0]).is_err());
        let junk = bundle(&[(b"MatI", (1, 0), vec![], &[1, 2, 3, 4])]);
        let b = Bundle::parse(&junk).unwrap();
        assert!(Instance::from_model_blob(&b.blobs[0]).is_err());
    }

    #[test]
    fn refuses_an_instance_with_more_than_its_parent_path() {
        let mut parent = text("p");
        parent.push(0);
        let bytes = bundle(&[
            (
                b"MATI",
                (1, 0),
                vec![(b"Name", 0, b"n"), (b"ATST", 2, &[0, 0]), (b"XTRA", 5, &[0; 5])],
                &parent,
            ),
            (b"MTPR", (2, 0), vec![], &mtpr(&[])),
        ]);
        assert!(matches!(Instance::parse(&bytes), Err(Error::Material(m)) if m.contains("bytes left")));
    }
}
