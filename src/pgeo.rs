//! Placement files (`.pgeo`): the rule-placed instances of one cell size, cell and category (buildings, props, trees,
//! bushes, barriers, rocks, signs, grass, infrastructure, bridges, smashables, crowds, crowd splines, the evolving
//! world), each a list of named blocks of 80-byte instance records.
//!
//! A track's `.pgeo` files are entries of its GeoChunks ([`crate::minizip`]), named
//! `...\scene\proc\cellsize\<size>\<x>_<z>\c<size>_<category>_<lod>[...].pgeo`; the folders give the cell
//! ([`crate::world::Cell::from_path`]). Layout (little-endian; a string is a u32 length and that many bytes of
//! printable ASCII, no terminator):
//!
//! | field | size | value |
//! |---|---|---|
//! | section name | string | |
//! | kind | 4 | u32 0, 2 or 5 ([`Kind`]) |
//! | three | 4 | u32 3, kind 5 only |
//! | constants | 8 | u32 13, u32 15 |
//! | bounding box | 32 | f32 min x, y, z, 0, max x, y, z, 1 |
//! | category | string | the template library's category (`PROPS`, `GRASSTEMPLATE`, ...) |
//! | unknown | 4 | f32 |
//! | route | string | empty when none |
//! | gameplay tag | string | empty when none |
//! | season exclusion | 4 | u8 mask ([`crate::world::SeasonMask`]), three zero bytes |
//! | zero | 4 | u32 0 |
//! | table | 4 + 32 n | u32 n, n entries |
//! | list A | | u32 n, n blocks |
//! | list B | | u32 n, n blocks |
//! | zero | 4 | u32 0 |
//! | crowd splines | | kind 5 only: u32 n, n splines |
//!
//! A table entry, 32 bytes: u32 0, u32 hash, then a range of list B and a range of list A, each i32 block, u32 first
//! record, u32 count; block -1 (with first and count 0) is no range. A range lies inside its block.
//!
//! A block: string name, u32 n, n 80-byte instance records, u32 m (0 or n), m 96-byte curve records, u32 k, k sources
//! (string template, u32 first, u32 count). The sources run on from record 0 and, when there are any, cover all n.
//!
//! The instance record, 80 bytes ([`Instance`]):
//!
//! | offset | size | field |
//! |---|---|---|
//! | 0 | 12 | position, u32 x, y, z in sign-magnitude fixed point ([`crate::world::fixed`]) |
//! | 12 | 24 | f32 first and second rows of the rotation |
//! | 36 | 12 | f32 scale x, y, z |
//! | 48 | 4 | u32 0x80800000 |
//! | 52 | 4 | u8 instance variation x, y, z, w |
//! | 56 | 4 | u32 0 |
//! | 60 | 1 | u8 material variant |
//! | 61 | 1 | u8 0 |
//! | 62 | 2 | u16 frame |
//! | 64 | 4 | u32, meaning not known |
//! | 68 | 4 | u32, meaning not known |
//! | 72 | 4 | u32 0 |
//! | 76 | 4 | u32, meaning not known |
//!
//! A curve record is 24 f32: the no-curve value `-990 - j / 3` (j = 0..24, whole division: three each of -990 to
//! -997), or the instance's spline: the coefficients c0..c3 of the position polynomial and c4..c7 of the up vector,
//! each three floats ([`Curve`]).
//!
//! A crowd spline (kind 5): string id, f32 box min x, y, z, 0, max x, y, z, 0, f32 length, f32 value, u32 n points of 80
//! bytes (three f32 vectors of x, y, z, 0, then eight f32), then two lists of f32 (u32 n, n f32 each).
//!
//! The file ends where that ends. [`Pgeo::parse`] refuses any other value or arrangement, with the offset.
//!
//! A block's name is the name of a model template of the category's template library ([`crate::plib`]), which
//! names the model; its sources name the library's placement templates.
//!
//! The cell a file is kept under does not bound its instances (a building's pieces can stand tens of metres outside
//! it, a bridge's hundreds); the header's box does, to a step of the fixed-point positions
//! ([`crate::world::FIXED_STEP`]) and an `f32` rounding. [`Header::parse`] reads only the header, and
//! [`Header::may_hold`] says whether a file can hold instances in an x/z [`Area`]; [`Pgeo::instances_in`] gives
//! those of a read file. The instances of a crowd spline file (kind 5) all stand at the origin: its places are its
//! splines ([`Pgeo::splines_in`]).
//!
//! What has been checked, and how: no third-party reader of these files exists, so the check is the census in
//! `tests/pgeo.rs`. All 96,234 `.pgeo` files of the Tokyo track's GeoChunks (Steam build 25588841: 665,298 blocks,
//! 29,164,671 instance records, 243,394 spline curves, 244 crowd splines) read with no refusal and every byte
//! accounted for (every file with `[run] gate = 'full'`, a seeded sample otherwise). Every position lies inside its
//! file's header box widened as above; 337,778 lie outside the cell the file is kept under. Every source range names
//! a placement template, and in each season a file is shown in, a block's name is a model template of its
//! category's library, but for 37,066 instance-seasons whose block names the library does not hold (27 names, all
//! winter `GRASSTEMPLATE` crops and tufty or long grass) and 3,579 whose names it holds twice (`bar_gbl_crct_foam_01_b_3D`
//! in every season, `tre_urb_ginkgo_d_3D` in autumn): those are counted, not refused. The placements in one box, read from the files whose header
//! box may hold some, are those read from every file.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::read::Reader;
use crate::world::{Area, FIXED_STEP, SeasonMask, fixed};
use crate::{Error, Result};

