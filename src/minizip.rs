//! The PGZP streaming archives (`GeoChunkN.minizip`) a track's world is stored in, and the two companion files beside
//! each that type and name its entries (`ChunkMapN.dat`, `ChunkContentsMiniZipN.txt`).
//!
//! An archive names nothing: an entry is a position, a size and a compression method, and belongs to a folder by its
//! position. Layout (little-endian):
//!
//! | offset | size | field |
//! |---|---|---|
//! | 0 | 4 | magic `PGZP` |
//! | 4 | 4 | version (u32; 101 in Forza Horizon 6, 100 or 101 in Forza Horizon 5) |
//! | 8 | 4 | offset of the folder index (u32; 0x20) |
//! | 12 | 4 | entry count (u32) |
//! | 16 | 4 | folder count (u32) |
//! | 20 | 4 | entries per sub-chunk (u32; 512) |
//! | 24 | 4 | sub-chunk count (u32; the entry count divided by the entries per sub-chunk, rounded up) |
//! | 28 | 4 | reserved (u32; 0) |
//!
//! At the folder index's offset: one u32 per folder, the index of its first entry, then the entry count; a folder's
//! entries run from its value to the next. The list is padded with a zero u32 to a multiple of 8 bytes. Then each
//! sub-chunk: its data start (u64, an absolute offset), then one 12-byte record for each of its entries (512, the last
//! sub-chunk the rest):
//!
//! | offset | size | field |
//! |---|---|---|
//! | 0 | 4 | data offset, relative to the sub-chunk's data start (u32) |
//! | 4 | 4 | size once decompressed (u32) |
//! | 8 | 2 | flags (u16): version 101, method `& 0x0FFF` and padding `>> 12`; version 100, `& 0x3FFF` and `>> 14` |
//! | 10 | 2 | the entry's folder, its low 16 bits (u16) |
//!
//! One more record ends the list; only its data offset is used, relative to the last sub-chunk's data start: where the
//! last entry's data ends. An entry's stored bytes run from its offset to the next entry's, less its padding. Methods
//! are 0 (stored), 8 (raw DEFLATE) and 31 (raw LZ4 block); [`MiniZip::decode`] decodes them with
//! [`primase_foundations::compress`].
//!
//! [`MiniZip::read`] takes a reader, never a path, and reads only the header and the directory; [`MiniZip::stored`]
//! reads one entry's bytes. A GeoChunk is tens of gigabytes; nothing here reads the whole of one.
//!
//! What has been checked, and how: on one install's Forza Horizon 6 Tokyo track (`media/Tracks/Brio`, Steam build
//! 25588841), all four GeoChunks (495,394 entries) read with every check [`MiniZip::parse`] makes holding, and every
//! entry decodes to exactly its size with every stored byte used. `tests/minizip.rs` repeats that, and compares every
//! entry, every folder index value and the contents of 1,800 sampled entries with Nenkai ForzaTools' reading of the
//! same files.
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "reader I/O: reads the archive through the reader the caller gives"
)]
#![deny(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use crate::{Error, Result};
use primase_foundations::bytes::{Cursor, Endian::Little as LE};
use primase_foundations::compress::{Decompressor, Deflate, Lz4};
use std::io::{Read, Seek, SeekFrom};
use std::ops::Range;

/// The file's first bytes.
pub const MAGIC: [u8; 4] = *b"PGZP";
/// The versions read.
pub const VERSIONS: [u32; 2] = [100, 101];
/// The header's length in bytes.
pub const HEADER_LEN: usize = 32;
/// An entry record's length in bytes.
pub const RECORD_LEN: usize = 12;

/// The header, as stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// One of [`VERSIONS`].
    pub version: u32,
    /// Where the folder index starts.
    pub folder_index_offset: u32,
    /// How many entries the archive holds.
    pub entry_count: u32,
    /// How many folders.
    pub folder_count: u32,
    /// How many entries each sub-chunk holds (the last one the rest).
    pub entries_per_sub_chunk: u32,
    /// How many sub-chunks.
    pub sub_chunk_count: u32,
    /// The last u32 of the header (0 in every archive read).
    pub reserved: u32,
}

