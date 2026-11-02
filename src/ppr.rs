//! The property files (`.ppr`) of `media\TimeOfDay`: the sky presets of a track (`TimeOfDay_Brio.ppr` for Tokyo,
//! `TimeOfDay.ppr` for the older tracks) and their selection rules. A file is a table of names and a flat list of
//! groups, each group a list of typed leaves.
//!
//! Layout (little-endian):
//!
//! | part | layout |
//! |---|---|
//! | header | u32 version (2 or 3), u32 name count |
//! | names | per name a u8 length and that many bytes of printable ASCII |
//! | root | u16 name index (the file's own name, index 0 in both files), u32 group count |
//! | groups | per group: u16 kind name, f32, f32, u16 label name, u32 leaf count, then that many leaves |
//! | leaf | u8 type, u16 name, u16 id, then the payload of the type |
//!
//! A leaf's name and id are indexes into the names table: the id is the table entry after the name when the leaf's
//! property first occurs (50,966 of the 54,109 leaves of `TimeOfDay_Brio.ppr`) and another entry otherwise. Payloads:
//!
//! | type | payload |
//! |---|---|
//! | 0 | i32 ([`Value::Flag`]: 0 or 1 in every file read) |
//! | 1 | i32 ([`Value::Int`]) |
//! | 2 | f32 ([`Value::Float`]) |
//! | 3 | u16 name index, a string ([`Value::Text`]) |
//! | 4 | f32 × 2 ([`Value::Vec2`]) |
//! | 5 | a curve of scalars ([`Value::Curve`]) |
//! | 6 | a curve of colours ([`Value::Colour`]) |
//! | 7 | f32 × 4 ([`Value::Vec4`]) |
//!
//! A curve is a u32 component count and per component a u8 flag, a u32 key count and the keys, as f32. A key's first
//! value is its time. The rest of a key depends on the type and flag: a type 5 key with flag 0 is the time and one
//! value; a type 6 key with flag 0 is the time and four values (red, green, blue and a fourth, 0 in every key read);
//! a type 5 key with flag 1 is the time and four values in version 2 files or five in version 3 files, whose order is
//! not established ([`Component::keys`] returns them as stored). A type 6 curve with flag 1 occurs in no file read and
//! is refused.
//!
//! A group's two floats and the kinds and labels of groups are the file's own; the crate keeps them as stored
//! ([`Group::range`], [`Group::kind`], [`Group::label`]). `TimeOfDay_Brio.ppr` has 2,350 groups of 22 kinds; the 203
//! of kind `Sky` each have the same 199 leaves in the same order.
//!
//! [`Ppr::parse`] refuses, with the offset: another version, a name index past the table, a name that is not printable
//! ASCII, a count that the rest of the file cannot hold, a leaf type other than 0 to 7, a curve flag other than 0, or
//! 1 on a type 5 curve, and any byte left after the last group. Nothing is skipped.
//!
//! What has been checked, and how: on one install of Steam build 25588841, `TimeOfDay_Brio.ppr` (1,714,176 bytes:
//! 2,954 names, 2,350 groups, 54,109 leaves), `TimeOfDay.ppr` (1,086,259 bytes: 1,962 names, 1,337 groups, 33,500
//! leaves) and `Homespace.ppr` (24 bytes: one name, no group) each read to their last byte, so every group header,
//! leaf and key stride fell where the next one began. No third-party reader of these files exists; `tests/ppr.rs`
//! repeats the census on your install.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::read::Reader;
use crate::{Error, Result};

fn fail(detail: String) -> Error {
    Error::Ppr(detail)
}

/// An index into a file's names table: a property name, a group's kind or label, a string value or an id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Name(u16);

impl Name {
    /// The position in the names table.
    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

/// A property file: its names and groups, in file order.
#[derive(Clone, Debug, PartialEq)]
pub struct Ppr {
    /// The file's version: 2 or 3.
    pub version: u32,
    names: Vec<String>,
    /// The name the file gives itself (`TimeOfDay_Brio`, `TimeOfDay`, `Homespace`).
    pub root: Name,
    /// The groups, in file order.
    pub groups: Vec<Group>,
}

/// A group: a kind, a label, two floats and its leaves.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    /// What the group is (`Sky`, `SkyPreset`, `SkyGlobals`, `FogGlobalMaterial`, ...).
    pub kind: Name,
    /// The group's two floats as stored. `TimeOfDay_Brio.ppr`'s `SkyGlobals` holds 0 and 1,440; a `Sky` or
    /// `SkyPreset` holds two numbers that lie between them (`Forte_Summer_01`'s `Sky` 181.86 and 1,181.46). What they
    /// measure is not established.
    pub range: [f32; 2],
    /// The group's own name (the sky's name for a `Sky`, the preset's for a `SkyPreset`), not unique in a file.
    pub label: Name,
    /// The leaves, in file order.
    pub leaves: Vec<Leaf>,
}

