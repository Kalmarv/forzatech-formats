//! Reads a real install's zip archives: every `.zip` under Forza Horizon 6's `media` folder, from the install the
//! `GAME_DATA` configuration names, with the keys it names when it names them (see tests/inputs/README.md). Without
//! an install the test skips with a line naming its key.
//!
//! Every archive's directory reads and every local header agrees with its central record. Entries decode to their
//! size and to the CRC-32 their archive's writer stored for them: a sample drawn with the day's seed, or every entry
//! with `[run] gate = 'full'`. With keys, the encrypted (method 22) entries are decrypted too, every MAC checked; the
//! test prints which usage decrypts the entries of each folder.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test reading the files you point it at"
)]

mod inputs;

use forzatech_formats::keys::Keys;
use forzatech_formats::zip::{Method, Zip};
use inputs::{Game, install, keys};
use primase_testdata::{Depth, depth, seed};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// Every `.zip` under `dir`, sorted.
fn zips(dir: &Path) -> Vec<PathBuf> {
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

/// Whether entry `k` of archive `a` is in a smoke sample drawn with `seed`: about one entry in `every`, and always
/// an archive's first and last.
fn sampled(seed: u64, a: usize, k: usize, last: usize, every: u64) -> bool {
    let mut z = seed ^ ((a as u64) << 32) ^ k as u64;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    k == 0 || k == last || (z ^ (z >> 31)).is_multiple_of(every)
}

#[test]
fn every_zip_reads_and_its_entries_decode() {
    let Some(root) = install(Game::Fh6) else {
        return;
    };
    let media = root.join("media");
    let keys: Option<Keys> = keys(Game::Fh6).map(|path| {
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        Keys::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    });
    let (depth, seed) = (depth(), seed());
    println!("depth {depth:?}, seed {seed}, keys {}", keys.is_some());
    let archives = zips(&media);
    let mut methods: BTreeMap<Method, usize> = BTreeMap::new();
    let mut decoded: BTreeMap<Method, usize> = BTreeMap::new();
    // the usages that decrypt each folder's encrypted entries, by folder under media
    let mut usages: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut zip64 = 0usize;
    for (a, path) in archives.iter().enumerate() {
        let mut file = BufReader::with_capacity(1 << 16, File::open(path).unwrap());
        let zip = Zip::read(&mut file).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        zip64 += usize::from(zip.zip64);
        let folder = path
            .parent()
            .unwrap()
            .strip_prefix(&media)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let last = zip.entries().len().saturating_sub(1);
        for (k, e) in zip.entries().iter().enumerate() {
            *methods.entry(e.method).or_default() += 1;
            let take = match depth {
                Depth::Full => true,
                Depth::Smoke => e.method == Method::Encrypted || sampled(seed, a, k, last, 64),
            };
            if !take {
                continue;
            }
            let stored = zip.stored(&mut file, k).unwrap();
            match (e.method, &keys) {
                (Method::Encrypted, None) => continue,
                (Method::Encrypted, Some(keys)) => {
                    let (bytes, usage) = zip
                        .decrypt(k, &stored, keys)
                        .unwrap_or_else(|err| panic!("{} {:?}: {err}", path.display(), e.name));
                    assert_eq!(bytes.len() as u64, e.size);
                    *usages
                        .entry(folder.clone())
                        .or_default()
                        .entry(usage.to_owned())
                        .or_default() += 1;
                }
                _ => {
                    let bytes = zip
                        .decode(k, &stored)
                        .unwrap_or_else(|err| panic!("{} {:?} (seed {seed}): {err}", path.display(), e.name));
                    assert_eq!(bytes.len() as u64, e.size);
                }
            }
            *decoded.entry(e.method).or_default() += 1;
        }
    }
    println!(
        "{} archives ({zip64} with ZIP64 records); entries by method {methods:?}; decoded {decoded:?}",
        archives.len()
    );
    for (folder, by_usage) in &usages {
        println!("  encrypted entries in media/{folder}: {by_usage:?}");
    }
}
