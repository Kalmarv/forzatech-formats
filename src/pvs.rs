//! The hand-placed instance file (`.pvs`, version `0xA4`): the instances of a track not placed by a [`crate::pgeo`]
//! rule, each with its transform and LOD models, and the model, texture, material and shader tables they use.
//!
//! A track has one, `Ribbon_00\<track>_00.pvs`; `Stripped\gs\generalsceneassets.pvs` has the same header and only
//! the texture descriptors. Layout (little-endian; a string is a u32 length and that many bytes of printable ASCII;
//! a list is a u32 count and that many entries):
//!
//! | section | layout |
//! |---|---|
//! | header | `SVPF`, u32 0xA4, u32 8, u32 0, u32 0, u8 world (1 or 0), u32 6, u32, u32 0 |
//! | megatextures | u8 0, list of u32 × 2, u32, u8 1, list (as long) of (string, u32, u32) |
//! | bounds | u32, u32, list of 29-byte [`Bound`]s |
//! | gameplay | u32, u32, u32 n, list of [`GameplayRecord`]s, list (n) of i32, list (n) of u8 |
//! | index tables | u32 0, list of (u32 its own index, list of i32) |
//! | fixed block | 49 bytes, list of u32 × 2, u8 0 |
//! | texture descriptors | list of 56-byte records |
//! | shaders, materials | two lists of (u32 hash, string path) |
//! | models | list of u32 |
//! | cells | list of 12-byte [`CellRecord`]s |
//! | dependencies | list of lists of u32 × 2 |
//! | frames | list of f32 × 12 |
//! | transforms | list of 58-byte [`Transform`]s |
//! | LOD rows | list of [`LodRow`]s (u32 model, u32 instance) |
//! | LOD profiles | list of lists of 50-byte [`LodProfile`]s |
//! | instances | list of 35-byte [`Instance`]s |
//! | model bounds | u32 0, list (as long as the models) of [`ModelBound`]s |
//! | tail | [`Tail`] |
//!
//! Without the world (header byte 20 is 0) the file is the header and the texture descriptors. The file ends where the
//! last section ends. Each record's layout is on its type.
//!
//! [`Pvs::parse`] refuses, with the offset, any other value where the layout has a constant, any byte left over, and
//! any reference past what it refers to: an instance's transform, parent, gameplay record and frame; a LOD row's
//! model; a LOD row that is not its instance's; a band naming a row the instance does not have; a transform's clip
//! and LOD profile group; a texture key's descriptor; a clip record's file.
//!
//! A model number ([`LodRow::model`]) indexes the models list, and is also the id of the GeoChunk entry whose chunk
//! map record has type 4 ([`crate::track::Track::pvs_model`]). A [`CellRecord`]'s references are chunk map records of
//! type 12, the `.pvsz` entries.
//!
//! What has been checked, and how: no third-party reader of these files exists, so the check is a census
//! (`tests/pvs.rs`): both `.pvs` files of the install read to their last byte with no refusal.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::read::Reader;
use crate::world::{Area, SeasonMask, half};
use crate::{Error, Result};
use std::collections::HashSet;

/// The file's first bytes.
pub const MAGIC: [u8; 4] = *b"SVPF";
/// The one version read.
pub const VERSION: u32 = 0xA4;
/// A texture descriptor's length in bytes.
pub const TEXTURE_LEN: usize = 56;
/// A transform record's length in bytes.
pub const TRANSFORM_LEN: usize = 58;
/// An instance record's length in bytes.
pub const INSTANCE_LEN: usize = 35;
/// A LOD profile record's length in bytes.
pub const LOD_PROFILE_LEN: usize = 50;
/// A bounds record's length in bytes.
pub const BOUND_LEN: usize = 29;
/// The value of [`Instance::band_rows`] for a band that draws nothing.
pub const NO_ROW: u8 = 0x7F;
/// The [`Instance::flags`] bit of a catalogue copy.
pub const CATALOGUE: u16 = 0x40;
/// The chunk map type of a [`CellRecord`] reference.
pub const CELL_REFERENCE_TYPE: u8 = 12;

fn fail(what: String) -> Error {
    Error::Pvs(what)
}

/// The header's fields that are not constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// Whether the file has the world sections (byte 20 is 1).
    pub world: bool,
    /// The u32 at byte 25, meaning not known.
    pub unknown_25: u32,
}

/// The megatexture section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Megatextures {
    /// The 8-byte entries, as two u32 each.
    pub entries: Vec<[u32; 2]>,
    /// The u32 after them, meaning not known.
    pub value: u32,
    /// One name per entry, each with two u32 whose meaning is not known.
    pub names: Vec<(String, u32, u32)>,
}

