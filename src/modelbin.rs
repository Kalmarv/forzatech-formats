//! Models (`.modelbin`): a [Grub bundle](crate::bundle) whose blobs are a model's header (`Modl`), skeleton (`Skel`),
//! material instances (`MatI`), meshes (`Mesh`), vertex and instance layouts (`VLay`, `ILay`), index, vertex, morph
//! and skin buffers (`IndB`, `VerB`, `MBuf`, `Skin`) and morph target names (`Mrph`).
//!
//! [`Model::parse`] reads every blob of a model and checks every reference between them; [`Model::mesh_data`] decodes
//! one mesh's indices and vertices. Nothing is chosen for the caller: no level of detail is picked, no axis or unit is
//! changed, and a value the file does not hold is not made up.
//!
//! All numbers are little-endian. Each blob's metadata records are given with it; `Name` is UTF-8 text, `Id  ` a u32,
//! `BBox` six f32 (the minimum x, y, z, then the maximum).
//!
//! # `Modl` (versions 1.4 and 1.5), metadata `BBox`
//!
//! | offset | size | field |
//! |---|---|---|
//! | 0 | 2 | mesh count (u16): the `Mesh` blobs |
//! | 2 | 2 | buffer count (u16): the `IndB`, `VerB`, `MBuf` and `Skin` blobs |
//! | 4 | 2 | vertex layout count (u16): the `VLay` blobs |
//! | 6 | 2 | material count (u16): the `MatI` blobs |
//! | 8 | 1 | has levels of detail (u8; 0 or 1) |
//! | 9 | 1 | first level of detail (i8) |
//! | 10 | 1 | last level of detail (i8) |
//! | 11 | 1 | reserved (u8; 0) |
//! | 12 | 2 | level-of-detail flags (u16) |
//! | 14 | 1 | decompress flags (u8) |
//! | 15 | 1 | reserved (u8; 0) |
//! | 16 | 1 | retain CPU buffer copy (u8) |
//! | 17 | 1 | visibility flags (u8) |
//! | 18 | 4 | version 1.5: variant count *v* (u32), then *v* rows of one u8 per mesh: the material group each mesh uses |
//!
//! # `Skel` (version 1.0)
//!
//! A bone count (u16), then per bone its name (u32 length, then that many bytes), parent, first child and next sibling
//! (three i16; -1 for none) and a 4×4 matrix (16 f32, in stored order); then a u32 ([`Skeleton::trailing`]).
//!
//! # `MatI` (version 1.0), metadata `Name`, `Id  `
//!
//! Either nothing (a material slot with no instance) or a Grub bundle, the material instance, returned as a
//! [`Bundle`] for the materials reader.
//!
//! # `Mesh` (versions 1.12 and 1.13), metadata `Name`, `BBox`
//!
//! | size | field |
//! |---|---|
//! | 4 | version 1.13: material group count *g* (i32); version 1.12 has one group |
//! | 8 × *g* | per group, four material ids (i16; a `MatI` id, or -1) |
//! | 2 | rigid bone index (i16) |
//! | 2 | level-of-detail flags (u16) |
//! | 1 + 1 | level of detail 1 and 2 (u8 each) |
//! | 2 | bucket flags (u16) |
//! | 1 | bucket order (u8) |
//! | 1 | skinning elements per vertex (u8) |
//! | 4 | morph target count (u32) |
//! | 1 | is a morph for damage (u8; 0 or 1) |
//! | 1 | 32-bit indices (u8; 0 or 1) |
//! | 2 | primitive topology (u16) |
//! | 4 | index buffer (i32): the `IndB` id |
//! | 4 | index buffer offset (i32), in bytes |
//! | 4 | start index (i32), in indices from the offset |
//! | 4 | base vertex (i32), added to every index |
//! | 4 | index count (i32) |
//! | 4 | primitive count (i32) |
//! | 4 | average cache miss ratio (f32) |
//! | 4 | referenced vertex count (u32) |
//! | 4 + 4 × *n* | referenced vertices: a count (u32), then that many u32 |
//! | 4 | vertex layout (i32): the `VLay` id |
//! | 4 + 20 × *n* | vertex buffers: a count (i32), then per buffer its `VerB` id (i32), input slot, stride, offset in bytes and a reserved u32 (u32 each; the reserved one 0) |
//! | 4 | morph buffer (i32): the `MBuf` id, or -1 |
//! | 4 | skinning buffer (i32): the `Skin` id, or -1 |
//! | 4 + 4 × *n* | constant buffers: a count (i32), then that many i32 |
//! | 4 | source mesh index (u32) |
//! | 80 | five texture coordinate transforms, each (u offset, u scale, v offset, v scale) (f32) |
//! | 16 | position scale (four f32) |
//! | 16 | position offset (four f32) |
//!
//! # `VLay` and `ILay` (version 1.1), metadata `Id  `
//!
//! An instance layout (`ILay`) is stored as a vertex layout is. A semantic name count (u16), the names (each a u32
//! length and that many bytes), an element count (u16), then per element 20
//! bytes:
//!
//! | offset | size | field |
//! |---|---|---|
//! | 0 | 2 | semantic name (i16): an index into the names |
//! | 2 | 2 | semantic index (i16) |
//! | 4 | 4 | input slot (u32) |
//! | 8 | 4 | DXGI format (u32) |
//! | 12 | 4 | byte offset in the vertex (i32; -1 to append to the slot's previous elements) |
//! | 16 | 4 | instance data step rate (u32) |
//!
//! then one u32 per element ([`VertexLayout::packed_formats`]) and a u32 of flags.
//!
//! # `IndB` (version 1.0), `VerB`, `MBuf`, `Skin` (version 1.1), metadata `Id  `
//!
//! | offset | size | field |
//! |---|---|---|
//! | 0 | 4 | element count (u32) |
//! | 4 | 4 | size in bytes (u32): the element count times the stride |
//! | 8 | 2 | stride (u16) |
//! | 10 | 1 | values per element (u8) |
//! | 11 | 1 | reserved (u8; 0) |
//! | 12 | 4 | DXGI format (u32) |
//! | 16 | size | the elements |
//!
//! # `Mrph` (version 0.0), no metadata
//!
//! See [`MorphTargets`].
//!
//! # What is refused
//!
//! Any other blob tag or version; metadata other than each blob's own, or missing; a blob whose fields do not end
//! exactly at its end; a reserved byte that is not 0; a flag byte other than 0 or 1; text that is not UTF-8; a count
//! that does not match the blobs; two blobs of a kind with one id; and any reference to a blob, a material, a vertex
//! layout's name or a variant's material group that does not exist. [`Model::mesh_data`] refuses a topology other than
//! a triangle list, an index or vertex outside its buffer, a vertex format [`primase_foundations::vertex`] does not
//! decode, and an input slot no vertex buffer is bound to.
//!
//! What has been checked, and how: see `tests/modelbin.rs`.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::bundle::{Blob, Bundle, Tag, Version, is_bundle};
use crate::{Error, Result};
use primase_foundations::bytes::{Cursor, Endian::Little as LE, Field, FieldError};
use primase_foundations::vertex::{self, Element as VertexElement, Format, ResolvedElement, Stream};
use std::fmt;

/// The model header's tag.
pub const MODL: Tag = Tag::new(b"Modl");
/// The skeleton's tag.
pub const SKEL: Tag = Tag::new(b"Skel");
/// A material instance's tag.
pub const MATI: Tag = Tag::new(b"MatI");
/// A mesh's tag.
pub const MESH: Tag = Tag::new(b"Mesh");
/// A vertex layout's tag.
pub const VLAY: Tag = Tag::new(b"VLay");
/// An index buffer's tag.
pub const INDB: Tag = Tag::new(b"IndB");
/// A vertex buffer's tag.
pub const VERB: Tag = Tag::new(b"VerB");
/// A morph buffer's tag.
pub const MBUF: Tag = Tag::new(b"MBuf");
/// A skinning buffer's tag.
pub const SKIN: Tag = Tag::new(b"Skin");
/// A morph target's tag.
pub const MRPH: Tag = Tag::new(b"Mrph");
/// The name metadata's tag.
pub const NAME: Tag = Tag::new(b"Name");
/// The id metadata's tag.
pub const ID: Tag = Tag::new(b"Id  ");
/// The bounding box metadata's tag.
pub const BBOX: Tag = Tag::new(b"BBox");
/// The texture reference metadata's tag.
pub const TREF: Tag = Tag::new(b"TRef");
/// An instance layout's tag.
pub const ILAY: Tag = Tag::new(b"ILay");

/// The primitive topology of a triangle list (`D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST`), the one decoded.
pub const TRIANGLE_LIST: u16 = 4;
/// The DXGI format of 16-bit indices (`DXGI_FORMAT_R16_UINT`).
pub const R16_UINT: u32 = 57;
/// The DXGI format of 32-bit indices (`DXGI_FORMAT_R32_UINT`).
pub const R32_UINT: u32 = 42;