impl Header {
    /// The header from the first [`HEADER_LEN`] bytes of `bytes`. Refused: fewer bytes, another magic or version, no
    /// entries per sub-chunk, a sub-chunk count other than the entry count divided by the entries per sub-chunk
    /// rounded up, and a folder index that starts inside the header.
    pub fn parse(bytes: &[u8]) -> Result<Header> {
        let mut c = Cursor::new(bytes);
        let fail = |e| Error::MiniZip(format!("the header: {e}"));
        let magic: [u8; 4] = c.array().map_err(fail)?;
        if magic != MAGIC {
            return Err(Error::MiniZip(format!("the magic is {magic:02x?}, not PGZP")));
        }
        let h = Header {
            version: c.u32(LE).map_err(fail)?,
            folder_index_offset: c.u32(LE).map_err(fail)?,
            entry_count: c.u32(LE).map_err(fail)?,
            folder_count: c.u32(LE).map_err(fail)?,
            entries_per_sub_chunk: c.u32(LE).map_err(fail)?,
            sub_chunk_count: c.u32(LE).map_err(fail)?,
            reserved: c.u32(LE).map_err(fail)?,
        };
        if !VERSIONS.contains(&h.version) {
            return Err(Error::MiniZip(format!(
                "version {} is not one of {VERSIONS:?}",
                h.version
            )));
        }
        if h.entries_per_sub_chunk == 0 {
            return Err(Error::MiniZip("the header states 0 entries per sub-chunk".into()));
        }
        let expected = h.entry_count.div_ceil(h.entries_per_sub_chunk);
        if h.sub_chunk_count != expected {
            return Err(Error::MiniZip(format!(
                "{} sub-chunks for {} entries of {} per sub-chunk: expected {expected}",
                h.sub_chunk_count, h.entry_count, h.entries_per_sub_chunk
            )));
        }
        if u64::from(h.folder_index_offset) < HEADER_LEN as u64 {
            return Err(Error::MiniZip(format!(
                "the folder index starts at {:#x}, inside the {HEADER_LEN}-byte header",
                h.folder_index_offset
            )));
        }
        Ok(h)
    }

    /// The folder index's length in bytes: a u32 per folder and one more, padded to a multiple of 8.
    pub fn folder_index_len(&self) -> u64 {
        u64::from(self.folder_count)
            .saturating_add(1)
            .saturating_mul(4)
            .next_multiple_of(8)
    }

    /// Where the directory ends (the folder index, every sub-chunk's data start and records, and the record that ends
    /// the list), counted from the start of the file. The archives read store their first entry's data right here.
    pub fn directory_end(&self) -> u64 {
        u64::from(self.folder_index_offset)
            .saturating_add(self.folder_index_len())
            .saturating_add(u64::from(self.sub_chunk_count).saturating_mul(8))
            .saturating_add(
                u64::from(self.entry_count)
                    .saturating_add(1)
                    .saturating_mul(RECORD_LEN as u64),
            )
    }

    /// How an entry's flags split into its method and padding in this version.
    fn split(&self, flags: u16) -> (u16, u8) {
        let (method_bits, shift) = if self.version >= 101 {
            (0x0FFF, 12)
        } else {
            (0x3FFF, 14)
        };
        (
            flags & method_bits,
            u8::try_from(flags >> shift).unwrap_or(u8::MAX),
        )
    }
}

/// How an entry's bytes are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Method {
    /// 0: as they are.
    Stored,
    /// 8: raw DEFLATE (RFC 1951).
    Deflate,
    /// 31: one raw LZ4 block.
    Lz4,
    /// Any other method number, kept as stored.
    Other(u16),
}

impl Method {
    /// The method of number `n`.
    pub const fn from_number(n: u16) -> Method {
        match n {
            0 => Method::Stored,
            8 => Method::Deflate,
            31 => Method::Lz4,
            n => Method::Other(n),
        }
    }

    /// The method's number, as stored.
    pub const fn number(self) -> u16 {
        match self {
            Method::Stored => 0,
            Method::Deflate => 8,
            Method::Lz4 => 31,
            Method::Other(n) => n,
        }
    }
}

/// One entry: its record, and where its bytes are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Where its stored bytes start in the file.
    pub offset: u64,
    /// How many stored bytes it has: to the next entry's offset (or the end of the last entry's data), less the
    /// padding.
    pub stored_size: u64,
    /// Its size once decoded.
    pub size: u32,
    /// The flags as stored.
    pub flags: u16,
    /// The compression method the flags give.
    pub method: Method,
    /// The bytes of padding after its stored bytes, which the flags give.
    pub padding: u8,
    /// The folder field as stored: the low 16 bits of [`MiniZip::folder_of`].
    pub folder_low: u16,
}