/// A 29-byte bounds record.
///
/// | offset | size | field |
/// |---|---|---|
/// | 0 | 4 | u32 |
/// | 4 | 1 | u8 0 or 0xFF |
/// | 5 | 12 | f32 × 3 |
/// | 17 | 8 | f16 × 4 |
/// | 25 | 2 | u16 0 |
/// | 27 | 2 | i16 |
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bound {
    /// The u32 at 0, meaning not known.
    pub index: u32,
    /// Whether byte 4 is 0xFF rather than 0.
    pub flag: bool,
    /// The three floats at 5.
    pub floats: [f32; 3],
    /// The four half floats at 17, as stored.
    pub halves: [u16; 4],
    /// The i16 at 27, meaning not known.
    pub value: i16,
}

/// A gameplay record, which instances name by index: u8 route count, u8 season mask, u16 × the route count, i32
/// link, and a tag of printable ASCII ended by a zero byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameplayRecord {
    /// The routes.
    pub routes: Vec<u16>,
    /// The seasons an instance with the record is not shown in.
    pub season_exclusion: SeasonMask,
    /// The link, -1 for none.
    pub link: i32,
    /// The tag, empty for none.
    pub tag: String,
}

/// The gameplay section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gameplay {
    /// The two u32 that start the section, meaning not known.
    pub unknown: [u32; 2],
    /// The records.
    pub records: Vec<GameplayRecord>,
    /// The i32 table (the section's third u32 is its length).
    pub values: Vec<i32>,
    /// The byte table, as long.
    pub bytes: Vec<u8>,
}

/// A 12-byte cell record: u32 reference, u32 reference, u8 level, three zero bytes. A reference is a chunk map
/// record, `type << 24 | id`, of type 12.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellRecord {
    /// The first reference.
    pub first: u32,
    /// The second reference.
    pub second: u32,
    /// The level byte.
    pub level: u8,
}

/// A 58-byte transform record.
///
/// | offset | size | field |
/// |---|---|---|
/// | 0 | 2 | u16 animation clip, 0xFFFF for none |
/// | 2 | 2 | u16, the same as at 0 |
/// | 4 | 1 | u8 0 with a LOD profile group, 1 without |
/// | 5 | 12 | f32 position x, y, z |
/// | 17 | 18 | f16 × 9, a 3 × 3 matrix row by row |
/// | 35 | 16 | f32 × 4 tint |
/// | 51 | 4 | u32 0 |
/// | 55 | 1 | u8 material variant |
/// | 56 | 1 | u8 LOD profile group, 0xFF for none |
/// | 57 | 1 | u8 0 with a LOD profile group, 0xFF without |
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    /// The animation clip ([`Tail::clips`]).
    pub clip: Option<u16>,
    /// The position.
    pub position: [f32; 3],
    /// The matrix's half floats, row by row, as stored.
    pub matrix: [u16; 9],
    /// The tint.
    pub tint: [f32; 4],
    /// The material variant byte.
    pub material_variant: u8,
    /// The LOD profile group ([`World::lod_profiles`]).
    pub lod_group: Option<u8>,
}

impl Transform {
    /// The matrix as floats, row by row.
    pub fn matrix_f32(&self) -> [f32; 9] {
        self.matrix.map(half)
    }
}

/// A LOD row: one model of an instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LodRow {
    /// The model number: an index into [`World::models`].
    pub model: u32,
    /// The instance whose row it is.
    pub instance: u32,
}

/// A band of a [`LodProfile`]: f16 distance, f16 fade, u8 0, u8 flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Band {
    /// The distance's half float, as stored.
    pub distance: u16,
    /// The fade's half float, as stored.
    pub fade: u16,
    /// The last byte.
    pub flag: u8,
}

/// A 50-byte LOD profile record: u16 tier, u16 pass, six [`Band`]s, u8 1, u8 flag, f32 1.0, f32 1.0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LodProfile {
    /// The tier.
    pub tier: u16,
    /// The pass.
    pub pass: u16,
    /// The bands.
    pub bands: [Band; 6],
    /// The byte at 41.
    pub flag: u8,
}

/// A 35-byte instance record.
///
/// | offset | size | field |
/// |---|---|---|
/// | 0 | 4 | u32 |
/// | 4 | 2 | u16 flags |
/// | 6 | 2 | u16 |
/// | 8 | 4 | i32 transform |
/// | 12 | 4 | i32 parent instance, -1 for none |
/// | 16 | 1 | u8 1 with a parent, 0xFF without |
/// | 17 | 2 | i16 gameplay record, -1 for none |
/// | 19 | 4 | i32 -1 |
/// | 23 | 4 | u32 first LOD row |
/// | 27 | 6 | u8 × 6: per band, the LOD row drawn counted from the first, 0x7F for none |
/// | 33 | 2 | u16 frame |
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Instance {
    /// The u32 at 0, meaning not known.
    pub unknown_0: u32,
    /// The flags.
    pub flags: u16,
    /// The u16 at 6, meaning not known.
    pub unknown_6: u16,
    /// The transform ([`World::transforms`]).
    pub transform: u32,
    /// The parent instance ([`World::instances`]).
    pub parent: Option<u32>,
    /// The gameplay record ([`Gameplay::records`]).
    pub gameplay: Option<u16>,
    /// The first of the instance's LOD rows; they run to the next instance's first.
    pub first_row: u32,
    /// Per band, the LOD row drawn, counted from the first; [`NO_ROW`] for none.
    pub band_rows: [u8; 6],
    /// The frame ([`World::frames`]).
    pub frame: u16,
}