/// An instance record's length in bytes.
pub const RECORD_LEN: usize = 80;
/// A curve record's length in bytes.
pub const CURVE_LEN: usize = 96;
/// A table entry's length in bytes.
pub const TABLE_ENTRY_LEN: usize = 32;
/// A crowd spline point's length in bytes.
pub const SPLINE_POINT_LEN: usize = 80;

fn fail(what: String) -> Error {
    Error::Pgeo(what)
}

/// The header's kind word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// 0: the files of every category but the crowds.
    Category,
    /// 2: the crowd template files (`crowdtemplate_cwd`).
    Crowd,
    /// 5: the crowd spline files (`cwdspline`), with a crowd spline section after the lists.
    CrowdSpline,
}

impl Kind {
    /// The word as stored.
    pub fn word(self) -> u32 {
        match self {
            Kind::Category => 0,
            Kind::Crowd => 2,
            Kind::CrowdSpline => 5,
        }
    }
}

/// The file header.
#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    /// The section name (`c75_grasstemplate_do_221_cellx55z114_section0`).
    pub section: String,
    /// The kind word.
    pub kind: Kind,
    /// The bounding box's minimum x, y, z.
    pub min: [f32; 3],
    /// The bounding box's maximum x, y, z.
    pub max: [f32; 3],
    /// The template library's category, upper case (`PROPS`).
    pub category: String,
    /// A float whose meaning is not known.
    pub unknown: f32,
    /// The route, empty when none.
    pub route: String,
    /// The gameplay tag, empty when none.
    pub gameplay_tag: String,
    /// The seasons the file is not shown in.
    pub season_exclusion: SeasonMask,
}

impl Header {
    /// Reads only a file's header: what [`Pgeo::parse`] reads first, without the rest.
    pub fn parse(bytes: &[u8]) -> Result<Header> {
        parse_header(&mut Reader::new(bytes, fail))
    }

