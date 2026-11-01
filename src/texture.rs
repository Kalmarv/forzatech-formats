//! Swatch textures: the track textures (`.pb` entries of the GeoChunks, named `<name>_<guid>_qualityN.pb`) and the
//! shared ones (`.swatchbin` entries of zips). A texture is a [Grub bundle](crate::bundle) of one blob, `TXCB`, whose
//! data is the pixel payload and whose one metadata record, `TXCH` (version 0), is the texture's header. The blob's
//! version says which header it is: 0.0 the PC header ([`PcHeader`]), 2.0 the Durango (Xbox One) header
//! ([`DurangoHeader`]).
//!
//! The PC header (little-endian, offsets from the start of the `TXCH` payload):
//!
//! | offset | size | field |
//! |---|---|---|
//! | 0x00 | 4 | 0x38: where the slice list starts |
//! | 0x04 | 4 | the first mip record's offset plus 4 |
//! | 0x08 | 16 | GUID (the one in the file name, stored as a Windows GUID) |
//! | 0x18 | 4 | width (u32) |
//! | 0x1C | 4 | height (u32) |
//! | 0x20 | 4 | depth (u32; 1, or the number of layers every mip holds) |
//! | 0x24 | 2 | slice count (u16; 1, or the array size) |
//! | 0x26 | 1 | mip count (u8) |
//! | 0x27 | 1 | flags (u8; kept as stored) |
//! | 0x28 | 4 | transcoding (u32; 0) |
//! | 0x2C | 4 | a second colour profile (u32; kept as stored) |
//! | 0x30 | 4 | colour profile (u32; 0 linear, 1 sRGB) |
//! | 0x34 | 4 | a u32 kept as stored (0 or 1) |
//! | 0x38 | 4 | 0x40: the first slice record's offset |
//! | 0x3C | 4 | 0x44: the first slice record's offset plus 4 |
//! | 0x40 | 12 per slice | slice records: encoding (u32, [`Encoding`]), the offset of the slice's first mip record, the next slice record's offset plus 4 (0xFFFFFFFF on the last) |
//! | then | 12 per mip per slice | mip records, slice by slice, largest mip first: size (u32), offset in the `TXCB` data (u32), the next mip record's offset plus 4 (0xFFFFFFFF on the very last); one list runs through every slice |
//!
//! The header ends with the last mip record. A mip's bytes are the `TXCB` data at its offset; the mips follow one
//! another with nothing between, from offset 0 to the data's end, and each holds exactly the bytes its format needs at
//! its size, times the depth (a block-compressed level a whole number of 4 × 4 blocks, at least one; the depth is
//! the same at every mip). [`Texture::parse`] refuses any other value or arrangement of these offsets, sizes, counts
//! and links, an encoding outside the table, a transcoding other than 0, a colour profile other than 0 or 1, and a
//! zero width, height, depth, slice count or mip count.
//!
//! The format is the slice's [`Encoding`]; [`Encoding::dxgi`] names its DXGI format, linear and sRGB, the colour
//! profile saying which one the texture is authored as. Decoding is not done here.
//!
//! The Durango header (35 bytes, little-endian): GUID (16), base mip width and height (u16 each), slice count (u16),
//! full width and height (u16 each), a u16, mip count, full mip count and base mip (u8 each), and a u32 of bit fields
//! (tile mode bits 0-4, encoding bits 5-10, transcoding bits 11-16, colour profile bits 20-22, kept whole in
//! [`DurangoHeader::bits`]). Its payload is tiled for the console; where each mip lies in it is not stored in the file
//! (it is the console tiling library's layout), so no mip ranges are given for it.
//!
//! What has been checked, and how: see `tests/texture.rs`.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::bundle::{Bundle, Tag, Version};
use crate::{Error, Result};
use primase_foundations::bytes::{Endian::Little as LE, View};
use std::fmt;
use std::ops::Range;

/// The texture blob's tag.
pub const TXCB: Tag = Tag::new(b"TXCB");
/// The texture header metadata's tag.
pub const TXCH: Tag = Tag::new(b"TXCH");
/// The `TXCB` version of a PC texture.
pub const PC: Version = Version { major: 0, minor: 0 };
/// The `TXCB` version of a Durango (Xbox One) texture.
pub const DURANGO: Version = Version { major: 2, minor: 0 };
/// The PC header's fixed part, before the slice list.
pub const PC_FIXED_LEN: usize = 0x38;
/// A slice or mip record's length.
pub const RECORD_LEN: usize = 12;
/// The Durango header's length.
pub const DURANGO_LEN: usize = 35;
/// The end-of-list link.
pub const END: u32 = u32::MAX;