/// One property of a group.
#[derive(Clone, Debug, PartialEq)]
pub struct Leaf {
    /// The property's name (`EmissiveScales.TrackEmissiveEV`).
    pub name: Name,
    /// The leaf's id: a table entry that holds a GUID when it follows the name entry.
    pub id: Name,
    /// The value.
    pub value: Value,
}

/// A leaf's value, as stored.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Type 0: an i32 that is 0 or 1 in every file read.
    Flag(i32),
    /// Type 1: an i32.
    Int(i32),
    /// Type 2: an f32.
    Float(f32),
    /// Type 3: a string, as a table entry.
    Text(Name),
    /// Type 4: two f32.
    Vec2([f32; 2]),
    /// Type 5: a curve of scalars.
    Curve(Curve),
    /// Type 6: a curve of colours.
    Colour(Curve),
    /// Type 7: four f32.
    Vec4([f32; 4]),
}

/// A curve: one or more components.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    /// The components, in file order: one for most curves, 2, 4 or 5 for the rest.
    pub components: Vec<Component>,
}

/// One component of a curve: its flag and its keys.
#[derive(Clone, Debug, PartialEq)]
pub struct Component {
    /// The flag byte: 0, or 1 (a type 5 curve whose keys carry more values).
    pub flag: u8,
    stride: usize,
    floats: Vec<f32>,
}

impl Component {
    /// How many keys the component has.
    pub fn len(&self) -> usize {
        self.floats.len().checked_div(self.stride).unwrap_or(0)
    }

    /// Whether the component has no key.
    pub fn is_empty(&self) -> bool {
        self.floats.is_empty()
    }

    /// The keys as stored, each its time followed by its values.
    pub fn keys(&self) -> impl Iterator<Item = &[f32]> {
        self.floats.chunks_exact(self.stride.max(1))
    }

    /// The keys of a scalar component (a type 5 curve with flag 0) as (time, value), and `None` for any other.
    pub fn scalar_keys(&self) -> Option<impl Iterator<Item = (f32, f32)> + '_> {
        (self.stride == 2).then(|| self.keys().filter_map(|k| Some((*k.first()?, *k.get(1)?))))
    }
}

impl Curve {
    /// The keys of a curve of one scalar component with flag 0, as (time, value): the shape of most curves.
    pub fn scalar_keys(&self) -> Option<impl Iterator<Item = (f32, f32)> + '_> {
        match self.components.as_slice() {
            [one] => one.scalar_keys(),
            _ => None,
        }
    }
}

impl Ppr {
    /// Reads a `.ppr`: the bytes of the whole file.
    pub fn parse(bytes: &[u8]) -> Result<Ppr> {
        let mut r = Reader::new(bytes, fail);
        let at = r.position();
        let version = r.u32("version")?;
        if !(2..=3).contains(&version) {
            return Err(r.fail_at(at, format_args!("version: expected 2 or 3, found {version}")));
        }
        let at = r.position();
        let count = r.count(1, "name count")?;
        if count > usize::from(u16::MAX) + 1 {
            return Err(r.fail_at(
                at,
                format_args!("{count} names: a u16 index reaches {}", 1u32 << 16),
            ));
        }
        let mut names = Vec::with_capacity(count);
        for _ in 0..count {
            let at = r.position();
            let len = usize::from(r.u8("name length")?);
            let text = r.take(len, "name")?;
            if text.iter().any(|b| !(0x20..=0x7E).contains(b)) {
                return Err(r.fail_at(at, "name: not printable ASCII"));
            }
            // printable ASCII is UTF-8
            names.push(text.iter().map(|&b| char::from(b)).collect());
        }
        let mut p = Parser {
            r,
            names: &names,
            version,
        };
        let root = p.name("root name")?;
        // every group is 16 bytes at least
        let groups = p.r.count(16, "group count")?;
        let mut out = Vec::with_capacity(groups);
        for _ in 0..groups {
            out.push(p.group()?);
        }
        p.r.end("groups")?;
        Ok(Ppr {
            version,
            names,
            root,
            groups: out,
        })
    }