    /// Whether the file can hold an instance (or, in a crowd spline file, a spline) inside `area`. An instance's
    /// position is stored to [`FIXED_STEP`] and the header's box to an `f32`, so every position lies inside the box
    /// widened by a step and an `f32` rounding; a file whose box so widened misses the area holds nothing there.
    pub fn may_hold(&self, area: &Area) -> bool {
        let widen = |v: f32, up: bool| {
            let rounded = if up { v.next_up() } else { v.next_down() };
            f64::from(rounded) + if up { FIXED_STEP } else { -FIXED_STEP }
        };
        let [min_x, _, min_z] = self.min.map(|v| widen(v, false));
        let [max_x, _, max_z] = self.max.map(|v| widen(v, true));
        min_x < area.max_x && area.min_x < max_x && min_z < area.max_z && area.min_z < max_z
    }
}

/// A range of one block's records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Range {
    /// The block's index in its list.
    pub block: u32,
    /// The first record.
    pub first: u32,
    /// How many records.
    pub count: u32,
}

/// A table entry: a hash and a range of each list. What the table is for is not known.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TableEntry {
    /// The hash (one value per block name; its function is not known).
    pub hash: u32,
    /// The range of list B, if any.
    pub b: Option<Range>,
    /// The range of list A, if any.
    pub a: Option<Range>,
}

/// One 80-byte instance record, as stored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance {
    /// The position words x, y, z, in sign-magnitude fixed point.
    pub position: [u32; 3],
    /// The first two rows of the rotation; the third is their cross product.
    pub rows: [[f32; 3]; 2],
    /// The scale along each local axis.
    pub scale: [f32; 3],
    /// The instance variation bytes x, y, z, w the instanced shaders read (each `byte / 255`).
    pub variation: [u8; 4],
    /// The row of the model's material variant table; 0 is the base.
    pub material_variant: u8,
    /// The frame index (the `.pvs` `ModulateConstants` frame table); 0 is none.
    pub frame: u16,
    /// The u32 at byte 64, meaning not known (0 on most records).
    pub unknown_64: u32,
    /// The u32 at byte 68, meaning not known (a different value on most records).
    pub unknown_68: u32,
    /// The u32 at byte 76, meaning not known (0x100 or 0x200 on the Tokyo track).
    pub unknown_76: u32,
}

impl Instance {
    /// The position in metres.
    pub fn metres(&self) -> [f64; 3] {
        self.position.map(fixed)
    }

    fn parse(r: &mut Reader<'_>) -> Result<Instance> {
        let position = [r.u32("position x")?, r.u32("position y")?, r.u32("position z")?];
        let row0 = r.f32s("rotation row 0")?;
        let row1 = r.f32s("rotation row 1")?;
        let scale = r.f32s("scale")?;
        r.expect_u32(0x8080_0000, "the u32 at byte 48")?;
        let variation = r.array("instance variation")?;
        r.expect_u32(0, "the u32 at byte 56")?;
        let material_variant = r.u8("material variant")?;
        r.expect_u8(0, "byte 61")?;
        let frame = r.u16("frame")?;
        let unknown_64 = r.u32("the u32 at byte 64")?;
        let unknown_68 = r.u32("the u32 at byte 68")?;
        r.expect_u32(0, "the u32 at byte 72")?;
        let unknown_76 = r.u32("the u32 at byte 76")?;
        Ok(Instance {
            position,
            rows: [row0, row1],
            scale,
            variation,
            material_variant,
            frame,
            unknown_64,
            unknown_68,
            unknown_76,
        })
    }
}

/// A curve record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    /// The no-curve value.
    None,
    /// The instance's spline: `P(t) = c0 + c1 t + c2 t² + c3 t³` and `Up(t) = c4 + c5 t + c6 t² + c7 t³`.
    Spline {
        /// c0, c1, c2, c3.
        position: [[f32; 3]; 4],
        /// c4, c5, c6, c7.
        up: [[f32; 3]; 4],
    },
}