/// An archive's header and directory.
#[derive(Clone, Debug)]
pub struct MiniZip {
    /// The header.
    pub header: Header,
    /// Every folder's first entry index, then the entry count.
    folders: Vec<u32>,
    /// Every sub-chunk's data start.
    sub_chunks: Vec<u64>,
    /// Every entry, in order.
    entries: Vec<Entry>,
    /// Where the last entry's data ends.
    data_end: u64,
}

impl MiniZip {
    /// The archive in `r`: its header and directory read and checked, no entry's bytes. See [`MiniZip::parse`] for
    /// what is refused, here with the length of `r` as the file's.
    pub fn read<R: Read + Seek>(r: &mut R) -> Result<MiniZip> {
        let file_len = r.seek(SeekFrom::End(0)).map_err(io("the archive's length"))?;
        let header = Header::parse(&read_at(r, 0, HEADER_LEN as u64, "the header")?)?;
        let directory = read_at(r, 0, header.directory_end(), "the directory")?;
        MiniZip::parse(&directory, file_len)
    }

    /// The archive whose first bytes, at least to its directory's end, are `bytes`, in a file `file_len` bytes long.
    /// Refused, besides what [`Header::parse`] refuses: a directory cut short; a folder index that does not start at 0,
    /// goes down, or does not end with the entry count; an entry whose stored folder is not the low 16 bits of the
    /// folder its position puts it in; an entry starting before the previous one, or with fewer bytes to the next than
    /// its padding; data starting inside the directory or ending past the file; a stored entry whose stored size is not
    /// its size.
    pub fn parse(bytes: &[u8], file_len: u64) -> Result<MiniZip> {
        let header = Header::parse(bytes)?;
        let short = |what: &'static str| short(what, header.directory_end());
        let start =
            usize::try_from(header.folder_index_offset).map_err(|_| Error::MiniZip("too large".into()))?;
        let mut c = Cursor::new(bytes);
        c.seek(start).map_err(short("the folder index"))?;
        let words =
            usize::try_from(header.folder_index_len() / 4).map_err(|_| Error::MiniZip("too large".into()))?;
        let mut index = Vec::with_capacity(words.min(bytes.len() / 4));
        for _ in 0..words {
            index.push(c.u32(LE).map_err(short("the folder index"))?);
        }
        let folder_count =
            usize::try_from(header.folder_count).map_err(|_| Error::MiniZip("too large".into()))?;
        index.truncate(folder_count.saturating_add(1));
        check_folders(&index, header.entry_count)?;

        let per = header.entries_per_sub_chunk;
        let mut sub_chunks = Vec::new();
        let mut records = Vec::new();
        let mut left = header.entry_count;
        for k in 0..header.sub_chunk_count {
            let data_start = c.u64(LE).map_err(short("a sub-chunk's data start"))?;
            sub_chunks.push(data_start);
            let n = left.min(per);
            for _ in 0..n {
                let rel = c.u32(LE).map_err(short("an entry record"))?;
                let size = c.u32(LE).map_err(short("an entry record"))?;
                let flags = c.u16(LE).map_err(short("an entry record"))?;
                let folder_low = c.u16(LE).map_err(short("an entry record"))?;
                let offset = data_start.checked_add(u64::from(rel)).ok_or_else(|| {
                    Error::MiniZip(format!(
                        "sub-chunk {k}: data start {data_start:#x} plus {rel:#x} overflows"
                    ))
                })?;
                records.push((offset, size, flags, folder_low));
            }
            left = left.saturating_sub(n);
        }
        let end_rel = c.u32(LE).map_err(short("the record that ends the list"))?;
        let last_start = sub_chunks.last().copied().unwrap_or(header.directory_end());
        let data_end = last_start
            .checked_add(u64::from(end_rel))
            .ok_or_else(|| Error::MiniZip("the end of the last entry's data overflows".into()))?;
        if data_end > file_len {
            return Err(Error::MiniZip(format!(
                "the last entry's data ends at {data_end:#x}, past the file's {file_len:#x} bytes"
            )));
        }

        let mut entries = Vec::with_capacity(records.len());
        let mut folder = 0usize;
        for (i, &(offset, size, flags, folder_low)) in records.iter().enumerate() {
            let next = records.get(i.saturating_add(1)).map_or(data_end, |r| r.0);
            let (method_number, padding) = header.split(flags);
            let method = Method::from_number(method_number);
            if i == 0 && offset < header.directory_end() {
                return Err(Error::MiniZip(format!(
                    "entry 0 starts at {offset:#x}, inside the directory, which ends at {:#x}",
                    header.directory_end()
                )));
            }
            let span = next.checked_sub(offset).ok_or_else(|| {
                Error::MiniZip(format!(
                    "entry {i} starts at {offset:#x}, after the next entry's {next:#x}"
                ))
            })?;
            let stored_size = span.checked_sub(u64::from(padding)).ok_or_else(|| {
                Error::MiniZip(format!(
                    "entry {i} has {span} bytes to the next, fewer than its padding of {padding}"
                ))
            })?;
            if method == Method::Stored && stored_size != u64::from(size) {
                return Err(Error::MiniZip(format!(
                    "entry {i} is stored, but its {stored_size} stored bytes are not its size of {size}"
                )));
            }
            while index
                .get(folder.saturating_add(1))
                .is_some_and(|&first| first as usize <= i)
            {
                folder = folder.saturating_add(1);
            }
            if u32::from(folder_low) as usize != folder & 0xFFFF {
                return Err(Error::MiniZip(format!(
                    "entry {i} is in folder {folder} by the folder index, but its record says folder {folder_low} \
                     (the folder's low 16 bits)"
                )));
            }
            entries.push(Entry {
                offset,
                stored_size,
                size,
                flags,
                method,
                padding,
                folder_low,
            });
        }
        Ok(MiniZip {
            header,
            folders: index,
            sub_chunks,
            entries,
            data_end,
        })
    }

