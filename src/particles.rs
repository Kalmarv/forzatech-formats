//! Particle instances (`.particlebin`): the effects a track places, kept beside it at
//! `Ribbon_00\ParticleInstances.particlebin`, and one file per route under `routedefinitions`.
//!
//! A `.particlebin` is a Grub bundle ([`crate::bundle`]) with one blob, tag `PCLB`, version 7.0 (6.0 in the home space's
//! file, which holds no instance), no metadata. Its data
//! is a memory image: every offset in it is an offset from the start of the blob's data. Little-endian:
//!
//! | field | size | value |
//! |---|---|---|
//! | header | 16 | u32 8, u32 instance count, u32 16, u32 16 (0xFFFFFFFF when there is no instance) |
//! | instances | count × 129 | [`Instance`] |
//! | strings | to the end | zero-terminated printable ASCII, each referenced, in the order of first use |
//!
//! An instance, with offsets from its start:
//!
//! | offset | size | field |
//! |---|---|---|
//! | 0 | 4 | u32 offset of the instance's name ([`Instance::name`]) |
//! | 4 | 4 | link to the instance's offset 8 |
//! | 8 | 4 | u32 offset of the effect's name ([`Instance::effect`]) |
//! | 12 | 4 | link to the instance's offset 112 |
//! | 16 | 16 | u32 1, u32 0 × 3 |
//! | 32 | 4 | f32 1 |
//! | 36 | 12 | f32 × 3 position ([`Instance::position`]) |
//! | 48 | 16 | f32 × 4 rotation, a unit quaternion x, y, z, w ([`Instance::rotation`]) |
//! | 64 | 12 | f32 × 3 ([`Instance::extent`]) |
//! | 76 | 4 | 0xFFFFFFFF |
//! | 80 | 4 | f32 ([`Instance::distance`]) |
//! | 84 | 4 | u32 0 |
//! | 88 | 4 | 0xFFFFFFFF |
//! | 92 | 4 | link to the instance's offset 96 |
//! | 96 | 4 | 0xFFFFFFFF |
//! | 100 | 4 | link to the next instance (0xFFFFFFFF for the last) |
//! | 104 | 4 | u32 offset of a string ([`Instance::tag`]) |
//! | 108 | 4 | link to the instance's offset 121 |
//! | 112 | 4 | u32 offset of a string ([`Instance::route`]) |
//! | 116 | 4 | link to the instance's offset 104 |
//! | 120 | 1 | u8 0 or 1 ([`Instance::flag`]) |
//! | 121 | 4 | u32 offset of a string ([`Instance::group`]) |
//! | 125 | 4 | link to the instance's offset 88 |
//!
//! The strings at offsets 0, 8, 112, 104 and 121 and the two links that follow the empty slots at 88 and 96 make one
//! singly linked chain through an instance's slots (0, 8, 112, 104, 121, 88, 96, then the next instance's 0); a link
//! holds the offset of the slot it leads to, and the reader checks every one. A string offset must be the start of a
//! string of the strings section and every string must be named by some instance. What the fields hold beyond what
//! their names say is not known: the values seen are listed on each field.
//!
//! Refused, with the offset: another bundle layout, a blob that is not the one above, a header, constant, link or string
//! offset that differs from the above, a rotation that is not a unit quaternion, a value that is not finite, a string
//! that is not printable ASCII or not referenced, and any byte left over.
//!
//! What has been checked, and how: no third-party reader of these files exists, so the check is the census in
//! `tests/particles.rs`: the 170 `.particlebin` files of one install (Steam build 25588841: the track's
//! `ParticleInstances.particlebin`, the home space's and 168 route definitions) read with no refusal, 4,467 instances
//! in all: 3,959 in the track's, 508 in 77 of the route definitions (the other 91 hold none), none in the home space's,
//! the only file of version 6.0.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::bundle::{Bundle, Tag, Version};
use crate::read::Reader;
use crate::{Error, Result};
use std::collections::BTreeSet;

/// The tag of the blob a `.particlebin` holds.
pub const TAG: Tag = Tag::new(b"PCLB");
/// The blob's version.
pub const VERSION: Version = Version { major: 7, minor: 0 };
/// The version of the one file seen that is not [`VERSION`]: the home space's, which holds no instance. Its header is
/// the same; the layout of its instances, if it had any, is not known, so a file of this version that has any is
/// refused.
pub const EMPTY_VERSION: Version = Version { major: 6, minor: 0 };
/// An instance's length in bytes.
pub const INSTANCE_LEN: usize = 129;
/// The header's length in bytes, and the offset of the first instance.
pub const HEADER_LEN: usize = 16;

const NULL: u32 = 0xFFFF_FFFF;