const fn v(major: u8, minor: u8) -> Version {
    Version { major, minor }
}

fn fail(what: impl fmt::Display) -> Error {
    Error::ModelBin(what.to_string())
}

fn usize_of(v: u32) -> usize {
    // u32 to usize is lossless on the targets this crate builds for (32- and 64-bit).
    v as usize
}

/// An axis-aligned box, as a `BBox` record stores it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundingBox {
    /// The smallest x, y and z.
    pub min: [f32; 3],
    /// The largest x, y and z.
    pub max: [f32; 3],
}

/// Reads one blob's data in order, its errors naming the blob, the field and the offset in the bundle.
struct Reader<'a> {
    c: Cursor<'a>,
    origin: usize,
    what: String,
}

macro_rules! read {
    ($($name:ident: $t:ty),*) => {
        $(
            fn $name(&mut self, field: &'static str) -> Result<$t> {
                let r = self.c.$name(LE).field(field);
                r.map_err(|e| self.bounds(e))
            }
        )*
    };
}

impl<'a> Reader<'a> {
    fn new(blob: &Blob<'a>, what: String) -> Reader<'a> {
        let origin = usize_of(blob.data_offset);
        Reader {
            c: Cursor::with_origin(blob.data, origin),
            origin,
            what,
        }
    }

    fn at(&self) -> usize {
        self.origin.saturating_add(self.c.position())
    }

    fn bounds(&self, e: FieldError) -> Error {
        fail(format!("{}: {e}", self.what))
    }

    fn refuse(&self, at: usize, what: impl fmt::Display) -> Error {
        fail(format!("{} at {at:#x}: {what}", self.what))
    }

    read!(u16: u16, i16: i16, u32: u32, i32: i32, f32: f32);

    fn u8(&mut self, field: &'static str) -> Result<u8> {
        let r = self.c.u8().field(field);
        r.map_err(|e| self.bounds(e))
    }

    fn i8(&mut self, field: &'static str) -> Result<i8> {
        let r = self.c.i8().field(field);
        r.map_err(|e| self.bounds(e))
    }

    fn f32s<const N: usize>(&mut self, field: &'static str) -> Result<[f32; N]> {
        let mut out = [0f32; N];
        for o in &mut out {
            *o = self.f32(field)?;
        }
        Ok(out)
    }

    fn flag(&mut self, field: &'static str) -> Result<bool> {
        let at = self.at();
        match self.u8(field)? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(self.refuse(at, format!("{field} is {other}, not 0 or 1"))),
        }
    }

    fn zero_u8(&mut self, field: &'static str) -> Result<()> {
        let at = self.at();
        match self.u8(field)? {
            0 => Ok(()),
            other => Err(self.refuse(at, format!("{field} is {other:#x}, not 0"))),
        }
    }

    fn zero_u32(&mut self, field: &'static str) -> Result<()> {
        let at = self.at();
        match self.u32(field)? {
            0 => Ok(()),
            other => Err(self.refuse(at, format!("{field} is {other:#x}, not 0"))),
        }
    }

    /// A count (u32 or a non-negative i32) of records of `each` bytes that the rest of the blob can hold.
    fn count(&mut self, field: &'static str, each: usize, signed: bool) -> Result<usize> {
        let at = self.at();
        let n = if signed {
            let n = self.i32(field)?;
            u32::try_from(n).map_err(|_| self.refuse(at, format!("{field} is {n}, below 0")))?
        } else {
            self.u32(field)?
        };
        let n = usize_of(n);
        if n.checked_mul(each).is_none_or(|len| len > self.c.remaining()) {
            return Err(self.refuse(
                at,
                format!(
                    "{field} is {n}, records of {each} bytes past the blob's end ({} bytes left)",
                    self.c.remaining()
                ),
            ));
        }
        Ok(n)
    }

    /// Text stored as a u32 length and that many bytes of UTF-8.
    fn text(&mut self, field: &'static str) -> Result<String> {
        let n = self.count(field, 1, false)?;
        let at = self.at();
        let r = self.c.take(n).field(field);
        let bytes = r.map_err(|e| self.bounds(e))?;
        String::from_utf8(bytes.to_vec())
            .map_err(|_| self.refuse(at, format!("{field} is not UTF-8: {bytes:02x?}")))
    }

    /// Every byte of the blob has been read.
    fn end(&self) -> Result<()> {
        match self.c.remaining() {
            0 => Ok(()),
            n => Err(self.refuse(
                self.at(),
                format!("{n} bytes after the last field, where the blob should end"),
            )),
        }
    }
}

/// The metadata a blob carries, each record read; which ones a blob must carry is the blob's reader's.
#[derive(Default)]
struct Meta {
    name: Option<String>,
    id: Option<u32>,
    bbox: Option<BoundingBox>,
    texture_refs: Option<Vec<u32>>,
}

impl Meta {
    fn need<T>(v: Option<T>, what: &str, tag: Tag) -> Result<T> {
        v.ok_or_else(|| fail(format!("{what}: no {tag} metadata")))
    }
    fn name(&mut self, what: &str) -> Result<String> {
        Meta::need(self.name.take(), what, NAME)
    }
    fn id(&self, what: &str) -> Result<u32> {
        Meta::need(self.id, what, ID)
    }
    fn bbox(&self, what: &str) -> Result<BoundingBox> {
        Meta::need(self.bbox, what, BBOX)
    }
}

fn version(blob: &Blob<'_>, what: &str, versions: &[Version]) -> Result<()> {
    if versions.contains(&blob.version) {
        Ok(())
    } else {
        let read: Vec<String> = versions.iter().map(ToString::to_string).collect();
        Err(fail(format!(
            "{what}: version {}, not {}",
            blob.version,
            read.join(" or ")
        )))
    }
}

/// The blob's metadata, whose tags must be one of the `allowed` sequences, in that order, each record version 0.
fn metadata(blob: &Blob<'_>, what: &str, allowed: &[&[Tag]]) -> Result<Meta> {
    let tags: Vec<Tag> = blob.metadata.iter().map(|m| m.tag).collect();
    if !allowed.contains(&tags.as_slice()) {
        let show = |t: &[Tag]| {
            format!(
                "[{}]",
                t.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
            )
        };
        let allowed: Vec<String> = allowed.iter().map(|a| show(a)).collect();
        return Err(fail(format!(
            "{what}: metadata {}, expected {}",
            show(&tags),
            allowed.join(" or ")
        )));
    }
    let mut meta = Meta::default();
    for m in &blob.metadata {
        let at = m.offset;
        if m.version != 0 {
            return Err(fail(format!(
                "{what}: metadata {} at {at:#x} is version {}, not 0",
                m.tag, m.version
            )));
        }
        let mut c = Cursor::with_origin(m.payload, usize_of(at));
        let field = |e: FieldError| fail(format!("{what}: metadata {}: {e}", m.tag));
        if m.tag == NAME {
            meta.name = Some(String::from_utf8(m.payload.to_vec()).map_err(|_| {
                fail(format!(
                    "{what}: the name at {at:#x} is not UTF-8: {:02x?}",
                    m.payload
                ))
            })?);
        } else if m.tag == ID {
            meta.id = Some(c.u32(LE).field("the id").map_err(field)?);
        } else if m.tag == BBOX {
            let mut f = [0f32; 6];
            for x in &mut f {
                *x = c.f32(LE).field("a bound").map_err(field)?;
            }
            let [a, b, cc, d, e, g] = f;
            meta.bbox = Some(BoundingBox {
                min: [a, b, cc],
                max: [d, e, g],
            });
        } else if m.tag == TREF {
            let n = c.u32(LE).field("the count").map_err(field)?;
            let mut refs = Vec::with_capacity(usize_of(n).min(c.remaining()));
            for _ in 0..n {
                refs.push(c.u32(LE).field("a reference").map_err(field)?);
            }
            meta.texture_refs = Some(refs);
        }
        if c.remaining() != 0 && m.tag != NAME {
            return Err(fail(format!(
                "{what}: metadata {} at {at:#x} is {} bytes, {} more than its fields",
                m.tag,
                m.payload.len(),
                c.remaining()
            )));
        }
    }
    Ok(meta)
}

/// The model header (`Modl`), as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    /// The blob's version (1.4 or 1.5).
    pub version: Version,
    /// The number of `Mesh` blobs.
    pub mesh_count: u16,
    /// The number of buffer blobs (`IndB`, `VerB`, `MBuf` and `Skin`).
    pub buffer_count: u16,
    /// The number of `VLay` blobs.
    pub layout_count: u16,
    /// The number of `MatI` blobs.
    pub material_count: u16,
    /// Whether the model is one level of detail of several.
    pub has_lod: bool,
    /// The first level of detail (byte 9).
    pub min_lod: i8,
    /// The last level of detail (byte 10).
    pub max_lod: i8,
    /// The level-of-detail flags.
    pub lod_flags: u16,
    /// The decompress flags.
    pub decompress_flags: u8,
    /// Whether the CPU keeps a copy of the buffers (byte 16).
    pub retain_cpu_copy: u8,
    /// The visibility flags.
    pub visibility_flags: u8,
    /// Version 1.5's material variants: one row per variant, one material group per mesh (in `Mesh` blob order) in
    /// each row. Empty for version 1.4, and for a 1.5 header that states no variant.
    pub variants: Vec<Vec<u8>>,
    /// The `BBox` metadata.
    pub bounds: BoundingBox,
    /// The `TRef` metadata, when the blob has it: a u32 count, then that many u32 (what they name is not known).
    pub texture_refs: Option<Vec<u32>>,
}