/// A model bounds record: list of u32, u8 0 or 1, f32 × 3 centre, f16 × 3 half extents, f16 × 9 axes, u16, u8.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelBound {
    /// The u32s, meaning not known.
    pub values: Vec<u32>,
    /// Whether the byte after them is 1 rather than 0.
    pub flag: bool,
    /// The centre.
    pub centre: [f32; 3],
    /// The half extents' half floats, as stored.
    pub half_extent: [u16; 3],
    /// The axes' half floats, row by row, as stored.
    pub axes: [u16; 9],
    /// The u16 after them, meaning not known.
    pub unknown: u16,
    /// The last byte, meaning not known.
    pub last: u8,
}

/// An animation clip file of the tail: u64 hash, string path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipFile {
    /// The hash clip records name it by.
    pub hash: u64,
    /// Its path.
    pub path: String,
}

/// A clip record of the tail: u32 id, u64 clip file hash, u32 × 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipRecord {
    /// The id, meaning not known.
    pub id: u32,
    /// The hash of its [`ClipFile`].
    pub file: u64,
    /// Three u32, meaning not known.
    pub values: [u32; 3],
}

/// What follows the model bounds: string name, u8 × 3, list of clip names, u32 0 × 3, u32 × 3, list (as long as
/// the texture descriptors) of (u32 key, u32 descriptor), u8 0, list of [`ClipFile`]s, list of [`ClipRecord`]s, u32.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tail {
    /// The name.
    pub name: String,
    /// The three bytes after it, meaning not known.
    pub bytes: [u8; 3],
    /// The animation clip names, which [`Transform::clip`] indexes.
    pub clips: Vec<String>,
    /// The three u32 after the zeros, meaning not known.
    pub values: [u32; 3],
    /// Texture keys in increasing order, each with the descriptor it names; every descriptor is named once.
    pub texture_keys: Vec<(u32, u32)>,
    /// The clip files.
    pub clip_files: Vec<ClipFile>,
    /// The clip records.
    pub clip_records: Vec<ClipRecord>,
    /// The last u32, meaning not known.
    pub last: u32,
}

/// The world sections of a track's `.pvs`.
#[derive(Clone, Debug, PartialEq)]
pub struct World {
    /// The megatextures.
    pub megatextures: Megatextures,
    /// The two u32 before the bounds records, meaning not known.
    pub bounds_head: [u32; 2],
    /// The bounds records.
    pub bounds: Vec<Bound>,
    /// The gameplay section.
    pub gameplay: Gameplay,
    /// The index tables, the k-th stored with index k.
    pub index_tables: Vec<Vec<i32>>,
    /// The 49-byte block, as stored.
    pub fixed: [u8; 49],
    /// The pairs after it.
    pub pairs: Vec<[u32; 2]>,
    /// The shaders: hash and path.
    pub shaders: Vec<(u32, String)>,
    /// The materials: hash and path.
    pub materials: Vec<(u32, String)>,
    /// One u32 per model, meaning not known.
    pub models: Vec<u32>,
    /// The cell records.
    pub cells: Vec<CellRecord>,
    /// The dependency lists.
    pub dependencies: Vec<Vec<[u32; 2]>>,
    /// The frames.
    pub frames: Vec<[f32; 12]>,
    /// The transforms.
    pub transforms: Vec<Transform>,
    /// The LOD rows, by instance.
    pub lod_rows: Vec<LodRow>,
    /// The LOD profile groups.
    pub lod_profiles: Vec<Vec<LodProfile>>,
    /// The instances.
    pub instances: Vec<Instance>,
    /// The model bounds, one per model.
    pub model_bounds: Vec<ModelBound>,
    /// The tail.
    pub tail: Tail,
}

impl World {
    /// The LOD rows of the instance at `index`, empty when there is none.
    pub fn rows_of(&self, index: usize) -> &[LodRow] {
        let Some(instance) = self.instances.get(index) else {
            return &[];
        };
        let end = self
            .instances
            .get(index.saturating_add(1))
            .map_or(self.lod_rows.len(), |next| usize_of(next.first_row));
        self.lod_rows
            .get(usize_of(instance.first_row)..end)
            .unwrap_or_default()
    }

    /// An instance's transform (parsing checked that there is one).
    pub fn transform_of(&self, instance: &Instance) -> Option<&Transform> {
        self.transforms.get(usize_of(instance.transform))
    }