fn fail(what: String) -> Error {
    Error::Particles(what)
}

/// One placed particle effect.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    /// The instance's name (`falling_leaves_customizable_Instance12`, 986 distinct in the track's file).
    pub name: String,
    /// The effect's name (`falling_leaves_customizable`, 50 distinct in the track's file).
    pub effect: String,
    /// Where it is placed, in the world frame ([`crate::world`]).
    pub position: [f32; 3],
    /// Its rotation as a unit quaternion x, y, z, w.
    pub rotation: [f32; 4],
    /// Three floats at offset 64, meaning not known (whole numbers of metres up to an `f32` rounding in the track's
    /// file: 2, 8, 2 for 681 instances; 5, 9, 4 for 247).
    pub extent: [f32; 3],
    /// The float at offset 80, meaning not known (0 or 50 to 6,000 in the track's file; 65 for 1,914 instances).
    pub distance: f32,
    /// The string at offset 104 (empty for 3,956 instances of the track's file; `[TIME_ATTACK_DRIFT_CIRCUIT_HORIZON]`
    /// for one; `0` for two).
    pub tag: String,
    /// The string at offset 112, in the form of a `.pgeo` header's route string (`~1281`, `~351;~8008`, `0`, or empty).
    pub route: String,
    /// The string at offset 121 (`leaves_ginkgo_falling`, `seagulls_audio`, `koi_carps`, ... or empty; 26 distinct in the
    /// track's file).
    pub group: String,
    /// The byte at offset 120: 1 for 248 instances of the track's file, 0 for the others.
    pub flag: bool,
}

/// A `.particlebin`: its instances, in file order.
#[derive(Clone, Debug, PartialEq)]
pub struct Particles {
    /// The instances.
    pub instances: Vec<Instance>,
}

impl Particles {
    /// Reads a `.particlebin`: the whole file, a Grub bundle.
    pub fn parse(bytes: &[u8]) -> Result<Particles> {
        let bundle = Bundle::parse(bytes)?;
        let [blob] = bundle.blobs.as_slice() else {
            return Err(fail(format!("expected one blob, found {}", bundle.blobs.len())));
        };
        if blob.tag != TAG || !blob.metadata.is_empty() || ![VERSION, EMPTY_VERSION].contains(&blob.version) {
            return Err(fail(format!(
                "expected one {TAG} blob of version {}.{} (or {}.{} with no instance) and no metadata, found {} {}.{} with {} records",
                VERSION.major,
                VERSION.minor,
                EMPTY_VERSION.major,
                EMPTY_VERSION.minor,
                blob.tag,
                blob.version.major,
                blob.version.minor,
                blob.metadata.len()
            )));
        }
        let particles = parse_data(blob.data)?;
        if blob.version == EMPTY_VERSION && !particles.instances.is_empty() {
            return Err(fail(format!(
                "version {}.{} holds {} instances: the layout of its instances is not known",
                EMPTY_VERSION.major,
                EMPTY_VERSION.minor,
                particles.instances.len()
            )));
        }
        Ok(particles)
    }
}

fn parse_data(data: &[u8]) -> Result<Particles> {
    let mut r = Reader::new(data, fail);
    r.expect_u32(8, "header word 0")?;
    let at = r.position();
    let count =
        usize::try_from(r.u32("instance count")?).map_err(|_| r.fail_at(at, "instance count: too large"))?;
    r.expect_u32(16, "header word 2")?;
    r.expect_u32(if count == 0 { NULL } else { 16 }, "header word 3")?;
    let bytes = count
        .checked_mul(INSTANCE_LEN)
        .and_then(|b| b.checked_add(HEADER_LEN))
        .filter(|&end| end <= data.len())
        .ok_or_else(|| {
            r.fail_at(
                at,
                format_args!("instance count {count}: the file cannot hold them"),
            )
        })?;
    let strings = Strings::new(data, bytes)?;
    let mut used = BTreeSet::new();
    let mut instances = Vec::with_capacity(count);
    for _ in 0..count {
        instances.push(instance(&mut r, &strings, &mut used, bytes)?);
    }
    if let Some(unused) = strings.starts.difference(&used).next() {
        return Err(r.fail_at(*unused, "string: named by no instance"));
    }
    // the strings section was checked whole above
    r.take(r.remaining(), "strings")?;
    r.end("particle instances")?;
    Ok(Particles { instances })
}

/// The strings section: where it starts and the offset of each string's first byte.
struct Strings<'a> {
    data: &'a [u8],
    base: usize,
    starts: BTreeSet<usize>,
}

