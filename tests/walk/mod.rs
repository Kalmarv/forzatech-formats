//! Walks the entries of a Forza Horizon 6 install for the real-data tests: the Tokyo track's four GeoChunks
//! (`media/Tracks/Brio/GeoChunkN.minizip`, entries named by `ChunkContentsMiniZipN.txt`), then every zip under `media`
//! in sorted path order (encrypted entries decrypted with the keys), each in stored order. The sources are named as
//! the oracle harnesses name them: `GeoChunkN`, or the zip's path under `media` with `/` separators.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test reading the files you point it at"
)]

use forzatech_formats::keys::Keys;
use forzatech_formats::minizip::{MiniZip, parse_contents};
use forzatech_formats::zip::Zip;
use primase_testdata::Depth;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// Every `.zip` under `dir`, sorted.
pub fn zips(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Whether entry `k` of archive `a` is in a smoke sample drawn with `seed`: about one entry in `every`.
pub fn sampled(seed: u64, a: usize, k: usize, every: u64) -> bool {
    let mut z = seed ^ ((a as u64) << 32) ^ k as u64;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)).is_multiple_of(every)
}

/// Whether a name ends with one of `suffixes`, ASCII case ignored.
pub fn named(name: &str, suffixes: &[&str]) -> bool {
    let low = name.to_ascii_lowercase();
    suffixes.iter().any(|s| low.ends_with(s))
}

/// The walk: where the install is and how much of it to read.
pub struct Walk<'k> {
    /// The track folder holding the GeoChunks.
    pub track: PathBuf,
    /// The install's `media` folder.
    pub media: PathBuf,
    /// The keys for encrypted zip entries.
    pub keys: &'k Keys,
    /// Smoke reads a sample; full reads everything.
    pub depth: Depth,
    /// The sample's seed.
    pub seed: u64,
}

impl Walk<'_> {
    /// Calls `f(source, index, name, bytes)` for every entry whose name `want` accepts: at smoke depth about one in
    /// `every.0` of a GeoChunk's accepted entries and one in `every.1` of a zip's. Only accepted entries are read.
    pub fn each(
        &self,
        want: impl Fn(&str) -> bool,
        every: (u64, u64),
        mut f: impl FnMut(&str, usize, &str, &[u8]),
    ) {
        for n in 0..4usize {
            let mut file = BufReader::with_capacity(
                1 << 20,
                File::open(self.track.join(format!("GeoChunk{n}.minizip"))).unwrap(),
            );
            let zip = MiniZip::read(&mut file).unwrap();
            let text =
                std::fs::read_to_string(self.track.join(format!("ChunkContentsMiniZip{n}.txt"))).unwrap();
            let lines = parse_contents(&text).unwrap();
            assert_eq!(lines.len(), zip.entries().len());
            let source = format!("GeoChunk{n}");
            for (k, line) in lines.iter().enumerate() {
                if !want(line.name) || (self.depth == Depth::Smoke && !sampled(self.seed, n, k, every.0)) {
                    continue;
                }
                let bytes = zip.read_entry(&mut file, k).unwrap();
                f(&source, k, line.name, &bytes);
            }
        }
        for (a, path) in zips(&self.media).into_iter().enumerate() {
            let source = path
                .strip_prefix(&self.media)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let mut file = BufReader::new(File::open(&path).unwrap());
            let zip = Zip::read(&mut file).unwrap();
            for (k, e) in zip.entries().iter().enumerate() {
                if e.name.ends_with('/')
                    || !want(&e.name)
                    || (self.depth == Depth::Smoke && !sampled(self.seed, 4 + a, k, every.1))
                {
                    continue;
                }
                let bytes = zip.read_entry(&mut file, k, Some(self.keys)).unwrap();
                f(&source, k, &e.name, &bytes);
            }
        }
    }
}

/// Lowercase hex of bytes.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(s, "{b:02x}").unwrap();
    }
    s
}

/// The SHA-256 of bytes, in lowercase hex.
pub fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex(&Sha256::digest(bytes))
}

/// The rows of a TSV file after its header line, each split at tabs.
pub fn tsv(path: &Path) -> Vec<Vec<String>> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .skip(1)
        .map(|l| l.split('\t').map(str::to_owned).collect())
        .collect()
}