    /// The instances whose transform's position (x, z) is in `area`, with their index. Catalogue copies are
    /// included when the area holds their position.
    pub fn instances_in<'a>(&'a self, area: &'a Area) -> impl Iterator<Item = (usize, &'a Instance)> + 'a {
        self.instances.iter().enumerate().filter(move |(_, i)| {
            self.transform_of(i)
                .is_some_and(|t| area.contains(f64::from(t.position[0]), f64::from(t.position[2])))
        })
    }
}

/// A `.pvs` file, as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct Pvs {
    /// The header.
    pub header: Header,
    /// The texture descriptors, as stored (their layout is not known).
    pub textures: Vec<[u8; TEXTURE_LEN]>,
    /// The world sections, when the header has them.
    pub world: Option<World>,
}

impl Pvs {
    /// Reads a whole file, refusing any byte the layout does not account for.
    pub fn parse(bytes: &[u8]) -> Result<Pvs> {
        let mut r = Reader::new(bytes, fail);
        let header = header(&mut r)?;
        let head = if header.world {
            Some(world_head(&mut r)?)
        } else {
            None
        };
        let textures = list(&mut r, TEXTURE_LEN, "texture descriptor count", |r| {
            r.array("texture descriptor")
        })?;
        let world = match head {
            Some(head) => Some(world_rest(&mut r, head, textures.len())?),
            None => None,
        };
        r.end("pvs")?;
        Ok(Pvs {
            header,
            textures,
            world,
        })
    }
}

fn usize_of(v: u32) -> usize {
    // lossless on the 32- and 64-bit targets the crate builds for
    v as usize
}

fn list<T>(
    r: &mut Reader<'_>,
    each_len: usize,
    what: &str,
    mut each: impl FnMut(&mut Reader<'_>) -> Result<T>,
) -> Result<Vec<T>> {
    let n = r.count(each_len, what)?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(each(r)?);
    }
    Ok(out)
}

/// A list whose count must be `expected`.
fn list_of<T>(
    r: &mut Reader<'_>,
    each_len: usize,
    expected: usize,
    what: &str,
    each: impl FnMut(&mut Reader<'_>) -> Result<T>,
) -> Result<Vec<T>> {
    let at = r.position();
    let out = list(r, each_len, what, each)?;
    if out.len() != expected {
        return Err(r.fail_at(at, format_args!("{what}: {}, expected {expected}", out.len())));
    }
    Ok(out)
}

fn u16s<const N: usize>(r: &mut Reader<'_>, what: &str) -> Result<[u16; N]> {
    let mut out = [0; N];
    for v in &mut out {
        *v = r.u16(what)?;
    }
    Ok(out)
}

fn u32s<const N: usize>(r: &mut Reader<'_>, what: &str) -> Result<[u32; N]> {
    let mut out = [0; N];
    for v in &mut out {
        *v = r.u32(what)?;
    }
    Ok(out)
}

fn header(r: &mut Reader<'_>) -> Result<Header> {
    let magic: [u8; 4] = r.array("magic")?;
    if magic != MAGIC {
        return Err(r.fail_at(0, "magic: not SVPF"));
    }
    r.expect_u32(VERSION, "version")?;
    r.expect_u32(8, "header word at 8")?;
    r.expect_u32(0, "header word at 12")?;
    r.expect_u32(0, "header word at 16")?;
    let at = r.position();
    let world = match r.u8("world byte")? {
        0 => false,
        1 => true,
        b => return Err(r.fail_at(at, format_args!("world byte: expected 0 or 1, found {b}"))),
    };
    r.expect_u32(6, "header word at 21")?;
    let unknown_25 = r.u32("header word at 25")?;
    r.expect_u32(0, "header word at 29")?;
    Ok(Header { world, unknown_25 })
}

/// The world sections before the texture descriptors.
struct WorldHead {
    megatextures: Megatextures,
    bounds_head: [u32; 2],
    bounds: Vec<Bound>,
    gameplay: Gameplay,
    index_tables: Vec<Vec<i32>>,
    fixed: [u8; 49],
    pairs: Vec<[u32; 2]>,
}

fn world_head(r: &mut Reader<'_>) -> Result<WorldHead> {
    r.expect_u8(0, "byte before the megatextures")?;
    let entries = list(r, 8, "megatexture count", |r| u32s(r, "megatexture entry"))?;
    let value = r.u32("megatexture word")?;
    r.expect_u8(1, "byte before the megatexture names")?;
    let names = list_of(r, 12, entries.len(), "megatexture name count", |r| {
        Ok((
            r.string("megatexture name")?,
            r.u32("megatexture name word")?,
            r.u32("megatexture name word")?,
        ))
    })?;
    let megatextures = Megatextures {
        entries,
        value,
        names,
    };
    let bounds_head = u32s(r, "bounds head")?;
    let bounds = list(r, BOUND_LEN, "bounds count", bound)?;
    let gameplay = gameplay(r)?;
    r.expect_u32(0, "word before the index tables")?;
    let mut k = 0u32;
    let index_tables = list(r, 8, "index table count", |r| {
        r.expect_u32(k, "index table's index")?;
        k = k.saturating_add(1);
        list(r, 4, "index table length", |r| r.i32("index table entry"))
    })?;
    let fixed = r.array("fixed block")?;
    let pairs = list(r, 8, "pair count", |r| u32s(r, "pair"))?;
    r.expect_u8(0, "byte after the pairs")?;
    Ok(WorldHead {
        megatextures,
        bounds_head,
        bounds,
        gameplay,
        index_tables,
        fixed,
        pairs,
    })
}

fn bound(r: &mut Reader<'_>) -> Result<Bound> {
    let index = r.u32("bound u32")?;
    let at = r.position();
    let flag = match r.u8("bound byte")? {
        0 => false,
        0xFF => true,
        b => return Err(r.fail_at(at, format_args!("bound byte: expected 0 or 0xFF, found {b}"))),
    };
    let floats = r.f32s("bound floats")?;
    let halves = u16s(r, "bound halves")?;
    let at = r.position();
    let zero = r.u16("bound u16")?;
    if zero != 0 {
        return Err(r.fail_at(at, format_args!("bound u16: expected 0, found {zero}")));
    }
    let value = r.i16("bound i16")?;
    Ok(Bound {
        index,
        flag,
        floats,
        halves,
        value,
    })
}

fn gameplay(r: &mut Reader<'_>) -> Result<Gameplay> {
    let unknown = u32s(r, "gameplay head")?;
    let n = usize_of(r.u32("gameplay table length")?);
    let records = list(r, 7, "gameplay record count", |r| {
        let count = r.u8("route count")?;
        let at = r.position();
        let mask = r.u8("gameplay season mask")?;
        let season_exclusion = SeasonMask::new(mask).ok_or_else(|| {
            r.fail_at(
                at,
                format_args!("gameplay season mask {mask:#04x}: the nibbles differ"),
            )
        })?;
        let mut routes = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            routes.push(r.u16("route")?);
        }
        let link = r.i32("gameplay link")?;
        let tag = r.cstring("gameplay tag")?;
        Ok(GameplayRecord {
            routes,
            season_exclusion,
            link,
            tag,
        })
    })?;
    let values = list_of(r, 4, n, "gameplay i32 table length", |r| {
        r.i32("gameplay i32 table entry")
    })?;
    let bytes = list_of(r, 1, n, "gameplay byte table length", |r| {
        r.u8("gameplay byte table entry")
    })?;
    Ok(Gameplay {
        unknown,
        records,
        values,
        bytes,
    })
}