impl Header {
    /// The `Modl` blob read. Refused: another version, other metadata, a has-LOD byte other than 0 or 1, a reserved
    /// byte other than 0, and a variant table that does not end at the blob's end.
    pub fn parse(blob: &Blob<'_>) -> Result<Header> {
        let what = "Modl";
        version(blob, what, &[v(1, 4), v(1, 5)])?;
        let meta = metadata(blob, what, &[&[BBOX], &[BBOX, TREF]])?;
        let mut r = Reader::new(blob, what.to_owned());
        let mut h = Header {
            version: blob.version,
            mesh_count: r.u16("the mesh count")?,
            buffer_count: r.u16("the buffer count")?,
            layout_count: r.u16("the vertex layout count")?,
            material_count: r.u16("the material count")?,
            has_lod: r.flag("the has-LOD byte")?,
            min_lod: r.i8("the first LOD")?,
            max_lod: r.i8("the last LOD")?,
            lod_flags: {
                r.zero_u8("byte 11")?;
                r.u16("the LOD flags")?
            },
            decompress_flags: r.u8("the decompress flags")?,
            retain_cpu_copy: {
                r.zero_u8("byte 15")?;
                r.u8("the retain-CPU-copy byte")?
            },
            visibility_flags: r.u8("the visibility flags")?,
            variants: Vec::new(),
            bounds: meta.bbox(what)?,
            texture_refs: meta.texture_refs,
        };
        if blob.version == v(1, 5) {
            let meshes = usize::from(h.mesh_count);
            let count = r.count("the variant count", meshes, false)?;
            for _ in 0..count {
                let row = r.c.take(meshes).field("a variant row");
                h.variants.push(row.map_err(|e| r.bounds(e))?.to_vec());
            }
        }
        r.end()?;
        Ok(h)
    }
}

/// A bone of the skeleton.
#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    /// The bone's name.
    pub name: String,
    /// The parent bone, or -1.
    pub parent: i16,
    /// The first child bone, or -1.
    pub first_child: i16,
    /// The next bone with the same parent, or -1.
    pub next_sibling: i16,
    /// The 4×4 matrix, its 16 values in stored order.
    pub matrix: [f32; 16],
}

/// The skeleton (`Skel`), as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct Skeleton {
    /// The bones, in stored order.
    pub bones: Vec<Bone>,
    /// The u32 after the last bone.
    pub trailing: u32,
}

impl Skeleton {
    /// The `Skel` blob read. Refused: another version, any metadata, a name that is not UTF-8, a parent, child or
    /// sibling that is neither -1 nor a bone, and bytes after the trailing u32.
    pub fn parse(blob: &Blob<'_>) -> Result<Skeleton> {
        let what = "Skel";
        version(blob, what, &[v(1, 0)])?;
        metadata(blob, what, &[&[]])?;
        let mut r = Reader::new(blob, what.to_owned());
        let count = r.u16("the bone count")?;
        let mut bones = Vec::with_capacity(usize::from(count).min(r.c.remaining()));
        for _ in 0..count {
            let name = r.text("a bone's name")?;
            let at = r.at();
            let bone = Bone {
                name,
                parent: r.i16("a bone's parent")?,
                first_child: r.i16("a bone's first child")?,
                next_sibling: r.i16("a bone's next sibling")?,
                matrix: r.f32s("a bone's matrix")?,
            };
            for link in [bone.parent, bone.first_child, bone.next_sibling] {
                if link != -1 && u16::try_from(link).is_ok_and(|l| l >= count) || link < -1 {
                    return Err(r.refuse(
                        at,
                        format!("bone {:?} links to {link}, of {count} bones", bone.name),
                    ));
                }
            }
            bones.push(bone);
        }
        let trailing = r.u32("the u32 after the bones")?;
        r.end()?;
        Ok(Skeleton { bones, trailing })
    }
}

/// A material instance (`MatI`): its id, its name and the instance itself, a bundle of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Material<'a> {
    /// The `Id  ` metadata: what a mesh's material ids refer to.
    pub id: u32,
    /// The `Name` metadata.
    pub name: String,
    /// The material instance, or `None` for a blob with no data (a slot with no instance).
    pub instance: Option<Bundle<'a>>,
}

impl<'a> Material<'a> {
    /// The `MatI` blob read. Refused: another version or metadata, and data that is neither empty nor a bundle
    /// [`Bundle::parse`] reads.
    pub fn parse(blob: &Blob<'a>) -> Result<Material<'a>> {
        let what = "MatI";
        version(blob, what, &[v(1, 0)])?;
        let mut meta = metadata(blob, what, &[&[NAME, ID]])?;
        let name = meta.name(what)?;
        let id = meta.id(what)?;
        let instance = if blob.data.is_empty() {
            None
        } else if is_bundle(blob.data) {
            Some(Bundle::parse(blob.data).map_err(|e| fail(format!("MatI {name:?}: the instance: {e}")))?)
        } else {
            return Err(fail(format!(
                "MatI at {:#x}: {} bytes that are not a bundle",
                blob.data_offset,
                blob.data.len()
            )));
        };
        Ok(Material { id, name, instance })
    }
}

/// A vertex buffer a mesh binds to an input slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexBufferRef {
    /// The `VerB` id.
    pub buffer: i32,
    /// The input slot.
    pub slot: u32,
    /// The stride in bytes.
    pub stride: u32,
    /// The offset in bytes of the first vertex in the buffer's elements.
    pub offset: u32,
}

/// A mesh (`Mesh`): one draw of the model, as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    /// The blob's version (1.12 or 1.13).
    pub version: Version,
    /// The `Name` metadata.
    pub name: String,
    /// The `BBox` metadata.
    pub bounds: BoundingBox,
    /// The material groups: four material ids each (a `MatI` id, or -1). Version 1.12 stores one group.
    pub material_groups: Vec<[i16; 4]>,
    /// The rigid bone index.
    pub rigid_bone: i16,
    /// The level-of-detail flags.
    pub lod_flags: u16,
    /// Level of detail 1.
    pub lod_level1: u8,
    /// Level of detail 2.
    pub lod_level2: u8,
    /// The bucket flags.
    pub bucket_flags: u16,
    /// The bucket order.
    pub bucket_order: u8,
    /// The skinning elements per vertex.
    pub skinning_elements: u8,
    /// The morph target count.
    pub morph_targets: u32,
    /// Whether the morph is for damage.
    pub morph_damage: bool,
    /// Whether the indices are 32-bit.
    pub index_32bit: bool,
    /// The primitive topology (`D3D_PRIMITIVE_TOPOLOGY`).
    pub topology: u16,
    /// The `IndB` id.
    pub index_buffer: i32,
    /// The offset in bytes of the mesh's index range in the index buffer's elements.
    pub index_offset: i32,
    /// The first index, counted from the offset.
    pub start_index: i32,
    /// The base vertex, added to every index.
    pub base_vertex: i32,
    /// The index count.
    pub index_count: i32,
    /// The primitive count.
    pub primitive_count: i32,
    /// The average cache miss ratio.
    pub acmr: f32,
    /// The referenced vertex count.
    pub referenced_vertex_count: u32,
    /// The referenced vertices.
    pub referenced_vertices: Vec<u32>,
    /// The `VLay` id.
    pub layout: i32,
    /// The vertex buffers, in stored order.
    pub vertex_buffers: Vec<VertexBufferRef>,
    /// The `MBuf` id, or -1.
    pub morph_buffer: i32,
    /// The `Skin` id, or -1.
    pub skinning_buffer: i32,
    /// The constant buffers.
    pub constant_buffers: Vec<i32>,
    /// The source mesh index.
    pub source_mesh: u32,
    /// Five texture coordinate transforms: (u offset, u scale, v offset, v scale).
    pub uv_transforms: [[f32; 4]; 5],
    /// The position scale.
    pub position_scale: [f32; 4],
    /// The position offset.
    pub position_offset: [f32; 4],
}

