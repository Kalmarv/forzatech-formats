//! The zip archives Forza Horizon 6 keeps its settings, cars, shaders and libraries in (`media/*.zip` and the folders
//! below), with their encrypted entries.
//!
//! They are PKWARE zip archives (APPNOTE 6.3): a local header and the entry's bytes for each entry, back to back from
//! offset 0, then the central directory, then the end of central directory record. An archive past 4 GiB stores its
//! sizes and offsets in ZIP64 records. Three methods occur: 0 (stored), 8 (raw DEFLATE) and 22, an entry encrypted in
//! the game's [container](crate::container): its bytes are exactly a small-block container whose plaintext is raw
//! DEFLATE. The CRC-32 of every entry, of method 22 too, is that of its decoded bytes.
//!
//! [`Zip::read`] reads the end records, the central directory and every local header, and checks each local header
//! against its central record; [`Zip::stored`] reads one entry's bytes; [`Zip::decode`] decodes a stored or DEFLATE
//! entry and [`Zip::decrypt`] an encrypted one with keys you supply (the usage whose MAC key authenticates the
//! entry's container is the one that decrypts it). Every decode is checked: its size and its CRC-32 must be the
//! central record's.
//!
//! What has been checked, and how: on one install of Steam build 25588841, all 9,076 zips (402,283 entries, one archive
//! with ZIP64 records) read with every check [`Zip::read`] makes holding, and every entry decodes to its size and
//! CRC-32, the 29,250 encrypted ones with the `General` usage's keys. `tests/zip.rs` repeats that.
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

use crate::container::{BlockSize, Container};
use crate::keys::Keys;
use crate::{Error, Result};
use primase_foundations::bytes::{Cursor, Endian::Little as LE};
use primase_foundations::compress::{Decompressor, Deflate};
use primase_foundations::hash::crc32;
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};

const LOCAL: u32 = 0x0403_4B50;
const CENTRAL: u32 = 0x0201_4B50;
const END: u32 = 0x0605_4B50;
const END64: u32 = 0x0606_4B50;
const LOCATOR64: u32 = 0x0706_4B50;
const END_LEN: u64 = 22;
const LOCATOR64_LEN: u64 = 20;
const LOCAL_LEN: u64 = 30;
/// The ZIP64 extended information extra field's id.
const ZIP64_EXTRA: u16 = 0x0001;
/// The value a 16- or 32-bit field holds when the ZIP64 record holds the real one.
const MAX16: u16 = 0xFFFF;
const MAX32: u32 = 0xFFFF_FFFF;

/// How an entry's bytes are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Method {
    /// 0: as they are.
    Stored,
    /// 8: raw DEFLATE (RFC 1951).
    Deflate,
    /// 22: a small-block [container](crate::container) whose plaintext is raw DEFLATE.
    Encrypted,
    /// Any other method number, kept as stored.
    Other(u16),
}

impl Method {
    /// The method of number `n`.
    pub const fn from_number(n: u16) -> Method {
        match n {
            0 => Method::Stored,
            8 => Method::Deflate,
            22 => Method::Encrypted,
            n => Method::Other(n),
        }
    }

    /// The method's number, as stored.
    pub const fn number(self) -> u16 {
        match self {
            Method::Stored => 0,
            Method::Deflate => 8,
            Method::Encrypted => 22,
            Method::Other(n) => n,
        }
    }
}

/// One entry, from its central directory record (ZIP64 values in place of the fields that defer to them) and its
/// local header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The name as stored: ASCII, `/`-separated; a name ending in `/` is a folder entry.
    pub name: String,
    /// The compression method.
    pub method: Method,
    /// The general purpose flags (0 on every entry read).
    pub flags: u16,
    /// The CRC-32 of the decoded bytes.
    pub crc32: u32,
    /// The stored bytes' length.
    pub compressed_size: u64,
    /// The decoded bytes' length.
    pub size: u64,
    /// Where the entry's local header starts.
    pub local_header: u64,
    /// Where its stored bytes start (after the local header, its name and its extra field).
    pub data: u64,
    /// The central record's version made by and version needed, as stored.
    pub versions: (u16, u16),
}

/// An archive's directory.
#[derive(Clone, Debug)]
pub struct Zip {
    entries: Vec<Entry>,
    by_name: BTreeMap<String, usize>,
    /// Whether the archive's end records are ZIP64.
    pub zip64: bool,
}