fn hashed_paths(r: &mut Reader<'_>, what: &str) -> Result<Vec<(u32, String)>> {
    list(r, 8, what, |r| Ok((r.u32("hash")?, r.string("path")?)))
}

fn cell_reference(r: &mut Reader<'_>) -> Result<u32> {
    let at = r.position();
    let v = r.u32("cell reference")?;
    let [.., kind] = v.to_le_bytes();
    if kind != CELL_REFERENCE_TYPE {
        return Err(r.fail_at(
            at,
            format_args!("cell reference: type {kind}, expected {CELL_REFERENCE_TYPE}"),
        ));
    }
    Ok(v)
}

fn cell(r: &mut Reader<'_>) -> Result<CellRecord> {
    let first = cell_reference(r)?;
    let second = cell_reference(r)?;
    let level = r.u8("cell level")?;
    let at = r.position();
    let pad: [u8; 3] = r.array("cell padding")?;
    if pad != [0; 3] {
        return Err(r.fail_at(at, "cell padding: not zero"));
    }
    Ok(CellRecord { first, second, level })
}

fn transform(r: &mut Reader<'_>) -> Result<Transform> {
    let at = r.position();
    let clip = r.u16("transform clip")?;
    let again = r.u16("transform clip repeat")?;
    if again != clip {
        return Err(r.fail_at(
            at,
            format_args!("transform clip {clip} and its repeat {again} differ"),
        ));
    }
    let b4 = r.u8("transform byte 4")?;
    let position = r.f32s("transform position")?;
    let matrix = u16s(r, "transform matrix")?;
    let tint = r.f32s("transform tint")?;
    r.expect_u32(0, "transform word at 51")?;
    let material_variant = r.u8("transform material variant")?;
    let group = r.u8("transform LOD profile group")?;
    let b57 = r.u8("transform byte 57")?;
    let lod_group = match (b4, group, b57) {
        (0, 0..=0xFE, 0) => Some(group),
        (1, 0xFF, 0xFF) => None,
        _ => {
            return Err(r.fail_at(
                at,
                format_args!(
                    "transform bytes 4, 56, 57: {b4}, {group}, {b57}, expected 0, a group, 0 or 1, 0xFF, 0xFF"
                ),
            ));
        }
    };
    Ok(Transform {
        clip: (clip != 0xFFFF).then_some(clip),
        position,
        matrix,
        tint,
        material_variant,
        lod_group,
    })
}