impl Curve {
    /// The 24 floats of the no-curve record.
    pub fn none_values() -> [f32; 24] {
        let mut v = [0.0; 24];
        for (j, x) in (0u8..).zip(v.iter_mut()) {
            *x = -990.0 - f32::from(j / 3);
        }
        v
    }

    fn parse(r: &mut Reader<'_>) -> Result<Curve> {
        let v: [f32; 24] = r.f32s("curve record")?;
        if v == Curve::none_values() {
            return Ok(Curve::None);
        }
        let mut c = [[0.0f32; 3]; 8];
        for (k, x) in c.iter_mut().flatten().zip(v) {
            *k = x;
        }
        let [c0, c1, c2, c3, c4, c5, c6, c7] = c;
        Ok(Curve::Spline {
            position: [c0, c1, c2, c3],
            up: [c4, c5, c6, c7],
        })
    }
}

/// One source range of a block: the placement template its records came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    /// The placement template's name.
    pub template: String,
    /// The first record.
    pub first: u32,
    /// How many records.
    pub count: u32,
}

/// A block: a name and its instances.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// The block's name (`<template>_3D`, `<template>_impostor`, `PEDESTRIAN`, ...).
    pub name: String,
    /// The instance records.
    pub instances: Vec<Instance>,
    /// One curve record per instance, or `None` when the block has none.
    pub curves: Option<Vec<Curve>>,
    /// The source ranges, in record order; empty on some blocks.
    pub sources: Vec<Source>,
}

impl Block {
    fn parse(r: &mut Reader<'_>) -> Result<Block> {
        let name = r.string("block name")?;
        let n = r.count(RECORD_LEN, "instance count")?;
        let instances = (0..n).map(|_| Instance::parse(r)).collect::<Result<Vec<_>>>()?;
        let at = r.position();
        let m = r.count(CURVE_LEN, "curve count")?;
        let curves = match m {
            0 => None,
            m if m == n => Some((0..m).map(|_| Curve::parse(r)).collect::<Result<Vec<_>>>()?),
            m => return Err(r.fail_at(at, format_args!("{name}: {m} curve records for {n} instances"))),
        };
        let k = r.count(12, "source count")?;
        let mut sources = Vec::with_capacity(k);
        let mut next = 0u32;
        for _ in 0..k {
            let at = r.position();
            let template = r.string("source template")?;
            let first = r.u32("source first record")?;
            let count = r.u32("source record count")?;
            if first != next {
                return Err(r.fail_at(
                    at,
                    format_args!("{name}: source {template} starts at record {first}, expected {next}"),
                ));
            }
            next = first
                .checked_add(count)
                .ok_or_else(|| r.fail_at(at, format_args!("{name}: source {template} runs past u32")))?;
            sources.push(Source {
                template,
                first,
                count,
            });
        }
        if k > 0 && usize::try_from(next).ok() != Some(n) {
            return Err(r.fail(format_args!("{name}: the sources cover {next} of {n} records")));
        }
        Ok(Block {
            name,
            instances,
            curves,
            sources,
        })
    }
}

/// A point of a crowd spline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplinePoint {
    /// Three vectors (each stored with a fourth float, 0), roles not known.
    pub vectors: [[f32; 3]; 3],
    /// Eight floats; on the last point the first is the spline's length.
    pub samples: [f32; 8],
}

/// A crowd spline (kind 5 files).
#[derive(Clone, Debug, PartialEq)]
pub struct CrowdSpline {
    /// The id (a GUID).
    pub id: String,
    /// The box's minimum x, y, z.
    pub min: [f32; 3],
    /// The box's maximum x, y, z.
    pub max: [f32; 3],
    /// The length.
    pub length: f32,
    /// A float whose meaning is not known.
    pub value: f32,
    /// The points.
    pub points: Vec<SplinePoint>,
    /// The first list of floats, meaning not known.
    pub list_0: Vec<f32>,
    /// The second list of floats, meaning not known.
    pub list_1: Vec<f32>,
}