impl Mesh {
    /// The `Mesh` blob read. Refused: another version or metadata, a negative group or buffer count, a flag byte
    /// other than 0 or 1, a reserved u32 other than 0, and bytes after the position offset.
    pub fn parse(blob: &Blob<'_>) -> Result<Mesh> {
        let what = "Mesh";
        version(blob, what, &[v(1, 12), v(1, 13)])?;
        let mut meta = metadata(blob, what, &[&[NAME, BBOX]])?;
        let name = meta.name(what)?;
        let bounds = meta.bbox(what)?;
        let mut r = Reader::new(blob, format!("Mesh {name:?}"));
        let groups = if blob.version == v(1, 13) {
            r.count("the material group count", 8, true)?
        } else {
            1
        };
        let mut material_groups = Vec::with_capacity(groups);
        for _ in 0..groups {
            let mut g = [0i16; 4];
            for id in &mut g {
                *id = r.i16("a material id")?;
            }
            material_groups.push(g);
        }
        let rigid_bone = r.i16("the rigid bone")?;
        let lod_flags = r.u16("the LOD flags")?;
        let lod_level1 = r.u8("LOD level 1")?;
        let lod_level2 = r.u8("LOD level 2")?;
        let bucket_flags = r.u16("the bucket flags")?;
        let bucket_order = r.u8("the bucket order")?;
        let skinning_elements = r.u8("the skinning element count")?;
        let morph_targets = r.u32("the morph target count")?;
        let morph_damage = r.flag("the morph-damage byte")?;
        let index_32bit = r.flag("the 32-bit-indices byte")?;
        let topology = r.u16("the topology")?;
        let index_buffer = r.i32("the index buffer")?;
        let index_offset = r.i32("the index buffer offset")?;
        let start_index = r.i32("the start index")?;
        let base_vertex = r.i32("the base vertex")?;
        let index_count = r.i32("the index count")?;
        let primitive_count = r.i32("the primitive count")?;
        let acmr = r.f32("the ACMR")?;
        let referenced_vertex_count = r.u32("the referenced vertex count")?;
        let n = r.count("the referenced vertex list's count", 4, false)?;
        let mut referenced_vertices = Vec::with_capacity(n);
        for _ in 0..n {
            referenced_vertices.push(r.u32("a referenced vertex")?);
        }
        let layout = r.i32("the vertex layout")?;
        let n = r.count("the vertex buffer count", 20, true)?;
        let mut vertex_buffers = Vec::with_capacity(n);
        for _ in 0..n {
            vertex_buffers.push(VertexBufferRef {
                buffer: r.i32("a vertex buffer's id")?,
                slot: r.u32("a vertex buffer's slot")?,
                stride: r.u32("a vertex buffer's stride")?,
                offset: r.u32("a vertex buffer's offset")?,
            });
            r.zero_u32("a vertex buffer's reserved u32")?;
        }
        let morph_buffer = r.i32("the morph buffer")?;
        let skinning_buffer = r.i32("the skinning buffer")?;
        let n = r.count("the constant buffer count", 4, true)?;
        let mut constant_buffers = Vec::with_capacity(n);
        for _ in 0..n {
            constant_buffers.push(r.i32("a constant buffer")?);
        }
        let source_mesh = r.u32("the source mesh index")?;
        let mut uv_transforms = [[0f32; 4]; 5];
        for t in &mut uv_transforms {
            *t = r.f32s("a texture coordinate transform")?;
        }
        let position_scale = r.f32s("the position scale")?;
        let position_offset = r.f32s("the position offset")?;
        r.end()?;
        Ok(Mesh {
            version: blob.version,
            name,
            bounds,
            material_groups,
            rigid_bone,
            lod_flags,
            lod_level1,
            lod_level2,
            bucket_flags,
            bucket_order,
            skinning_elements,
            morph_targets,
            morph_damage,
            index_32bit,
            topology,
            index_buffer,
            index_offset,
            start_index,
            base_vertex,
            index_count,
            primitive_count,
            acmr,
            referenced_vertex_count,
            referenced_vertices,
            layout,
            vertex_buffers,
            morph_buffer,
            skinning_buffer,
            constant_buffers,
            source_mesh,
            uv_transforms,
            position_scale,
            position_offset,
        })
    }
}

/// An element of a vertex or instance layout, as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutElement {
    /// The semantic name (`POSITION`, `NORMAL`, ...), from the layout's names.
    pub semantic: String,
    /// The semantic index.
    pub semantic_index: i16,
    /// The input slot.
    pub slot: u16,
    /// Whether the element is per instance (input classification 1) rather than per vertex (0).
    pub per_instance: bool,
    /// The DXGI format.
    pub format: u32,
    /// The byte offset in the vertex, or -1 to append to the slot's previous elements.
    pub offset: i32,
    /// The instance data step rate.
    pub step_rate: u32,
}

/// A vertex layout (`VLay`) or instance layout (`ILay`), as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VertexLayout {
    /// The `Id  ` metadata.
    pub id: u32,
    /// The semantic names, in stored order.
    pub names: Vec<String>,
    /// The elements, in stored order.
    pub elements: Vec<LayoutElement>,
    /// The u32 stored per element after the elements.
    pub packed_formats: Vec<u32>,
    /// The u32 of flags at the end.
    pub flags: u32,
}

impl VertexLayout {
    /// A `VLay` or `ILay` blob read. Refused: another tag, version or metadata, a name that is not UTF-8, a semantic
    /// name index that is not one of the names, an input classification other than 0 or 1, and bytes after the flags.
    pub fn parse(blob: &Blob<'_>) -> Result<VertexLayout> {
        if blob.tag != VLAY && blob.tag != ILAY {
            return Err(fail(format!("{} is not a layout", blob.tag)));
        }
        let what = blob.tag.to_string();
        version(blob, &what, &[v(1, 1)])?;
        let id = metadata(blob, &what, &[&[ID]])?.id(&what)?;
        let mut r = Reader::new(blob, what);
        let n = r.u16("the semantic name count")?;
        let mut names = Vec::with_capacity(usize::from(n).min(r.c.remaining()));
        for _ in 0..n {
            names.push(r.text("a semantic name")?);
        }
        let n = r.u16("the element count")?;
        let mut elements = Vec::with_capacity(usize::from(n).min(r.c.remaining()));
        for _ in 0..n {
            let at = r.at();
            let index = r.i16("an element's semantic name")?;
            let semantic = usize::try_from(index)
                .ok()
                .and_then(|i| names.get(i))
                .ok_or_else(|| r.refuse(at, format!("semantic name {index}, of {} names", names.len())))?
                .clone();
            let semantic_index = r.i16("an element's semantic index")?;
            let slot = r.u16("an element's input slot")?;
            let at = r.at();
            let per_instance = match r.u16("an element's input classification")? {
                0 => false,
                1 => true,
                other => return Err(r.refuse(at, format!("input classification {other}, not 0 or 1"))),
            };
            elements.push(LayoutElement {
                semantic,
                semantic_index,
                slot,
                per_instance,
                format: r.u32("an element's format")?,
                offset: r.i32("an element's offset")?,
                step_rate: r.u32("an element's step rate")?,
            });
        }
        let mut packed_formats = Vec::with_capacity(elements.len());
        for _ in 0..n {
            packed_formats.push(r.u32("a packed format")?);
        }
        let flags = r.u32("the flags")?;
        r.end()?;
        Ok(VertexLayout {
            id,
            names,
            elements,
            packed_formats,
            flags,
        })
    }
}

/// A buffer (`IndB`, `VerB`, `MBuf` or `Skin`): its header and elements, as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Buffer<'a> {
    /// The `Id  ` metadata.
    pub id: u32,
    /// The element count.
    pub count: u32,
    /// The bytes per element.
    pub stride: u16,
    /// The values per element.
    pub values: u8,
    /// The DXGI format.
    pub format: u32,
    /// The elements' bytes: the count times the stride.
    pub data: &'a [u8],
}

impl<'a> Buffer<'a> {
    /// A buffer blob read (`IndB` version 1.0, `VerB`, `MBuf` and `Skin` version 1.1). Refused: another tag,
    /// version or metadata, a size other than the count times the stride, a reserved byte other than 0, and a blob
    /// that does not end where the elements do.
    pub fn parse(blob: &Blob<'a>) -> Result<Buffer<'a>> {
        let tag = blob.tag;
        let what = tag.to_string();
        let versions = match tag {
            INDB => [v(1, 0)],
            VERB | MBUF | SKIN => [v(1, 1)],
            _ => return Err(fail(format!("{tag} is not a buffer"))),
        };
        version(blob, &what, &versions)?;
        let id = metadata(blob, &what, &[&[ID]])?.id(&what)?;
        let mut r = Reader::new(blob, what);
        let count = r.u32("the element count")?;
        let at = r.at();
        let size = r.u32("the size")?;
        let stride = r.u16("the stride")?;
        let values = r.u8("the values per element")?;
        r.zero_u8("byte 11")?;
        let format = r.u32("the format")?;
        if u64::from(count).saturating_mul(u64::from(stride)) != u64::from(size) {
            return Err(r.refuse(
                at,
                format!("the size is {size}, not the count {count} times the stride {stride}"),
            ));
        }
        let data = r.c.take(usize_of(size)).field("the elements");
        let data = data.map_err(|e| r.bounds(e))?;
        r.end()?;
        Ok(Buffer {
            id,
            count,
            stride,
            values,
            format,
            data,
        })
    }
}

/// The morph targets' names (`Mrph`): a u16 count, then each name as a u32 length and that many bytes of UTF-8.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MorphTargets {
    /// The names, in stored order.
    pub names: Vec<String>,
}