fn lod_profile(r: &mut Reader<'_>) -> Result<LodProfile> {
    let tier = r.u16("LOD profile tier")?;
    let pass = r.u16("LOD profile pass")?;
    let mut bands = [Band {
        distance: 0,
        fade: 0,
        flag: 0,
    }; 6];
    for b in &mut bands {
        let distance = r.u16("band distance")?;
        let fade = r.u16("band fade")?;
        r.expect_u8(0, "band zero byte")?;
        let flag = r.u8("band flag")?;
        *b = Band { distance, fade, flag };
    }
    r.expect_u8(1, "LOD profile byte 40")?;
    let flag = r.u8("LOD profile byte 41")?;
    for what in ["LOD profile float at 42", "LOD profile float at 46"] {
        let at = r.position();
        let v = r.f32(what)?;
        if v.to_bits() != 1.0f32.to_bits() {
            return Err(r.fail_at(at, format_args!("{what}: expected 1.0, found {v}")));
        }
    }
    Ok(LodProfile {
        tier,
        pass,
        bands,
        flag,
    })
}

fn instance(r: &mut Reader<'_>) -> Result<Instance> {
    let unknown_0 = r.u32("instance u32")?;
    let flags = r.u16("instance flags")?;
    let unknown_6 = r.u16("instance u16")?;
    let at = r.position();
    let transform = u32::try_from(r.i32("instance transform")?)
        .map_err(|_| r.fail_at(at, "instance transform: negative"))?;
    let at = r.position();
    let parent = r.i32("instance parent")?;
    let has_parent = r.u8("instance parent byte")?;
    let parent = match (parent, has_parent) {
        (-1, 0xFF) => None,
        (0.., 1) => Some(parent.unsigned_abs()),
        _ => {
            return Err(r.fail_at(
                at,
                format_args!(
                    "instance parent {parent} with byte {has_parent}: expected -1 with 0xFF or a parent with 1"
                ),
            ));
        }
    };
    let at = r.position();
    let gameplay = match r.i16("instance gameplay record")? {
        -1 => None,
        g => Some(u16::try_from(g).map_err(|_| r.fail_at(at, format_args!("instance gameplay record {g}")))?),
    };
    let at = r.position();
    let minus = r.i32("instance i32 at 19")?;
    if minus != -1 {
        return Err(r.fail_at(at, format_args!("instance i32 at 19: expected -1, found {minus}")));
    }
    let first_row = r.u32("instance first LOD row")?;
    let band_rows = r.array("instance band rows")?;
    let frame = r.u16("instance frame")?;
    Ok(Instance {
        unknown_0,
        flags,
        unknown_6,
        transform,
        parent,
        gameplay,
        first_row,
        band_rows,
        frame,
    })
}

fn model_bound(r: &mut Reader<'_>) -> Result<ModelBound> {
    let values = list(r, 4, "model bound value count", |r| r.u32("model bound value"))?;
    let at = r.position();
    let flag = match r.u8("model bound byte")? {
        0 => false,
        1 => true,
        b => return Err(r.fail_at(at, format_args!("model bound byte: expected 0 or 1, found {b}"))),
    };
    Ok(ModelBound {
        values,
        flag,
        centre: r.f32s("model bound centre")?,
        half_extent: u16s(r, "model bound half extent")?,
        axes: u16s(r, "model bound axes")?,
        unknown: r.u16("model bound u16")?,
        last: r.u8("model bound last byte")?,
    })
}