impl Zip {
    /// The archive in `r`: its end records, central directory and every entry's local header read and checked, no
    /// entry's bytes. Refused: no end record in the last 22 bytes (the archives carry no comment), a multi-disk
    /// archive, general purpose flags other than 0, a central directory that does not end where the end records start, a
    /// record of another signature or
    /// cut short, ZIP64 values a field defers to but no ZIP64 field gives, a name that is not ASCII or is repeated, a
    /// local header that disagrees with its central record (flags, method, CRC-32, sizes, name), entries that overlap
    /// or leave a gap, or data that does not end where the central directory starts.
    pub fn read<R: Read + Seek>(r: &mut R) -> Result<Zip> {
        let file_len = r.seek(SeekFrom::End(0)).map_err(io("the archive's length"))?;
        let end_at = file_len.checked_sub(END_LEN).ok_or_else(|| {
            fail(format!(
                "{file_len} bytes is shorter than an end of central directory record"
            ))
        })?;
        let end = read_at(r, end_at, END_LEN, "the end of central directory record")?;
        let mut c = Cursor::new(&end);
        let rec = |e| fail(format!("the end of central directory record: {e}"));
        if c.u32(LE).map_err(rec)? != END {
            return Err(fail(
                "no end of central directory record in the last 22 bytes (an archive with a comment is not read)".into(),
            ));
        }
        let (disk, cd_disk, here, total) = (
            c.u16(LE).map_err(rec)?,
            c.u16(LE).map_err(rec)?,
            c.u16(LE).map_err(rec)?,
            c.u16(LE).map_err(rec)?,
        );
        let (mut cd_size, mut cd_offset) = (
            u64::from(c.u32(LE).map_err(rec)?),
            u64::from(c.u32(LE).map_err(rec)?),
        );
        let comment = c.u16(LE).map_err(rec)?;
        if disk != 0 || cd_disk != 0 || here != total {
            return Err(fail(format!(
                "a multi-disk archive (disk {disk}, directory disk {cd_disk}, {here} of {total} entries here)"
            )));
        }
        if comment != 0 {
            return Err(fail(format!(
                "the end record states a {comment}-byte comment past the end of the file"
            )));
        }
        let mut count = u64::from(total);
        let mut directory_end = end_at;
        let zip64 = total == MAX16 || cd_size == u64::from(MAX32) || cd_offset == u64::from(MAX32);
        if zip64 {
            let at = end_at
                .checked_sub(LOCATOR64_LEN)
                .ok_or_else(|| fail("no room for a ZIP64 end of central directory locator".into()))?;
            let loc = read_at(r, at, LOCATOR64_LEN, "the ZIP64 end of central directory locator")?;
            let mut c = Cursor::new(&loc);
            let rec = |e| fail(format!("the ZIP64 locator: {e}"));
            if c.u32(LE).map_err(rec)? != LOCATOR64 {
                return Err(fail(
                    "the end record defers to ZIP64 values, but no ZIP64 locator precedes it".into(),
                ));
            }
            let (disk, end64_at, disks) = (
                c.u32(LE).map_err(rec)?,
                c.u64(LE).map_err(rec)?,
                c.u32(LE).map_err(rec)?,
            );
            if disk != 0 || disks != 1 {
                return Err(fail(format!("the ZIP64 locator names disk {disk} of {disks}")));
            }
            let head = read_at(r, end64_at, 56, "the ZIP64 end of central directory record")?;
            let mut c = Cursor::new(&head);
            let rec = |e| fail(format!("the ZIP64 end record: {e}"));
            if c.u32(LE).map_err(rec)? != END64 {
                return Err(fail(format!(
                    "no ZIP64 end record at {end64_at:#x}, where the locator points"
                )));
            }
            let record_len = c.u64(LE).map_err(rec)?;
            let _versions = (c.u16(LE).map_err(rec)?, c.u16(LE).map_err(rec)?);
            let (disk, cd_disk) = (c.u32(LE).map_err(rec)?, c.u32(LE).map_err(rec)?);
            let (here, all) = (c.u64(LE).map_err(rec)?, c.u64(LE).map_err(rec)?);
            if disk != 0 || cd_disk != 0 || here != all {
                return Err(fail(format!(
                    "a multi-disk ZIP64 archive (disk {disk}, directory disk {cd_disk})"
                )));
            }
            count = all;
            cd_size = c.u64(LE).map_err(rec)?;
            cd_offset = c.u64(LE).map_err(rec)?;
            // the record's length counts what follows its first 12 bytes; it and the locator end where the end record starts
            let ends = end64_at
                .checked_add(12)
                .and_then(|v| v.checked_add(record_len))
                .ok_or_else(|| fail("the ZIP64 end record's length overflows".into()))?;
            if ends != at {
                return Err(fail(format!(
                    "the ZIP64 end record ends at {ends:#x}, not where its locator starts ({at:#x})"
                )));
            }
            directory_end = end64_at;
        }
        if cd_offset.checked_add(cd_size) != Some(directory_end) {
            return Err(fail(format!(
                "the central directory ({cd_size} bytes at {cd_offset:#x}) does not end where the end records start \
                 ({directory_end:#x})"
            )));
        }
        let cd = read_at(r, cd_offset, cd_size, "the central directory")?;
        let mut entries = Vec::new();
        let mut c = Cursor::new(&cd);
        for k in 0..count {
            entries.push(central_record(&mut c, k)?);
        }
        if c.remaining() != 0 {
            return Err(fail(format!(
                "{} bytes of the central directory follow its {count} records",
                c.remaining()
            )));
        }
        let mut by_name = BTreeMap::new();
        for (k, e) in entries.iter_mut().enumerate() {
            local_header(r, e)?;
            if let Some(first) = by_name.insert(e.name.clone(), k) {
                return Err(fail(format!(
                    "entries {first} and {k} are both named {:?}",
                    e.name
                )));
            }
        }
        let mut order: Vec<&Entry> = entries.iter().collect();
        order.sort_by_key(|e| e.local_header);
        let mut at = 0u64;
        for e in order {
            if e.local_header != at {
                return Err(fail(format!(
                    "{:?}'s local header is at {:#x}, but the entry before it ends at {at:#x}",
                    e.name, e.local_header
                )));
            }
            at = e
                .data
                .checked_add(e.compressed_size)
                .ok_or_else(|| fail(format!("{:?}'s data overflows", e.name)))?;
        }
        if at != cd_offset {
            return Err(fail(format!(
                "the entries' data ends at {at:#x}, the central directory starts at {cd_offset:#x}"
            )));
        }
        Ok(Zip {
            entries,
            by_name,
            zip64,
        })
    }