impl MorphTargets {
    /// The `Mrph` blob read. Refused: another version, any metadata, a name that is not UTF-8, and bytes after the
    /// names.
    pub fn parse(blob: &Blob<'_>) -> Result<MorphTargets> {
        let what = "Mrph";
        version(blob, what, &[v(0, 0)])?;
        metadata(blob, what, &[&[]])?;
        let mut r = Reader::new(blob, what.to_owned());
        let n = r.u16("the name count")?;
        let mut names = Vec::with_capacity(usize::from(n).min(r.c.remaining()));
        for _ in 0..n {
            names.push(r.text("a name")?);
        }
        r.end()?;
        Ok(MorphTargets { names })
    }
}

/// A model: every blob of a `.modelbin` read, in stored order within each kind.
#[derive(Clone, Debug, PartialEq)]
pub struct Model<'a> {
    /// The `Modl` blob.
    pub header: Header,
    /// The `Skel` blob.
    pub skeleton: Skeleton,
    /// The `MatI` blobs.
    pub materials: Vec<Material<'a>>,
    /// The `Mesh` blobs, in stored order (the order of a variant row).
    pub meshes: Vec<Mesh>,
    /// The `VLay` blobs.
    pub layouts: Vec<VertexLayout>,
    /// The `ILay` blobs.
    pub instance_layouts: Vec<VertexLayout>,
    /// The `IndB` blobs.
    pub index_buffers: Vec<Buffer<'a>>,
    /// The `VerB` blobs.
    pub vertex_buffers: Vec<Buffer<'a>>,
    /// The `MBuf` blobs.
    pub morph_buffers: Vec<Buffer<'a>>,
    /// The `Skin` blobs.
    pub skin_buffers: Vec<Buffer<'a>>,
    /// The `Mrph` blob, when the model has one.
    pub morph_targets: Option<MorphTargets>,
}

fn by_id<T>(items: &[T], id: impl TryInto<u32>, key: impl Fn(&T) -> u32) -> Option<&T> {
    let id = id.try_into().ok()?;
    items.iter().find(|t| key(t) == id)
}

fn unique_ids<T>(items: &[T], key: impl Fn(&T) -> u32, what: &str) -> Result<()> {
    let mut ids: Vec<u32> = items.iter().map(key).collect();
    ids.sort_unstable();
    match ids.windows(2).find(|w| w.first() == w.get(1)) {
        Some(w) => Err(fail(format!(
            "two {what} blobs have id {}",
            w.first().copied().unwrap_or_default()
        ))),
        None => Ok(()),
    }
}

