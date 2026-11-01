//! What the placement tests share: the Tokyo track's chunk maps and contents lists, the index built from them, and a
//! seeded sample.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "shared by tests that read the files you point them at; each uses some"
)]

use forzatech_formats::minizip::{ChunkMapRecord, ContentsLine, parse_chunk_map, parse_contents};
use forzatech_formats::track::Track;
use std::path::Path;

/// The four chunk maps and contents lists of `media/Tracks/Brio`.
pub struct Chunks {
    pub maps: Vec<Vec<ChunkMapRecord>>,
    pub texts: Vec<String>,
}

impl Chunks {
    pub fn read(track: &Path) -> Chunks {
        let mut maps = Vec::new();
        let mut texts = Vec::new();
        for n in 0..4 {
            let map = std::fs::read(track.join(format!("ChunkMap{n}.dat"))).unwrap();
            let count = u32::try_from(map.len() / 4).unwrap();
            maps.push(parse_chunk_map(&map, count).unwrap());
            texts.push(std::fs::read_to_string(track.join(format!("ChunkContentsMiniZip{n}.txt"))).unwrap());
        }
        Chunks { maps, texts }
    }

    pub fn lines(&self) -> Vec<Vec<ContentsLine<'_>>> {
        self.texts.iter().map(|t| parse_contents(t).unwrap()).collect()
    }

    /// The `(chunk, entry)` of every record of `resource_type`, in order.
    pub fn of_type(&self, resource_type: u8) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (chunk, map) in self.maps.iter().enumerate() {
            for (entry, r) in map.iter().enumerate() {
                if r.resource_type == resource_type {
                    out.push((chunk, entry));
                }
            }
        }
        out
    }
}

/// The track's index.
pub fn index(chunks: &Chunks) -> Track {
    let lines = chunks.lines();
    Track::new(chunks.maps.iter().zip(&lines).map(|(m, l)| (&m[..], &l[..]))).unwrap()
}

/// Whether item `k` of a population is in a smoke sample drawn with `seed`: about one in `every`.
pub fn sampled(seed: u64, k: usize, every: u64) -> bool {
    let mut z = seed ^ (k as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)).is_multiple_of(every)
}
