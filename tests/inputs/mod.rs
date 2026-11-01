//! The real-data inputs this crate's tests read, each with what it is and where it comes from
//! (`primase_testdata::inputs!`; README.md here lists them). A test whose input is not configured skips with a line
//! naming its key. An input declared `gate: true` is one a complete run needs: with `[run] require = 'gate'` in the
//! `GAME_DATA` file, a missing one fails the test instead of skipping it. Integration tests reach this module with
//! `mod inputs;`, unit tests with `#[cfg(test)] #[path = "../tests/inputs/mod.rs"] mod inputs;` in the crate root, and
//! tests/real_data_inputs.rs checks that every query the crate's sources write is declared here. Declare an input
//! here when a test first asks for it.

primase_testdata::inputs! {
    games {
        Fh6 = "fh6",
    }
    installs {
        Fh6 {
            about: "Forza Horizon 6's install folder (the folder that holds `media`)",
            source: "the user's own install",
            gate: false,
        },
    }
    keys {
        Fh6 {
            about: "the user's Forza Horizon 6 container keys: a text file in the layout `keys::Keys::parse` reads, with the usages `GameDB` and `General` and `gamedb.scramble`",
            source: "the user's own copy of the keys; none is distributed here",
            gate: true,
        },
    }
    tools {}
    oracle_sets {
        ForzaTools = "forzatools" {
            about: "Nenkai ForzaTools' readings of the game's files: for `minizip`, per GeoChunk the header fields, the folder index and every entry (offset, sizes, method, padding, folder field, chunk-map record, name) as TSV, and the SHA-256 of sampled entries' contents",
            source: "independent: Nenkai/ForzaTools 269a822 (MIT, `ForzaTools.PlaygroundMiniZip`, opened read-only and built for .NET 8), with K4os.Compression.LZ4 1.3.8 decoding the method-31 entries it locates, through a small dump harness that is not part of this repository",
        },
        Doliman = "doliman" {
            about: "Doliman100 ForzaTech-extraction-tools' readings of the game's files: for `bundle`, every Grub bundle's blobs and metadata as that library's bundle classes read them, one SHA-256 per bundle and every 100th bundle in full, as TSV",
            source: "independent: Doliman100/ForzaTech-extraction-tools d126767 (GPL-3.0, the `Bundle`, `Blob` and `Metadata` classes of `scripts/modelbin_importer.py`, unchanged), through a small dump harness that is not part of this repository; the bundles' bytes come from this crate's archive readers, checked by tests/minizip.rs and tests/zip.rs",
        },
    }
    declared {
        Query::Install(Game::Fh6, Some("media/Tracks/Brio")) => {
            status: Live,
            gate: true,
            about: r#"the Tokyo track's folder (Steam build 25588841): `GeoChunk0.minizip` to `GeoChunk3.minizip` (39.7, 3.6, 49.4 and 4.8 GB; 411,013, 3,065, 79,840 and 1,476 entries), each with its `ChunkMapN.dat` and `ChunkContentsMiniZipN.txt`; tests/minizip.rs, tests/pgeo.rs, tests/plib.rs, tests/pvs.rs"#,
            source: r#"the user's own install"#,
        },
        Query::Oracle(OracleSet::ForzaTools, Game::Fh6, "minizip") => {
            status: Live,
            gate: true,
            about: r#"ForzaTools' reading of installs.fh6/media/Tracks/Brio's four GeoChunks: `chunks.tsv`, `GeoChunkN.indices.tsv`, `GeoChunkN.entries.tsv` (495,394 rows) and `samples.tsv` (200 entries of each method of each chunk, 1,800 rows); tests/minizip.rs"#,
            source: r#"independent: ForzaTools 269a822's `MiniZip`, with K4os.Compression.LZ4 1.3.8 for method 31, through a dump harness kept with the private oracle tools"#,
        },
        Query::Oracle(OracleSet::Doliman, Game::Fh6, "bundle") => {
            status: Live,
            gate: true,
            about: r#"Doliman100's reading of every Grub bundle of installs.fh6 (the Tokyo track's GeoChunks and every zip under `media`): `bundles.tsv` (one row per bundle: source, entry, name, blob and nested-bundle counts, the SHA-256 of its reading), `sample.tsv` (every 100th bundle's reading line by line) and `errors.tsv`; tests/bundle.rs"#,
            source: r#"independent: ForzaTech-extraction-tools d126767's bundle classes, through a dump harness kept with the private oracle tools"#,
        },
        Query::Install(Game::Fh6, Some("media/Stripped/gamedbRC.slt")) => {
            status: Live,
            gate: true,
            about: r#"the game database (Steam build 25588841, 16,123,860 bytes): a large-block container, read by tests/real_data.rs to an SQLite image of 207 tables, every table read whole and every car's stock wheels read"#,
            source: r#"the user's own install"#,
        },
        Query::Install(Game::Fh6, Some("media/Stripped/gs/generalsceneassets.pvs")) => {
            status: Live,
            gate: true,
            about: r#"the general scene assets instance file (Steam build 25588841, 381,229 bytes): a `.pvs` with the header and 6,807 texture descriptors only; tests/pvs.rs"#,
            source: r#"the user's own install"#,
        },
        Query::Install(Game::Fh6, Some("media/NerdData/NerdData.json")) => {
            status: Live,
            gate: true,
            about: r#"a one-block small container, read by tests/real_data.rs with every MAC checked"#,
            source: r#"the user's own install"#,
        },
    }
    debug {}
}