impl<'a> Strings<'a> {
    fn new(data: &'a [u8], base: usize) -> Result<Strings<'a>> {
        let section = data.get(base..).unwrap_or_default();
        if let Some(bad) = section
            .iter()
            .position(|&b| b != 0 && !(0x20..=0x7E).contains(&b))
        {
            return Err(fail(format!(
                "string at {:#x}: byte {bad} is not printable ASCII",
                base
            )));
        }
        if section.last().is_some_and(|&b| b != 0) {
            return Err(fail(format!(
                "strings at {base:#x}: the last string has no zero byte"
            )));
        }
        let mut starts = BTreeSet::new();
        let mut from = 0usize;
        while from < section.len() {
            starts.insert(base.saturating_add(from));
            let rest = section.get(from..).unwrap_or_default();
            let len = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
            from = from.saturating_add(len).saturating_add(1);
        }
        Ok(Strings { data, base, starts })
    }

    /// The string a stored offset names.
    fn get(&self, r: &Reader<'_>, at: usize, what: &str, offset: u32) -> Result<(String, usize)> {
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        if !self.starts.contains(&start) {
            return Err(r.fail_at(
                at,
                format_args!(
                    "{what}: {offset:#x} is not the start of a string at or after {:#x}",
                    self.base
                ),
            ));
        }
        let rest = self.data.get(start..).unwrap_or_default();
        let len = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        let text = rest.get(..len).unwrap_or_default();
        Ok((text.iter().map(|&b| char::from(b)).collect(), start))
    }
}

fn instance(
    r: &mut Reader<'_>,
    strings: &Strings<'_>,
    used: &mut BTreeSet<usize>,
    end: usize,
) -> Result<Instance> {
    let start = r.position();
    let at = |offset: usize| start.saturating_add(offset);
    let to = |offset: usize| u32::try_from(at(offset)).unwrap_or(NULL);
    let mut string = |r: &mut Reader<'_>, what: &str| -> Result<String> {
        let here = r.position();
        let offset = r.u32(what)?;
        let (text, first) = strings.get(r, here, what, offset)?;
        used.insert(first);
        Ok(text)
    };
    let name = string(r, "instance name")?;
    link(r, to(8), "link after the name")?;
    let effect = string(r, "effect name")?;
    link(r, to(112), "link after the effect")?;
    r.expect_u32(1, "instance constant")?;
    for _ in 0..3 {
        r.expect_u32(0, "instance zero")?;
    }
    r.expect_u32(1.0f32.to_bits(), "instance one")?;
    let position_at = r.position();
    let position = r.f32s::<3>("position")?;
    finite(r, position_at, &position, "position")?;
    let rotation_at = r.position();
    let rotation = r.f32s::<4>("rotation")?;
    finite(r, rotation_at, &rotation, "rotation")?;
    let norm: f32 = rotation.iter().map(|v| v * v).sum::<f32>().sqrt();
    if (norm - 1.0).abs() > 1e-5 {
        return Err(r.fail_at(
            rotation_at,
            format_args!("rotation: expected a unit quaternion, found length {norm}"),
        ));
    }
    let extent_at = r.position();
    let extent = r.f32s::<3>("extent")?;
    finite(r, extent_at, &extent, "extent")?;
    r.expect_u32(NULL, "instance null at 76")?;
    let distance_at = r.position();
    let distance = r.f32("distance")?;
    finite(r, distance_at, &[distance], "distance")?;
    r.expect_u32(0, "instance zero at 84")?;
    r.expect_u32(NULL, "instance null at 88")?;
    link(r, to(96), "link after the slot at 88")?;
    r.expect_u32(NULL, "instance null at 96")?;
    let next = if at(INSTANCE_LEN) < end {
        u32::try_from(at(INSTANCE_LEN)).unwrap_or(NULL)
    } else {
        NULL
    };
    link(r, next, "link to the next instance")?;
    let tag = string(r, "tag")?;
    link(r, to(121), "link after the tag")?;
    let route = string(r, "route")?;
    link(r, to(104), "link after the route")?;
    let flag_at = r.position();
    let flag = match r.u8("flag")? {
        0 => false,
        1 => true,
        b => return Err(r.fail_at(flag_at, format_args!("flag: expected 0 or 1, found {b}"))),
    };
    let group = string(r, "group")?;
    link(r, to(88), "link after the group")?;
    Ok(Instance {
        name,
        effect,
        position,
        rotation,
        extent,
        distance,
        tag,
        route,
        group,
        flag,
    })
}

fn link(r: &mut Reader<'_>, expected: u32, what: &str) -> Result<()> {
    r.expect_u32(expected, what)
}

