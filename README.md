# forzatech-formats

[![crates.io: forzatech-formats](https://img.shields.io/crates/v/forzatech-formats?label=forzatech-formats)](https://crates.io/crates/forzatech-formats) [![docs.rs](https://img.shields.io/docsrs/forzatech-formats)](https://docs.rs/forzatech-formats) [![CI](https://img.shields.io/github/actions/workflow/status/Kalmarv/forzatech-formats/ci.yml?branch=main&label=CI)](https://github.com/Kalmarv/forzatech-formats/actions/workflows/ci.yml)

A Rust library that reads the file formats of Forza Horizon 6, a game on the ForzaTech engine.

## Features

- Decrypts the Arxan TransformIT container around the game database, encrypted zip entries, `sfsdata`, `NerdData.json` and the player profile
- Parses a keys file you supply and works out which key usage a container was written with
- Turns the decrypted game database into an SQLite image and reads each car's stock tyre and rim sizes from it
- Reads the PGZP `GeoChunkN.minizip` track archives, their chunk maps and contents lists, and decodes stored, DEFLATE and LZ4 entries
- Reads the zip archives under `media`, including ZIP64 records and encrypted method-22 entries
- Reads Grub bundles and the formats built on them: models (`.modelbin`), textures (`.pb`, `.swatchbin`), materials (`.materialbin`) and shaders (`.shaderbin`)
- Decodes a mesh's indices and vertices (positions, normals, tangents, UVs, colours, morph and skin data) as stored
- Reads texture headers (PC and Xbox One) and finds each mip in the payload without decoding pixels
- Reads track placements: `.pgeo` placement files, `.plib` template libraries and the `.pvs` hand-placed instance file
- Finds the placements inside an x/z box and resolves model references to GeoChunk entries
- Reads `AssetManifest.xml`, `GameObjs.xml`, `DatabaseFile` settings XML, `.particlebin` particle instances and `.ppr` time-of-day files
- Reads from bytes or a `Read + Seek` reader you pass in and opens no files itself
- Returns an `Error` for malformed input instead of panicking, and `unsafe` code is denied
- Ships no keys and never writes or re-encrypts files

## Getting Started

### Requirements

- Rust 1.99 or later (`rust-toolchain.toml` pins 1.99.0)
- Your own copy of Forza Horizon 6
- To read encrypted files, a keys file of your own. The crate ships none.

### Installation

```sh
cargo add forzatech-formats
```

It depends on [primase-foundations](https://crates.io/crates/primase-foundations) from crates.io for decompression,
vertex decoding and SQLite.

### Usage

This program opens the Tokyo track's first GeoChunk, finds its models by name in the contents list, and decodes
the meshes of the first one:

```rust
use forzatech_formats::minizip::{MiniZip, parse_contents};
use forzatech_formats::modelbin::Model;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let track = Path::new("ForzaHorizon6/media/Tracks/Brio");
    let mut file = BufReader::new(File::open(track.join("GeoChunk0.minizip"))?);
    let archive = MiniZip::read(&mut file)?; // reads the directory only
    let text = std::fs::read_to_string(track.join("ChunkContentsMiniZip0.txt"))?;
    let names = parse_contents(&text)?;

    let (k, line) = names
        .iter()
        .enumerate()
        .find(|(_, line)| line.name.ends_with(".modelbin"))
        .ok_or("no model in this archive")?;
    let bytes = archive.read_entry(&mut file, k)?;
    let model = Model::parse(&bytes)?;
    for m in 0..model.meshes.len() {
        let mesh = model.mesh_data(m)?;
        println!("{} mesh {m}: {} vertices, {} indices", line.name, mesh.vertex_count, mesh.indices.len());
    }
    Ok(())
}
```

The keys file has one `name = hex` per line. Pass its text to `keys::Keys::parse`:

```text
transport.salt  = <64 hex digits>
transport.info  = <64 hex digits>
GameDB.data     = <64 hex digits>
GameDB.mac      = <32 hex digits>
gamedb.scramble = <8 hex digits>
```

The other entry points are:

- `container::Container::parse` and `decrypt_verified` decrypt a container.
- `gamedb::decrypt` and `gamedb::car_wheels` work on the game database.
- `zip::Zip::read` and `read_entry` read a zip.
- `texture::Texture::parse`, `material::Material::parse` and `shader::Shader::parse` read textures, materials and shaders.
- `pgeo::Pgeo::parse`, `plib::Library::parse`, `pvs::Pvs::parse` and `track::Track::new` read placements.
- `ppr::Ppr::parse` reads time-of-day files.

`cargo doc --open` builds the API documentation.

`cargo test --workspace` runs the tests that need no game files. The real-data tests read an install, keys and
third-party oracle dumps from the TOML file that `GAME_DATA` points to. Without it they skip. See
[tests/inputs/README.md](tests/inputs/README.md) and [docs/verification.md](docs/verification.md). Building and
reporting unreadable files are covered in [CONTRIBUTING.md](CONTRIBUTING.md). Licensed under GPL-3.0-or-later.
