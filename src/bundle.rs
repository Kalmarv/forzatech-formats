//! Grub bundles: the container ForzaTech's models (`.modelbin`), textures (`.pb`, `.swatchbin`), materials
//! (`.materialbin`), shaders (`.shaderbin`), crowds (`.crowdbin`) and some tables (`ManufacturerColors.bin`) are
//! stored in, and that a model's material instances are nested in.
//!
//! A bundle is a list of blobs, each a tagged, versioned byte range with tagged metadata records. Layout
//! (little-endian; a tag is a u32 whose big-endian bytes spell its name, so `Grub` is stored as the bytes `burG`):
//!
//! | offset | size | field |
//! |---|---|---|
//! | 0 | 4 | tag `Grub` (the bytes `burG`, [`MAGIC`]) |
//! | 4 | 1 | major version (1) |
//! | 5 | 1 | minor version (1) |
//! | 6 | 2 | reserved (u16; 0) |
//! | 8 | 4 | header size (u32): where the first blob's data starts |
//! | 12 | 4 | total size (u32): the bundle's length |
//! | 16 | 4 | blob count (u32) |
//! | 20 | 24 per blob | blob records |
//!
//! A blob record:
//!
//! | offset | size | field |
//! |---|---|---|
//! | 0 | 4 | tag (u32) |
//! | 4 | 1 | major version |
//! | 5 | 1 | minor version |
//! | 6 | 2 | metadata count (u16) |
//! | 8 | 4 | metadata offset (u32, from the bundle's start) |
//! | 12 | 4 | data offset (u32, from the bundle's start) |
//! | 16 | 4 | data size (u32) |
//! | 20 | 4 | data size again (u32) |
//!
//! A metadata record, 8 bytes at the blob's metadata offset (one after another):
//!
//! | offset | size | field |
//! |---|---|---|
//! | 0 | 4 | tag (u32) |
//! | 4 | 2 | payload size `>> 4` and version `& 0xF` (u16) |
//! | 6 | 2 | payload offset (u16, from the record's own start) |
//!
//! A payload of 0xFFF bytes or more has 0xFFF ([`SIZE_ESCAPE`]) in its size field and its size in a u32 just before
//! it; the record's offset points past that u32.
//!
//! The parts follow one another with nothing unaccounted for: after the blob records, each blob in turn has its
//! metadata records and then their payloads, in record order (a blob without metadata has its offset where the next
//! one's would start). The header ends where the metadata ends, or that rounded up to 4 with zeros. Then each blob's
//! data in record order, the first at the header's end and each next one at the previous one's end rounded up to 4,
//! the gaps zero; the bundle ends where the last blob's data ends, or that rounded up to 4 with zeros.
//! [`Bundle::parse`] refuses any other value or arrangement, with the offset it found and the one it expected.
//!
//! A blob's contents are not read here: they are the family readers' (a model's `Modl`, `Mesh`, `VLay`, ...; a
//! texture's `TXCB` and its `TXCH` metadata; a material's `MATI`, `MTPR`). A blob whose data is itself a bundle (a
//! model's `MatI` material instances, a crowd's `ClsL`) is read with [`Bundle::parse`] on [`Blob::data`].
//!
//! What has been checked, and how: on one install of Forza Horizon 6 (Steam build 25588841), every bundle of the Tokyo
//! track's four GeoChunks and of the zips under `media` (526,129: 196,620 models, 191,602 `.pb` and 126,527
//! `.swatchbin` textures, 9,707 materials, 987 shaders, 682 car colour lists, 4 crowds) and every bundle nested in their
//! blobs (1,526,529) reads with every check [`Bundle::parse`] makes holding. 16 metadata payloads have an escaped size,
//! 11 bundles a header that ends unaligned and 8,418 an unpadded end. `tests/bundle.rs` repeats that, and compares
//! every bundle's blobs, metadata records and payloads, nested ones included, with Doliman100's
//! ForzaTech-extraction-tools' reading of the same bundles: all 526,129 agree (that reader takes a payload's size from
//! the 12-bit field alone, so it sees the first 0xFFF bytes of an escaped payload).
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::{Error, Result};
use primase_foundations::bytes::{Cursor, Endian::Little as LE, View};
use std::fmt;