    /// Every entry, in central directory order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The entry named `name`, exactly as stored.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }

    /// Entry `k`.
    pub fn entry(&self, k: usize) -> Result<&Entry> {
        self.entries
            .get(k)
            .ok_or_else(|| fail(format!("entry {k} is past the {} entries", self.entries.len())))
    }

    /// Entry `k`'s stored bytes, read from `r`, the archive this directory was read from.
    pub fn stored<R: Read + Seek>(&self, r: &mut R, k: usize) -> Result<Vec<u8>> {
        let e = self.entry(k)?;
        read_at(r, e.data, e.compressed_size, "an entry's stored bytes")
    }

    /// Entry `k`'s bytes decoded from its stored bytes `stored`: method 0 or 8. Refused: stored bytes not of its
    /// compressed size, another method (an encrypted entry is [`Zip::decrypt`]'s), a DEFLATE stream that does not
    /// decode to exactly its size with every byte used, a CRC-32 other than its record's.
    pub fn decode(&self, k: usize, stored: &[u8]) -> Result<Vec<u8>> {
        let e = self.entry(k)?;
        check_stored(e, stored)?;
        let bytes = match e.method {
            Method::Stored => stored.to_vec(),
            Method::Deflate => inflate(e, stored)?,
            Method::Encrypted => {
                return Err(fail(format!(
                    "{:?} is encrypted (method 22): decrypt it with keys",
                    e.name
                )));
            }
            Method::Other(n) => {
                return Err(fail(format!("{:?}: compression method {n} is not read", e.name)));
            }
        };
        check_crc(e, &bytes)?;
        Ok(bytes)
    }