fn tail(r: &mut Reader<'_>, textures: usize) -> Result<Tail> {
    let name = r.string("tail name")?;
    let bytes = r.array("tail bytes")?;
    let clips = list(r, 4, "clip name count", |r| r.string("clip name"))?;
    for _ in 0..3 {
        r.expect_u32(0, "tail zero")?;
    }
    let values = u32s(r, "tail words")?;
    let at = r.position();
    let texture_keys = list_of(r, 8, textures, "texture key count", |r| {
        Ok((r.u32("texture key")?, r.u32("texture key descriptor")?))
    })?;
    let mut named = vec![false; textures];
    let mut previous = None;
    for &(key, descriptor) in &texture_keys {
        if previous.is_some_and(|p| p >= key) {
            return Err(r.fail_at(at, format_args!("texture key {key:#x}: keys do not increase")));
        }
        previous = Some(key);
        match named.get_mut(usize_of(descriptor)) {
            Some(seen @ false) => *seen = true,
            Some(true) => {
                return Err(r.fail_at(at, format_args!("texture descriptor {descriptor}: named twice")));
            }
            None => {
                return Err(r.fail_at(
                    at,
                    format_args!("texture descriptor {descriptor}: past {textures}"),
                ));
            }
        }
    }
    r.expect_u8(0, "byte after the texture keys")?;
    let clip_files = list(r, 12, "clip file count", |r| {
        Ok(ClipFile {
            hash: r.u64("clip file hash")?,
            path: r.string("clip file path")?,
        })
    })?;
    let hashes: HashSet<u64> = clip_files.iter().map(|f| f.hash).collect();
    let clip_records = list(r, 24, "clip record count", |r| {
        let id = r.u32("clip record id")?;
        let at = r.position();
        let file = r.u64("clip record file")?;
        if !hashes.contains(&file) {
            return Err(r.fail_at(
                at,
                format_args!("clip record file {file:#x}: no clip file has the hash"),
            ));
        }
        Ok(ClipRecord {
            id,
            file,
            values: u32s(r, "clip record words")?,
        })
    })?;
    let last = r.u32("last word")?;
    Ok(Tail {
        name,
        bytes,
        clips,
        values,
        texture_keys,
        clip_files,
        clip_records,
        last,
    })
}

fn world_rest(r: &mut Reader<'_>, head: WorldHead, textures: usize) -> Result<World> {
    let shaders = hashed_paths(r, "shader count")?;
    let materials = hashed_paths(r, "material count")?;
    let models = list(r, 4, "model count", |r| r.u32("model"))?;
    let cells = list(r, 12, "cell count", cell)?;
    let dependencies = list(r, 4, "dependency count", |r| {
        list(r, 8, "dependency length", |r| u32s(r, "dependency"))
    })?;
    let frames = list(r, 48, "frame count", |r| r.f32s("frame"))?;
    let at = Sections {
        transforms: r.position(),
        rows: 0,
        instances: 0,
    };
    let transforms = list(r, TRANSFORM_LEN, "transform count", transform)?;
    let at = Sections {
        rows: r.position(),
        ..at
    };
    let lod_rows = list(r, 8, "LOD row count", |r| {
        Ok(LodRow {
            model: r.u32("LOD row model")?,
            instance: r.u32("LOD row instance")?,
        })
    })?;
    let lod_profiles = list(r, 4, "LOD profile group count", |r| {
        list(r, LOD_PROFILE_LEN, "LOD profile count", lod_profile)
    })?;
    let at = Sections {
        instances: r.position(),
        ..at
    };
    let instances = list(r, INSTANCE_LEN, "instance count", instance)?;
    r.expect_u32(0, "word before the model bounds")?;
    let model_bounds = list_of(r, 32, models.len(), "model bound count", model_bound)?;
    let tail = tail(r, textures)?;
    let world = World {
        megatextures: head.megatextures,
        bounds_head: head.bounds_head,
        bounds: head.bounds,
        gameplay: head.gameplay,
        index_tables: head.index_tables,
        fixed: head.fixed,
        pairs: head.pairs,
        shaders,
        materials,
        models,
        cells,
        dependencies,
        frames,
        transforms,
        lod_rows,
        lod_profiles,
        instances,
        model_bounds,
        tail,
    };
    check_references(r, &world, at)?;
    Ok(world)
}

/// Where the transform, LOD row and instance lists start (their counts).
#[derive(Clone, Copy)]
struct Sections {
    transforms: usize,
    rows: usize,
    instances: usize,
}

/// The offset of record `k` of a list of `len`-byte records whose count is at `list`.
fn record_at(list: usize, len: usize, k: usize) -> usize {
    list.saturating_add(4).saturating_add(len.saturating_mul(k))
}

