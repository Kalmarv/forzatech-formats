//! Reads the asset manifests of the install the `GAME_DATA` configuration names: the `AssetManifest.xml` of each
//! `bin.zip` under `media` (the Tokyo track's, the home space's and the open world's) and the loose one beside the
//! track's GeoChunks. No third-party reader of these files exists, so the check is the census: every manifest reads
//! whole (`AssetManifest::parse` refuses any element, attribute or byte the layout does not account for), and what it
//! says is checked against the files it describes:
//!
//! - a `bin.zip` manifest's shader and material names are its zip's entries (with `/` for `\`, ASCII case ignored), and
//!   every `materials/*.materialbin` and `shaders/<folder>/*.shaderbin` entry of the zip is named by one;
//! - each season folder of the zip (`materials/spring`, `autumn`, `winter`) holds the base folder's names, no more;
//! - every texture's `SourceHash` is the CRC-32 of its lower-case game path `game:\media\<source less
//!   .swatch>.swatchbin`, and the loose manifest's `PVSTextureIndex` values count up from 0 in file order.
//!
//! Without the install each test skips with a line naming its key.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test reading the files you point it at"
)]

mod inputs;

use forzatech_formats::asset_manifest::AssetManifest;
use forzatech_formats::pvs::Pvs;
use forzatech_formats::zip::Zip;
use inputs::{Game, install, install_file};
use primase_foundations::hash::crc32;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Every texture's source hash is its game path's CRC-32; the count of textures.
fn hashes(m: &AssetManifest) -> usize {
    let textures = m.textures.as_deref().unwrap_or_default();
    for t in textures {
        let low = t.source.to_ascii_lowercase();
        let game = format!("game:\\media\\{}.swatchbin", low.strip_suffix(".swatch").unwrap());
        assert_eq!(crc32(game.as_bytes()), t.source_hash, "{}", t.source);
    }
    textures.len()
}

/// The manifest of the zip at `path`, read and checked against the zip's entries.
fn census(path: &Path) -> AssetManifest {
    let mut f = BufReader::new(File::open(path).unwrap());
    let zip = Zip::read(&mut f).unwrap();
    let k = zip
        .find("AssetManifest.xml")
        .unwrap_or_else(|| panic!("{}: no AssetManifest.xml", path.display()));
    let stored = zip.stored(&mut f, k).unwrap();
    let bytes = zip.decode(k, &stored).unwrap();
    let m = AssetManifest::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let entries: BTreeSet<String> = zip
        .entries()
        .iter()
        .map(|e| e.name.to_ascii_lowercase())
        .collect();
    let named: BTreeSet<String> = m
        .shaders
        .iter()
        .chain(&m.materials)
        .map(|a| a.name.replace('\\', "/").to_ascii_lowercase())
        .collect();
    let missing: Vec<_> = named.difference(&entries).collect();
    assert!(
        missing.is_empty(),
        "{}: names that are no entry: {missing:?}",
        path.display()
    );
    // each folder's file names
    let mut folders: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for e in &entries {
        let (folder, file) = e.rsplit_once('/').unwrap_or(("", e.as_str()));
        folders.entry(folder).or_default().insert(file);
    }
    let unnamed: Vec<_> = entries
        .iter()
        .filter(|e| {
            let (folder, file) = e.rsplit_once('/').unwrap_or(("", e.as_str()));
            let shader = folder.strip_prefix("shaders/").is_some_and(|f| !f.contains('/'))
                && file.ends_with(".shaderbin");
            (folder == "materials" && file.ends_with(".materialbin")) || shader
        })
        .filter(|e| !named.contains(*e))
        .collect();
    assert!(
        unnamed.is_empty(),
        "{}: entries no manifest line names: {unnamed:?}",
        path.display()
    );
    let base = folders.get("materials").cloned().unwrap_or_default();
    for season in ["spring", "autumn", "winter"] {
        let names = folders
            .get(format!("materials/{season}").as_str())
            .cloned()
            .unwrap_or_default();
        assert_eq!(names, base, "{}: materials/{season}", path.display());
    }
    let textures = hashes(&m);
    println!(
        "{}: {} shaders, {} materials ({} in each season folder), {textures} textures, {} models; {} entries in the \n         zip",
        path.display(),
        m.shaders.len(),
        m.materials.len(),
        base.len(),
        m.models.as_ref().map_or(0, Vec::len),
        entries.len()
    );
    m
}