    /// Encrypted entry `k`'s bytes from its stored bytes `stored`, with `keys`: the stored bytes read as a small-block
    /// container, decrypted with every MAC checked under the usage whose MAC key authenticates its header, then
    /// inflated. Returns the bytes and the usage's name. Refused, besides what [`Zip::decode`] refuses: an entry not of
    /// method 22, stored bytes that are not a container, keys with no transport keys or with no usage that
    /// authenticates the header, a block whose MAC does not match.
    pub fn decrypt<'k>(&self, k: usize, stored: &[u8], keys: &'k Keys) -> Result<(Vec<u8>, &'k str)> {
        let e = self.entry(k)?;
        check_stored(e, stored)?;
        if e.method != Method::Encrypted {
            return Err(fail(format!(
                "{:?} is not encrypted (method {})",
                e.name,
                e.method.number()
            )));
        }
        let container = Container::parse(stored, BlockSize::Small)?;
        let transport = keys
            .transport()
            .ok_or_else(|| fail("the keys give no transport keys".into()))?;
        let (usage, usage_keys) = keys.usage_for(&container).ok_or_else(|| {
            fail(format!(
                "{:?}: none of the keys' usages authenticates its container",
                e.name
            ))
        })?;
        let plain = container.decrypt_verified(transport, usage_keys)?;
        let bytes = inflate(e, &plain)?;
        check_crc(e, &bytes)?;
        Ok((bytes, usage))
    }

    /// Entry `k` read from `r` and decoded: [`Zip::decrypt`] with `keys` for an encrypted entry, else [`Zip::decode`].
    /// `keys` may be `None` for an archive with no encrypted entry.
    pub fn read_entry<R: Read + Seek>(&self, r: &mut R, k: usize, keys: Option<&Keys>) -> Result<Vec<u8>> {
        let stored = self.stored(r, k)?;
        match (self.entry(k)?.method, keys) {
            (Method::Encrypted, Some(keys)) => Ok(self.decrypt(k, &stored, keys)?.0),
            _ => self.decode(k, &stored),
        }
    }
}

/// One central directory record, the k-th.
fn central_record(c: &mut Cursor<'_>, k: u64) -> Result<Entry> {
    let rec = |e| fail(format!("central record {k}: {e}"));
    if c.u32(LE).map_err(rec)? != CENTRAL {
        return Err(fail(format!("central record {k}: another signature")));
    }
    let made = c.u16(LE).map_err(rec)?;
    let needed = c.u16(LE).map_err(rec)?;
    let flags = c.u16(LE).map_err(rec)?;
    if flags != 0 {
        // bit 0 (encrypted), bit 3 (a data descriptor after the data) and the rest change the layout this reads
        return Err(fail(format!(
            "central record {k}: general purpose flags {flags:#06x}"
        )));
    }
    let method = Method::from_number(c.u16(LE).map_err(rec)?);
    let _time_date = (c.u16(LE).map_err(rec)?, c.u16(LE).map_err(rec)?);
    let crc = c.u32(LE).map_err(rec)?;
    let compressed32 = c.u32(LE).map_err(rec)?;
    let size32 = c.u32(LE).map_err(rec)?;
    let name_len = usize::from(c.u16(LE).map_err(rec)?);
    let extra_len = usize::from(c.u16(LE).map_err(rec)?);
    let comment_len = usize::from(c.u16(LE).map_err(rec)?);
    let disk = c.u16(LE).map_err(rec)?;
    let _attributes = (c.u16(LE).map_err(rec)?, c.u32(LE).map_err(rec)?);
    let local32 = c.u32(LE).map_err(rec)?;
    let name = c.take(name_len).map_err(rec)?;
    let extra = c.take(extra_len).map_err(rec)?;
    c.take(comment_len).map_err(rec)?;
    if disk != 0 && disk != MAX16 {
        return Err(fail(format!("central record {k}: on disk {disk}")));
    }
    let name = ascii(name).ok_or_else(|| fail(format!("central record {k}: its name is not ASCII")))?;
    // ZIP64 values come in this order, each only when its field holds the maximum
    let mut wide = zip64_values(extra)
        .map_err(|why| fail(format!("{name:?}: {why}")))?
        .into_iter();
    let mut take = |narrow: u32, what: &str| -> Result<u64> {
        if narrow != MAX32 {
            return Ok(u64::from(narrow));
        }
        wide.next().ok_or_else(|| {
            fail(format!(
                "{name:?}: its {what} defers to a ZIP64 value the extra field does not give"
            ))
        })
    };
    let size = take(size32, "size")?;
    let compressed_size = take(compressed32, "compressed size")?;
    let local_header = take(local32, "local header offset")?;
    Ok(Entry {
        name,
        method,
        flags,
        crc32: crc,
        compressed_size,
        size,
        local_header,
        data: 0,
        versions: (made, needed),
    })
}