impl<'a> Model<'a> {
    /// The model `bytes` hold: the bundle ([`Bundle::parse`]) and every blob, and the references between them checked.
    pub fn parse(bytes: &'a [u8]) -> Result<Model<'a>> {
        Model::from_bundle(&Bundle::parse(bytes)?)
    }

    /// The model a bundle holds. Refused: a blob tag this module does not read, not exactly one `Modl` and one
    /// `Skel`, counts in the header other than the blobs', two blobs of a kind with one id, and a mesh whose material
    /// ids, buffers or vertex layout do not exist, whose index width is not its index buffer's, or that binds two
    /// buffers to one slot; and a variant row that picks a material group a mesh does not have.
    pub fn from_bundle(bundle: &Bundle<'a>) -> Result<Model<'a>> {
        let mut header = None;
        let mut skeleton = None;
        let mut morph_targets = None;
        let mut materials = Vec::new();
        let mut meshes = Vec::new();
        let mut layouts = Vec::new();
        let mut instance_layouts = Vec::new();
        let mut index_buffers = Vec::new();
        let mut vertex_buffers = Vec::new();
        let mut morph_buffers = Vec::new();
        let mut skin_buffers = Vec::new();
        for (i, blob) in bundle.blobs.iter().enumerate() {
            fn once<T>(slot: &mut Option<T>, i: usize, tag: Tag, value: Result<T>) -> Result<()> {
                if slot.is_some() {
                    return Err(fail(format!("a second {tag} blob (blob {i})")));
                }
                *slot = Some(value?);
                Ok(())
            }
            match blob.tag {
                MODL => once(&mut header, i, blob.tag, Header::parse(blob))?,
                SKEL => once(&mut skeleton, i, blob.tag, Skeleton::parse(blob))?,
                MRPH => once(&mut morph_targets, i, blob.tag, MorphTargets::parse(blob))?,
                MATI => materials.push(Material::parse(blob)?),
                MESH => meshes.push(Mesh::parse(blob)?),
                VLAY => layouts.push(VertexLayout::parse(blob)?),
                ILAY => instance_layouts.push(VertexLayout::parse(blob)?),
                INDB => index_buffers.push(Buffer::parse(blob)?),
                VERB => vertex_buffers.push(Buffer::parse(blob)?),
                MBUF => morph_buffers.push(Buffer::parse(blob)?),
                SKIN => skin_buffers.push(Buffer::parse(blob)?),
                other => {
                    return Err(fail(format!(
                        "blob {i}: a {other} blob, which this module does not read"
                    )));
                }
            }
        }
        let m = Model {
            header: header.ok_or_else(|| fail("no Modl blob"))?,
            skeleton: skeleton.ok_or_else(|| fail("no Skel blob"))?,
            materials,
            meshes,
            layouts,
            instance_layouts,
            index_buffers,
            vertex_buffers,
            morph_buffers,
            skin_buffers,
            morph_targets,
        };
        m.check()?;
        Ok(m)
    }

    fn check(&self) -> Result<()> {
        let h = &self.header;
        let buffers = self
            .index_buffers
            .len()
            .saturating_add(self.vertex_buffers.len())
            .saturating_add(self.morph_buffers.len())
            .saturating_add(self.skin_buffers.len());
        for (what, stated, found) in [
            ("meshes", h.mesh_count, self.meshes.len()),
            ("buffers", h.buffer_count, buffers),
            ("vertex layouts", h.layout_count, self.layouts.len()),
            ("materials", h.material_count, self.materials.len()),
        ] {
            if usize::from(stated) != found {
                return Err(fail(format!(
                    "Modl states {stated} {what}, the bundle holds {found}"
                )));
            }
        }
        unique_ids(&self.materials, |x| x.id, "MatI")?;
        unique_ids(&self.layouts, |x| x.id, "VLay")?;
        unique_ids(&self.index_buffers, |x| x.id, "IndB")?;
        unique_ids(&self.vertex_buffers, |x| x.id, "VerB")?;
        unique_ids(&self.morph_buffers, |x| x.id, "MBuf")?;
        unique_ids(&self.skin_buffers, |x| x.id, "Skin")?;
        unique_ids(&self.instance_layouts, |x| x.id, "ILay")?;
        for (k, mesh) in self.meshes.iter().enumerate() {
            let what = |e: String| fail(format!("mesh {k} {:?}: {e}", mesh.name));
            for g in &mesh.material_groups {
                for &id in g {
                    if id != -1 && by_id(&self.materials, id, |x| x.id).is_none() {
                        return Err(what(format!("material id {id} is no MatI's")));
                    }
                }
            }
            let ib = by_id(&self.index_buffers, mesh.index_buffer, |x| x.id)
                .ok_or_else(|| what(format!("index buffer {} is no IndB's", mesh.index_buffer)))?;
            let (bits, stride, format) = if mesh.index_32bit {
                (32, 4, R32_UINT)
            } else {
                (16, 2, R16_UINT)
            };
            if (ib.stride, ib.format) != (stride, format) {
                return Err(what(format!(
                    "{bits}-bit indices, its IndB has stride {} and format {}",
                    ib.stride, ib.format
                )));
            }
            by_id(&self.layouts, mesh.layout, |x| x.id)
                .ok_or_else(|| what(format!("vertex layout {} is no VLay's", mesh.layout)))?;
            for (j, b) in mesh.vertex_buffers.iter().enumerate() {
                let vb = by_id(&self.vertex_buffers, b.buffer, |x| x.id)
                    .ok_or_else(|| what(format!("vertex buffer {} is no VerB's", b.buffer)))?;
                if u32::from(vb.stride) != b.stride {
                    return Err(what(format!(
                        "vertex buffer {} has stride {}, the mesh states {}",
                        b.buffer, vb.stride, b.stride
                    )));
                }
                if mesh.vertex_buffers.iter().take(j).any(|o| o.slot == b.slot) {
                    return Err(what(format!("two vertex buffers on slot {}", b.slot)));
                }
            }
            if mesh.morph_buffer != -1 && by_id(&self.morph_buffers, mesh.morph_buffer, |x| x.id).is_none() {
                return Err(what(format!("morph buffer {} is no MBuf's", mesh.morph_buffer)));
            }
            if mesh.skinning_buffer != -1
                && by_id(&self.skin_buffers, mesh.skinning_buffer, |x| x.id).is_none()
            {
                return Err(what(format!(
                    "skinning buffer {} is no Skin's",
                    mesh.skinning_buffer
                )));
            }
        }
        for (row, groups) in h.variants.iter().enumerate() {
            for (mesh, &g) in self.meshes.iter().zip(groups) {
                if usize::from(g) >= mesh.material_groups.len() {
                    return Err(fail(format!(
                        "variant {row} picks material group {g} of mesh {:?}, which has {}",
                        mesh.name,
                        mesh.material_groups.len()
                    )));
                }
            }
        }
        Ok(())
    }

    /// The vertex layout with an id.
    pub fn layout(&self, id: i32) -> Option<&VertexLayout> {
        by_id(&self.layouts, id, |x| x.id)
    }

    /// The material instance with an id.
    pub fn material(&self, id: i16) -> Option<&Material<'a>> {
        by_id(&self.materials, id, |x| x.id)
    }

    /// One mesh's indices and the vertices they use, decoded. Refused: a mesh index out of range, a topology other
    /// than a triangle list, an index count other than three times the primitive count, indices or vertices outside
    /// their buffers, a vertex format [`Format::from_dxgi`] does not list, an element on a slot no vertex buffer is
    /// bound to, and morph or skinning buffers whose layout is not the one stated in [`MeshData`].
    pub fn mesh_data(&self, index: usize) -> Result<MeshData<'_>> {
        let mesh = self
            .meshes
            .get(index)
            .ok_or_else(|| fail(format!("mesh {index} of {}", self.meshes.len())))?;
        let what = |e: String| fail(format!("mesh {index} {:?}: {e}", mesh.name));
        if mesh.topology != TRIANGLE_LIST {
            return Err(what(format!(
                "topology {}, not a triangle list ({TRIANGLE_LIST})",
                mesh.topology
            )));
        }
        if i64::from(mesh.index_count) != i64::from(mesh.primitive_count).saturating_mul(3) {
            return Err(what(format!(
                "{} indices for {} triangles",
                mesh.index_count, mesh.primitive_count
            )));
        }
        let ib = by_id(&self.index_buffers, mesh.index_buffer, |x| x.id)
            .ok_or_else(|| what(format!("index buffer {} is no IndB's", mesh.index_buffer)))?;
        let stride: usize = if mesh.index_32bit { 4 } else { 2 };
        let range = || -> Option<std::ops::Range<usize>> {
            let offset = usize::try_from(mesh.index_offset).ok()?;
            let start = usize::try_from(mesh.start_index)
                .ok()?
                .checked_mul(stride)?
                .checked_add(offset)?;
            let len = usize::try_from(mesh.index_count).ok()?.checked_mul(stride)?;
            Some(start..start.checked_add(len)?)
        };
        let bytes = range().and_then(|r| ib.data.get(r)).ok_or_else(|| {
            what(format!(
                "indices from byte {} + {} × {stride}, {} of them, outside the index buffer's {} bytes",
                mesh.index_offset,
                mesh.start_index,
                mesh.index_count,
                ib.data.len()
            ))
        })?;
        let indices: Vec<u32> = if mesh.index_32bit {
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| u32::from_le_bytes(*b))
                .collect()
        } else {
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u32::from(u16::from_le_bytes(*b)))
                .collect()
        };
        let (Some(&min), Some(&max)) = (indices.iter().min(), indices.iter().max()) else {
            return Ok(MeshData {
                mesh,
                indices,
                first_vertex: 0,
                vertex_count: 0,
                elements: Vec::new(),
                morph: None,
                skin: None,
            });
        };
        let first = i64::from(min).saturating_add(i64::from(mesh.base_vertex));
        let first_vertex = u32::try_from(first).map_err(|_| {
            what(format!(
                "index {min} plus the base vertex {} is below 0",
                mesh.base_vertex
            ))
        })?;
        // max >= min, so the difference and the count fit a u32 and a usize
        let vertex_count = usize_of(max.saturating_sub(min)).saturating_add(1);
        let first_vertex_usize = usize_of(first_vertex);

        let layout = by_id(&self.layouts, mesh.layout, |x| x.id)
            .ok_or_else(|| what(format!("vertex layout {} is no VLay's", mesh.layout)))?;
        let mut declared = Vec::with_capacity(layout.elements.len());
        for e in &layout.elements {
            let format = Format::from_dxgi(e.format)
                .map_err(|err| what(format!("{}{}: {err}", e.semantic, e.semantic_index)))?;
            let offset = match e.offset {
                -1 => None,
                o => Some(
                    usize::try_from(o)
                        .map_err(|_| what(format!("{}{}: offset {o}", e.semantic, e.semantic_index)))?,
                ),
            };
            declared.push(VertexElement {
                semantic: e.semantic.clone(),
                index: u32::try_from(e.semantic_index)
                    .map_err(|_| what(format!("{}: semantic index {}", e.semantic, e.semantic_index)))?,
                format,
                slot: u32::from(e.slot),
                offset,
            });
        }
        let resolved = vertex::resolve_layout(&declared).map_err(|e| what(e.to_string()))?;
        let mut elements = Vec::with_capacity(resolved.len());
        for el in resolved {
            let name = format!("{}{}", el.element.semantic, el.element.index);
            let bind = mesh
                .vertex_buffers
                .iter()
                .find(|b| b.slot == el.element.slot)
                .ok_or_else(|| what(format!("{name}: no vertex buffer on slot {}", el.element.slot)))?;
            let vb = by_id(&self.vertex_buffers, bind.buffer, |x| x.id)
                .ok_or_else(|| what(format!("vertex buffer {} is no VerB's", bind.buffer)))?;
            let values = decode(&el, vb, usize_of(bind.offset), first_vertex_usize, vertex_count)
                .map_err(|e| what(format!("{name}, vertex buffer {}: {e}", vb.id)))?;
            elements.push(ElementData { element: el, values });
        }

        let morph = if mesh.morph_buffer == -1 {
            None
        } else {
            let mb = by_id(&self.morph_buffers, mesh.morph_buffer, |x| x.id)
                .ok_or_else(|| what(format!("morph buffer {} is no MBuf's", mesh.morph_buffer)))?;
            Some(
                per_vertex(
                    mb,
                    Format::Half4,
                    mesh.morph_targets.saturating_mul(2),
                    first_vertex_usize,
                    vertex_count,
                )
                .map_err(|e| what(format!("morph buffer {}: {e}", mb.id)))?,
            )
        };
        let skin = if mesh.skinning_buffer == -1 {
            None
        } else {
            let sb = by_id(&self.skin_buffers, mesh.skinning_buffer, |x| x.id)
                .ok_or_else(|| what(format!("skinning buffer {} is no Skin's", mesh.skinning_buffer)))?;
            Some(
                per_vertex(
                    sb,
                    Format::Half2,
                    u32::from(mesh.skinning_elements),
                    first_vertex_usize,
                    vertex_count,
                )
                .map_err(|e| what(format!("skinning buffer {}: {e}", sb.id)))?,
            )
        };
        Ok(MeshData {
            mesh,
            indices,
            first_vertex,
            vertex_count,
            elements,
            morph,
            skin,
        })
    }
}

/// `count` vertices from `first` of one element of a vertex buffer whose first vertex is `offset` bytes in.
fn decode(
    el: &ResolvedElement,
    vb: &Buffer<'_>,
    offset: usize,
    first: usize,
    count: usize,
) -> std::result::Result<Vec<[f32; 4]>, String> {
    let last = first.saturating_add(count);
    if last > usize_of(vb.count) {
        return Err(format!(
            "vertices {first} to {} past its {} elements",
            last.saturating_sub(1),
            vb.count
        ));
    }
    let stream = Stream {
        bytes: vb.data,
        stride: usize::from(vb.stride),
        offset,
    };
    vertex::decode_stream_raw(el, stream, first, count).map_err(|e| e.to_string())
}

/// A morph or skinning buffer's elements for `count` vertices from `first`: each element `values` values of `format`.
fn per_vertex(
    b: &Buffer<'_>,
    format: Format,
    values: u32,
    first: usize,
    count: usize,
) -> std::result::Result<Vec<[f32; 4]>, String> {
    let stride = usize_of(values).saturating_mul(format.size());
    if b.format != format as u32 || u32::from(b.values) != values || usize::from(b.stride) != stride {
        return Err(format!(
            "format {}, {} values, stride {}: expected format {}, {values} values, stride {stride}",
            b.format, b.values, b.stride, format as u32
        ));
    }
    let mut out = Vec::with_capacity(count.saturating_mul(usize_of(values)));
    for k in 0..usize_of(values) {
        let el = ResolvedElement {
            element: VertexElement {
                semantic: String::new(),
                index: 0,
                format,
                slot: 0,
                offset: None,
            },
            offset: k.saturating_mul(format.size()),
        };
        let column = decode(&el, b, 0, first, count)?;
        out.push(column);
    }
    // interleave the columns back into per-vertex order
    let mut interleaved = Vec::with_capacity(out.iter().map(Vec::len).sum());
    for i in 0..count {
        for column in &out {
            if let Some(v) = column.get(i) {
                interleaved.push(*v);
            }
        }
    }
    Ok(interleaved)
}

