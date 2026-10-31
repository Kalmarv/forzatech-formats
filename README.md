# forzatech-formats

A Rust library that reads ForzaTech files, as Forza Horizon 6 ships them.

It reads, from bytes you give it:

- **The encrypted container** (the Arxan TransformIT envelope) that wraps Forza Horizon 6's game database
  (`gamedbRC.slt`), the method-22 entries of its settings archives, `sfsdata`, `NerdData.json` and the player's profile:
  its 36-byte header, its 0x200- or 0x20000-byte blocks and their MACs, decryption, and the check of every MAC.
- **The keys** it is decrypted with, from a short text file you keep (`keys::Keys::parse`). **No key ships with this
  crate**, and none is published here.
- **The game database's second layer**: the keystream that turns the decrypted `gamedbRC.slt` into an SQLite image.

More of the engine's formats (its streaming archives, models, materials, textures and placements) will follow. Used by
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
- **Not yet checked against real files:** how those steps chain (which IV each block and MAC uses, what the header's
  MAC covers) follows a community tool's description. `tests/real_data.rs` checks it on a real install with your keys.
  Until that has run, treat decryption as unproven.
- For the game database, whether the header's MAC and padding field mean what they mean in small containers is not
  established, so `gamedb::decrypt` does not rely on them.

## What it does not do

- It does not encrypt or re-encrypt, and it does not write any file.
- It does not ship, find or derive keys.
- It does not yet read the zip archives the encrypted entries live in, or inflate them.

## Running the tests

```sh
cargo test --workspace
```

runs the tests that need no game file. The tests that read a real install, `tests/real_data.rs`, skip with a line
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