fn fail(what: impl fmt::Display) -> Error {
    Error::Texture(what.to_string())
}

fn usize_of(v: u32) -> usize {
    // u32 to usize is lossless on the targets this crate builds for (32- and 64-bit).
    v as usize
}

/// A PC slice's or a Durango texture's encoding, as stored: which pixel format its payload is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Encoding {
    /// 0: BC1.
    Bc1,
    /// 1: BC2.
    Bc2,
    /// 2: BC3.
    Bc3,
    /// 3: BC4, unsigned.
    Bc4Unorm,
    /// 4: BC4, signed.
    Bc4Snorm,
    /// 5: BC5, unsigned.
    Bc5Unorm,
    /// 6: BC5, signed.
    Bc5Snorm,
    /// 7: BC6H, unsigned half floats.
    Bc6hUf16,
    /// 8: BC6H, signed half floats.
    Bc6hSf16,
    /// 9: BC7.
    Bc7,
    /// 10: four 32-bit floats.
    R32G32B32A32Float,
    /// 11: four 16-bit unsigned normalized channels.
    R16G16B16A16Unorm,
    /// 12: four 16-bit floats.
    R16G16B16A16Float,
    /// 13: four 8-bit unsigned normalized channels.
    R8G8B8A8Unorm,
    /// 14: 16-bit 5-6-5.
    B5G6R5Unorm,
    /// 15: 16-bit 5-5-5-1.
    B5G5R5A1Unorm,
    /// 19: one 8-bit channel.
    R8Unorm,
    /// 20: 8-bit alpha.
    A8Unorm,
}

impl Encoding {
    /// The encoding a stored value names, if it names one in the table.
    pub const fn from_value(v: u32) -> Option<Encoding> {
        Some(match v {
            0 => Encoding::Bc1,
            1 => Encoding::Bc2,
            2 => Encoding::Bc3,
            3 => Encoding::Bc4Unorm,
            4 => Encoding::Bc4Snorm,
            5 => Encoding::Bc5Unorm,
            6 => Encoding::Bc5Snorm,
            7 => Encoding::Bc6hUf16,
            8 => Encoding::Bc6hSf16,
            9 => Encoding::Bc7,
            10 => Encoding::R32G32B32A32Float,
            11 => Encoding::R16G16B16A16Unorm,
            12 => Encoding::R16G16B16A16Float,
            13 => Encoding::R8G8B8A8Unorm,
            14 => Encoding::B5G6R5Unorm,
            15 => Encoding::B5G5R5A1Unorm,
            19 => Encoding::R8Unorm,
            20 => Encoding::A8Unorm,
            _ => return None,
        })
    }

    /// The stored value.
    pub const fn value(self) -> u32 {
        match self {
            Encoding::Bc1 => 0,
            Encoding::Bc2 => 1,
            Encoding::Bc3 => 2,
            Encoding::Bc4Unorm => 3,
            Encoding::Bc4Snorm => 4,
            Encoding::Bc5Unorm => 5,
            Encoding::Bc5Snorm => 6,
            Encoding::Bc6hUf16 => 7,
            Encoding::Bc6hSf16 => 8,
            Encoding::Bc7 => 9,
            Encoding::R32G32B32A32Float => 10,
            Encoding::R16G16B16A16Unorm => 11,
            Encoding::R16G16B16A16Float => 12,
            Encoding::R8G8B8A8Unorm => 13,
            Encoding::B5G6R5Unorm => 14,
            Encoding::B5G5R5A1Unorm => 15,
            Encoding::R8Unorm => 19,
            Encoding::A8Unorm => 20,
        }
    }