#[test]
fn the_track_manifests_read_whole() {
    let Some(track) = install_file(Game::Fh6, "media/Tracks/Brio") else {
        return;
    };
    let m = census(&track.join("bin.zip"));
    assert_eq!(
        (
            m.shaders.len(),
            m.materials.len(),
            m.textures.is_none(),
            m.models.is_none()
        ),
        (46, 419, true, true)
    );

    let bytes = std::fs::read(track.join("AssetManifest.xml")).unwrap();
    let loose = AssetManifest::parse(&bytes).unwrap();
    let textures = loose.textures.as_deref().unwrap();
    let models = loose.models.as_deref().unwrap();
    let instances: Vec<_> = models.iter().flat_map(|m| &m.instances).collect();
    let indices: BTreeSet<u32> = instances.iter().map(|i| i.index).collect();
    let ids: BTreeSet<&str> = instances.iter().map(|i| i.id.as_str()).collect();
    println!(
        "the loose manifest: {} shaders, {} materials, {} textures, {} models ({} with no instance), {} instances \
         ({} indices, {} ids)",
        loose.shaders.len(),
        loose.materials.len(),
        hashes(&loose),
        models.len(),
        models.iter().filter(|m| m.instances.is_empty()).count(),
        instances.len(),
        indices.len(),
        ids.len()
    );
    assert!(
        textures
            .iter()
            .enumerate()
            .all(|(k, t)| t.pvs_texture_index == u32::try_from(k).ok() && t.built.is_none())
    );
    assert!(models.iter().all(|m| m.material_keys.is_none()));
    assert!(instances.iter().all(|i| i.ribbon == 0 && i.is_lone.is_none()));
    assert_eq!((loose.shaders.len(), loose.materials.len()), (48, 0));
    assert_eq!(
        (textures.len(), models.len(), instances.len()),
        (48_733, 134_330, 593_706)
    );
    // the indices are 0 to one less than the count, each once
    assert_eq!((indices.len(), indices.last().copied()), (593_706, Some(593_705)));
}

#[test]
fn the_home_space_manifest_reads_whole() {
    let Some(root) = install(Game::Fh6) else {
        return;
    };
    let m = census(&root.join("media/Tracks/homespace02/bin.zip"));
    let textures = m.textures.as_deref().unwrap();
    assert!(
        textures
            .iter()
            .all(|t| t.built.is_some() && t.pvs_texture_index.is_none())
    );
    assert_eq!((m.shaders.len(), m.materials.len(), textures.len()), (1, 2, 6));
    assert_eq!(m.models.as_ref().map(Vec::len), Some(1));
}

#[test]
fn the_open_world_manifest_reads_whole() {
    let Some(root) = install(Game::Fh6) else {
        return;
    };
    let m = census(&root.join("media/Stripped/gs/bin.zip"));
    assert_eq!(
        (
            m.shaders.len(),
            m.materials.len(),
            m.textures.is_none(),
            m.models.is_none()
        ),
        (31, 68, true, true)
    );
}

/// The loose manifest against the track's hand-placed instance file (`Ribbon_00/brio_00.pvs`): one texture per texture
/// descriptor, one model per model number (`brioout.<number>.modelbin`), and each instance's `Index` a LOD row of its
/// model's number, every LOD row once. And the instance file's shader and material paths against the `bin.zip`
/// manifest: each is `game:\media\tracks\brio\bin\<a shader's or material's name>`.
#[test]
fn the_loose_manifest_describes_the_track_instance_file() {
    let Some(track) = install_file(Game::Fh6, "media/Tracks/Brio") else {
        return;
    };
    let loose = AssetManifest::parse(&std::fs::read(track.join("AssetManifest.xml")).unwrap()).unwrap();
    let pvs = Pvs::parse(&std::fs::read(track.join("Ribbon_00/brio_00.pvs")).unwrap()).unwrap();
    let w = pvs.world.as_ref().unwrap();
    let models = loose.models.as_deref().unwrap();
    assert_eq!(loose.textures.as_ref().map(Vec::len), Some(pvs.textures.len()));

    let number = |name: &str| -> u32 {
        let digits = name
            .strip_prefix("brioout.")
            .and_then(|n| n.strip_suffix(".modelbin"));
        digits
            .and_then(|d| d.parse().ok())
            .unwrap_or_else(|| panic!("{name}"))
    };
    let numbers: BTreeSet<u32> = models.iter().map(|m| number(&m.name)).collect();
    assert_eq!(numbers, (0..u32::try_from(w.models.len()).unwrap()).collect());
    let mut rows = BTreeSet::new();
    for m in models {
        let n = number(&m.name);
        for i in &m.instances {
            assert_eq!(
                w.lod_rows[i.index as usize].model, n,
                "{} instance {}",
                m.name, i.index
            );
            assert!(rows.insert(i.index));
        }
    }
    assert_eq!(rows.len(), w.lod_rows.len());
    let in_place = models
        .iter()
        .enumerate()
        .filter(|(k, m)| number(&m.name) as usize == *k)
        .count();
    println!(
        "{} models, numbered 0 to {} each once ({in_place} at their own position in the file); {} instances, one per \
         LOD row",
        models.len(),
        w.models.len() - 1,
        rows.len()
    );

    let mut f = BufReader::new(File::open(track.join("bin.zip")).unwrap());
    let zip = Zip::read(&mut f).unwrap();
    let k = zip.find("AssetManifest.xml").unwrap();
    let m = AssetManifest::parse(&zip.decode(k, &zip.stored(&mut f, k).unwrap()).unwrap()).unwrap();
    let names: BTreeSet<String> = m
        .shaders
        .iter()
        .chain(&m.materials)
        .map(|a| a.name.to_ascii_lowercase())
        .collect();
    for (_, path) in w.shaders.iter().chain(&w.materials) {
        let low = path.to_ascii_lowercase();
        let name = low.strip_prefix("game:\\media\\tracks\\brio\\bin\\");
        assert!(name.is_some_and(|n| names.contains(n)), "{path}");
    }
    println!(
        "the instance file's {} shaders and {} materials each name a bin.zip manifest entry",
        w.shaders.len(),
        w.materials.len()
    );
}