/// A `.pgeo` file, as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct Pgeo {
    /// The header.
    pub header: Header,
    /// The table.
    pub table: Vec<TableEntry>,
    /// The first block list (impostors, billboards, vista copies, `ANIMATED_SPRITE`).
    pub list_a: Vec<Block>,
    /// The second block list (the model blocks, `PEDESTRIAN`, `ANIMATED`).
    pub list_b: Vec<Block>,
    /// The crowd splines (kind 5 files; empty on the others).
    pub splines: Vec<CrowdSpline>,
}

impl Pgeo {
    /// Reads a whole file, refusing any byte the layout does not account for.
    pub fn parse(bytes: &[u8]) -> Result<Pgeo> {
        let mut r = Reader::new(bytes, fail);
        let header = parse_header(&mut r)?;
        r.expect_u32(0, "word after the season mask")?;
        let n = r.count(TABLE_ENTRY_LEN, "table count")?;
        let mut raw = Vec::with_capacity(n);
        for _ in 0..n {
            let at = r.position();
            r.expect_u32(0, "table entry's first word")?;
            let hash = r.u32("table hash")?;
            let b = [
                r.i32("list B block")?,
                r.i32("list B first")?,
                r.i32("list B count")?,
            ];
            let a = [
                r.i32("list A block")?,
                r.i32("list A first")?,
                r.i32("list A count")?,
            ];
            raw.push((at, hash, b, a));
        }
        let list_a = blocks(&mut r, "list A count")?;
        let list_b = blocks(&mut r, "list B count")?;
        let mut table = Vec::with_capacity(n);
        for (at, hash, b, a) in raw {
            table.push(TableEntry {
                hash,
                b: range(&r, at, b, &list_b, "list B")?,
                a: range(&r, at, a, &list_a, "list A")?,
            });
        }
        r.expect_u32(0, "word after the lists")?;
        let mut splines = Vec::new();
        if header.kind == Kind::CrowdSpline {
            let n = r.count(1, "crowd spline count")?;
            for _ in 0..n {
                splines.push(parse_spline(&mut r)?);
            }
        }
        r.end("pgeo")?;
        Ok(Pgeo {
            header,
            table,
            list_a,
            list_b,
            splines,
        })
    }

    /// Both lists' blocks, list A first.
    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.list_a.iter().chain(&self.list_b)
    }

    /// Every instance whose position is inside `area`, with its block and its index in the block, list A first. A
    /// crowd spline file's instances all stand at the origin (the splines carry the places: [`Pgeo::splines_in`]), so
    /// it yields none.
    pub fn instances_in<'a>(
        &'a self,
        area: &'a Area,
    ) -> impl Iterator<Item = (&'a Block, usize, &'a Instance)> + 'a {
        let placed = self.header.kind != Kind::CrowdSpline;
        self.blocks().filter(move |_| placed).flat_map(move |b| {
            b.instances.iter().enumerate().filter_map(move |(k, i)| {
                let [x, _, z] = i.metres();
                area.contains(x, z).then_some((b, k, i))
            })
        })
    }

    /// The crowd splines whose box overlaps `area`.
    pub fn splines_in<'a>(&'a self, area: &'a Area) -> impl Iterator<Item = &'a CrowdSpline> + 'a {
        self.splines.iter().filter(move |s| {
            let [min_x, _, min_z] = s.min.map(f64::from);
            let [max_x, _, max_z] = s.max.map(f64::from);
            min_x < area.max_x && area.min_x <= max_x && min_z < area.max_z && area.min_z <= max_z
        })
    }
}