/// The bundle's first bytes: the tag `Grub` as stored.
pub const MAGIC: [u8; 4] = *b"burG";
/// The one version read.
pub const VERSION: Version = Version { major: 1, minor: 1 };
/// The fixed header's length in bytes, before the blob records.
pub const HEADER_LEN: usize = 20;
/// A blob record's length in bytes.
pub const BLOB_RECORD_LEN: usize = 24;
/// A metadata record's length in bytes.
pub const METADATA_RECORD_LEN: usize = 8;
/// The size field's largest value: a record whose field holds it has its payload's size in a u32 before the payload.
pub const SIZE_ESCAPE: u16 = 0xFFF;

/// A four-character tag: a u32 whose big-endian bytes spell the name (`Modl` is stored as the bytes `ldoM`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tag(pub u32);

impl Tag {
    /// The tag a four-character name spells.
    pub const fn new(name: &[u8; 4]) -> Tag {
        Tag(u32::from_be_bytes(*name))
    }

    /// The tag's four characters.
    pub const fn name(self) -> [u8; 4] {
        self.0.to_be_bytes()
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.name() {
            if b.is_ascii_graphic() || b == b' ' {
                write!(f, "{}", char::from(b))?;
            } else {
                write!(f, "\\x{b:02x}")?;
            }
        }
        Ok(())
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tag({self})")
    }
}

/// A major and minor version, as stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// The first byte.
    pub major: u8,
    /// The second byte.
    pub minor: u8,
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// A metadata record and its payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metadata<'a> {
    /// The record's tag (`Name`, `Id  `, `BBox`, `TXCH`, `ATST`, ...).
    pub tag: Tag,
    /// The low four bits of the record's size field.
    pub version: u8,
    /// Where the payload starts, from the bundle's start.
    pub offset: u32,
    /// The payload.
    pub payload: &'a [u8],
}

/// A blob: its record, its metadata and its data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blob<'a> {
    /// The blob's tag (`Modl`, `Mesh`, `TXCB`, `MatI`, ...).
    pub tag: Tag,
    /// The blob's version.
    pub version: Version,
    /// The record's metadata offset, from the bundle's start (where its metadata records start; for a blob without
    /// metadata, where the next blob's would).
    pub metadata_offset: u32,
    /// The metadata records, in stored order.
    pub metadata: Vec<Metadata<'a>>,
    /// Where the data starts, from the bundle's start.
    pub data_offset: u32,
    /// The data.
    pub data: &'a [u8],
}

impl<'a> Blob<'a> {
    /// The metadata records with a tag, in stored order.
    pub fn metadata_tagged(&self, tag: Tag) -> impl Iterator<Item = &Metadata<'a>> {
        self.metadata.iter().filter(move |m| m.tag == tag)
    }
}

/// A bundle, read from its bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bundle<'a> {
    /// The bundle's version ([`VERSION`]).
    pub version: Version,
    /// Where the blob data starts: the end of the metadata, or that rounded up to 4.
    pub header_size: u32,
    /// The blobs, in record order.
    pub blobs: Vec<Blob<'a>>,
}

/// Whether bytes start with a bundle's [`MAGIC`].
pub fn is_bundle(bytes: &[u8]) -> bool {
    bytes.starts_with(&MAGIC)
}

fn fail(what: impl fmt::Display) -> Error {
    Error::Bundle(what.to_string())
}

fn align4(v: usize) -> Result<usize> {
    v.checked_next_multiple_of(4)
        .ok_or_else(|| fail(format!("an offset {v:#x} past the address space")))
}