    /// The DXGI format number: `(linear, sRGB)`, the second only for the formats DXGI has an sRGB form of (BC1,
    /// BC2, BC3, BC7, R8G8B8A8).
    pub const fn dxgi(self) -> (u32, Option<u32>) {
        match self {
            Encoding::Bc1 => (71, Some(72)),
            Encoding::Bc2 => (74, Some(75)),
            Encoding::Bc3 => (77, Some(78)),
            Encoding::Bc4Unorm => (80, None),
            Encoding::Bc4Snorm => (81, None),
            Encoding::Bc5Unorm => (83, None),
            Encoding::Bc5Snorm => (84, None),
            Encoding::Bc6hUf16 => (95, None),
            Encoding::Bc6hSf16 => (96, None),
            Encoding::Bc7 => (98, Some(99)),
            Encoding::R32G32B32A32Float => (2, None),
            Encoding::R16G16B16A16Unorm => (11, None),
            Encoding::R16G16B16A16Float => (10, None),
            Encoding::R8G8B8A8Unorm => (28, Some(29)),
            Encoding::B5G6R5Unorm => (85, None),
            Encoding::B5G5R5A1Unorm => (86, None),
            Encoding::R8Unorm => (61, None),
            Encoding::A8Unorm => (65, None),
        }
    }

    /// Bytes per 4 × 4 block for a block-compressed format, `None` for the others.
    pub const fn block_bytes(self) -> Option<u32> {
        match self {
            Encoding::Bc1 | Encoding::Bc4Unorm | Encoding::Bc4Snorm => Some(8),
            Encoding::Bc2
            | Encoding::Bc3
            | Encoding::Bc5Unorm
            | Encoding::Bc5Snorm
            | Encoding::Bc6hUf16
            | Encoding::Bc6hSf16
            | Encoding::Bc7 => Some(16),
            _ => None,
        }
    }

    /// Bytes per pixel for an uncompressed format, `None` for the block-compressed ones.
    pub const fn pixel_bytes(self) -> Option<u32> {
        match self {
            Encoding::R32G32B32A32Float => Some(16),
            Encoding::R16G16B16A16Unorm | Encoding::R16G16B16A16Float => Some(8),
            Encoding::R8G8B8A8Unorm => Some(4),
            Encoding::B5G6R5Unorm | Encoding::B5G5R5A1Unorm => Some(2),
            Encoding::R8Unorm | Encoding::A8Unorm => Some(1),
            _ => None,
        }
    }

    /// The bytes one level of `width` × `height` pixels holds in this format, tightly packed (`None` past `u64`).
    pub fn level_bytes(self, width: u32, height: u32) -> Option<u64> {
        let (w, h) = (u64::from(width.max(1)), u64::from(height.max(1)));
        match (self.block_bytes(), self.pixel_bytes()) {
            (Some(b), _) => w
                .div_ceil(4)
                .checked_mul(h.div_ceil(4))?
                .checked_mul(u64::from(b)),
            (None, Some(p)) => w.checked_mul(h)?.checked_mul(u64::from(p)),
            (None, None) => None,
        }
    }
}

/// A mip level of a PC slice: where its bytes are in the `TXCB` data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mip {
    /// The size in bytes.
    pub size: u32,
    /// The offset in the `TXCB` data.
    pub offset: u32,
}

impl Mip {
    /// The byte range in the `TXCB` data.
    pub fn range(&self) -> Range<usize> {
        let start = usize_of(self.offset);
        start..start.saturating_add(usize_of(self.size))
    }
}

/// A PC slice (an array element, or the one image): its encoding and its mips, largest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slice {
    /// The encoding.
    pub encoding: Encoding,
    /// The mips, largest first; mip `i` is `max(1, width >> i)` × `max(1, height >> i)` pixels.
    pub mips: Vec<Mip>,
}

/// The PC texture header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PcHeader {
    /// The GUID, as stored (the bytes of a Windows GUID).
    pub guid: [u8; 16],
    /// The width of mip 0.
    pub width: u32,
    /// The height of mip 0.
    pub height: u32,
    /// The depth: 1, or the number of layers each mip holds (each layer the mip's size; the depth does not shrink
    /// from mip to mip).
    pub depth: u32,
    /// The u16 at 0x24: the slice count.
    pub slice_count: u16,
    /// The mip count of each slice.
    pub mip_count: u8,
    /// The byte at 0x27, as stored.
    pub flags: u8,
    /// The transcoding (0).
    pub transcoding: u32,
    /// The u32 at 0x2C, as stored.
    pub colour_profile_2c: u32,
    /// The colour profile at 0x30: 0 linear, 1 sRGB.
    pub colour_profile: u32,
    /// The u32 at 0x34, as stored.
    pub field_34: u32,
    /// The slices, in stored order.
    pub slices: Vec<Slice>,
}