    /// Every entry, in order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Entry `k`.
    pub fn entry(&self, k: usize) -> Result<&Entry> {
        self.entries
            .get(k)
            .ok_or_else(|| Error::MiniZip(format!("entry {k} is past the {} entries", self.entries.len())))
    }

    /// How many folders.
    pub fn folder_count(&self) -> usize {
        self.folders.len().saturating_sub(1)
    }

    /// The entries of folder `f`, by index.
    pub fn folder(&self, f: usize) -> Option<Range<usize>> {
        let first = *self.folders.get(f)? as usize;
        let end = *self.folders.get(f.checked_add(1)?)? as usize;
        Some(first..end)
    }

    /// The folder entry `k` is in, by the folder index.
    pub fn folder_of(&self, k: usize) -> Option<usize> {
        if k >= self.entries.len() {
            return None;
        }
        let k = u32::try_from(k).ok()?;
        Some(
            self.folders
                .partition_point(|&first| first <= k)
                .saturating_sub(1),
        )
    }

    /// Every sub-chunk's data start.
    pub fn sub_chunk_starts(&self) -> &[u64] {
        &self.sub_chunks
    }

    /// Where the last entry's data ends.
    pub fn data_end(&self) -> u64 {
        self.data_end
    }

    /// Entry `k`'s stored bytes, read from `r`, the archive this directory was read from.
    pub fn stored<R: Read + Seek>(&self, r: &mut R, k: usize) -> Result<Vec<u8>> {
        let e = self.entry(k)?;
        read_at(r, e.offset, e.stored_size, "an entry's stored bytes")
    }

    /// Entry `k`'s bytes decoded from its stored bytes `stored`. Refused: stored bytes not of its stored size; a
    /// method other than 0, 8 and 31; a stream that does not decode to exactly its size with every byte used.
    pub fn decode(&self, k: usize, stored: &[u8]) -> Result<Vec<u8>> {
        let e = self.entry(k)?;
        if stored.len() as u64 != e.stored_size {
            return Err(Error::MiniZip(format!(
                "entry {k}: {} stored bytes given, its stored size is {}",
                stored.len(),
                e.stored_size
            )));
        }
        let size = e.size as usize;
        let decoded = match e.method {
            Method::Stored => return Ok(stored.to_vec()),
            Method::Deflate => Deflate.decompress(stored, size),
            Method::Lz4 => Lz4.decompress(stored, size),
            Method::Other(n) => {
                return Err(Error::MiniZip(format!(
                    "entry {k}: compression method {n} is not read"
                )));
            }
        };
        decoded.map_err(|err| Error::MiniZip(format!("entry {k} ({:?}): {err}", e.method)))
    }

    /// Entry `k` read from `r` and decoded ([`MiniZip::stored`], then [`MiniZip::decode`]).
    pub fn read_entry<R: Read + Seek>(&self, r: &mut R, k: usize) -> Result<Vec<u8>> {
        let stored = self.stored(r, k)?;
        self.decode(k, &stored)
    }
}