/// Every index a transform, LOD row or instance holds is inside what it indexes, and each instance's LOD rows are
/// its own and follow the previous instance's.
fn check_references(r: &Reader<'_>, world: &World, at: Sections) -> Result<()> {
    for (k, t) in world.transforms.iter().enumerate() {
        let fault = |what: std::fmt::Arguments<'_>| {
            r.fail_at(
                record_at(at.transforms, TRANSFORM_LEN, k),
                format_args!("transform {k}: {what}"),
            )
        };
        if let Some(clip) = t.clip
            && usize::from(clip) >= world.tail.clips.len()
        {
            return Err(fault(format_args!("clip {clip} of {}", world.tail.clips.len())));
        }
        if let Some(group) = t.lod_group
            && usize::from(group) >= world.lod_profiles.len()
        {
            return Err(fault(format_args!(
                "LOD profile group {group} of {}",
                world.lod_profiles.len()
            )));
        }
    }
    for (k, row) in world.lod_rows.iter().enumerate() {
        if usize_of(row.model) >= world.models.len() {
            return Err(r.fail_at(
                record_at(at.rows, 8, k),
                format_args!("LOD row {k}: model {} of {}", row.model, world.models.len()),
            ));
        }
    }
    let mut next_row = 0usize;
    for (k, i) in world.instances.iter().enumerate() {
        let fault = |what: std::fmt::Arguments<'_>| {
            r.fail_at(
                record_at(at.instances, INSTANCE_LEN, k),
                format_args!("instance {k}: {what}"),
            )
        };
        if usize_of(i.transform) >= world.transforms.len() {
            return Err(fault(format_args!(
                "transform {} of {}",
                i.transform,
                world.transforms.len()
            )));
        }
        if let Some(p) = i.parent
            && usize_of(p) >= world.instances.len()
        {
            return Err(fault(format_args!("parent {p} of {}", world.instances.len())));
        }
        if let Some(g) = i.gameplay
            && usize::from(g) >= world.gameplay.records.len()
        {
            return Err(fault(format_args!(
                "gameplay record {g} of {}",
                world.gameplay.records.len()
            )));
        }
        if usize::from(i.frame) >= world.frames.len() {
            return Err(fault(format_args!("frame {} of {}", i.frame, world.frames.len())));
        }
        if usize_of(i.first_row) != next_row {
            return Err(fault(format_args!(
                "first LOD row {}, expected {next_row}",
                i.first_row
            )));
        }
        let end = world
            .instances
            .get(k.saturating_add(1))
            .map_or(world.lod_rows.len(), |next| usize_of(next.first_row));
        let Some(rows) = world.lod_rows.get(next_row..end) else {
            return Err(fault(format_args!(
                "LOD rows {next_row}..{end} of {}",
                world.lod_rows.len()
            )));
        };
        if rows.iter().any(|row| usize_of(row.instance) != k) {
            return Err(fault(format_args!(
                "a LOD row from {next_row} is another instance's"
            )));
        }
        if let Some(b) = i
            .band_rows
            .iter()
            .find(|&&b| b != NO_ROW && usize::from(b) >= rows.len())
        {
            return Err(fault(format_args!("band row {b} of {}", rows.len())));
        }
        next_row = end;
    }
    if next_row != world.lod_rows.len() {
        return Err(r.fail_at(
            at.rows,
            format_args!(
                "LOD rows: {} stored, the instances have {next_row}",
                world.lod_rows.len()
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "tests on records built by hand"
)]
mod tests {
    use super::*;

    fn header_bytes(world: u8) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"SVPF");
        for v in [VERSION, 8, 0, 0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(world);
        for v in [6u32, 0, 0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b
    }

    #[test]
    fn texture_only_file() {
        let mut b = header_bytes(0);
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&[7; TEXTURE_LEN]);
        let p = Pvs::parse(&b).unwrap();
        assert!(!p.header.world);
        assert_eq!(p.textures, vec![[7; TEXTURE_LEN]]);
        assert!(p.world.is_none());
        for len in 0..b.len() {
            assert!(Pvs::parse(&b[..len]).is_err(), "truncated at {len}");
        }
        b.push(0);
        assert!(Pvs::parse(&b).is_err());
    }

    #[test]
    fn another_version_is_refused() {
        // the home space's file starts `SVPF`, 0x63, 2, 0, 0
        let mut b = header_bytes(0);
        b[4] = 0x63;
        b.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            Pvs::parse(&b).unwrap_err().to_string(),
            "pvs: version: expected 164, found 99 at 0x4"
        );
    }

    #[test]
    fn header_constants() {
        let mut b = header_bytes(2);
        b.extend_from_slice(&0u32.to_le_bytes());
        let e = Pvs::parse(&b).unwrap_err().to_string();
        assert!(e.contains("world byte") && e.contains("0x14"), "{e}");
        let mut b = header_bytes(0);
        b.extend_from_slice(&0u32.to_le_bytes());
        b[8] = 9;
        assert!(
            Pvs::parse(&b)
                .unwrap_err()
                .to_string()
                .contains("header word at 8")
        );
    }

    #[test]
    fn transform_bytes() {
        let mut t = vec![0xFF, 0xFF, 0xFF, 0xFF, 0];
        t.extend_from_slice(&[0; 12 + 18 + 16 + 4]);
        t.extend_from_slice(&[3, 7, 0]);
        let parsed = transform(&mut Reader::new(&t, fail)).unwrap();
        assert_eq!(
            (parsed.clip, parsed.lod_group, parsed.material_variant),
            (None, Some(7), 3)
        );
        let last = t.len() - 1;
        t[last] = 0xFF;
        assert!(transform(&mut Reader::new(&t, fail)).is_err());
        t[4] = 1;
        t[last - 1] = 0xFF;
        assert_eq!(transform(&mut Reader::new(&t, fail)).unwrap().lod_group, None);
        t[2] = 0;
        assert!(
            transform(&mut Reader::new(&t, fail))
                .unwrap_err()
                .to_string()
                .contains("repeat")
        );
    }
}
