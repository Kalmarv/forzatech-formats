//! A Grub bundle built by hand, for the unit tests of the family readers.
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    reason = "tests"
)]

use crate::bundle::{BLOB_RECORD_LEN, HEADER_LEN, METADATA_RECORD_LEN, Tag};

/// A metadata record to build: tag, version, payload (under 0xFFF bytes).
pub type Meta<'a> = (&'a [u8; 4], u8, &'a [u8]);

/// A blob to build: tag, version (major, minor), metadata records, data.
pub type BlobSpec<'a> = (&'a [u8; 4], (u8, u8), Vec<Meta<'a>>, &'a [u8]);

fn pad(b: &mut Vec<u8>) {
    while !b.len().is_multiple_of(4) {
        b.push(0);
    }
}

/// The bytes of a bundle with these blobs, laid out as the game's are (see [`crate::bundle`]).
pub fn bundle(blobs: &[BlobSpec<'_>]) -> Vec<u8> {
    let records = HEADER_LEN + BLOB_RECORD_LEN * blobs.len();
    // metadata: records then payloads, blob by blob
    let mut meta: Vec<u8> = Vec::new();
    let mut meta_at = Vec::new();
    for (_, _, records_of, _) in blobs {
        let start = records + meta.len();
        meta_at.push(start);
        let payload_at = start + METADATA_RECORD_LEN * records_of.len();
        let mut payloads: Vec<u8> = Vec::new();
        for (i, (tag, version, payload)) in records_of.iter().enumerate() {
            assert!(payload.len() < 0xFFF);
            meta.extend_from_slice(&Tag::new(tag).0.to_le_bytes());
            let size_and_version = ((payload.len() as u16) << 4) | u16::from(*version);
            meta.extend_from_slice(&size_and_version.to_le_bytes());
            let record_start = start + METADATA_RECORD_LEN * i;
            let offset = payload_at + payloads.len() - record_start;
            meta.extend_from_slice(&(offset as u16).to_le_bytes());
            payloads.extend_from_slice(payload);
        }
        meta.extend_from_slice(&payloads);
    }
    let mut header_end = records + meta.len();
    header_end = header_end.next_multiple_of(4);
    let mut data: Vec<u8> = Vec::new();
    let mut data_at = Vec::new();
    for (_, _, _, d) in blobs {
        while !(header_end + data.len()).is_multiple_of(4) {
            data.push(0);
        }
        data_at.push(header_end + data.len());
        data.extend_from_slice(d);
    }
    let total = header_end + data.len();
    let mut b = Vec::new();
    b.extend_from_slice(b"burG");
    b.extend_from_slice(&[1, 1, 0, 0]);
    b.extend_from_slice(&(header_end as u32).to_le_bytes());
    b.extend_from_slice(&(total as u32).to_le_bytes());
    b.extend_from_slice(&(blobs.len() as u32).to_le_bytes());
    for (i, (tag, (major, minor), records_of, d)) in blobs.iter().enumerate() {
        b.extend_from_slice(&Tag::new(tag).0.to_le_bytes());
        b.extend_from_slice(&[*major, *minor]);
        b.extend_from_slice(&(records_of.len() as u16).to_le_bytes());
        b.extend_from_slice(&(meta_at[i] as u32).to_le_bytes());
        b.extend_from_slice(&(data_at[i] as u32).to_le_bytes());
        b.extend_from_slice(&(d.len() as u32).to_le_bytes());
        b.extend_from_slice(&(d.len() as u32).to_le_bytes());
    }
    b.extend_from_slice(&meta);
    pad(&mut b);
    b.extend_from_slice(&data);
    b
}