/// A directory read that ran past `bytes`, named.
fn short(
    what: &'static str,
    directory_end: u64,
) -> impl Fn(primase_foundations::bytes::BoundsError) -> Error {
    move |e| Error::MiniZip(format!("{what}: {e} (the directory ends at {directory_end:#x})"))
}

/// The folder index's values: from 0, never down, ending with the entry count.
fn check_folders(index: &[u32], entry_count: u32) -> Result<()> {
    if index.first() != Some(&0) {
        return Err(Error::MiniZip(format!(
            "the folder index starts with {:?}, not entry 0",
            index.first()
        )));
    }
    if let Some(f) = index.windows(2).position(|w| matches!(w, [a, b] if b < a)) {
        return Err(Error::MiniZip(format!(
            "the folder index goes down after folder {f}"
        )));
    }
    if index.last() != Some(&entry_count) {
        return Err(Error::MiniZip(format!(
            "the folder index ends with {:?}, not the entry count {entry_count}",
            index.last()
        )));
    }
    Ok(())
}

/// The `len` bytes at `offset` of `r`.
fn read_at<R: Read + Seek>(r: &mut R, offset: u64, len: u64, what: &'static str) -> Result<Vec<u8>> {
    let n = usize::try_from(len)
        .map_err(|_| Error::MiniZip(format!("{what}: {len} bytes do not fit in memory")))?;
    r.seek(SeekFrom::Start(offset)).map_err(io(what))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(n)
        .map_err(|_| Error::MiniZip(format!("{what}: {len} bytes could not be allocated")))?;
    (&mut *r).take(len).read_to_end(&mut bytes).map_err(io(what))?;
    if bytes.len() != n {
        return Err(Error::MiniZip(format!(
            "{what}: {len} bytes at {offset:#x} asked for, the file ends after {}",
            bytes.len()
        )));
    }
    Ok(bytes)
}

fn io(what: &'static str) -> impl Fn(std::io::Error) -> Error {
    move |e| Error::Io(format!("{what}: {e}"))
}

/// One record of `ChunkMapN.dat`, the entry of the same index's resource type and id: a little-endian u32,
/// `resource_type << 24 | id`.
///
/// The id numbers the resources of one type across the track's chunk maps: on the Forza Horizon 6 Tokyo track no
/// (type, id) pair is in two records of its four chunk maps, and other files name an entry by the whole u32
/// ([`ChunkMapRecord::reference`]): a `.pvs` model number is the id of a type-4 (`.modelbin`) record, and a `.pvs`
/// cell record's second reference is a type-12 (`.pvsz`) record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkMapRecord {
    /// The id (24 bits).
    pub id: u32,
    /// The resource type (4 a `.modelbin`, 5 a `.pb` texture, 0 a `.pgeo` placement file, 12 a `.pvsz`, and so on).
    pub resource_type: u8,
}

impl ChunkMapRecord {
    /// The record as other files name it: `resource_type << 24 | id`.
    pub fn reference(&self) -> u32 {
        (u32::from(self.resource_type) << 24) | self.id
    }
}

/// `ChunkMapN.dat`: one 4-byte record per entry of `GeoChunkN`. Refused: a length other than 4 × `entry_count`.
pub fn parse_chunk_map(bytes: &[u8], entry_count: u32) -> Result<Vec<ChunkMapRecord>> {
    if bytes.len() as u64 != u64::from(entry_count).saturating_mul(4) {
        return Err(Error::MiniZip(format!(
            "the chunk map is {} bytes, not 4 per entry of {entry_count}",
            bytes.len()
        )));
    }
    let (records, _) = bytes.as_chunks::<4>();
    Ok(records
        .iter()
        .map(|&[a, b, c, d]| ChunkMapRecord {
            id: u32::from_le_bytes([a, b, c, 0]),
            resource_type: d,
        })
        .collect())
}

/// One line of `ChunkContentsMiniZipN.txt`: the entry of the same index's name and folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContentsLine<'a> {
    /// Whether the line starts with `<PREZIPPED>`.
    pub prezipped: bool,
    /// The name as written, after `<PREZIPPED>`: a path on the machine that built the archive
    /// (`d:\...\zipcache\pc\tracks\brio\...`), or a bare name (`__04965.pvsz`).
    pub name: &'a str,
    /// The folder the entry is in, as written after the last `|`: the whole folder number.
    pub folder: u32,
}