fn add(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b)
        .ok_or_else(|| fail(format!("an offset {a:#x} + {b:#x} past the address space")))
}

fn zeros(view: &View<'_>, from: usize, to: usize, what: &str) -> Result<()> {
    let gap = view
        .slice(from, to.saturating_sub(from))
        .map_err(|e| fail(format!("{what}: {e}")))?;
    match gap.iter().position(|&b| b != 0) {
        None => Ok(()),
        Some(k) => Err(fail(format!(
            "{what}: byte {:#x} is {:#04x}, not 0",
            from.saturating_add(k),
            gap.get(k).copied().unwrap_or(0)
        ))),
    }
}

fn usize_of(v: u32) -> usize {
    // u32 to usize is lossless on the targets this crate builds for (32- and 64-bit).
    v as usize
}

impl<'a> Bundle<'a> {
    /// The bundle `bytes` hold, all of them. Refused: another magic, version or reserved value; a total size other
    /// than the length; records, metadata or data out of bounds, out of order, overlapping or with gaps the layout
    /// does not have (see the module docs); a nonzero padding byte; and a second data size unlike the first.
    pub fn parse(bytes: &'a [u8]) -> Result<Bundle<'a>> {
        let view = View::new(bytes);
        let mut c = Cursor::new(bytes);
        let head = |e| fail(format!("the header: {e}"));
        let magic: [u8; 4] = c.array().map_err(head)?;
        if magic != MAGIC {
            return Err(fail(format!("the magic is {magic:02x?}, not `burG`")));
        }
        let version = Version {
            major: c.u8().map_err(head)?,
            minor: c.u8().map_err(head)?,
        };
        if version != VERSION {
            return Err(fail(format!("version {version}, not {VERSION}")));
        }
        let reserved = c.u16(LE).map_err(head)?;
        if reserved != 0 {
            return Err(fail(format!("the u16 at 6 is {reserved:#x}, not 0")));
        }
        let header_size = c.u32(LE).map_err(head)?;
        let total = c.u32(LE).map_err(head)?;
        let count = c.u32(LE).map_err(head)?;
        if usize_of(total) != bytes.len() {
            return Err(fail(format!(
                "the total size is {total:#x}, the bytes are {:#x} long",
                bytes.len()
            )));
        }
        let records_len = usize_of(count)
            .checked_mul(BLOB_RECORD_LEN)
            .ok_or_else(|| fail(format!("{count} blob records")))?;
        let mut cursor = add(HEADER_LEN, records_len)?;
        if cursor > bytes.len() {
            return Err(fail(format!(
                "{count} blob records end at {cursor:#x}, past the end {:#x}",
                bytes.len()
            )));
        }

        struct Record {
            tag: Tag,
            version: Version,
            metadata_offset: u32,
            metadata: Vec<(Tag, u8, usize, usize)>,
            data_offset: u32,
            data_size: u32,
        }
        let mut records = Vec::with_capacity(usize_of(count));
        for i in 0..usize_of(count) {
            let at = add(HEADER_LEN, i.saturating_mul(BLOB_RECORD_LEN))?;
            let rec = view
                .slice(at, BLOB_RECORD_LEN)
                .map_err(|e| fail(format!("blob {i}: {e}")))?;
            let mut r = Cursor::with_origin(rec, at);
            let what = |e| fail(format!("blob {i}: {e}"));
            let tag = Tag(r.u32(LE).map_err(what)?);
            let version = Version {
                major: r.u8().map_err(what)?,
                minor: r.u8().map_err(what)?,
            };
            let metadata_count = r.u16(LE).map_err(what)?;
            let metadata_offset = r.u32(LE).map_err(what)?;
            let data_offset = r.u32(LE).map_err(what)?;
            let data_size = r.u32(LE).map_err(what)?;
            let again = r.u32(LE).map_err(what)?;
            if again != data_size {
                return Err(fail(format!(
                    "blob {i} ({tag}): the second data size {again:#x} is not the first, {data_size:#x}"
                )));
            }
            if usize_of(metadata_offset) != cursor {
                return Err(fail(format!(
                    "blob {i} ({tag}): metadata at {metadata_offset:#x}, expected {cursor:#x}"
                )));
            }
            let first_payload = add(
                cursor,
                usize::from(metadata_count).saturating_mul(METADATA_RECORD_LEN),
            )?;
            let mut payload_at = first_payload;
            let mut metadata = Vec::with_capacity(usize::from(metadata_count));
            for j in 0..usize::from(metadata_count) {
                let m = add(cursor, j.saturating_mul(METADATA_RECORD_LEN))?;
                let what = |e| fail(format!("blob {i} ({tag}) metadata {j}: {e}"));
                let mtag = Tag(view.u32(m, LE).map_err(what)?);
                let size_version = view.u16(add(m, 4)?, LE).map_err(what)?;
                let relative = view.u16(add(m, 6)?, LE).map_err(what)?;
                let at = add(m, usize::from(relative))?;
                let field = size_version >> 4;
                let (start, size) = if field == SIZE_ESCAPE {
                    let long = view.u32(payload_at, LE).map_err(what)?;
                    if long < u32::from(SIZE_ESCAPE) {
                        return Err(fail(format!(
                            "blob {i} ({tag}) metadata {j} ({mtag}): the size field is {SIZE_ESCAPE:#x} and the size before the payload {long:#x}, which the field could hold"
                        )));
                    }
                    (add(payload_at, 4)?, usize_of(long))
                } else {
                    (payload_at, usize::from(field))
                };
                if at != start {
                    return Err(fail(format!(
                        "blob {i} ({tag}) metadata {j} ({mtag}): payload at {at:#x}, expected {start:#x}"
                    )));
                }
                let mversion = u8::try_from(size_version & 0xF).unwrap_or_default();
                metadata.push((mtag, mversion, at, size));
                payload_at = add(at, size)?;
            }
            cursor = payload_at;
            records.push(Record {
                tag,
                version,
                metadata_offset,
                metadata,
                data_offset,
                data_size,
            });
        }

        let aligned = align4(cursor)?;
        let header = usize_of(header_size);
        if header != cursor && header != aligned {
            return Err(fail(format!(
                "the header size is {header_size:#x}; the metadata ends at {cursor:#x}, so expected {cursor:#x} or {aligned:#x}"
            )));
        }
        zeros(&view, cursor, header, "the header's padding")?;

        let mut at = header;
        let last = records.len().checked_sub(1);
        let mut blobs = Vec::with_capacity(records.len());
        for (i, r) in records.into_iter().enumerate() {
            if usize_of(r.data_offset) != at {
                return Err(fail(format!(
                    "blob {i} ({}): data at {:#x}, expected {at:#x}",
                    r.tag, r.data_offset
                )));
            }
            let data = view
                .slice(at, usize_of(r.data_size))
                .map_err(|e| fail(format!("blob {i} ({}) data: {e}", r.tag)))?;
            let end = add(at, data.len())?;
            at = if Some(i) == last && end == bytes.len() {
                end
            } else {
                let next = align4(end)?;
                zeros(&view, end, next, &format!("blob {i} ({})'s padding", r.tag))?;
                next
            };
            let mut metadata = Vec::with_capacity(r.metadata.len());
            for (tag, version, offset, size) in r.metadata {
                let payload = view
                    .slice(offset, size)
                    .map_err(|e| fail(format!("blob {i} ({}) metadata {tag}: {e}", r.tag)))?;
                metadata.push(Metadata {
                    tag,
                    version,
                    // In bounds of `bytes`, whose length is a u32 (the total size).
                    offset: u32::try_from(offset).unwrap_or(u32::MAX),
                    payload,
                });
            }
            blobs.push(Blob {
                tag: r.tag,
                version: r.version,
                metadata_offset: r.metadata_offset,
                metadata,
                data_offset: r.data_offset,
                data,
            });
        }
        if at != bytes.len() {
            return Err(fail(format!(
                "the last blob's data ends at {at:#x} (rounded up to 4 unless it ends the bundle), the bundle at {:#x}",
                bytes.len()
            )));
        }
        Ok(Bundle {
            version,
            header_size,
            blobs,
        })
    }

    /// The blobs with a tag, in record order.
    pub fn tagged(&self, tag: Tag) -> impl Iterator<Item = &Blob<'a>> {
        self.blobs.iter().filter(move |b| b.tag == tag)
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

    /// A bundle built by hand: blob `Modl` v1.4 with a `Name` and an `Id  ` record and 6 data bytes, blob `IndB` v1.0
    /// with no metadata and 4 data bytes.
    fn sample() -> Vec<u8> {
        let mut b = Vec::new();
        let records = HEADER_LEN + 2 * BLOB_RECORD_LEN; // 68
        let meta0 = records; // two records, then "abc" and 4 bytes
        let pay0 = meta0 + 16;
        let meta1 = pay0 + 3 + 4; // 91
        let header = 92;
        b.extend_from_slice(b"burG");
        b.extend_from_slice(&[1, 1, 0, 0]);
        b.extend_from_slice(&(header as u32).to_le_bytes());
        b.extend_from_slice(&104u32.to_le_bytes());
        b.extend_from_slice(&2u32.to_le_bytes());
        for (tag, ver, mcount, moff, doff, dsize) in [
            (b"Modl", [1u8, 4u8], 2u16, meta0, header, 6u32),
            (b"IndB", [1, 0], 0, meta1, header + 8, 4),
        ] {
            b.extend_from_slice(&Tag::new(tag).0.to_le_bytes());
            b.extend_from_slice(&ver);
            b.extend_from_slice(&mcount.to_le_bytes());
            b.extend_from_slice(&(moff as u32).to_le_bytes());
            b.extend_from_slice(&(doff as u32).to_le_bytes());
            b.extend_from_slice(&dsize.to_le_bytes());
            b.extend_from_slice(&dsize.to_le_bytes());
        }
        // metadata records: offsets relative to each record
        b.extend_from_slice(&Tag::new(b"Name").0.to_le_bytes());
        b.extend_from_slice(&(3u16 << 4).to_le_bytes());
        b.extend_from_slice(&((pay0 - meta0) as u16).to_le_bytes());
        b.extend_from_slice(&Tag::new(b"Id  ").0.to_le_bytes());
        b.extend_from_slice(&(4u16 << 4).to_le_bytes());
        b.extend_from_slice(&((pay0 + 3 - (meta0 + 8)) as u16).to_le_bytes());
        b.extend_from_slice(b"abc");
        b.extend_from_slice(&7u32.to_le_bytes());
        b.push(0); // header padding to 92
        b.extend_from_slice(&[1, 2, 3, 4, 5, 6, 0, 0]);
        b.extend_from_slice(&[9, 9, 9, 9]);
        assert_eq!(b.len(), 104);
        b
    }

    #[test]
    fn reads_a_bundle_built_by_hand() {
        let b = sample();
        let bundle = Bundle::parse(&b).unwrap();
        assert_eq!(bundle.header_size, 92);
        assert_eq!(bundle.blobs.len(), 2);
        let modl = &bundle.blobs[0];
        assert_eq!(modl.tag.to_string(), "Modl");
        assert_eq!(modl.version, Version { major: 1, minor: 4 });
        assert_eq!(modl.data, &[1, 2, 3, 4, 5, 6]);
        assert_eq!(modl.metadata[0].tag, Tag::new(b"Name"));
        assert_eq!(modl.metadata[0].payload, b"abc");
        assert_eq!(modl.metadata[1].payload, &7u32.to_le_bytes());
        assert_eq!(modl.metadata[1].offset, 87);
        let indb = &bundle.blobs[1];
        assert!(indb.metadata.is_empty());
        assert_eq!(indb.metadata_offset, 91);
        assert_eq!(indb.data, &[9, 9, 9, 9]);
        assert_eq!(bundle.tagged(Tag::new(b"IndB")).count(), 1);
    }

    #[test]
    fn refuses_what_the_layout_does_not_account_for() {
        let good = sample();
        let cases: [(usize, u8, &str); 6] = [
            (5, 0, "version 1.0"),
            (6, 1, "the u16 at 6"),
            (12, 105, "the total size"),
            (91, 1, "the header's padding"),
            (98, 1, "blob 0 (Modl)'s padding"),
            (40, 1, "the second data size"),
        ];
        for (at, value, expected) in cases {
            let mut b = good.clone();
            b[at] = value;
            let e = Bundle::parse(&b).unwrap_err().to_string();
            assert!(e.contains(expected), "byte {at} = {value}: {e}");
        }
        let mut b = good.clone();
        b[20 + 24 + 12] = 101; // IndB data offset 100 -> 101
        assert!(
            Bundle::parse(&b)
                .unwrap_err()
                .to_string()
                .contains("data at 0x65, expected 0x64")
        );
        assert!(Bundle::parse(&good[..103]).is_err());
    }

    type MetaSpec<'a> = (&'a [u8; 4], u8, &'a [u8]);
    type BlobSpec<'a> = (&'a [u8; 4], [u8; 2], &'a [MetaSpec<'a>], &'a [u8]);