fn parse_header(r: &mut Reader<'_>) -> Result<Header> {
    let section = r.string("section name")?;
    let at = r.position();
    let kind = match r.u32("kind")? {
        0 => Kind::Category,
        2 => Kind::Crowd,
        5 => Kind::CrowdSpline,
        k => return Err(r.fail_at(at, format_args!("kind: expected 0, 2 or 5, found {k}"))),
    };
    if kind == Kind::CrowdSpline {
        r.expect_u32(3, "kind 5's second word")?;
    }
    r.expect_u32(13, "header constant 13")?;
    r.expect_u32(15, "header constant 15")?;
    let (min, max) = bounds(r, 1.0, "bounding box")?;
    let category = r.string("category")?;
    let unknown = r.f32("header float")?;
    let route = r.string("route")?;
    let gameplay_tag = r.string("gameplay tag")?;
    let at = r.position();
    let byte = r.u8("season mask")?;
    let season_exclusion = SeasonMask::new(byte).ok_or_else(|| {
        r.fail_at(
            at,
            format_args!("season mask {byte:#04x}: its two nibbles differ"),
        )
    })?;
    for _ in 0..3 {
        r.expect_u8(0, "season mask padding")?;
    }
    Ok(Header {
        section,
        kind,
        min,
        max,
        category,
        unknown,
        route,
        gameplay_tag,
        season_exclusion,
    })
}

/// A box of `min x, y, z, 0, max x, y, z, w`.
fn bounds(r: &mut Reader<'_>, w: f32, what: &str) -> Result<([f32; 3], [f32; 3])> {
    let min = vec3(r, 0.0, what)?;
    let max = vec3(r, w, what)?;
    Ok((min, max))
}

/// Three floats and a fourth that must be `w` (bit for bit: `0.0` is not `-0.0`).
fn vec3(r: &mut Reader<'_>, w: f32, what: &str) -> Result<[f32; 3]> {
    let v = r.f32s(what)?;
    let at = r.position();
    let found = r.f32(what)?;
    if found.to_bits() != w.to_bits() {
        return Err(r.fail_at(
            at,
            format_args!("{what}: fourth float expected {w}, found {found}"),
        ));
    }
    Ok(v)
}

fn blocks(r: &mut Reader<'_>, what: &str) -> Result<Vec<Block>> {
    let n = r.count(4, what)?;
    (0..n).map(|_| Block::parse(r)).collect()
}

fn range(
    r: &Reader<'_>,
    at: usize,
    [block, first, count]: [i32; 3],
    list: &[Block],
    what: &str,
) -> Result<Option<Range>> {
    if block == -1 {
        if first != 0 || count != 0 {
            return Err(r.fail_at(
                at,
                format_args!("table entry's {what} range: block -1 with first {first}, count {count}"),
            ));
        }
        return Ok(None);
    }
    let inside = (|| {
        let b = list.get(usize::try_from(block).ok()?)?;
        let end = usize::try_from(first)
            .ok()?
            .checked_add(usize::try_from(count).ok()?)?;
        (end <= b.instances.len()).then_some(())
    })();
    if inside.is_none() {
        return Err(r.fail_at(
            at,
            format_args!("table entry's {what} range: block {block}, first {first}, count {count} is not inside the list"),
        ));
    }
    // each value is non-negative here
    Ok(Some(Range {
        block: block.unsigned_abs(),
        first: first.unsigned_abs(),
        count: count.unsigned_abs(),
    }))
}

fn parse_spline(r: &mut Reader<'_>) -> Result<CrowdSpline> {
    let id = r.string("crowd spline id")?;
    let (min, max) = bounds(r, 0.0, "crowd spline box")?;
    let length = r.f32("crowd spline length")?;
    let value = r.f32("crowd spline value")?;
    let n = r.count(SPLINE_POINT_LEN, "crowd spline point count")?;
    let mut points = Vec::with_capacity(n);
    for _ in 0..n {
        let vectors = [
            vec3(r, 0.0, "crowd spline point vector")?,
            vec3(r, 0.0, "crowd spline point vector")?,
            vec3(r, 0.0, "crowd spline point vector")?,
        ];
        let samples = r.f32s("crowd spline point samples")?;
        points.push(SplinePoint { vectors, samples });
    }
    let list_0 = floats(r, "crowd spline first list")?;
    let list_1 = floats(r, "crowd spline second list")?;
    Ok(CrowdSpline {
        id,
        min,
        max,
        length,
        value,
        points,
        list_0,
        list_1,
    })
}

