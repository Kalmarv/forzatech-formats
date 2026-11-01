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
| `oracles.forzatools` | Nenkai ForzaTools' readings of the game's files: for `minizip`, per GeoChunk the header fields, the folder index and every entry (offset, sizes, method, padding, folder field, chunk-map record, name) as TSV, and the SHA-256 of sampled entries' contents | independent: Nenkai/ForzaTools 269a822 (MIT, `ForzaTools.PlaygroundMiniZip`, opened read-only and built for .NET 8), with K4os.Compression.LZ4 1.3.8 decoding the method-31 entries it locates, through a small dump harness that is not part of this repository |

#### Declared files

| key | status | gate | about | source |
| --- | --- | --- | --- | --- |
| `installs.fh6/media/Tracks/Brio` | live | yes | the Tokyo track's folder (Steam build 25588841): `GeoChunk0.minizip` to `GeoChunk3.minizip` (39.7, 3.6, 49.4 and 4.8 GB; 411,013, 3,065, 79,840 and 1,476 entries), each with its `ChunkMapN.dat` and `ChunkContentsMiniZipN.txt`; tests/minizip.rs | the user's own install |
| `oracles.forzatools.fh6/minizip` | live | yes | ForzaTools' reading of installs.fh6/media/Tracks/Brio's four GeoChunks: `chunks.tsv`, `GeoChunkN.indices.tsv`, `GeoChunkN.entries.tsv` (495,394 rows) and `samples.tsv` (200 entries of each method of each chunk, 1,800 rows); tests/minizip.rs | independent: ForzaTools 269a822's `MiniZip`, with K4os.Compression.LZ4 1.3.8 for method 31, through a dump harness kept with the private oracle tools |
| `installs.fh6/media/Stripped/gamedbRC.slt` | unheld: Forza Horizon 6 is not installed on any machine that runs these tests yet | - | the game database: a large-block container, read by tests/real_data.rs to an SQLite image | the user's own install |
| `installs.fh6/media/NerdData/NerdData.json` | unheld: Forza Horizon 6 is not installed on any machine that runs these tests yet | - | a one-block small container, read by tests/real_data.rs with every MAC checked | the user's own install |

<!-- end of real-data inputs -->