impl ContentsLine<'_> {
    /// The name's path within the build's `zipcache\pc\` folder (`tracks\brio\...`), when it has one.
    pub fn zipcache_path(&self) -> Option<&str> {
        const AT: &str = "\\zipcache\\pc\\";
        self.name
            .find(AT)
            .and_then(|i| self.name.get(i.saturating_add(AT.len())..))
    }
}

/// `ChunkContentsMiniZipN.txt`: one line per entry, `[<PREZIPPED>]name|folder`, each ended by CR LF. Refused: text
/// that does not end with CR LF, a line ended by a bare LF, a line with no `|`, or whose folder is not a decimal number.
pub fn parse_contents(text: &str) -> Result<Vec<ContentsLine<'_>>> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let body = text
        .strip_suffix("\r\n")
        .ok_or_else(|| Error::MiniZip("the contents list does not end with CR LF".into()))?;
    body.split("\r\n")
        .enumerate()
        .map(|(n, line)| {
            let line_no = n.saturating_add(1);
            if line.contains('\n') {
                return Err(Error::MiniZip(format!(
                    "contents line {line_no}: ended by a bare LF, not CR LF"
                )));
            }
            let (name, folder) = line.rsplit_once('|').ok_or_else(|| {
                Error::MiniZip(format!("contents line {line_no}: no '|' before the folder"))
            })?;
            let folder = folder.parse().map_err(|_| {
                Error::MiniZip(format!(
                    "contents line {line_no}: folder {folder:?} is not a number"
                ))
            })?;
            let (prezipped, name) = match name.strip_prefix("<PREZIPPED>") {
                Some(rest) => (true, rest),
                None => (false, name),
            };
            Ok(ContentsLine {
                prezipped,
                name,
                folder,
            })
        })
        .collect()
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::unwrap_used,
    reason = "tests on archives built by hand"
)]
mod tests {
    use super::*;

    /// An archive built by hand: `entries` of (size, flags, folder-low), stored bytes of the given lengths placed
    /// back to back after the directory with each entry's padding, `per` entries per sub-chunk, and `folders` the
    /// folder index (first entries, then the count).
    fn archive(per: u32, folders: &[u32], entries: &[(u32, u16, u16, &[u8])]) -> Vec<u8> {
        let count = entries.len() as u32;
        let subs = count.div_ceil(per);
        let header = Header {
            version: 101,
            folder_index_offset: 0x20,
            entry_count: count,
            folder_count: folders.len() as u32 - 1,
            entries_per_sub_chunk: per,
            sub_chunk_count: subs,
            reserved: 0,
        };
        let data_start = header.directory_end();
        let mut out = Vec::new();
        out.extend_from_slice(&MAGIC);
        for v in [101, 0x20, count, header.folder_count, per, subs, 0] {
            out.extend_from_slice(&u32::to_le_bytes(v));
        }
        let mut index: Vec<u8> = folders.iter().flat_map(|v| v.to_le_bytes()).collect();
        index.resize(header.folder_index_len() as usize, 0);
        out.extend_from_slice(&index);
        let mut data = Vec::new();
        let mut sub_start = data_start;
        for (i, (size, flags, folder, stored)) in entries.iter().enumerate() {
            if (i as u32).is_multiple_of(per) {
                sub_start = data_start + data.len() as u64;
                out.extend_from_slice(&sub_start.to_le_bytes());
            }
            let rel = (data_start + data.len() as u64 - sub_start) as u32;
            out.extend_from_slice(&rel.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&flags.to_le_bytes());
            out.extend_from_slice(&folder.to_le_bytes());
            data.extend_from_slice(stored);
            data.resize(data.len() + (flags >> 12) as usize, 0);
        }
        let end = (data_start + data.len() as u64 - sub_start) as u32;
        out.extend_from_slice(&[end.to_le_bytes(), [0; 4], [0; 4]].concat());
        assert_eq!(out.len() as u64, data_start);
        out.extend_from_slice(&data);
        out
    }

    fn lz4(bytes: &[u8]) -> Vec<u8> {
        use primase_foundations::compress::Compressor;
        Lz4.compress(bytes).unwrap()
    }

    fn deflate(bytes: &[u8]) -> Vec<u8> {
        use primase_foundations::compress::Compressor;
        Deflate.compress(bytes).unwrap()
    }

