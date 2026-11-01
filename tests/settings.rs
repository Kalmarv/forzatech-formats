//! Reads every `DatabaseFile` settings file of a real install: every `.xml` entry of every zip under Forza Horizon 6's
//! `media` folder, decrypted with the keys the `GAME_DATA` configuration names (see tests/inputs/README.md), and read
//! with `settings::DatabaseFile::parse` when its root element is `DatabaseFile`. Without the install or the keys the
//! test skips with a line naming its key.
//!
//! On Steam build 25588841 every one reads but `Rendering.zip`'s `Cubemap.xml`, whose last `Settings` block is closed
//! by `</property>`: the test requires exactly that refusal, so a change in either direction is seen.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test reading the files you point it at"
)]

mod inputs;

use forzatech_formats::keys::Keys;
use forzatech_formats::settings::{self, DatabaseFile};
use forzatech_formats::zip::{Method, Zip};
use inputs::{Game, install, keys};
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

#[test]
fn every_database_file_reads() {
    let (Some(root), Some(keys_path)) = (install(Game::Fh6), keys(Game::Fh6)) else {
        return;
    };
    let text = std::fs::read_to_string(&keys_path).unwrap();
    let keys = Keys::parse(&text).unwrap();
    let media = root.join("media");
    let mut roots: BTreeMap<String, usize> = BTreeMap::new();
    let mut read = Vec::new();
    let mut refused = Vec::new();
    for path in zips(&media) {
        let mut file = BufReader::with_capacity(1 << 16, File::open(&path).unwrap());
        let zip = Zip::read(&mut file).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let rel = path
            .strip_prefix(&media)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        for (k, e) in zip.entries().iter().enumerate() {
            if !e.name.to_ascii_lowercase().ends_with(".xml") {
                continue;
            }
            let stored = zip.stored(&mut file, k).unwrap();
            let bytes = if e.method == Method::Encrypted {
                zip.decrypt(k, &stored, &keys).unwrap().0
            } else {
                zip.decode(k, &stored).unwrap()
            };
            let name = format!("{rel}/{}", e.name);
            let Ok(root) = settings::root(&bytes) else {
                // an entry whose root does not read must not hold a DatabaseFile in another form
                let utf16: Vec<u8> = "<DatabaseFile".bytes().flat_map(|b| [b, 0]).collect();
                for needle in [&b"<DatabaseFile"[..], &utf16] {
                    assert!(
                        !bytes.windows(needle.len()).any(|w| w == needle),
                        "{name}: a DatabaseFile whose root does not read"
                    );
                }
                let kind = if bytes.starts_with(b"BXML") {
                    "(binary, BXML)"
                } else {
                    "(no root element read)"
                };
                *roots.entry(kind.into()).or_default() += 1;
                continue;
            };
            *roots.entry(root.clone()).or_default() += 1;
            if root != "DatabaseFile" {
                continue;
            }
            match DatabaseFile::parse(&bytes) {
                Ok(f) => read.push((name, f.blocks.len())),
                Err(err) => refused.push(format!("{name}: {err}")),
            }
        }
    }
    let blocks: usize = read.iter().map(|(_, n)| n).sum();
    println!(
        "{} DatabaseFile files read ({blocks} blocks), {} refused; XML entries by root element: {roots:?}",
        read.len(),
        refused.len()
    );
    for r in &refused {
        println!("  refused {r}");
    }
    assert!(!read.is_empty(), "no DatabaseFile read");
    assert_eq!(refused.len(), 1, "{refused:#?}");
    assert!(
        refused[0].starts_with("Rendering.zip/Cubemap.xml: "),
        "{refused:#?}"
    );
}
