# forzatech-formats

A Rust library that reads ForzaTech files, as Forza Horizon 6 ships them.

It reads, from bytes or a reader you give it:

- **The encrypted container** (the Arxan TransformIT envelope) that wraps Forza Horizon 6's game database
  (`gamedbRC.slt`), the method-22 entries of its settings archives, `sfsdata`, `NerdData.json` and the player's profile:
  its 36-byte header, its 0x200- or 0x20000-byte blocks and their MACs, decryption, and the check of every MAC.
- **The keys** it is decrypted with, from a short text file you keep (`keys::Keys::parse`). **No key ships with this
  crate**, and none is published here.
- **The game database's second layer**: the keystream that turns the decrypted `gamedbRC.slt` into an SQLite image,
  which `primase_foundations::sqlite` reads, and each car's stock tyre and rim sizes and stock wheel read from it
  (`gamedb::car_wheels`).
- **The streaming archives** a track's world is stored in (`GeoChunkN.minizip`, PGZP): the header, the folder index,
  every entry's position, sizes, compression and folder, each entry decoded (stored, raw DEFLATE or raw LZ4, through
  [primase-foundations](https://github.com/Kalmarv/primase-foundations)), and the two files beside each archive that
  type and name its entries (`ChunkMapN.dat`, `ChunkContentsMiniZipN.txt`). `minizip::MiniZip::read` reads only the
  directory; an entry is read when you ask for it, so a 40 GB archive costs a few megabytes to open.
- **The zip archives** the settings, cars, shaders and libraries are kept in (`media/*.zip` and below): PKWARE zip
  with ZIP64 records where an archive needs them, every local header checked against its central record, stored and
  DEFLATE entries decoded, and the encrypted entries (method 22: a small-block container around raw DEFLATE)
  decrypted with your keys, every MAC checked. Every decode must give the entry's size and the CRC-32 its archive
  stores.
- **The settings files** those encrypted entries hold whose root is `DatabaseFile` (`Rendering.zip`'s
  `Megatexture.xml`, `Livery.xml`, `DecalManager.xml` and the rest): their `Data` and `Settings` blocks, properties,
  lists and maps, every value kept as the text the file stores (`settings::DatabaseFile::parse`).

- **Grub bundles**, the container the models (`.modelbin`), textures (`.pb`, `.swatchbin`), materials
  (`.materialbin`), shaders (`.shaderbin`), crowds and car colour lists are stored in: the header, every blob's record,
  metadata records and data, and the bundles nested in blobs (a model's material instances), with every byte of the
  layout checked (`bundle::Bundle::parse`). What a blob holds is read by the formats' own modules.

More of the engine's formats (the contents of its models, materials, textures and placements) will follow. Used by
[primase](https://github.com/Kalmarv/primase), a game-asset conversion library.

## Example

```rust
use forzatech_formats::container::{BlockSize, Container};

fn main() -> Result<(), forzatech_formats::Error> {
    // A container header and one 512-byte block with its MAC, built by hand: the header states 12 bytes of padding.
    let mut file = vec![0u8; 36 + 512 + 16];
    file[16..20].copy_from_slice(&12u32.to_le_bytes());
    let container = Container::parse(&file, BlockSize::Small)?;
    assert_eq!(container.block_count(), 1);
    assert_eq!(container.plaintext_len(), 500);
    Ok(())
}
```

This is the crate's doc test (`src/lib.rs`). With keys, `Container::decrypt_verified` returns the plaintext after
checking the header's MAC and every block's; `Keys::usage_for` finds which of your keys' usages a container was
written with.

## Keys

The keys file has one `name = hexadecimal` per line:

```text
transport.salt = <64 hex digits>     # the IV chain's HKDF salt, shared by every container
transport.info = <64 hex digits>     # the IV chain's HKDF info
GameDB.data    = <64 hex digits>     # a usage's AES-256 data key
GameDB.mac     = <32 hex digits>     # the same usage's AES-128 CMAC key
General.data   = ...                 # as many usages as you have
General.mac    = ...
gamedb.scramble = <8 hex digits>     # the game database's keystream constant
```

Your program reads the file and passes its text to `Keys::parse`; the crate itself does no file access.

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

- **The Grub bundles** read on one install (Steam build 25588841): all 526,129 bundles of the Tokyo track's GeoChunks
  and the zips under `media` (196,620 models, 191,602 `.pb` and 126,527 `.swatchbin` textures, 9,707 materials, 987
  shaders, 682 car colour lists and 4 crowds), and the 1,526,529 bundles nested in their blobs, read with no refusals.
  Each gives the same blobs, metadata records and payloads as Doliman100's ForzaTech-extraction-tools reads (whose
  reader sees the first 0xFFF bytes of the 16 payloads whose size is escaped). `tests/bundle.rs` repeats this on your
  install (a sample, or every bundle with `[run] gate = 'full'`).

## What it does not do

- It does not encrypt or re-encrypt, and it does not write any file or write into an archive.
- It does not ship, find or derive keys.

## Running the tests

```sh
cargo test --workspace
```

runs the tests that need no game file. The tests that read a real install (`tests/real_data.rs`, `tests/minizip.rs`,
`tests/zip.rs`, `tests/settings.rs`) skip with a line
naming what they need when `GAME_DATA` is not set. To run them, point the `GAME_DATA` environment variable at a TOML
file naming your own copy and your keys file, in the layout of
[primase-testdata](https://github.com/Kalmarv/primase-testdata):

```toml
[installs]
fh6 = 'D:/games/ForzaHorizon6'          # the folder that holds media
[keys]
fh6 = 'D:/keys/fh6.keys'                # your keys file
```

[tests/inputs/README.md](tests/inputs/README.md) lists every input the tests can read (generated from
`tests/inputs/mod.rs`).

## Documentation, contributing, license

API documentation: `cargo doc --open` (once published, <https://docs.rs/forzatech-formats>). See
[CONTRIBUTING.md](CONTRIBUTING.md) to build or to report a file the crate cannot read.

Licensed under the GNU General Public License, version 3 or later (see [LICENSE](LICENSE)).