impl PcHeader {
    /// Whether the texture is authored as sRGB (colour profile 1).
    pub fn srgb(&self) -> bool {
        self.colour_profile == 1
    }

    /// The size of mip `level`: `max(1, width >> level)` × `max(1, height >> level)`.
    pub fn mip_size(&self, level: u32) -> (u32, u32) {
        let shrink = |v: u32| v.checked_shr(level).unwrap_or(0).max(1);
        (shrink(self.width), shrink(self.height))
    }
}

/// The Durango (Xbox One) texture header, as stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DurangoHeader {
    /// The GUID, as stored.
    pub guid: [u8; 16],
    /// The width of the first mip this file holds.
    pub base_width: u16,
    /// The height of the first mip this file holds.
    pub base_height: u16,
    /// The slice count.
    pub slice_count: u16,
    /// The full texture's width.
    pub full_width: u16,
    /// The full texture's height.
    pub full_height: u16,
    /// The u16 at 26, as stored.
    pub field_26: u16,
    /// The mip count this file holds.
    pub mip_count: u8,
    /// The full texture's mip count.
    pub full_mip_count: u8,
    /// The first mip this file holds.
    pub base_mip: u8,
    /// The bit fields, whole.
    pub bits: u32,
}

impl DurangoHeader {
    /// The console tile mode (bits 0-4).
    pub fn tile_mode(&self) -> u32 {
        self.bits & 0x1F
    }

    /// The encoding value (bits 5-10).
    pub fn encoding_value(&self) -> u32 {
        (self.bits >> 5) & 0x3F
    }

    /// The transcoding (bits 11-16).
    pub fn transcoding(&self) -> u32 {
        (self.bits >> 11) & 0x3F
    }

    /// The colour profile (bits 20-22).
    pub fn colour_profile(&self) -> u32 {
        (self.bits >> 20) & 0x7
    }
}

/// Which header a texture has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Header {
    /// A PC texture (`TXCB` 0.0).
    Pc(PcHeader),
    /// A Durango texture (`TXCB` 2.0).
    Durango(DurangoHeader),
}

/// A texture: its header and its payload as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Texture<'a> {
    /// The header.
    pub header: Header,
    /// The `TXCB` data: the payload as stored (for a PC texture every mip of every slice, see [`Texture::mip`]).
    pub data: &'a [u8],
}