/// The 64-bit values of an extra field's ZIP64 block, in order (none without one).
fn zip64_values(extra: &[u8]) -> std::result::Result<Vec<u64>, String> {
    let mut c = Cursor::new(extra);
    while c.remaining() > 0 {
        let id = c.u16(LE).map_err(|e| format!("its extra field: {e}"))?;
        let len = usize::from(c.u16(LE).map_err(|e| format!("its extra field: {e}"))?);
        let body = c.take(len).map_err(|e| format!("its extra field: {e}"))?;
        if id == ZIP64_EXTRA {
            let (words, rest) = body.as_chunks::<8>();
            if !rest.is_empty() {
                return Err(format!("its ZIP64 field is {len} bytes, not a multiple of 8"));
            }
            return Ok(words.iter().map(|w| u64::from_le_bytes(*w)).collect());
        }
    }
    Ok(Vec::new())
}

/// Reads `e`'s local header, checks it against the central record and sets where the data starts.
fn local_header<R: Read + Seek>(r: &mut R, e: &mut Entry) -> Result<()> {
    let head = read_at(r, e.local_header, LOCAL_LEN, "a local header")?;
    let mut c = Cursor::new(&head);
    let rec = |err| fail(format!("{:?}'s local header: {err}", e.name));
    if c.u32(LE).map_err(rec)? != LOCAL {
        return Err(fail(format!(
            "no local header for {:?} at {:#x}",
            e.name, e.local_header
        )));
    }
    let _needed = c.u16(LE).map_err(rec)?;
    let flags = c.u16(LE).map_err(rec)?;
    let method = Method::from_number(c.u16(LE).map_err(rec)?);
    let _time_date = (c.u16(LE).map_err(rec)?, c.u16(LE).map_err(rec)?);
    let crc = c.u32(LE).map_err(rec)?;
    let compressed32 = c.u32(LE).map_err(rec)?;
    let size32 = c.u32(LE).map_err(rec)?;
    let name_len = u64::from(c.u16(LE).map_err(rec)?);
    let extra_len = u64::from(c.u16(LE).map_err(rec)?);
    let after = e
        .local_header
        .checked_add(LOCAL_LEN)
        .ok_or_else(|| fail(format!("{:?}'s local header overflows", e.name)))?;
    let tail_len = name_len.saturating_add(extra_len);
    let tail = read_at(r, after, tail_len, "a local header's name and extra field")?;
    let (name, extra) = tail.split_at(usize::try_from(name_len).unwrap_or(usize::MAX).min(tail.len()));
    let mut wide = zip64_values(extra)
        .map_err(|why| fail(format!("{:?}'s local header: {why}", e.name)))?
        .into_iter();
    // a local header's ZIP64 field gives both sizes when either field holds the maximum
    let (size, compressed) = if size32 == MAX32 || compressed32 == MAX32 {
        (wide.next(), wide.next())
    } else {
        (Some(u64::from(size32)), Some(u64::from(compressed32)))
    };
    if (flags, method, crc, name, size, compressed)
        != (
            e.flags,
            e.method,
            e.crc32,
            e.name.as_bytes(),
            Some(e.size),
            Some(e.compressed_size),
        )
    {
        return Err(fail(format!(
            "{:?}'s local header disagrees with its central record (flags, method, CRC-32, sizes or name)",
            e.name
        )));
    }
    e.data = after
        .checked_add(tail_len)
        .ok_or_else(|| fail(format!("{:?}'s data offset overflows", e.name)))?;
    Ok(())
}

fn check_stored(e: &Entry, stored: &[u8]) -> Result<()> {
    if stored.len() as u64 != e.compressed_size {
        return Err(fail(format!(
            "{:?}: {} stored bytes given, its compressed size is {}",
            e.name,
            stored.len(),
            e.compressed_size
        )));
    }
    Ok(())
}

fn inflate(e: &Entry, stream: &[u8]) -> Result<Vec<u8>> {
    let size = usize::try_from(e.size)
        .map_err(|_| fail(format!("{:?}: {} bytes do not fit in memory", e.name, e.size)))?;
    Deflate
        .decompress(stream, size)
        .map_err(|err| fail(format!("{:?} (method {}): {err}", e.name, e.method.number())))
}