    #[test]
    fn reads_a_hand_built_archive() {
        let text = b"PGZP archives name nothing; PGZP archives name nothing.".repeat(4);
        let (l, d) = (lz4(&text), deflate(&text));
        let n = text.len() as u32;
        // three entries in two sub-chunks of two; folders: {0, 1}, {2}
        let file = archive(
            2,
            &[0, 2, 3],
            &[(5, 0x3000, 0, b"hello"), (n, 0x1000 | 31, 0, &l), (n, 8, 1, &d)],
        );
        let zip = MiniZip::read(&mut std::io::Cursor::new(&file)).unwrap();
        assert_eq!(zip.header.sub_chunk_count, 2);
        assert_eq!(zip.sub_chunk_starts().len(), 2);
        assert_eq!(zip.folder_count(), 2);
        assert_eq!(zip.folder(0), Some(0..2));
        assert_eq!(zip.folder(1), Some(2..3));
        assert_eq!(zip.folder_of(2), Some(1));
        assert_eq!(zip.folder_of(3), None);
        let e = zip.entries();
        assert_eq!(
            (e[0].method, e[0].padding, e[0].stored_size),
            (Method::Stored, 3, 5)
        );
        assert_eq!(
            (e[1].method, e[1].padding, e[1].stored_size),
            (Method::Lz4, 1, l.len() as u64)
        );
        assert_eq!(
            (e[2].method, e[2].padding, e[2].stored_size),
            (Method::Deflate, 0, d.len() as u64)
        );
        assert_eq!(zip.data_end(), file.len() as u64);
        let mut r = std::io::Cursor::new(&file);
        assert_eq!(zip.read_entry(&mut r, 0).unwrap(), b"hello");
        assert_eq!(zip.read_entry(&mut r, 1).unwrap(), text);
        assert_eq!(zip.read_entry(&mut r, 2).unwrap(), text);
        assert!(zip.read_entry(&mut r, 3).is_err());
    }

    #[test]
    fn version_100_splits_flags_at_bit_14() {
        let h = Header {
            version: 100,
            ..Header::parse(&archive(1, &[0, 1], &[(1, 0, 0, b"x")])).unwrap()
        };
        assert_eq!(h.split(0x4000 | 31), (31, 1));
        assert_eq!(h.split(0x3FFF), (0x3FFF, 0));
        let h = Header { version: 101, ..h };
        assert_eq!(h.split(0x3000 | 31), (31, 3));
    }

    #[test]
    fn the_folder_index_pads_to_eight_bytes() {
        let h = |folder_count| Header {
            version: 101,
            folder_index_offset: 0x20,
            entry_count: 0,
            folder_count,
            entries_per_sub_chunk: 512,
            sub_chunk_count: 0,
            reserved: 0,
        };
        assert_eq!(h(1).folder_index_len(), 8);
        assert_eq!(h(2).folder_index_len(), 16);
        assert_eq!(h(81_657).folder_index_len(), 326_632);
    }

    #[test]
    fn the_folder_field_is_the_folders_low_16_bits() {
        // 65,537 folders of one entry each: the last entry's record says folder 0
        let count = 65_537u32;
        let folders: Vec<u32> = (0..=count).collect();
        let rows: Vec<(u32, u16, u16, &[u8])> = (0..count)
            .map(|i| (1, 0, (i & 0xFFFF) as u16, &b"x"[..]))
            .collect();
        let zip = MiniZip::read(&mut std::io::Cursor::new(archive(512, &folders, &rows))).unwrap();
        assert_eq!(zip.folder_of(65_536), Some(65_536));
        assert_eq!(zip.entries()[65_536].folder_low, 0);
    }