fn floats(r: &mut Reader<'_>, what: &str) -> Result<Vec<f32>> {
    let n = r.count(4, what)?;
    (0..n).map(|_| r.f32(what)).collect()
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::unwrap_used,
    reason = "tests on records built by hand"
)]
mod tests {
    use super::*;

    fn s(out: &mut Vec<u8>, v: &str) {
        out.extend((v.len() as u32).to_le_bytes());
        out.extend(v.as_bytes());
    }
    fn u(out: &mut Vec<u8>, v: u32) {
        out.extend(v.to_le_bytes());
    }
    fn f(out: &mut Vec<u8>, v: f32) {
        out.extend(v.to_le_bytes());
    }

    fn header(out: &mut Vec<u8>, kind: u32) {
        s(out, "c50_props_do_cellx1z2_section0");
        u(out, kind);
        if kind == 5 {
            u(out, 3);
        }
        u(out, 13);
        u(out, 15);
        for v in [50.0, 0.0, 100.0, 0.0, 100.0, 10.0, 150.0, 1.0] {
            f(out, v);
        }
        s(out, "PROPS");
        f(out, 2.5);
        s(out, "");
        s(out, "TAXIJOB");
        out.extend([0x77, 0, 0, 0]);
        u(out, 0);
    }

    fn record(out: &mut Vec<u8>, x: u32) {
        u(out, x);
        u(out, 0x0002_8000);
        u(out, 0x8064_0000);
        for v in [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0] {
            f(out, v);
        }
        u(out, 0x8080_0000);
        out.extend([255, 1, 2, 3, 0, 0, 0, 0, 4, 0]);
        out.extend(7u16.to_le_bytes());
        for v in [1, 2, 0, 0x200] {
            u(out, v);
        }
    }

    /// One table entry, an empty list A, and a list B of one block of two instances with a no-curve record and a
    /// spline record, and one source.
    fn file() -> Vec<u8> {
        let mut out = Vec::new();
        header(&mut out, 0);
        u(&mut out, 1);
        for v in [0, 0xABCD, 0, 1, 1, u32::MAX, 0, 0] {
            u(&mut out, v);
        }
        u(&mut out, 0);
        u(&mut out, 1);
        s(&mut out, "prp_box_a_3D");
        u(&mut out, 2);
        record(&mut out, 0x0032_0000);
        record(&mut out, 0x0033_0000);
        u(&mut out, 2);
        for v in Curve::none_values() {
            f(&mut out, v);
        }
        for k in 0..24 {
            f(&mut out, k as f32);
        }
        u(&mut out, 1);
        s(&mut out, "PRP_BOX_A");
        u(&mut out, 0);
        u(&mut out, 2);
        u(&mut out, 0);
        out
    }

    #[test]
    fn reads_a_file() {
        let p = Pgeo::parse(&file()).unwrap();
        assert_eq!(p.header.kind, Kind::Category);
        assert_eq!(p.header.min, [50.0, 0.0, 100.0]);
        assert_eq!(p.header.gameplay_tag, "TAXIJOB");
        assert_eq!(p.header.season_exclusion.byte(), 0x77);
        assert_eq!(
            p.table,
            [TableEntry {
                hash: 0xABCD,
                b: Some(Range {
                    block: 0,
                    first: 1,
                    count: 1
                }),
                a: None
            }]
        );
        assert!(p.list_a.is_empty());
        let b = &p.list_b[0];
        assert_eq!(b.instances[0].metres(), [50.0, 2.0 + 32768.0 / 65535.0, -100.0]);
        assert_eq!(b.instances[1].material_variant, 4);
        assert_eq!(b.instances[1].frame, 7);
        assert_eq!(b.instances[1].variation, [255, 1, 2, 3]);
        assert_eq!(
            (
                b.instances[1].unknown_64,
                b.instances[1].unknown_68,
                b.instances[1].unknown_76
            ),
            (1, 2, 0x200)
        );
        let curves = b.curves.as_ref().unwrap();
        assert_eq!(curves[0], Curve::None);
        assert!(
            matches!(curves[1], Curve::Spline { position, up } if position[1] == [3.0, 4.0, 5.0] && up[3] == [21.0, 22.0, 23.0])
        );
        assert_eq!(b.sources[0].template, "PRP_BOX_A");
    }