/// One element of a mesh's vertices, decoded by its DXGI format ([`Format::decode_raw`]).
#[derive(Clone, Debug, PartialEq)]
pub struct ElementData {
    /// The element, its offset in the vertex worked out.
    pub element: ResolvedElement,
    /// One value per vertex, from [`MeshData::first_vertex`]; components the format lacks are 0.
    pub values: Vec<[f32; 4]>,
}

/// One mesh's indices and the vertices they use, decoded.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshData<'m> {
    /// The mesh.
    pub mesh: &'m Mesh,
    /// The indices, as stored.
    pub indices: Vec<u32>,
    /// The vertex buffers' element number of the first vertex decoded: the smallest index plus the base vertex. Index
    /// `i` is vertex `i + base vertex - first_vertex` of the arrays.
    pub first_vertex: u32,
    /// The vertices decoded: the largest index less the smallest, plus one.
    pub vertex_count: usize,
    /// Each element of the vertex layout, in its order.
    pub elements: Vec<ElementData>,
    /// The morph buffer's elements (`R16G16B16A16_FLOAT`), [`Mesh::morph_targets`] × 2 per vertex.
    pub morph: Option<Vec<[f32; 4]>>,
    /// The skinning buffer's elements (`R16G16_FLOAT`), [`Mesh::skinning_elements`] per vertex.
    pub skin: Option<Vec<[f32; 4]>>,
}

impl MeshData<'_> {
    /// The element with a semantic and index.
    pub fn element(&self, semantic: &str, index: u32) -> Option<&ElementData> {
        self.elements
            .iter()
            .find(|e| e.element.element.semantic == semantic && e.element.element.index == index)
    }

    /// The indices as numbers of the decoded vertices: each index less the smallest one.
    pub fn local_indices(&self) -> Vec<u32> {
        let min = self.indices.iter().min().copied().unwrap_or(0);
        self.indices.iter().map(|i| i.saturating_sub(min)).collect()
    }

    /// `POSITION0`'s x, y and z times the mesh's position scale plus its position offset, per axis
    /// ([`vertex::scale_translate`]); `None` without a `POSITION0`. Refused: a value that is not finite.
    pub fn positions(&self) -> Option<Result<Vec<[f32; 3]>>> {
        let p = self.element("POSITION", 0)?;
        let [sx, sy, sz, _] = self.mesh.position_scale;
        let [tx, ty, tz, _] = self.mesh.position_offset;
        Some(
            p.values
                .iter()
                .map(|[x, y, z, _]| {
                    vertex::scale_translate([*x, *y, *z], [sx, sy, sz], [tx, ty, tz])
                        .map_err(|e| fail(format!("mesh {:?}: {e}", self.mesh.name)))
                })
                .collect(),
        )
    }

    /// `NORMAL0` as stored, not normalised: for a two-value format (`R16G16_SNORM`), `POSITION0`'s w then the two
    /// values; otherwise its first three values. `None` without a `NORMAL0`, or a two-value one without a `POSITION0`.
    pub fn normals(&self) -> Option<Vec<[f32; 3]>> {
        let n = self.element("NORMAL", 0)?;
        if n.element.element.format == Format::Snorm16x2 {
            let p = self.element("POSITION", 0)?;
            Some(
                p.values
                    .iter()
                    .zip(&n.values)
                    .map(|(p, n)| [p[3], n[0], n[1]])
                    .collect(),
            )
        } else {
            Some(n.values.iter().map(|[x, y, z, _]| [*x, *y, *z]).collect())
        }
    }

    /// `TEXCOORD<set>` times the set's scale plus its offset ([`Mesh::uv_transforms`]): `u × u scale + u offset`,
    /// `v × v scale + v offset`. `None` without that element, or for a set past the five transforms.
    pub fn uvs(&self, set: u32) -> Option<Vec<[f32; 2]>> {
        let t = self.element("TEXCOORD", set)?;
        let [uo, us, vo, vs] = *self.mesh.uv_transforms.get(usize_of(set))?;
        Some(
            t.values
                .iter()
                .map(|[u, v, _, _]| [u * us + uo, v * vs + vo])
                .collect(),
        )
    }

    /// `TANGENT<set>`, stored as `R10G10B10A2_UNORM`: x, y and z mapped from 0..1 to -1..1 (`× 2 - 1`, not normalised),
    /// and the 2-bit field's value (0 to 3) as the fourth. `None` without that element or in another format.
    pub fn tangents(&self, set: u32) -> Option<Vec<[f32; 4]>> {
        let t = self.element("TANGENT", set)?;
        if t.element.element.format != Format::Unorm10x3A2 {
            return None;
        }
        Some(
            t.values
                .iter()
                .map(|[x, y, z, a]| [x * 2. - 1., y * 2. - 1., z * 2. - 1., (a * 3.).round()])
                .collect(),
        )
    }

    /// `COLOR0`'s four values; `None` without it.
    pub fn colors(&self) -> Option<Vec<[f32; 4]>> {
        Some(self.element("COLOR", 0)?.values.clone())
    }
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "tests"
)]
mod tests {
    use super::*;