    #[test]
    fn refusals() {
        let good = archive(2, &[0, 3], &[(1, 0, 0, b"a"), (1, 0, 0, b"b"), (1, 0, 0, b"c")]);
        let read = |b: &[u8]| MiniZip::parse(b, b.len() as u64);
        assert!(read(&good).is_ok());
        let edit = |at: usize, v: &[u8]| {
            let mut b = good.clone();
            b[at..at + v.len()].copy_from_slice(v);
            b
        };
        assert!(read(&edit(0, b"PGZQ")).is_err(), "magic");
        assert!(read(&edit(4, &102u32.to_le_bytes())).is_err(), "version");
        assert!(
            read(&edit(20, &0u32.to_le_bytes())).is_err(),
            "no entries per sub-chunk"
        );
        assert!(read(&edit(24, &3u32.to_le_bytes())).is_err(), "sub-chunk count");
        assert!(
            read(&edit(0x20, &1u32.to_le_bytes())).is_err(),
            "folder index not from 0"
        );
        assert!(
            read(&edit(0x24, &2u32.to_le_bytes())).is_err(),
            "folder index not ending at the count"
        );
        // the first record: after the header, the index (8 bytes) and the sub-chunk's data start
        let record = 0x20 + 8 + 8;
        assert!(
            read(&edit(record + 10, &1u16.to_le_bytes())).is_err(),
            "folder field"
        );
        assert!(
            read(&edit(record + 4, &2u32.to_le_bytes())).is_err(),
            "stored size is not the size"
        );
        assert!(
            read(&edit(record + 8, &0x2000u16.to_le_bytes())).is_err(),
            "padding past the next entry"
        );
        assert!(read(&good[..good.len() - 1]).is_err(), "data past the file");
        assert!(read(&good[..40]).is_err(), "directory cut short");
        let zip = read(&good).unwrap();
        assert!(zip.decode(0, b"ab").is_err(), "stored bytes of another size");
    }

    #[test]
    fn decode_refuses_streams_that_do_not_fit() {
        let text = b"abcabcabcabcabcabc";
        let d = deflate(text);
        // padding is not part of the stream
        let file = archive(1, &[0, 1], &[(text.len() as u32, 8 | 0x2000, 0, &d)]);
        let zip = MiniZip::read(&mut std::io::Cursor::new(&file)).unwrap();
        assert_eq!(zip.entries()[0].stored_size, d.len() as u64);
        assert_eq!(zip.read_entry(&mut std::io::Cursor::new(&file), 0).unwrap(), text);
        // a byte after the stream that the padding does not account for is refused, as is a wrong size
        let mut long = d.clone();
        long.push(0);
        let file = archive(1, &[0, 1], &[(text.len() as u32, 8, 0, &long)]);
        let zip = MiniZip::read(&mut std::io::Cursor::new(&file)).unwrap();
        assert!(zip.read_entry(&mut std::io::Cursor::new(&file), 0).is_err());
        let file = archive(1, &[0, 1], &[(text.len() as u32 + 1, 8, 0, &d)]);
        let zip = MiniZip::read(&mut std::io::Cursor::new(&file)).unwrap();
        assert!(zip.read_entry(&mut std::io::Cursor::new(&file), 0).is_err());
        let other = archive(1, &[0, 1], &[(1, 22, 0, b"x")]);
        let zip = MiniZip::read(&mut std::io::Cursor::new(&other)).unwrap();
        assert!(matches!(zip.decode(0, b"x"), Err(Error::MiniZip(m)) if m.contains("method 22")));
    }

    #[test]
    fn chunk_map_records() {
        let map = parse_chunk_map(&[1, 2, 3, 4, 0, 0, 0, 5], 2).unwrap();
        assert_eq!(
            map,
            [
                ChunkMapRecord {
                    id: 0x03_0201,
                    resource_type: 4
                },
                ChunkMapRecord {
                    id: 0,
                    resource_type: 5
                },
            ]
        );
        assert_eq!(map[0].reference(), 0x0403_0201);
        assert!(parse_chunk_map(&[0; 7], 2).is_err());
    }

    #[test]
    fn contents_lines() {
        let text = "<PREZIPPED>d:\\scratch\\zipcache\\pc\\tracks\\brio\\a|b.pgeo|0\r\n__04965.pvsz|65536\r\n";
        let lines = parse_contents(text).unwrap();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].prezipped);
        assert_eq!(lines[0].name, "d:\\scratch\\zipcache\\pc\\tracks\\brio\\a|b.pgeo");
        assert_eq!(lines[0].zipcache_path(), Some("tracks\\brio\\a|b.pgeo"));
        assert_eq!(lines[0].folder, 0);
        assert_eq!(
            lines[1],
            ContentsLine {
                prezipped: false,
                name: "__04965.pvsz",
                folder: 65_536
            }
        );
        assert_eq!(lines[1].zipcache_path(), None);
        assert!(parse_contents("").unwrap().is_empty());
        assert!(parse_contents("no-folder\r\n").is_err());
        assert!(parse_contents("a|x\r\n").is_err());
        assert!(parse_contents("a|0\nb|1\r\n").is_err(), "a bare LF");
        assert!(parse_contents("a|0").is_err(), "no CR LF at the end");
    }
}