fn check_crc(e: &Entry, bytes: &[u8]) -> Result<()> {
    let crc = crc32(bytes);
    if crc != e.crc32 {
        return Err(fail(format!(
            "{:?}: the decoded bytes' CRC-32 is {crc:08x}, its record's {:08x}",
            e.name, e.crc32
        )));
    }
    Ok(())
}

fn ascii(bytes: &[u8]) -> Option<String> {
    bytes
        .is_ascii()
        .then(|| bytes.iter().map(|&b| char::from(b)).collect())
}

fn fail(why: String) -> Error {
    Error::Zip(why)
}

/// The `len` bytes at `offset` of `r`.
fn read_at<R: Read + Seek>(r: &mut R, offset: u64, len: u64, what: &'static str) -> Result<Vec<u8>> {
    let n = usize::try_from(len).map_err(|_| fail(format!("{what}: {len} bytes do not fit in memory")))?;
    r.seek(SeekFrom::Start(offset)).map_err(io(what))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(n)
        .map_err(|_| fail(format!("{what}: {len} bytes could not be allocated")))?;
    (&mut *r).take(len).read_to_end(&mut bytes).map_err(io(what))?;
    if bytes.len() != n {
        return Err(fail(format!(
            "{what}: {len} bytes at {offset:#x} asked for, the file ends after {}",
            bytes.len()
        )));
    }
    Ok(bytes)
}

