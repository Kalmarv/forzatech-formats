# Real-data inputs

What this crate's real-data tests read, as tests/inputs/mod.rs declares them: the keys of the configuration file
`GAME_DATA` names (its layout is that of [primase-testdata](https://github.com/Kalmarv/primase-testdata), which the
tests use), the oracle sets and the files of each, and the debug keys `GAME_DATA_DEBUG` sets. A test whose input is not
configured skips with a line naming its key; the "gate" column marks the inputs that fail instead of skipping when the
file says `[run] require = 'gate'`. The tables below are generated from the declarations (the `real_data_inputs` test
fails when they differ); [the README](../../README.md#running-the-tests) says how to set the inputs up.

<!-- real-data inputs: made by primase_testdata::doc::tables from tests/inputs/mod.rs -->

#### Configuration keys

| key | gate | about | source |
| --- | --- | --- | --- |
| `installs.fh6` | - | Forza Horizon 6's install folder (the folder that holds `media`) | the user's own install |
| `keys.fh6` | yes | the user's Forza Horizon 6 container keys: a text file in the layout `keys::Keys::parse` reads, with the usages `GameDB` and `General` and `gamedb.scramble` | the user's own copy of the keys; none is distributed here |
| `scratch.root` | - | where real-data tests write their outputs (the OS temporary folder when unset) | - |

#### Oracle sets (`<oracles.root>/<set>/<game>/`)

| set | about | source |
| --- | --- | --- |
| `oracles.forzatools` | Nenkai ForzaTools' readings of the game's files: for `minizip`, per GeoChunk the header fields, the folder index and every entry (offset, sizes, method, padding, folder field, chunk-map record, name) as TSV, and the SHA-256 of sampled entries' contents; for `modelbin`, the blobs and metadata of every Tokyo model as that library's classes hold them (the model header, the leading fields of each mesh, vertex layouts, buffer headers with the SHA-256 of their elements, skeletons, material instances), as TSV | independent: Nenkai/ForzaTools 269a822 (MIT, `ForzaTools.PlaygroundMiniZip`, opened read-only and built for .NET 8), with K4os.Compression.LZ4 1.3.8 decoding the method-31 entries it locates, through a small dump harness that is not part of this repository; for `modelbin` the library's bundle classes with the one change that a tag it has no class for is kept as its raw bytes instead of ending the load (the harness lists those) |
| `oracles.doliman` | Doliman100 ForzaTech-extraction-tools' readings of the game's files: for `bundle`, every Grub bundle's blobs and metadata as that library's bundle classes read them, one SHA-256 per bundle and every 100th bundle in full, as TSV; for `modelbin`, what the importer script computes for every Tokyo model (the fields it reads of each mesh, and the faces, positions, normals, texture coordinates and colours it builds, as it builds them) as TSV, with every vertex of every 500th model | independent: Doliman100/ForzaTech-extraction-tools d126767 (GPL-3.0, the `Bundle`, `Blob` and `Metadata` classes of `scripts/modelbin_importer.py`, unchanged), through a small dump harness that is not part of this repository; the bundles' bytes come from this crate's archive readers, checked by tests/minizip.rs and tests/zip.rs; for `modelbin` the importer's own per-model code, run unchanged with a recording stand-in for Blender |

#### Declared files

| key | status | gate | about | source |
| --- | --- | --- | --- | --- |
| `installs.fh6/media/Tracks/Brio` | live | yes | the Tokyo track's folder (Steam build 25588841): `GeoChunk0.minizip` to `GeoChunk3.minizip` (39.7, 3.6, 49.4 and 4.8 GB; 411,013, 3,065, 79,840 and 1,476 entries), each with its `ChunkMapN.dat` and `ChunkContentsMiniZipN.txt`, `bin.zip` (27,544 entries) and the loose `AssetManifest.xml`; tests/minizip.rs, tests/pgeo.rs, tests/plib.rs, tests/pvs.rs, tests/asset_manifest.rs | the user's own install |
| `oracles.forzatools.fh6/minizip` | live | yes | ForzaTools' reading of installs.fh6/media/Tracks/Brio's four GeoChunks: `chunks.tsv`, `GeoChunkN.indices.tsv`, `GeoChunkN.entries.tsv` (495,394 rows) and `samples.tsv` (200 entries of each method of each chunk, 1,800 rows); tests/minizip.rs | independent: ForzaTools 269a822's `MiniZip`, with K4os.Compression.LZ4 1.3.8 for method 31, through a dump harness kept with the private oracle tools |
| `oracles.doliman.fh6/bundle` | live | yes | Doliman100's reading of every Grub bundle of installs.fh6 (the Tokyo track's GeoChunks and every zip under `media`): `bundles.tsv` (one row per bundle: source, entry, name, blob and nested-bundle counts, the SHA-256 of its reading), `sample.tsv` (every 100th bundle's reading line by line) and `errors.tsv`; tests/bundle.rs | independent: ForzaTech-extraction-tools d126767's bundle classes, through a dump harness kept with the private oracle tools |
| `oracles.forzatools.fh6/modelbin` | live | yes | ForzaTools' reading of the 113,868 models of installs.fh6/media/Tracks/Brio's GeoChunk0: `models.tsv` (blobs, the model header's eight u16, its bounds, the tags the library kept opaque), `meshes.tsv`, `layouts.tsv`, `buffers.tsv`, `skeletons.tsv`, `morphs.tsv`, `materials.tsv` and `errors.tsv` (the models its classes throw on); tests/modelbin.rs | independent: ForzaTools 269a822's `ForzaTools.Bundles`, through a dump harness kept with the private oracle tools |
| `oracles.doliman.fh6/modelbin` | live | yes | Doliman100's reading of the 113,868 models of installs.fh6/media/Tracks/Brio's GeoChunk0: `models.tsv`, `meshes.tsv` (the fields its `Mesh` reads and the faces, positions, normals, texture coordinates and colours the importer builds, as statistics and a faces hash), `sample.tsv` (every vertex of every 500th model) and `errors.tsv`; tests/modelbin.rs | independent: ForzaTech-extraction-tools d126767's `modelbin_importer.py`, run per model by a dump harness kept with the private oracle tools |
| `installs.fh6/media/Stripped/gamedbRC.slt` | live | yes | the game database (Steam build 25588841, 16,123,860 bytes): a large-block container, read by tests/real_data.rs to an SQLite image of 207 tables, every table read whole and every car's stock wheels read | the user's own install |
| `installs.fh6/media/Stripped/gs/generalsceneassets.pvs` | live | yes | the general scene assets instance file (Steam build 25588841, 381,229 bytes): a `.pvs` with the header and 6,807 texture descriptors only; tests/pvs.rs | the user's own install |
| `installs.fh6/media/Stripped/gs/brio` | live | yes | the open world's folder of gameplay data (Steam build 25588841; 287 files, 241 MB): `gameobjs.xml` (59 game objects) and `routedefinitions` with 168 `.particlebin` files (508 particle instances in all) beside the routes' sound scapes; tests/gameobjs.rs and tests/particles.rs | the user's own install |
| `installs.fh6/media/Tracks/homespace02/Ribbon_00/ParticleInstances.particlebin` | live | yes | the home space's particle instances file (Steam build 25588841, 60 bytes): a bundle whose list holds no instance; tests/particles.rs | the user's own install |
| `installs.fh6/media/Tracks/homespace02/Ribbon_00/GameObjs.xml` | live | yes | the home space's game objects file (Steam build 25588841, 104 bytes): no object; tests/gameobjs.rs | the user's own install |
| `installs.fh6/media/NerdData/NerdData.json` | live | yes | a one-block small container, read by tests/real_data.rs with every MAC checked | the user's own install |

<!-- end of real-data inputs -->

