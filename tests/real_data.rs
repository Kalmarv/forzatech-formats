//! Reads a game's own files: Forza Horizon 6's encrypted containers, from the install and the keys the `GAME_DATA`
//! configuration names (see tests/inputs/README.md). Without them each test skips with a line naming its key; no game
//! file or key is kept in this repository.
//!
//! - The game database decrypts and descrambles to an SQLite image whose header's page size times page count fits
//!   the decrypted length.
//! - `NerdData.json`'s header is authenticated by one of the keys' usages, and every block's MAC matches.
#![allow(
    clippy::unwrap_used,
    reason = "a test reading the files you point it at; the library does no I/O"
)]

mod inputs;

use forzatech_formats::container::{BlockSize, Container};
use forzatech_formats::gamedb;
use forzatech_formats::keys::Keys;
use inputs::{Game, install_file, keys};

fn read_keys() -> Option<Keys> {
    let path = keys(Game::Fh6)?;
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Some(Keys::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
}

#[test]
fn game_database_decodes_to_sqlite() {
    let (Some(file), Some(keys)) = (
        install_file(Game::Fh6, "media/Stripped/gamedbRC.slt"),
        read_keys(),
    ) else {
        return;
    };
    let bytes = std::fs::read(&file).unwrap();
    let container = Container::parse(&bytes, BlockSize::Large).unwrap();
    let transport = keys
        .transport()
        .expect("the keys file gives transport.salt and transport.info");
    let usage = keys
        .usage("GameDB")
        .expect("the keys file gives the usage GameDB");
    let scramble = keys
        .gamedb_scramble()
        .expect("the keys file gives gamedb.scramble");
    println!(
        "gamedbRC.slt: {} blocks, padding {}; the GameDB MAC key authenticates its header: {}",
        container.block_count(),
        container.padding(),
        container.header_authentic(usage)
    );

    let image = gamedb::decrypt(&container, transport, usage, scramble);
    assert!(
        gamedb::is_sqlite(&image),
        "the image starts with SQLite's header string"
    );
    let page_size = match u16::from_be_bytes([image[16], image[17]]) {
        1 => 65536,
        n => usize::from(n),
    };
    let pages = u32::from_be_bytes(image[28..32].try_into().unwrap()) as usize;
    assert!(
        page_size.is_power_of_two() && (512..=65536).contains(&page_size),
        "page size {page_size}"
    );
    assert!(
        page_size * pages <= image.len(),
        "{pages} pages of {page_size} fit {} bytes",
        image.len()
    );
}

#[test]
fn small_container_authenticates_and_every_mac_matches() {
    let (Some(file), Some(keys)) = (
        install_file(Game::Fh6, "media/NerdData/NerdData.json"),
        read_keys(),
    ) else {
        return;
    };
    let bytes = std::fs::read(&file).unwrap();
    let container = Container::parse(&bytes, BlockSize::Small).unwrap();
    let transport = keys
        .transport()
        .expect("the keys file gives transport.salt and transport.info");
    let (usage, usage_keys) = keys
        .usage_for(&container)
        .expect("one of the keys' usages authenticates the header");
    let plain = container.decrypt_verified(transport, usage_keys).unwrap();
    assert_eq!(plain.len(), container.plaintext_len());
    println!("NerdData.json: usage {usage}, {} bytes of plaintext", plain.len());
}
