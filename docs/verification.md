# What has been checked

This file holds the guarantees, verification record and limits that the earlier README carried. Its figures come from
one Steam install of Forza Horizon 6 (build 25588841). The real-data tests in `tests/` repeat each check on your own
install; see [tests/inputs/README.md](../tests/inputs/README.md).

## What it guarantees

- Every read is bounds-checked. Malformed bytes give an `Err` naming what was expected, never a panic (`unsafe` is
  denied). A cut container reads only when the cut falls on a block boundary, which a test checks for every length.
- Containers whose body is not whole blocks with their MACs, or whose padding is not less than a block, are refused.
- `decrypt_verified` returns nothing unless the header's MAC and every block's MAC match.
- A `Keys` value's `Debug` names what it holds, never a key.

## What has been checked, and what has not

- The header layout and block sizes match one install's game database, `sfsdata`, `NerdData.json`, a profile and the
  67 encrypted entries of `Rendering.zip`.
- The AES-CBC, AES-CMAC and HKDF-SHA256 steps match their standards' published vectors (NIST SP 800-38A, RFC 4493,
  RFC 5869), and the game database keystream matches values worked by hand with an independent CRC-32.
- **The chaining is checked on files the game wrote**, not only taken from the description: for every container
  checked (a profile and a backup, estate and flyer blobs, `sfsdata`, `gamedbRC.slt`, `NerdData.json`, and five
  encrypted settings entries) the header MAC authenticates under its usage's key and every block's MAC matches, and the
  plaintext is the promised inner format — zlib for a profile, raw DEFLATE for a settings entry, an SQLite image after
  the keystream for the game database. `tests/real_data.rs` repeats it on your own install with your keys.
- For the game database, whether the header's MAC and padding field mean what they mean in small containers is not
  established, so `gamedb::decrypt` does not rely on them.
- **The streaming archives** read on one install's Tokyo track (Steam build 25588841): all four GeoChunks, 495,394
  entries, pass every check the reader makes, and every entry decodes to exactly its stated size. Every entry's
  position, sizes, method, padding, folder and name, and every folder index value, equal what Nenkai's ForzaTools
  reads from the same files, and 1,800 sampled entries decode to the same bytes (ForzaTools for stored and DEFLATE
  entries, K4os.Compression.LZ4 for LZ4 ones). `tests/minizip.rs` repeats this on your install.
- **The zip archives** read on one install (Steam build 25588841): all 9,076 zips under `media` (402,283 entries:
  371,516 DEFLATE, 29,250 encrypted, 1,517 stored; one archive with ZIP64 records) pass every check the reader makes.
  Every entry decodes to its size and CRC-32, and every encrypted entry decrypts with the `General` usage, its MACs
  matching. `tests/zip.rs` repeats this on your install (a sample of the unencrypted entries, or every entry with
  `[run] gate = 'full'`).
- **The game database** decrypts to an SQLite image whose 207 tables (141,832 rows) all read whole with the strict
  reader. The tyre and rim columns of `Data_Car` (682 cars) give the sizes manufacturers publish for the cars checked
  (a 2023 Porsche 911 GT3 RS: 275/35 R20 front, 335/30 R21 rear). `tests/real_data.rs` repeats this.
- **The settings files**: the install's zips hold 72 `DatabaseFile` files (in `Rendering.zip` and
  `GameTunableSettings.zip`); 71 read, and `Rendering.zip`'s `Cubemap.xml` is refused because its last `Settings`
  block is closed by `</property>`, which is not well-formed XML. `tests/settings.rs` repeats this.