    type Blob4<'a> = (&'a [u8; 4], [u8; 2], Vec<(&'a [u8; 4], Vec<u8>)>, Vec<u8>);

    /// A bundle laid out as the `bundle` module docs state, every metadata record version 0, the header and the last
    /// blob's data rounded up to 4.
    fn bundle(blobs: &[Blob4<'_>]) -> Vec<u8> {
        let first = 20 + 24 * blobs.len();
        let (mut meta, mut moffs) = (Vec::new(), Vec::new());
        for (_, _, records, _) in blobs {
            let at = first + meta.len();
            moffs.push(at);
            let mut pay = at + 8 * records.len();
            let (mut recs, mut payloads) = (Vec::new(), Vec::new());
            for (j, (tag, payload)) in records.iter().enumerate() {
                recs.extend_from_slice(&Tag::new(tag).0.to_le_bytes());
                recs.extend_from_slice(&((payload.len() as u16) << 4).to_le_bytes());
                recs.extend_from_slice(&((pay - (at + 8 * j)) as u16).to_le_bytes());
                payloads.extend_from_slice(payload);
                pay += payload.len();
            }
            meta.extend_from_slice(&recs);
            meta.extend_from_slice(&payloads);
        }
        let header = (first + meta.len()).next_multiple_of(4);
        meta.resize(header - first, 0);
        let (mut data, mut doffs) = (Vec::new(), Vec::new());
        for (_, _, _, d) in blobs {
            doffs.push(header + data.len());
            data.extend_from_slice(d);
            data.resize((header + data.len()).next_multiple_of(4) - header, 0);
        }
        let mut b = Vec::new();
        b.extend_from_slice(b"burG");
        b.extend_from_slice(&[1, 1, 0, 0]);
        b.extend_from_slice(&(header as u32).to_le_bytes());
        b.extend_from_slice(&((header + data.len()) as u32).to_le_bytes());
        b.extend_from_slice(&(blobs.len() as u32).to_le_bytes());
        for (k, (tag, version, records, d)) in blobs.iter().enumerate() {
            b.extend_from_slice(&Tag::new(tag).0.to_le_bytes());
            b.extend_from_slice(version);
            b.extend_from_slice(&(records.len() as u16).to_le_bytes());
            b.extend_from_slice(&(moffs[k] as u32).to_le_bytes());
            b.extend_from_slice(&(doffs[k] as u32).to_le_bytes());
            b.extend_from_slice(&(d.len() as u32).to_le_bytes());
            b.extend_from_slice(&(d.len() as u32).to_le_bytes());
        }
        b.extend_from_slice(&meta);
        b.extend_from_slice(&data);
        b
    }

    fn bbox() -> Vec<u8> {
        [-1f32, -2., -3., 1., 2., 3.]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect()
    }

    fn id(n: u32) -> Vec<u8> {
        n.to_le_bytes().to_vec()
    }

    fn buffer(count: u32, stride: u16, values: u8, format: u32, data: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&count.to_le_bytes());
        b.extend_from_slice(&(u32::from(stride) * count).to_le_bytes());
        b.extend_from_slice(&stride.to_le_bytes());
        b.extend_from_slice(&[values, 0]);
        b.extend_from_slice(&format.to_le_bytes());
        b.extend_from_slice(data);
        b
    }

    fn i16s(values: &[i16]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    /// One triangle: three vertices with positions (slot 0, `R16G16B16A16_SNORM`) and normals (slot 1,
    /// `R16G16_SNORM`), one 16-bit index buffer, one material slot with no instance.
    fn triangle() -> Vec<u8> {
        let mut skel = vec![1, 0];
        skel.extend_from_slice(&6u32.to_le_bytes());
        skel.extend_from_slice(b"<root>");
        skel.extend_from_slice(&i16s(&[-1, -1, -1]));
        for i in 0..16 {
            skel.extend_from_slice(&(if i % 5 == 0 { 1f32 } else { 0. }).to_le_bytes());
        }
        skel.extend_from_slice(&0u32.to_le_bytes());

        let mut mesh = i16s(&[-1, 0, -1, -1]); // material ids
        mesh.extend_from_slice(&0i16.to_le_bytes()); // rigid bone
        mesh.extend_from_slice(&2u16.to_le_bytes()); // LOD flags
        mesh.extend_from_slice(&[0, 0]); // LOD levels
        mesh.extend_from_slice(&0x19u16.to_le_bytes()); // bucket flags
        mesh.extend_from_slice(&[0, 0]); // bucket order, skinning elements
        mesh.extend_from_slice(&0u32.to_le_bytes()); // morph targets
        mesh.extend_from_slice(&[0, 0]); // morph damage, 32-bit indices
        mesh.extend_from_slice(&TRIANGLE_LIST.to_le_bytes());
        for v in [0i32, 0, 0, 0, 3, 1] {
            mesh.extend_from_slice(&v.to_le_bytes()); // IndB id, offset, start, base vertex, indices, triangles
        }
        mesh.extend_from_slice(&1f32.to_le_bytes()); // ACMR
        mesh.extend_from_slice(&3u32.to_le_bytes()); // referenced vertex count
        mesh.extend_from_slice(&0u32.to_le_bytes()); // referenced vertices
        mesh.extend_from_slice(&0i32.to_le_bytes()); // layout
        mesh.extend_from_slice(&2i32.to_le_bytes());
        for (id, slot, stride) in [(0i32, 0u32, 8u32), (1, 1, 4)] {
            mesh.extend_from_slice(&id.to_le_bytes());
            mesh.extend_from_slice(&slot.to_le_bytes());
            mesh.extend_from_slice(&stride.to_le_bytes());
            mesh.extend_from_slice(&[0; 8]); // offset, reserved
        }
        mesh.extend_from_slice(&(-1i32).to_le_bytes()); // morph buffer
        mesh.extend_from_slice(&(-1i32).to_le_bytes()); // skinning buffer
        mesh.extend_from_slice(&0i32.to_le_bytes()); // constant buffers
        mesh.extend_from_slice(&0u32.to_le_bytes()); // source mesh
        for _ in 0..5 {
            for f in [0f32, 1., 0., 1.] {
                mesh.extend_from_slice(&f.to_le_bytes());
            }
        }
        for f in [2f32, 2., 2., 0., 1., 0., 0., 0.] {
            mesh.extend_from_slice(&f.to_le_bytes());
        }

        let mut vlay = Vec::new();
        vlay.extend_from_slice(&2u16.to_le_bytes());
        for n in ["POSITION", "NORMAL"] {
            vlay.extend_from_slice(&(n.len() as u32).to_le_bytes());
            vlay.extend_from_slice(n.as_bytes());
        }
        vlay.extend_from_slice(&2u16.to_le_bytes());
        for (name, slot, format) in [(0i16, 0u16, 13u32), (1, 1, 37)] {
            vlay.extend_from_slice(&name.to_le_bytes());
            vlay.extend_from_slice(&0i16.to_le_bytes());
            vlay.extend_from_slice(&slot.to_le_bytes());
            vlay.extend_from_slice(&0u16.to_le_bytes());
            vlay.extend_from_slice(&format.to_le_bytes());
            vlay.extend_from_slice(&(-1i32).to_le_bytes());
            vlay.extend_from_slice(&0u32.to_le_bytes());
        }
        for packed in [49u32, 52, 0x423] {
            vlay.extend_from_slice(&packed.to_le_bytes()); // two packed formats, then the flags
        }

        let positions = i16s(&[32767, 0, 0, 16384, -32767, 0, 0, 0, 0, 32767, 0, -16384]);
        let normals = i16s(&[0, 32767, 0, 0, -32767, 0]);
        let indices = i16s(&[0, 1, 2]);

        let mut modl = Vec::new();
        for n in [1u16, 3, 1, 1] {
            modl.extend_from_slice(&n.to_le_bytes());
        }
        modl.extend_from_slice(&[1, 0, 0, 0]); // has LOD, first, last, reserved
        modl.extend_from_slice(&2u16.to_le_bytes());
        modl.extend_from_slice(&[1, 0, 0, 1]); // decompress, reserved, retain, visibility

        bundle(&[
            (b"Skel", [1, 0], vec![], skel),
            (
                b"MatI",
                [1, 0],
                vec![(b"Name", b"m".to_vec()), (b"Id  ", id(0))],
                vec![],
            ),
            (
                b"Mesh",
                [1, 12],
                vec![(b"Name", b"tri".to_vec()), (b"BBox", bbox())],
                mesh,
            ),
            (
                b"IndB",
                [1, 0],
                vec![(b"Id  ", id(0))],
                buffer(3, 2, 1, R16_UINT, &indices),
            ),
            (b"VLay", [1, 1], vec![(b"Id  ", id(0))], vlay),
            (
                b"VerB",
                [1, 1],
                vec![(b"Id  ", id(0))],
                buffer(3, 8, 1, 13, &positions),
            ),
            (
                b"VerB",
                [1, 1],
                vec![(b"Id  ", id(1))],
                buffer(3, 4, 1, 37, &normals),
            ),
            (b"Modl", [1, 4], vec![(b"BBox", bbox())], modl),
        ])
    }

    #[test]
    fn reads_a_model_built_by_hand() {
        let bytes = triangle();
        let m = Model::parse(&bytes).unwrap();
        assert_eq!((m.header.mesh_count, m.header.buffer_count), (1, 3));
        assert_eq!(m.skeleton.bones[0].name, "<root>");
        assert_eq!(m.materials[0].name, "m");
        assert!(m.materials[0].instance.is_none());
        assert_eq!(m.meshes[0].name, "tri");
        assert_eq!(m.meshes[0].material_groups, vec![[-1, 0, -1, -1]]);
        let d = m.mesh_data(0).unwrap();
        assert_eq!(
            (d.indices.clone(), d.first_vertex, d.vertex_count),
            (vec![0, 1, 2], 0, 3)
        );
        // snorm16 × scale 2 + offset (1, 0, 0)
        let p = d.positions().unwrap().unwrap();
        assert_eq!(p, vec![[3., 0., 0.], [-1., 0., 0.], [1., 2., 0.]]);
        // (position w, x, y), not normalised
        let n = d.normals().unwrap();
        assert!((n[0][0] - 16384. / 32767.).abs() < 1e-6 && n[0][1] == 0. && n[0][2] == 1.);
        assert!((n[2][0] + 16384. / 32767.).abs() < 1e-6 && n[2][2] == 0.);
        assert!(d.uvs(0).is_none() && d.colors().is_none() && d.tangents(0).is_none());
    }

    /// The model with the byte `offset` after the first occurrence of `find` set to `value`.
    fn changed(find: &[u8], offset: usize, value: u8) -> Vec<u8> {
        let mut b = triangle();
        let i = b.windows(find.len()).position(|w| w == find).unwrap() + offset;
        b[i] = value;
        b
    }

    #[test]
    fn refuses_what_the_layout_does_not_account_for() {
        let cases: [(Vec<u8>, &str); 5] = [
            // Modl: the buffer count 3 -> 4
            (changed(&[1, 0, 3, 0, 1, 0, 1, 0], 2, 4), "Modl states 4 buffers"),
            // Modl: the reserved byte 11
            (changed(&[1, 0, 0, 0, 2, 0, 1, 0], 3, 1), "byte 11"),
            // Mesh: the 32-bit-indices byte 0 -> 1, where its IndB is 16-bit
            (
                changed(&[0x19, 0, 0, 0, 0, 0, 0, 0, 0, 0], 9, 1),
                "32-bit indices",
            ),
            // Mesh: topology 4 -> 5, read, and refused when the mesh is decoded
            (
                changed(&[0x19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0], 10, 5),
                "topology 5",
            ),
            // a blob tag this module does not read
            (changed(b"ItaM", 3, b'X'), "does not read"),
        ];
        for (bytes, expected) in cases {
            let r = Model::parse(&bytes)
                .and_then(|m| (0..m.meshes.len()).try_for_each(|k| m.mesh_data(k).map(drop)));
            let e = r.unwrap_err().to_string();
            assert!(e.contains(expected), "{expected}: {e}");
        }
    }

    #[test]
    fn a_truncated_or_changed_model_is_read_or_refused_never_a_panic() {
        let good = triangle();
        for end in 0..good.len() {
            assert!(Model::parse(&good[..end]).is_err(), "{end} bytes");
        }
        for at in 0..good.len() {
            for value in [0u8, 1, 0x7F, 0xFF] {
                let mut b = good.clone();
                b[at] = value;
                if let Ok(m) = Model::parse(&b) {
                    for k in 0..m.meshes.len() {
                        let _ = m.mesh_data(k);
                    }
                }
            }
        }
    }
}