    /// The text of a name. A name read from this file always resolves.
    pub fn name(&self, n: Name) -> &str {
        self.names.get(n.index()).map_or("", String::as_str)
    }

    /// How many names the table has.
    pub fn name_count(&self) -> usize {
        self.names.len()
    }

    /// The groups of a kind, in file order.
    pub fn groups_of<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a Group> {
        self.groups.iter().filter(move |g| self.name(g.kind) == kind)
    }

    /// The one group of a kind with a label: `None` when there is none, and a refusal when two share it (labels are
    /// not unique: 13 of the 203 `Sky` labels of `TimeOfDay_Brio.ppr` occur twice).
    pub fn group(&self, kind: &str, label: &str) -> Result<Option<&Group>> {
        let mut found = self
            .groups
            .iter()
            .filter(|g| self.name(g.kind) == kind && self.name(g.label) == label);
        let first = found.next();
        if found.next().is_some() {
            return Err(fail(format!(
                "more than one group of kind {kind} is labelled {label}"
            )));
        }
        Ok(first)
    }

    /// A leaf of a group by its property name: `None` when the group has none, and a refusal when it has two.
    pub fn leaf<'a>(&self, group: &'a Group, name: &str) -> Result<Option<&'a Leaf>> {
        let mut found = group.leaves.iter().filter(|l| self.name(l.name) == name);
        let first = found.next();
        if found.next().is_some() {
            return Err(fail(format!("a group has more than one leaf named {name}")));
        }
        Ok(first)
    }
}

struct Parser<'a> {
    r: Reader<'a>,
    names: &'a [String],
    version: u32,
}

impl Parser<'_> {
    fn name(&mut self, what: &str) -> Result<Name> {
        let at = self.r.position();
        let index = self.r.u16(what)?;
        if usize::from(index) >= self.names.len() {
            return Err(self.r.fail_at(
                at,
                format_args!("{what}: index {index} of a table of {}", self.names.len()),
            ));
        }
        Ok(Name(index))
    }

    fn group(&mut self) -> Result<Group> {
        let kind = self.name("group kind")?;
        let range = self.r.f32s::<2>("group range")?;
        let label = self.name("group label")?;
        // the smallest leaf, a string, is 7 bytes
        let count = self.r.count(7, "leaf count")?;
        let mut leaves = Vec::with_capacity(count);
        for _ in 0..count {
            leaves.push(self.leaf()?);
        }
        Ok(Group {
            kind,
            range,
            label,
            leaves,
        })
    }

    fn leaf(&mut self) -> Result<Leaf> {
        let at = self.r.position();
        let kind = self.r.u8("leaf type")?;
        let name = self.name("leaf name")?;
        let id = self.name("leaf id")?;
        let value = match kind {
            0 => Value::Flag(self.r.i32("flag")?),
            1 => Value::Int(self.r.i32("integer")?),
            2 => Value::Float(self.r.f32("float")?),
            3 => Value::Text(self.name("string")?),
            4 => Value::Vec2(self.r.f32s("vector")?),
            5 => Value::Curve(self.curve(5)?),
            6 => Value::Colour(self.curve(6)?),
            7 => Value::Vec4(self.r.f32s("vector")?),
            other => return Err(self.r.fail_at(at, format_args!("leaf type {other}"))),
        };
        Ok(Leaf { name, id, value })
    }

    /// The floats of a key of a type's curve with a flag, or `None` where the layout does not have one.
    fn stride(&self, kind: u8, flag: u8) -> Option<usize> {
        match (kind, flag, self.version) {
            (5, 0, _) => Some(2),
            (5, 1, 2) => Some(5),
            (5, 1, _) => Some(6),
            (6, 0, _) => Some(5),
            _ => None,
        }
    }

    fn curve(&mut self, kind: u8) -> Result<Curve> {
        // a component is a flag and a key count at least
        let count = self.r.count(5, "component count")?;
        let mut components = Vec::with_capacity(count);
        for _ in 0..count {
            let at = self.r.position();
            let flag = self.r.u8("curve flag")?;
            let Some(stride) = self.stride(kind, flag) else {
                return Err(self
                    .r
                    .fail_at(at, format_args!("curve flag {flag} on a type {kind} curve")));
            };
            let keys = self.r.count(stride.saturating_mul(4), "key count")?;
            let mut floats = Vec::with_capacity(keys.saturating_mul(stride));
            for _ in 0..keys.saturating_mul(stride) {
                floats.push(self.r.f32("key")?);
            }
            components.push(Component { flag, stride, floats });
        }
        Ok(Curve { components })
    }
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "tests on files built by hand"
)]
mod tests {
    use super::*;