- **The placement files**: no third-party reader of them exists, so the check is a census over every instance of one
  install (Steam build 25588841), with the strict readers refusing any value or byte the layouts do not account for.
  Our own earlier exports are not a check. **Template libraries:** all 65 version 38 libraries (545 draw behaviours,
  19,776 model templates, 47,835 placement templates) and the 5 version 9 group libraries read to their last byte
  (`tests/plib.rs`); 12,566 of the 19,776 model paths name a model whose files are in the track's GeoChunks and 7,210
  (30 of them empty) name none, so the files do not hold them. **Hand-placed instances:** `brio_00.pvs` (154,254
  instances, 111,452 transforms, 593,706 LOD rows, 134,330 models) and the texture-only `generalsceneassets.pvs` read
  to their last byte with every reference inside what it refers to; the third `.pvs` of the install, the home
  space's, is version 0x63 and is refused. A model number is the id of the type-4 record of the chunk map: all 113,868
  `.modelbin` entries are named by one, and 20,462 of the 134,330 numbers (19,436 of them used by LOD rows) name no
  entry of the install. Each of the 13,702 `.pvsz` entries is named by three cell records (`tests/pvs.rs`).
  **Placement files:** All 96,234 `.pgeo` files (665,298 blocks, 29,164,671 instance
  records, 243,394 spline curves, 244 crowd splines) read to their last byte with no refusal (`tests/pgeo.rs`, every
  file with `[run] gate = 'full'`); every position lies inside its file's header box, every source range names a
  placement template, and 337,778 records lie outside the cell the file is kept under. In 37,066 instance-seasons the
  block name is in no model template of the category's library (27 winter `GRASSTEMPLATE` names) and in 3,579 it is in
  two; they are counted, not refused. The placements in one box, read from the files whose header box may hold some,
  are those read from every file (Shibuya Scramble box: 297 files, 68,556 instances, 30 splines).
  **Particle instances:** the 170 `.particlebin` files (the track's 3,959 instances, the home
  space's none, and 168 route definitions with 508) read with every constant, link and string offset checked
  (`tests/particles.rs`). **Game objects:** the track's 793, the open world's 59 and the home space's none read whole
  (`tests/gameobjs.rs`).

- **The asset manifests**: no third-party reader of them exists, so the check is a census of the four in one install
  (Steam build 25588841: the three `bin.zip` manifests and the Tokyo track's loose one), each read whole and checked
  against what it describes (`tests/asset_manifest.rs`). Every shader and material name is an entry of its zip and
  every material and shader entry is named; every texture's `SourceHash` is the CRC-32 of its game path (48,733 and
  6); the loose manifest's 134,330 models are the instance file's model numbers, each once, and its 593,706 instances
  its LOD rows, each once and each of its model; the instance file's 46 shader and 419 material paths each name an
  entry of the track's `bin.zip` manifest.

- **The Grub bundles** read on one install (Steam build 25588841): all 526,129 bundles of the Tokyo track's GeoChunks
  and the zips under `media` (196,620 models, 191,602 `.pb` and 126,527 `.swatchbin` textures, 9,707 materials, 987
  shaders, 682 car colour lists and 4 crowds), and the 1,526,529 bundles nested in their blobs, read with no refusals.
  Each gives the same blobs, metadata records and payloads as Doliman100's ForzaTech-extraction-tools reads (whose
  reader sees the first 0xFFF bytes of the 16 payloads whose size is escaped). `tests/bundle.rs` repeats this on your
  install (a sample, or every bundle with `[run] gate = 'full'`).
- **The swatch textures** read on one install (Steam build 25588841): all 318,129 (169,351 `.pb` in the Tokyo track's
  GeoChunks, 22,251 `.pb` and 126,527 `.swatchbin` in zips; 318,118 PC and 11 Durango) read with no refusals and no
  header byte unaccounted for, and every mip of every slice (1,309,343 in all) lies where the header says, the mips
  filling the payload exactly. Every PC header equals what Nenkai's ForzaTools reads (and its Durango header the 11
  Durango ones), 25,057 mips equal ForzaTools.TexConv's DDS as BCnEncoder.Net reads it (every 50th texture), and
  311,455 textures equal Doliman100's library (which has no Durango reading, and does not know the encoding of 9,943).
  `tests/texture.rs` repeats this on your install.
- **The materials and shaders** read on one install: all 9,707 `.materialbin`, 987 `.shaderbin` and the 1,524,285
  material instances in 196,620 models (6,539 empty `MatI` blobs besides) read with no refusals, 18,367,250 parameters
  in all. The hash after each of 569,170 texture paths is the CRC-32 of the lowercased path, and in every shaderbin the
  `CBMP`, `TXMP` and `SPMP` hashes are `PARM` hashes. 206,621 readings (a material, a shader or a model's instances) equal
  those of Doliman100's library, which reads a sampler as 12 bytes and has no type 12, so its list stops after the first
  of either. `tests/material.rs` repeats this on your install.

- **The models** read on one install (Steam build 25588841): all 113,868 `.modelbin` of the Tokyo track's `GeoChunk0`
  (407,461 meshes) and all 82,752 under `media` in zips (1,339,215 meshes) read with no refusals, and every mesh's
  indices and vertices decode with every index inside its buffer. Nenkai's ForzaTools reads 75,108 of the Tokyo models
  (it throws on 5,617 with a material slot that has no instance and 33,143 in its mesh reader, written for Forza Horizon
  5); on those the header, skeleton, meshes, layouts, buffers and material slots agree field by field. Doliman100's
  importer reads 110,591 (it raises on 3,208: 3,140 with a mesh of several material groups, which it misreads, and 68
  with a vertex whose stored normal is zero; 69 more models with several material groups are not compared); on those
  the meshes agree and 444,752 sampled vertices decode to the same values. `tests/modelbin.rs` repeats this on your
  install (a sample, or every model with `[run] gate = 'full'`).

## What it does not do

- It does not encrypt or re-encrypt, and it does not write any file or write into an archive.
- It does not ship, find or derive keys.
- It does not read the `.pvsz` visibility files, the track's cameras (`PreRaceCams.xml`, `TrackSegCams.xml`), `.hex` or
  `.deform_ban` files, nor `Locators.xml`, `TriggerVolumes.xml` and `.cub`, which hold only an empty root element in the
  install it was checked on. Where a placement file's field is not understood it keeps the raw value and says so.
