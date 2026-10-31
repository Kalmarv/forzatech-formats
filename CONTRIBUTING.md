# Contributing

## Build and test

The toolchain is pinned in `rust-toolchain.toml` (rustup installs it on first use).

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

CI runs these four on Linux and Windows. The tests build their own bytes in memory or use published test vectors, and
need no game files (`tests/real_data.rs` reads a Forza Horizon 6 install and your keys when `GAME_DATA` names them;
see the README). Reads go through checked accessors and return an `Error`, never a panic.

## Keys

Never commit a key, a file that holds one, or a digest of one, in code, tests, fixtures, issues or pull requests.
Tests that need keys ask for them through `GAME_DATA` and skip without them.

## Reporting a file the crate cannot read

Open an issue with the "Unreadable file" template. Please include:

- the error message, whole;
- the game and its build;
- the file's size and its first 36 bytes as hex (`xxd -l 36 file`): the container header holds no key.

Please do not attach game files, extracted assets, decrypted data or keys.

## Changes

Keep a change small and tested with a fixture built in the test itself, or published vectors. A format fact should say
how it is known (what was checked, on how many files).