    #[test]
    fn refuses_what_the_layout_does_not_hold() {
        let good = file();
        assert!(Pgeo::parse(&good).is_ok());
        // a trailing byte
        let mut bad = good.clone();
        bad.push(0);
        assert!(Pgeo::parse(&bad).is_err());
        // every truncation
        for len in 0..good.len() {
            assert!(Pgeo::parse(good.get(..len).unwrap()).is_err(), "{len}");
        }
        let at = |needle: &[u8]| good.windows(needle.len()).position(|w| w == needle).unwrap();
        // a kind the layout does not have
        let mut bad = good.clone();
        let k = 4 + "c50_props_do_cellx1z2_section0".len();
        bad[k] = 1;
        assert!(Pgeo::parse(&bad).unwrap_err().to_string().contains("kind"));
        // a season mask whose nibbles differ
        let mut bad = good.clone();
        bad[at(b"TAXIJOB") + 7] = 0x71;
        assert!(Pgeo::parse(&bad).unwrap_err().to_string().contains("nibbles"));
        // byte 61 of a record
        let mut bad = good.clone();
        let rec = at(b"prp_box_a_3D") + 12 + 4;
        bad[rec + 61] = 1;
        assert!(Pgeo::parse(&bad).unwrap_err().to_string().contains("byte 61"));
        // a table range past its block
        let mut bad = good.clone();
        let t = at(&0xABCDu32.to_le_bytes());
        bad[t + 12] = 2;
        assert!(Pgeo::parse(&bad).unwrap_err().to_string().contains("not inside"));
        // a source that does not cover the block
        let mut bad = good.clone();
        let src = at(b"PRP_BOX_A") + 9 + 4;
        bad[src] = 1;
        assert!(
            Pgeo::parse(&bad)
                .unwrap_err()
                .to_string()
                .contains("cover 1 of 2")
        );
    }

    #[test]
    fn crowd_splines() {
        let mut out = Vec::new();
        header(&mut out, 5);
        for _ in 0..3 {
            u(&mut out, 0);
        }
        u(&mut out, 0);
        u(&mut out, 1);
        s(&mut out, "{guid}");
        for _ in 0..8 {
            f(&mut out, 0.0);
        }
        f(&mut out, 12.0);
        f(&mut out, 0.8);
        u(&mut out, 1);
        for v in [1.0, 2.0, 3.0, 0.0, 4.0, 5.0, 6.0, 0.0, 7.0, 8.0, 9.0, 0.0] {
            f(&mut out, v);
        }
        f(&mut out, 12.0);
        for _ in 0..7 {
            f(&mut out, 0.0);
        }
        u(&mut out, 1);
        f(&mut out, 0.5);
        u(&mut out, 0);
        let p = Pgeo::parse(&out).unwrap();
        assert_eq!(p.header.kind, Kind::CrowdSpline);
        let sp = &p.splines[0];
        assert_eq!(
            (sp.length, sp.points[0].vectors[2], sp.points[0].samples[0]),
            (12.0, [7.0, 8.0, 9.0], 12.0)
        );
        assert_eq!((sp.list_0.as_slice(), sp.list_1.len()), (&[0.5][..], 0));
    }
}