fn io(what: &'static str) -> impl Fn(std::io::Error) -> Error {
    move |e| Error::Io(format!("{what}: {e}"))
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
    use primase_foundations::compress::Compressor;

    /// A zip built by hand from (name, method, stored bytes, decoded bytes), optionally with ZIP64 end records and
    /// every entry's sizes and offset in ZIP64 fields.
    fn archive(entries: &[(&str, u16, Vec<u8>, Vec<u8>)], zip64: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, method, stored, decoded) in entries {
            let at = out.len() as u64;
            let crc = crc32(decoded);
            let (c32, s32) = if zip64 {
                (MAX32, MAX32)
            } else {
                (stored.len() as u32, decoded.len() as u32)
            };
            let local_extra: Vec<u8> = if zip64 {
                [
                    &1u16.to_le_bytes()[..],
                    &16u16.to_le_bytes(),
                    &(decoded.len() as u64).to_le_bytes(),
                    &(stored.len() as u64).to_le_bytes(),
                ]
                .concat()
            } else {
                Vec::new()
            };
            out.extend_from_slice(&LOCAL.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0]);
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&[0; 4]);
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&c32.to_le_bytes());
            out.extend_from_slice(&s32.to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&(local_extra.len() as u16).to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&local_extra);
            out.extend_from_slice(stored);
            let central_extra: Vec<u8> = if zip64 {
                [
                    &1u16.to_le_bytes()[..],
                    &24u16.to_le_bytes(),
                    &(decoded.len() as u64).to_le_bytes(),
                    &(stored.len() as u64).to_le_bytes(),
                    &at.to_le_bytes(),
                ]
                .concat()
            } else {
                Vec::new()
            };
            central.extend_from_slice(&CENTRAL.to_le_bytes());
            central.extend_from_slice(&[20, 0, 20, 0, 0, 0]);
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&[0; 4]);
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&c32.to_le_bytes());
            central.extend_from_slice(&s32.to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&(central_extra.len() as u16).to_le_bytes());
            // comment length, disk, internal and external attributes
            central.extend_from_slice(&[0; 10]);
            central.extend_from_slice(&(if zip64 { MAX32 } else { at as u32 }).to_le_bytes());
            central.extend_from_slice(name.as_bytes());
            central.extend_from_slice(&central_extra);
        }
        let cd_at = out.len() as u64;
        out.extend_from_slice(&central);
        let n = entries.len() as u16;
        if zip64 {
            let end64_at = out.len() as u64;
            out.extend_from_slice(&END64.to_le_bytes());
            out.extend_from_slice(&44u64.to_le_bytes());
            out.extend_from_slice(&[45, 0, 45, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend_from_slice(&u64::from(n).to_le_bytes());
            out.extend_from_slice(&u64::from(n).to_le_bytes());
            out.extend_from_slice(&(central.len() as u64).to_le_bytes());
            out.extend_from_slice(&cd_at.to_le_bytes());
            out.extend_from_slice(&LOCATOR64.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&end64_at.to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes());
        }
        out.extend_from_slice(&END.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        let (count, size, offset) = if zip64 {
            (MAX16, MAX32, MAX32)
        } else {
            (n, central.len() as u32, cd_at as u32)
        };
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn sample() -> Vec<(&'static str, u16, Vec<u8>, Vec<u8>)> {
        let text = b"<Settings><Value>1</Value></Settings>".repeat(8);
        vec![
            ("a/stored.txt", 0, b"hello".to_vec(), b"hello".to_vec()),
            ("a/deflated.xml", 8, Deflate.compress(&text).unwrap(), text),
            ("a/", 0, Vec::new(), Vec::new()),
        ]
    }

    #[test]
    fn reads_a_hand_built_archive() {
        for zip64 in [false, true] {
            let file = archive(&sample(), zip64);
            let zip = Zip::read(&mut std::io::Cursor::new(&file)).unwrap();
            assert_eq!(zip.zip64, zip64);
            assert_eq!(zip.entries().len(), 3);
            let k = zip.find("a/deflated.xml").unwrap();
            assert_eq!(zip.entries()[k].method, Method::Deflate);
            let mut r = std::io::Cursor::new(&file);
            assert_eq!(zip.read_entry(&mut r, 0, None).unwrap(), b"hello");
            assert_eq!(zip.read_entry(&mut r, k, None).unwrap(), sample()[1].3);
            assert!(zip.read_entry(&mut r, 2, None).unwrap().is_empty());
            assert_eq!(zip.find("a/missing"), None);
        }
    }

    #[test]
    fn refusals() {
        let good = archive(&sample(), false);
        let read = |b: &[u8]| Zip::read(&mut std::io::Cursor::new(b));
        assert!(read(&good).is_ok());
        let edit = |at: usize, v: &[u8]| {
            let mut b = good.clone();
            b[at..at + v.len()].copy_from_slice(v);
            b
        };
        let end = good.len() - 22;
        assert!(read(&good[..good.len() - 1]).is_err(), "no end record at the end");
        assert!(read(&edit(end + 4, &1u16.to_le_bytes())).is_err(), "multi-disk");
        assert!(read(&edit(end + 20, &3u16.to_le_bytes())).is_err(), "a comment");
        // the local header's CRC-32 of the first entry
        assert!(read(&edit(14, &[1, 2, 3, 4])).is_err(), "local header disagrees");
        assert!(read(&edit(0, b"PK\x03\x05")).is_err(), "local signature");
        // a data descriptor flag on the first entry, in its local header and its central record alike
        let cd = u32::from_le_bytes(good[end + 16..end + 20].try_into().unwrap()) as usize;
        let mut flagged = edit(6, &8u16.to_le_bytes());
        flagged[cd + 8..cd + 10].copy_from_slice(&8u16.to_le_bytes());
        assert!(matches!(read(&flagged), Err(Error::Zip(m)) if m.contains("flags 0x0008")));
        let mut dup = sample();
        dup[1].0 = "a/stored.txt";
        assert!(read(&archive(&dup, false)).is_err(), "a repeated name");
        let mut wide = sample();
        wide[0].0 = "a/\u{e9}.txt";
        assert!(read(&archive(&wide, false)).is_err(), "a name that is not ASCII");
    }

    #[test]
    fn decode_checks_size_and_crc() {
        let mut entries = sample();
        // a stored entry whose CRC-32 is that of other bytes
        entries[0].3 = b"hellp".to_vec();
        let file = archive(&entries, false);
        let zip = Zip::read(&mut std::io::Cursor::new(&file)).unwrap();
        let mut r = std::io::Cursor::new(&file);
        assert!(matches!(zip.read_entry(&mut r, 0, None), Err(Error::Zip(m)) if m.contains("CRC-32")));
        assert!(zip.decode(1, b"short").is_err(), "stored bytes of another size");
        let other = archive(&[("x", 22, b"x".to_vec(), b"x".to_vec())], false);
        let zip = Zip::read(&mut std::io::Cursor::new(&other)).unwrap();
        assert!(matches!(zip.decode(0, b"x"), Err(Error::Zip(m)) if m.contains("method 22")));
        let other = archive(&[("x", 21, b"x".to_vec(), b"x".to_vec())], false);
        let zip = Zip::read(&mut std::io::Cursor::new(&other)).unwrap();
        assert!(matches!(zip.decode(0, b"x"), Err(Error::Zip(m)) if m.contains("method 21")));
    }
}