impl<'a> Texture<'a> {
    /// The texture a bundle's bytes hold. Refused: a bundle with blobs other than one `TXCB`, a `TXCB` with metadata
    /// other than one version-0 `TXCH`, another `TXCB` version, and any header the module docs do not describe.
    pub fn parse(bytes: &'a [u8]) -> Result<Texture<'a>> {
        let bundle = Bundle::parse(bytes)?;
        Texture::from_bundle(&bundle)
    }

    /// The texture of a bundle already read.
    pub fn from_bundle(bundle: &Bundle<'a>) -> Result<Texture<'a>> {
        let [blob] = bundle.blobs.as_slice() else {
            return Err(fail(format!(
                "{} blobs ({}), expected one TXCB",
                bundle.blobs.len(),
                bundle
                    .blobs
                    .iter()
                    .map(|b| b.tag.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        };
        if blob.tag != TXCB {
            return Err(fail(format!("the blob is {}, not TXCB", blob.tag)));
        }
        let [meta] = blob.metadata.as_slice() else {
            return Err(fail(format!(
                "the TXCB blob has {} metadata records, expected one TXCH",
                blob.metadata.len()
            )));
        };
        if meta.tag != TXCH || meta.version != 0 {
            return Err(fail(format!(
                "the TXCB metadata is {} version {}, expected TXCH version 0",
                meta.tag, meta.version
            )));
        }
        let header = match blob.version {
            PC => Header::Pc(parse_pc(meta.payload, blob.data.len())?),
            DURANGO => Header::Durango(parse_durango(meta.payload)?),
            v => {
                return Err(fail(format!(
                    "TXCB version {v}, expected {PC} (PC) or {DURANGO} (Durango)"
                )));
            }
        };
        Ok(Texture {
            header,
            data: blob.data,
        })
    }

    /// The bytes of mip `level` of slice `slice` of a PC texture (`None` for a Durango texture or past the counts).
    pub fn mip(&self, slice: usize, level: usize) -> Option<&'a [u8]> {
        let Header::Pc(h) = &self.header else {
            return None;
        };
        let m = h.slices.get(slice)?.mips.get(level)?;
        self.data.get(m.range())
    }
}

fn u32_at(h: &View<'_>, at: usize, what: &str) -> Result<u32> {
    h.u32(at, LE).map_err(|e| fail(format!("the TXCH {what}: {e}")))
}

fn link(next_record: usize) -> Result<u32> {
    next_record
        .checked_add(4)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| fail(format!("a record at {next_record:#x} past a u32 offset")))
}

fn expect(at: usize, found: u32, expected: u32, what: &str) -> Result<()> {
    if found == expected {
        Ok(())
    } else {
        Err(fail(format!(
            "the TXCH {what} at {at:#x} is {found:#x}, expected {expected:#x}"
        )))
    }
}

fn parse_pc(payload: &[u8], data_len: usize) -> Result<PcHeader> {
    let h = View::new(payload);
    expect(0, u32_at(&h, 0, "slice list offset")?, 0x38, "slice list offset")?;
    let guid: [u8; 16] = h.array(8).map_err(|e| fail(format!("the TXCH GUID: {e}")))?;
    let width = u32_at(&h, 0x18, "width")?;
    let height = u32_at(&h, 0x1C, "height")?;
    let depth = u32_at(&h, 0x20, "depth")?;
    let slice_count = h
        .u16(0x24, LE)
        .map_err(|e| fail(format!("the TXCH slice count: {e}")))?;
    let mip_count = h.u8(0x26).map_err(|e| fail(format!("the TXCH mip count: {e}")))?;
    let flags = h.u8(0x27).map_err(|e| fail(format!("the TXCH flags: {e}")))?;
    let transcoding = u32_at(&h, 0x28, "transcoding")?;
    let colour_profile_2c = u32_at(&h, 0x2C, "u32 at 0x2C")?;
    let colour_profile = u32_at(&h, 0x30, "colour profile")?;
    let field_34 = u32_at(&h, 0x34, "u32 at 0x34")?;
    if width == 0 || height == 0 {
        return Err(fail(format!("the TXCH states {width} × {height} pixels")));
    }
    if depth == 0 || slice_count == 0 || mip_count == 0 {
        return Err(fail(format!(
            "the TXCH states a depth of {depth} and {slice_count} slices of {mip_count} mips"
        )));
    }
    if transcoding != 0 {
        return Err(fail(format!(
            "the TXCH transcoding is {transcoding}, not 0 (the only one read)"
        )));
    }
    if colour_profile > 1 {
        return Err(fail(format!(
            "the TXCH colour profile is {colour_profile}, not 0 or 1"
        )));
    }

    let slices_at = 0x40usize;
    expect(
        0x38,
        u32_at(&h, 0x38, "first slice record")?,
        0x40,
        "first slice record",
    )?;
    expect(
        0x3C,
        u32_at(&h, 0x3C, "first slice link")?,
        0x44,
        "first slice link",
    )?;
    let slices_len = usize::from(slice_count).saturating_mul(RECORD_LEN);
    let mips_at = slices_at.saturating_add(slices_len);
    let mips_per_slice = usize::from(mip_count);
    let mip_records = usize::from(slice_count).saturating_mul(mips_per_slice);
    let end = mips_at.saturating_add(mip_records.saturating_mul(RECORD_LEN));
    if end != payload.len() {
        return Err(fail(format!(
            "the TXCH is {:#x} bytes; {slice_count} slices of {mip_count} mips end it at {end:#x}",
            payload.len()
        )));
    }
    expect(
        4,
        u32_at(&h, 4, "first mip link")?,
        link(mips_at)?,
        "first mip link",
    )?;

    let mut slices = Vec::with_capacity(usize::from(slice_count));
    let mut data_at = 0u64;
    for s in 0..usize::from(slice_count) {
        let rec = slices_at.saturating_add(s.saturating_mul(RECORD_LEN));
        let value = u32_at(&h, rec, "slice encoding")?;
        let encoding = Encoding::from_value(value).ok_or_else(|| {
            fail(format!(
                "slice {s}: the encoding at {rec:#x} is {value}, which names no format in the table"
            ))
        })?;
        let first_mip = mips_at.saturating_add(s.saturating_mul(mips_per_slice).saturating_mul(RECORD_LEN));
        expect(
            rec.saturating_add(4),
            u32_at(&h, rec.saturating_add(4), "slice mips")?,
            u32::try_from(first_mip).map_err(|_| fail("a mip record past a u32 offset"))?,
            &format!("slice {s}'s first mip record"),
        )?;
        let next = if s.saturating_add(1) == usize::from(slice_count) {
            END
        } else {
            link(rec.saturating_add(RECORD_LEN))?
        };
        expect(
            rec.saturating_add(8),
            u32_at(&h, rec.saturating_add(8), "slice link")?,
            next,
            &format!("slice {s}'s link"),
        )?;
        let mut mips = Vec::with_capacity(mips_per_slice);
        for m in 0..mips_per_slice {
            let at = first_mip.saturating_add(m.saturating_mul(RECORD_LEN));
            let size = u32_at(&h, at, "mip size")?;
            let offset = u32_at(&h, at.saturating_add(4), "mip offset")?;
            let last =
                s.saturating_add(1) == usize::from(slice_count) && m.saturating_add(1) == mips_per_slice;
            let next = if last {
                END
            } else {
                link(at.saturating_add(RECORD_LEN))?
            };
            expect(
                at.saturating_add(8),
                u32_at(&h, at.saturating_add(8), "mip link")?,
                next,
                &format!("slice {s} mip {m}'s link"),
            )?;
            if u64::from(offset) != data_at {
                return Err(fail(format!(
                    "slice {s} mip {m} starts at {offset:#x} in the data, expected {data_at:#x} (where the previous one ends)"
                )));
            }
            let level = u32::try_from(m).unwrap_or(u32::MAX);
            let shrink = |v: u32| v.checked_shr(level).unwrap_or(0).max(1);
            let (w, hh) = (shrink(width), shrink(height));
            let need = encoding
                .level_bytes(w, hh)
                .and_then(|b| b.checked_mul(u64::from(depth)));
            if need != Some(u64::from(size)) {
                return Err(fail(format!(
                    "slice {s} mip {m} ({w} × {hh}, {encoding:?}) is {size} bytes, the format needs {need:?}"
                )));
            }
            data_at = data_at.saturating_add(u64::from(size));
            mips.push(Mip { size, offset });
        }
        slices.push(Slice { encoding, mips });
    }
    if data_at != data_len as u64 {
        return Err(fail(format!(
            "the mips end at {data_at:#x}, the TXCB data at {data_len:#x}"
        )));
    }
    Ok(PcHeader {
        guid,
        width,
        height,
        depth,
        slice_count,
        mip_count,
        flags,
        transcoding,
        colour_profile_2c,
        colour_profile,
        field_34,
        slices,
    })
}

fn parse_durango(payload: &[u8]) -> Result<DurangoHeader> {
    if payload.len() != DURANGO_LEN {
        return Err(fail(format!(
            "the Durango TXCH is {} bytes, not {DURANGO_LEN}",
            payload.len()
        )));
    }
    let h = View::new(payload);
    let e = |e| fail(format!("the Durango TXCH: {e}"));
    Ok(DurangoHeader {
        guid: h.array(0).map_err(e)?,
        base_width: h.u16(16, LE).map_err(e)?,
        base_height: h.u16(18, LE).map_err(e)?,
        slice_count: h.u16(20, LE).map_err(e)?,
        full_width: h.u16(22, LE).map_err(e)?,
        full_height: h.u16(24, LE).map_err(e)?,
        field_26: h.u16(26, LE).map_err(e)?,
        mip_count: h.u8(28).map_err(e)?,
        full_mip_count: h.u8(29).map_err(e)?,
        base_mip: h.u8(30).map_err(e)?,
        bits: h.u32(31, LE).map_err(e)?,
    })
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
    use crate::test_bundle::bundle;

    /// A PC `TXCH` payload: `slices` slices of `mips` mips of `width` × `height` pixels in `encoding`, depth `depth`,
    /// each mip the size its format needs; also the `TXCB` data length it implies.
    fn pc(
        width: u32,
        height: u32,
        depth: u32,
        encoding: u32,
        slices: u16,
        mips: u8,
        profile: u32,
    ) -> (Vec<u8>, usize) {
        let e = Encoding::from_value(encoding).unwrap();
        let mips_at = 0x40 + RECORD_LEN * usize::from(slices);
        let mut h = vec![0u8; mips_at];
        let put = |h: &mut Vec<u8>, at: usize, v: u32| h[at..at + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut h, 0, 0x38);
        put(&mut h, 4, mips_at as u32 + 4);
        h[8..24].copy_from_slice(&[7; 16]);
        put(&mut h, 0x18, width);
        put(&mut h, 0x1C, height);
        put(&mut h, 0x20, depth);
        h[0x24..0x26].copy_from_slice(&slices.to_le_bytes());
        h[0x26] = mips;
        h[0x27] = 4;
        put(&mut h, 0x2C, profile);
        put(&mut h, 0x30, profile);
        put(&mut h, 0x38, 0x40);
        put(&mut h, 0x3C, 0x44);
        let mut records = Vec::new();
        let mut data_at = 0u32;
        for s in 0..usize::from(slices) {
            let rec = 0x40 + RECORD_LEN * s;
            put(&mut h, rec, encoding);
            put(
                &mut h,
                rec + 4,
                (mips_at + RECORD_LEN * s * usize::from(mips)) as u32,
            );
            let next = if s + 1 == usize::from(slices) {
                END
            } else {
                rec as u32 + RECORD_LEN as u32 + 4
            };
            put(&mut h, rec + 8, next);
            for m in 0..u32::from(mips) {
                let size = e.level_bytes((width >> m).max(1), (height >> m).max(1)).unwrap() as u32 * depth;
                let at = mips_at + RECORD_LEN * (s * usize::from(mips) + m as usize);
                let last = s + 1 == usize::from(slices) && m + 1 == u32::from(mips);
                records.extend_from_slice(&size.to_le_bytes());
                records.extend_from_slice(&data_at.to_le_bytes());
                let next = if last {
                    END
                } else {
                    at as u32 + RECORD_LEN as u32 + 4
                };
                records.extend_from_slice(&next.to_le_bytes());
                data_at += size;
            }
        }
        h.extend_from_slice(&records);
        (h, data_at as usize)
    }

    fn texture(version: (u8, u8), header: &[u8], data: &[u8]) -> Vec<u8> {
        bundle(&[(b"TXCB", version, vec![(b"TXCH", 0, header)], data)])
    }

    fn payload(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn reads_a_mip_chain() {
        let (h, len) = pc(8, 8, 1, 0, 1, 4, 1);
        assert_eq!(len, 32 + 8 + 8 + 8);
        let data = payload(len);
        let bytes = texture((0, 0), &h, &data);
        let t = Texture::parse(&bytes).unwrap();
        let Header::Pc(p) = &t.header else {
            panic!("not PC")
        };
        assert_eq!(
            (p.width, p.height, p.depth, p.slice_count, p.mip_count),
            (8, 8, 1, 1, 4)
        );
        assert!(p.srgb());
        assert_eq!(p.slices[0].encoding, Encoding::Bc1);
        assert_eq!(p.slices[0].encoding.dxgi(), (71, Some(72)));
        assert_eq!(p.mip_size(2), (2, 2));
        assert_eq!(p.slices[0].mips[1], Mip { size: 8, offset: 32 });
        assert_eq!(t.mip(0, 0).unwrap(), &data[..32]);
        assert_eq!(t.mip(0, 3).unwrap(), &data[48..]);
        assert!(t.mip(0, 4).is_none() && t.mip(1, 0).is_none());
    }

    #[test]
    fn reads_an_array_of_a_deep_uncompressed_texture() {
        let (h, len) = pc(4, 2, 3, 13, 2, 2, 0);
        assert_eq!(len, 2 * (32 * 3 + 8 * 3));
        let bytes = texture((0, 0), &h, &payload(len));
        let t = Texture::parse(&bytes).unwrap();
        let Header::Pc(p) = &t.header else {
            panic!("not PC")
        };
        assert_eq!(p.slices.len(), 2);
        assert_eq!(p.slices[1].mips[0].offset, 120);
        assert_eq!(p.slices[1].mips[0].size, 96);
        assert!(!p.srgb());
    }

    #[test]
    fn refuses_what_the_header_does_not_account_for() {
        let (h, len) = pc(8, 8, 1, 0, 1, 4, 0);
        let data = payload(len);
        let refused = |h: &[u8], data: &[u8], version: (u8, u8), expect: &str| {
            let bytes = texture(version, h, data);
            match Texture::parse(&bytes) {
                Err(Error::Texture(m)) => assert!(m.contains(expect), "{m:?} lacks {expect:?}"),
                other => panic!("expected a refusal naming {expect:?}, got {other:?}"),
            }
        };
        let with = |at: usize, v: u32| {
            let mut c = h.clone();
            c[at..at + 4].copy_from_slice(&v.to_le_bytes());
            c
        };
        refused(&with(0, 0x3C), &data, (0, 0), "slice list offset");
        refused(&with(0x40, 99), &data, (0, 0), "names no format");
        refused(&with(0x28, 1), &data, (0, 0), "transcoding");
        refused(&with(0x30, 2), &data, (0, 0), "colour profile");
        refused(&with(0x18, 0), &data, (0, 0), "0 × 8");
        refused(&with(0x4C + 12 + 4, 33), &data, (0, 0), "starts at");
        refused(&with(0x4C + 12, 9), &data, (0, 0), "is 9 bytes");
        refused(&h, &data[..len - 1], (0, 0), "TXCB data");
        refused(&h, &payload(len + 4), (0, 0), "TXCB data");
        let mut long = h.clone();
        long.push(0);
        refused(&long, &data, (0, 0), "bytes");
        refused(&h, &data, (1, 0), "TXCB version");
        let two = bundle(&[
            (b"TXCB", (0, 0), vec![(b"TXCH", 0, &h[..])], &data[..]),
            (b"TXCB", (0, 0), vec![(b"TXCH", 0, &h[..])], &data[..]),
        ]);
        assert!(matches!(Texture::parse(&two), Err(Error::Texture(m)) if m.contains("2 blobs")));
        let other_version = bundle(&[(b"TXCB", (0, 0), vec![(b"TXCH", 1, &h[..])], &data[..])]);
        assert!(matches!(Texture::parse(&other_version), Err(Error::Texture(m)) if m.contains("version 1")));
    }

    #[test]
    fn reads_a_durango_header() {
        let mut h = vec![0u8; DURANGO_LEN];
        h[..16].copy_from_slice(&[9; 16]);
        h[16..18].copy_from_slice(&64u16.to_le_bytes());
        h[18..20].copy_from_slice(&32u16.to_le_bytes());
        h[20..22].copy_from_slice(&1u16.to_le_bytes());
        h[22..24].copy_from_slice(&64u16.to_le_bytes());
        h[24..26].copy_from_slice(&32u16.to_le_bytes());
        h[28] = 5;
        h[29] = 5;
        let bits = 13u32 | (13 << 5) | (1 << 20);
        h[31..35].copy_from_slice(&bits.to_le_bytes());
        let bytes = texture((2, 0), &h, &[1, 2, 3, 4]);
        let t = Texture::parse(&bytes).unwrap();
        let Header::Durango(d) = &t.header else {
            panic!("not Durango")
        };
        assert_eq!((d.base_width, d.base_height, d.mip_count), (64, 32, 5));
        assert_eq!(
            (
                d.tile_mode(),
                d.encoding_value(),
                d.transcoding(),
                d.colour_profile()
            ),
            (13, 13, 0, 1)
        );
        assert!(t.mip(0, 0).is_none());
        let short = texture((2, 0), &h[..34], &[1]);
        assert!(matches!(Texture::parse(&short), Err(Error::Texture(m)) if m.contains("35")));
    }

    #[test]
    fn sizes_levels_by_format() {
        assert_eq!(Encoding::Bc1.level_bytes(1, 1), Some(8));
        assert_eq!(Encoding::Bc7.level_bytes(5, 5), Some(64));
        assert_eq!(Encoding::R8G8B8A8Unorm.level_bytes(3, 2), Some(24));
        for v in 0..=20 {
            if let Some(e) = Encoding::from_value(v) {
                assert_eq!(e.value(), v);
            }
        }
        assert!(Encoding::from_value(16).is_none() && Encoding::from_value(21).is_none());
    }
}