fn finite(r: &Reader<'_>, at: usize, values: &[f32], what: &str) -> Result<()> {
    if values.iter().any(|v| !v.is_finite()) {
        return Err(r.fail_at(at, format_args!("{what}: not finite")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects,
        reason = "tests build their own bytes"
    )]
    use super::*;

    fn put(out: &mut Vec<u8>, w: u32) {
        out.extend_from_slice(&w.to_le_bytes());
    }

    /// The data of one instance with the given strings, built from the layout.
    fn data(name: &str, count: usize) -> Vec<u8> {
        let strings_at = HEADER_LEN + count * INSTANCE_LEN;
        let mut strings = Vec::new();
        let at = |s: &str, strings: &mut Vec<u8>| -> u32 {
            let offset = u32::try_from(strings_at + strings.len()).unwrap();
            strings.extend_from_slice(s.as_bytes());
            strings.push(0);
            offset
        };
        let (n, e, empty) = (
            at(name, &mut strings),
            at("effect", &mut strings),
            at("", &mut strings),
        );
        let mut out = Vec::new();
        for w in [
            8,
            u32::try_from(count).unwrap(),
            16,
            if count == 0 { NULL } else { 16 },
        ] {
            out.extend_from_slice(&w.to_le_bytes());
        }
        for k in 0..count {
            let r = u32::try_from(HEADER_LEN + k * INSTANCE_LEN).unwrap();
            let next = if k + 1 < count { r + 129 } else { NULL };
            put(&mut out, n);
            put(&mut out, r + 8);
            put(&mut out, e);
            put(&mut out, r + 112);
            put(&mut out, 1);
            put(&mut out, 0);
            put(&mut out, 0);
            put(&mut out, 0);
            put(&mut out, 1.0f32.to_bits());
            for v in [10.0f32, 20.0, 30.0, 0.0, 0.0, 0.0, 1.0, 2.0, 8.0, 2.0] {
                put(&mut out, v.to_bits());
            }
            put(&mut out, NULL);
            put(&mut out, 65.0f32.to_bits());
            put(&mut out, 0);
            put(&mut out, NULL);
            put(&mut out, r + 96);
            put(&mut out, NULL);
            put(&mut out, next);
            put(&mut out, empty);
            put(&mut out, r + 121);
            put(&mut out, empty);
            put(&mut out, r + 104);
            out.push(1);
            put(&mut out, empty);
            put(&mut out, r + 88);
        }
        out.extend_from_slice(&strings);
        out
    }

    #[test]
    fn one_instance_reads() {
        let p = parse_data(&data("a_Instance1", 1)).unwrap();
        let [i] = p.instances.as_slice() else { panic!() };
        assert_eq!(
            (i.name.as_str(), i.effect.as_str(), i.tag.as_str()),
            ("a_Instance1", "effect", "")
        );
        assert_eq!(
            (i.position, i.rotation, i.extent),
            ([10.0, 20.0, 30.0], [0.0, 0.0, 0.0, 1.0], [2.0, 8.0, 2.0])
        );
        assert_eq!((i.distance, i.flag), (65.0, true));
    }

    #[test]
    fn two_instances_chain_and_none_is_empty() {
        assert_eq!(parse_data(&data("n", 2)).unwrap().instances.len(), 2);
        let mut empty = Vec::new();
        for w in [8, 0, 16, NULL] {
            empty.extend_from_slice(&u32::to_le_bytes(w));
        }
        assert!(parse_data(&empty).unwrap().instances.is_empty());
        empty.push(0);
        assert!(parse_data(&empty).is_err(), "a string with no instance names it");
    }

    #[test]
    fn a_wrong_link_is_refused() {
        let mut d = data("n", 1);
        d[HEADER_LEN + 4] ^= 1;
        let e = parse_data(&d).unwrap_err().to_string();
        assert!(e.contains("link after the name"), "{e}");
    }

    #[test]
    fn a_rotation_that_is_not_a_unit_is_refused() {
        let mut d = data("n", 1);
        d[HEADER_LEN + 60..HEADER_LEN + 64].copy_from_slice(&2.0f32.to_le_bytes());
        let e = parse_data(&d).unwrap_err().to_string();
        assert!(e.contains("unit quaternion"), "{e}");
    }

    #[test]
    fn a_string_offset_inside_a_string_is_refused() {
        let mut d = data("name", 1);
        let off = u32::from_le_bytes(d[HEADER_LEN..HEADER_LEN + 4].try_into().unwrap());
        d[HEADER_LEN..HEADER_LEN + 4].copy_from_slice(&(off + 1).to_le_bytes());
        let e = parse_data(&d).unwrap_err().to_string();
        assert!(e.contains("not the start of a string"), "{e}");
    }

    #[test]
    fn a_cut_file_is_refused_at_every_length() {
        let d = data("n", 2);
        for len in 0..d.len() {
            assert!(parse_data(&d[..len]).is_err(), "length {len}");
        }
    }
}