    /// A file builder: the header, the names, the root and the groups put in as bytes.
    fn file(version: u32, names: &[&str], root: u16, groups: &[Vec<u8>]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&version.to_le_bytes());
        b.extend_from_slice(&u32::try_from(names.len()).unwrap().to_le_bytes());
        for n in names {
            b.push(u8::try_from(n.len()).unwrap());
            b.extend_from_slice(n.as_bytes());
        }
        b.extend_from_slice(&root.to_le_bytes());
        b.extend_from_slice(&u32::try_from(groups.len()).unwrap().to_le_bytes());
        for g in groups {
            b.extend_from_slice(g);
        }
        b
    }

    fn group(kind: u16, range: [f32; 2], label: u16, leaves: &[Vec<u8>]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&kind.to_le_bytes());
        for f in range {
            b.extend_from_slice(&f.to_le_bytes());
        }
        b.extend_from_slice(&label.to_le_bytes());
        b.extend_from_slice(&u32::try_from(leaves.len()).unwrap().to_le_bytes());
        for l in leaves {
            b.extend_from_slice(l);
        }
        b
    }

    fn leaf(kind: u8, name: u16, id: u16, payload: &[u8]) -> Vec<u8> {
        let mut b = vec![kind];
        b.extend_from_slice(&name.to_le_bytes());
        b.extend_from_slice(&id.to_le_bytes());
        b.extend_from_slice(payload);
        b
    }

    fn floats(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|f| f.to_le_bytes()).collect()
    }

    /// A curve payload: components of (flag, floats per key, key floats).
    fn curve(components: &[(u8, &[f32])]) -> Vec<u8> {
        let mut b = u32::try_from(components.len()).unwrap().to_le_bytes().to_vec();
        for (flag, keys) in components {
            let stride = match flag {
                0 => 2,
                _ => 5,
            };
            b.push(*flag);
            b.extend_from_slice(&u32::try_from(keys.len() / stride).unwrap().to_le_bytes());
            b.extend_from_slice(&floats(keys));
        }
        b
    }

    const NAMES: [&str; 6] = [
        "Root",
        "Sky",
        "Summer",
        "EmissiveScales.TrackEmissiveEV",
        "guid",
        "Text",
    ];

    fn sample() -> Vec<u8> {
        let scalar = curve(&[(0, &[0.0, 1.0, 0.5, 50.0, 1.0, 1.0])]);
        let leaves = vec![
            leaf(5, 3, 4, &scalar),
            leaf(0, 5, 4, &1i32.to_le_bytes()),
            leaf(3, 1, 4, &5u16.to_le_bytes()),
        ];
        file(3, &NAMES, 0, &[group(1, [180.0, 1180.0], 2, &leaves)])
    }

    #[test]
    fn a_small_file_reads_whole() {
        let b = sample();
        let p = Ppr::parse(&b).unwrap();
        assert_eq!((p.version, p.name_count(), p.groups.len()), (3, 6, 1));
        assert_eq!(p.name(p.root), "Root");
        let g = p.group("Sky", "Summer").unwrap().unwrap();
        assert_eq!(g.range, [180.0, 1180.0]);
        assert!(p.group("Sky", "Winter").unwrap().is_none());
        assert!(p.group("Fog", "Summer").unwrap().is_none());
        let leaf = p.leaf(g, "EmissiveScales.TrackEmissiveEV").unwrap().unwrap();
        let Value::Curve(c) = &leaf.value else {
            panic!("not a curve");
        };
        let keys: Vec<_> = c.scalar_keys().unwrap().collect();
        assert_eq!(keys, vec![(0.0, 1.0), (0.5, 50.0), (1.0, 1.0)]);
        assert_eq!(c.components[0].len(), 3);
        assert_eq!(g.leaves[1].value, Value::Flag(1));
        let Value::Text(t) = g.leaves[2].value else {
            panic!("not a string");
        };
        assert_eq!(p.name(t), "Text");
        assert!(p.leaf(g, "Missing").unwrap().is_none());
    }

    #[test]
    fn a_repeated_label_or_leaf_name_is_refused_on_lookup() {
        let leaves = [leaf(1, 3, 4, &[0; 4]), leaf(1, 3, 4, &[0; 4])];
        let b = file(
            3,
            &NAMES,
            0,
            &[group(1, [0.0; 2], 2, &leaves), group(1, [0.0; 2], 2, &[])],
        );
        let p = Ppr::parse(&b).unwrap();
        assert_eq!(p.groups_of("Sky").count(), 2);
        assert!(p.group("Sky", "Summer").is_err());
        assert!(p.leaf(&p.groups[0], "EmissiveScales.TrackEmissiveEV").is_err());
    }

    #[test]
    fn a_cut_or_extended_file_is_refused() {
        let mut b = sample();
        for len in 0..b.len() {
            assert!(Ppr::parse(&b[..len]).is_err(), "truncated at {len}");
        }
        b.push(0);
        let e = Ppr::parse(&b).unwrap_err();
        assert!(e.to_string().contains("bytes follow the end"), "{e}");
    }

    #[test]
    fn what_the_layout_does_not_have_is_refused() {
        let ok = sample();
        // version
        let mut b = ok.clone();
        b[0] = 4;
        assert!(Ppr::parse(&b).unwrap_err().to_string().contains("version"));
        // a name that is not printable
        let mut b = ok.clone();
        b[9] = 0x01;
        assert!(Ppr::parse(&b).unwrap_err().to_string().contains("printable"));
        // a group kind past the table
        let bad = file(3, &NAMES, 0, &[group(9, [0.0; 2], 2, &[])]);
        assert!(Ppr::parse(&bad).unwrap_err().to_string().contains("group kind"));
        // a root past the table
        let bad = file(3, &NAMES, 9, &[]);
        assert!(Ppr::parse(&bad).unwrap_err().to_string().contains("root name"));
        // a leaf type 8
        let bad = file(3, &NAMES, 0, &[group(1, [0.0; 2], 2, &[leaf(8, 3, 4, &[0; 4])])]);
        assert!(Ppr::parse(&bad).unwrap_err().to_string().contains("leaf type 8"));
        // a string past the table
        let bad = file(
            3,
            &NAMES,
            0,
            &[group(1, [0.0; 2], 2, &[leaf(3, 3, 4, &9u16.to_le_bytes())])],
        );
        assert!(Ppr::parse(&bad).unwrap_err().to_string().contains("string"));
        // a curve flag 2, and a flag 1 on a colour curve
        for (kind, flag) in [(5u8, 2u8), (6, 1)] {
            let mut payload = 1u32.to_le_bytes().to_vec();
            payload.push(flag);
            payload.extend_from_slice(&0u32.to_le_bytes());
            let bad = file(
                3,
                &NAMES,
                0,
                &[group(1, [0.0; 2], 2, &[leaf(kind, 3, 4, &payload)])],
            );
            assert!(
                Ppr::parse(&bad).unwrap_err().to_string().contains("curve flag"),
                "{kind} {flag}"
            );
        }
        // a group count the file cannot hold
        let mut bad = file(3, &NAMES, 0, &[]);
        let at = bad.len() - 4;
        bad[at..].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Ppr::parse(&bad).is_err());
    }

    #[test]
    fn a_flag_one_key_is_five_floats_in_version_two_and_six_in_version_three() {
        for (version, stride) in [(2u32, 5usize), (3, 6)] {
            let mut payload = 1u32.to_le_bytes().to_vec();
            payload.push(1);
            payload.extend_from_slice(&1u32.to_le_bytes());
            payload.extend_from_slice(&floats(&vec![0.25; stride]));
            let b = file(
                version,
                &NAMES,
                0,
                &[group(1, [0.0; 2], 2, &[leaf(5, 3, 4, &payload)])],
            );
            let p = Ppr::parse(&b).unwrap();
            let Value::Curve(c) = &p.groups[0].leaves[0].value else {
                panic!("not a curve");
            };
            let keys: Vec<_> = c.components[0].keys().collect();
            assert_eq!(keys.len(), 1);
            assert_eq!(keys[0].len(), stride);
            assert!(c.scalar_keys().is_none());
        }
    }

    #[test]
    fn a_file_with_no_group_reads() {
        // Homespace.ppr: version 2, one name, the root and no group
        let mut b = vec![2, 0, 0, 0, 1, 0, 0, 0, 9];
        b.extend_from_slice(b"Homespace");
        b.extend_from_slice(&[0; 6]);
        assert_eq!(b.len(), 24);
        let p = Ppr::parse(&b).unwrap();
        assert_eq!(p.name(p.root), "Homespace");
        assert!(p.groups.is_empty());
    }
}