    /// A bundle laid out as the module docs state: `pad_header` rounds the header up to 4, `pad_end` the last blob's
    /// data; a payload of 0xFFF bytes or more gets the escaped size.
    fn build(blobs: &[BlobSpec<'_>], pad_header: bool, pad_end: bool) -> Vec<u8> {
        let mut meta = Vec::new();
        let mut moffs = Vec::new();
        let mut at = HEADER_LEN + blobs.len() * BLOB_RECORD_LEN;
        for (_, _, metas, _) in blobs {
            moffs.push(at);
            let records_at = at;
            let mut payloads = Vec::new();
            let mut pay = records_at + metas.len() * METADATA_RECORD_LEN;
            let mut recs = Vec::new();
            for (j, (tag, version, payload)) in metas.iter().enumerate() {
                let field = if payload.len() >= 0xFFF {
                    payloads.extend_from_slice(&(payload.len() as u32).to_le_bytes());
                    pay += 4;
                    0xFFF
                } else {
                    payload.len() as u16
                };
                let rec = records_at + j * METADATA_RECORD_LEN;
                recs.extend_from_slice(&Tag::new(tag).0.to_le_bytes());
                recs.extend_from_slice(&(field << 4 | u16::from(*version)).to_le_bytes());
                recs.extend_from_slice(&((pay - rec) as u16).to_le_bytes());
                payloads.extend_from_slice(payload);
                pay += payload.len();
            }
            meta.extend_from_slice(&recs);
            meta.extend_from_slice(&payloads);
            at = pay;
        }
        let header = if pad_header { at.next_multiple_of(4) } else { at };
        meta.resize(header - (HEADER_LEN + blobs.len() * BLOB_RECORD_LEN), 0);
        let mut data = Vec::new();
        let mut doffs = Vec::new();
        for (k, (_, _, _, d)) in blobs.iter().enumerate() {
            doffs.push(header + data.len());
            data.extend_from_slice(d);
            if k + 1 < blobs.len() || pad_end {
                data.resize((header + data.len()).next_multiple_of(4) - header, 0);
            }
        }
        let total = header + data.len();
        let mut b = Vec::new();
        b.extend_from_slice(b"burG");
        b.extend_from_slice(&[1, 1, 0, 0]);
        b.extend_from_slice(&(header as u32).to_le_bytes());
        b.extend_from_slice(&(total as u32).to_le_bytes());
        b.extend_from_slice(&(blobs.len() as u32).to_le_bytes());
        for (k, (tag, version, metas, d)) in blobs.iter().enumerate() {
            b.extend_from_slice(&Tag::new(tag).0.to_le_bytes());
            b.extend_from_slice(version);
            b.extend_from_slice(&(metas.len() as u16).to_le_bytes());
            b.extend_from_slice(&(moffs[k] as u32).to_le_bytes());
            b.extend_from_slice(&(doffs[k] as u32).to_le_bytes());
            b.extend_from_slice(&(d.len() as u32).to_le_bytes());
            b.extend_from_slice(&(d.len() as u32).to_le_bytes());
        }
        b.extend_from_slice(&meta);
        b.extend_from_slice(&data);
        b
    }

    #[test]
    fn the_builder_gives_the_hand_built_bundle() {
        let b = build(
            &[
                (
                    b"Modl",
                    [1, 4],
                    &[(b"Name", 0, b"abc"), (b"Id  ", 0, &[7, 0, 0, 0])],
                    &[1, 2, 3, 4, 5, 6],
                ),
                (b"IndB", [1, 0], &[], &[9, 9, 9, 9]),
            ],
            true,
            true,
        );
        assert_eq!(b, sample());
    }

    #[test]
    fn reads_an_unpadded_header_and_an_unpadded_end() {
        let blobs: [BlobSpec<'_>; 2] = [
            (b"TXCB", [2, 0], &[(b"TXCH", 0, b"abcde")], &[1, 2, 3]),
            (b"TXCB", [2, 0], &[], &[4, 5, 6, 7, 8]),
        ];
        for pad_header in [false, true] {
            for pad_end in [false, true] {
                let b = build(&blobs, pad_header, pad_end);
                let bundle = Bundle::parse(&b).unwrap_or_else(|e| panic!("{pad_header} {pad_end}: {e}"));
                assert_eq!(bundle.blobs[0].data, &[1, 2, 3]);
                assert_eq!(bundle.blobs[1].data, &[4, 5, 6, 7, 8]);
                assert_eq!(bundle.blobs[1].data_offset % 4, 0);
                assert_eq!(bundle.header_size.is_multiple_of(4), pad_header);
            }
        }
        // Padding between blobs is not optional: the second blob moved to the first one's unpadded end is refused.
        let mut b = build(&blobs, true, true);
        let first_end = Bundle::parse(&b).unwrap().blobs[0].data_offset + 3;
        b[20 + 24 + 12..20 + 24 + 16].copy_from_slice(&first_end.to_le_bytes());
        assert!(Bundle::parse(&b).unwrap_err().to_string().contains("data at"));
    }

    #[test]
    fn reads_an_escaped_payload_size() {
        let long = vec![0xAB; 5000];
        let b = build(
            &[(
                b"LSCE",
                [1, 10],
                &[(b"BLEN", 1, &[1, 2]), (b"VDCL", 4, &long), (b"XTRA", 8, b"xyz")],
                &[0; 8],
            )],
            true,
            true,
        );
        let bundle = Bundle::parse(&b).unwrap();
        let m = &bundle.blobs[0].metadata;
        assert_eq!(m[1].payload.len(), 5000);
        assert_eq!(m[1].version, 4);
        assert_eq!(m[2].payload, b"xyz");
        // An escaped size that the field could have held is refused.
        let at = m[1].offset as usize - 4;
        let mut b = b.clone();
        b[at..at + 4].copy_from_slice(&100u32.to_le_bytes());
        assert!(
            Bundle::parse(&b)
                .unwrap_err()
                .to_string()
                .contains("which the field could hold")
        );
    }
}
