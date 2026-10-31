//! The encrypted container Forza Horizon 6 wraps its game database, settings archives, `sfsdata`, `NerdData.json` and
//! profile in (the Arxan TransformIT envelope).
//!
//! A container is a 36-byte header, then whole blocks, each followed by a 16-byte MAC:
//!
//! | offset | size | field |
//! |---|---|---|
//! | 0 | 16 | IV of the first block |
//! | 16 | 4 | padding: bytes at the end of the last block that are not plaintext (u32, little-endian) |
//! | 20 | 16 | MAC of the header |
//! | 36 | N × (block + 16) | each block's ciphertext, then its MAC's |
//!
//! The block is 0x200 bytes ([`BlockSize::Small`]: settings entries, `NerdData.json`, the profile) or 0x20000
//! ([`BlockSize::Large`]: the game database, `sfsdata`). Each block is AES-256-CBC under the data key; each MAC is the
//! AES-CMAC of the block's plaintext under the MAC key (AES-128), stored AES-CBC-encrypted under the data key. Every
//! block and every MAC has its own IV: the next IV is HKDF-SHA256 of the previous one, with the transport keys as
//! salt and info, so the chain depends only on the first IV. The header's MAC is the AES-CMAC of the padded length
//! (u32, little-endian), the first IV and the padding field.
//!
//! What has been checked, and how: the header layout and the block sizes on one install's game database, `sfsdata`,
//! `NerdData.json`, a profile and the 67 method-22 entries of `Rendering.zip` (every size divides exactly, every
//! padding is below its block size). The cipher, the IV chain and the MACs are as a community tool
//! that decrypts these files describes them; this crate's tests check its AES-CBC, CMAC and HKDF steps against their
//! standards' published vectors, and `tests/real_data.rs` checks the whole against a real install's files when you
//! have one and its keys. For the game database, whether the header's MAC and the padding field mean the same as in
//! the small-block containers is not established: [`Container::decrypt`] does not rely on them.
use crate::keys::{TransportKeys, UsageKeys};
use crate::{Error, Result};
use aes::cipher::{Array, BlockCipherDecrypt, KeyInit};
use aes::{Aes128, Aes256};

/// The header's length in bytes.
pub const HEADER_LEN: usize = 36;
/// A MAC's length in bytes, after each block.
pub const MAC_LEN: usize = 16;

/// A container's block size, which the file's kind decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlockSize {
    /// 0x200 bytes: settings archives' method-22 entries, `NerdData.json`, the profile.
    Small,
    /// 0x20000 bytes: the game database (`gamedbRC.slt`) and `sfsdata`.
    Large,
}

impl BlockSize {
    /// The block's length in bytes.
    pub const fn bytes(self) -> usize {
        match self {
            BlockSize::Small => 0x200,
            BlockSize::Large => 0x20000,
        }
    }
}

/// A container, read from its bytes: the header's fields, and its blocks still encrypted.
#[derive(Clone, Copy, Debug)]
pub struct Container<'a> {
    iv: [u8; 16],
    padding: u32,
    header_mac: [u8; 16],
    block_size: BlockSize,
    body: &'a [u8],
}

impl<'a> Container<'a> {
    /// Reads a container whose blocks are `block_size` long. Refused: fewer than 36 bytes, a body that is not a whole,
    /// non-zero number of blocks each with its MAC, and padding not less than a block.
    pub fn parse(bytes: &'a [u8], block_size: BlockSize) -> Result<Self> {
        let (Some(header), Some(body)) = (bytes.get(..HEADER_LEN), bytes.get(HEADER_LEN..)) else {
            return Err(Error::Container(format!(
                "{} bytes is shorter than the {HEADER_LEN}-byte header",
                bytes.len()
            )));
        };
        let slot = block_size.bytes() + MAC_LEN;
        if body.is_empty() || body.len() % slot != 0 {
            return Err(Error::Container(format!(
                "the {} bytes after the header are not a whole number of {:#x}-byte blocks each followed by a \
                 {MAC_LEN}-byte MAC",
                body.len(),
                block_size.bytes()
            )));
        }
        let mut iv = [0u8; 16];
        let mut padding = [0u8; 4];
        let mut header_mac = [0u8; 16];
        iv.copy_from_slice(header.get(..16).unwrap_or_default());
        padding.copy_from_slice(header.get(16..20).unwrap_or_default());
        header_mac.copy_from_slice(header.get(20..36).unwrap_or_default());
        let padding = u32::from_le_bytes(padding);
        if usize::try_from(padding).map_or(true, |p| p >= block_size.bytes()) {
            return Err(Error::Container(format!(
                "the padding at offset 16, {padding}, is not less than the block size {:#x}",
                block_size.bytes()
            )));
        }
        Ok(Self {
            iv,
            padding,
            header_mac,
            block_size,
            body,
        })
    }

    /// The first block's IV, as stored.
    pub fn iv(&self) -> &[u8; 16] {
        &self.iv
    }
    /// The padding field, as stored: bytes at the end of the last block that are not plaintext.
    pub fn padding(&self) -> u32 {
        self.padding
    }
    /// The header's MAC, as stored.
    pub fn header_mac(&self) -> &[u8; 16] {
        &self.header_mac
    }
    /// The block size the container was read with.
    pub fn block_size(&self) -> BlockSize {
        self.block_size
    }
    /// The number of blocks.
    pub fn block_count(&self) -> usize {
        self.body.len() / (self.block_size.bytes() + MAC_LEN)
    }
    /// The blocks' total length: the plaintext with its padding.
    pub fn padded_len(&self) -> usize {
        self.block_count() * self.block_size.bytes()
    }
    /// The plaintext's length as the header states it: the padded length less the padding.
    pub fn plaintext_len(&self) -> usize {
        // parse checked padding < block size <= padded length
        self.padded_len() - self.padding as usize
    }

    /// Whether the header's MAC matches under `keys`' MAC key: the check that `keys` are the ones this container was
    /// written with. It reads nothing but the header.
    pub fn header_authentic(&self, keys: &UsageKeys) -> bool {
        let Ok(padded) = u32::try_from(self.padded_len()) else {
            return false;
        };
        let mut message = [0u8; 24];
        message[..4].copy_from_slice(&padded.to_le_bytes());
        message[4..20].copy_from_slice(&self.iv);
        message[20..].copy_from_slice(&self.padding.to_le_bytes());
        cmac(keys.mac(), &message) == self.header_mac
    }

    /// Decrypts every block with `keys`' data key, without checking a MAC: the padded plaintext
    /// ([`Self::padded_len`] bytes; its first [`Self::plaintext_len`] are the plaintext as the header states it).
    /// Wrong keys give wrong bytes, not an error; [`Self::decrypt_verified`] checks.
    pub fn decrypt(&self, transport: &TransportKeys, keys: &UsageKeys) -> Vec<u8> {
        let cipher = Aes256::new(&Array::from(*keys.data()));
        let mut out = Vec::with_capacity(self.padded_len());
        let mut iv = self.iv;
        for (block, _mac) in self.slots() {
            out.extend_from_slice(block);
            let start = out.len() - block.len();
            cbc_decrypt(&cipher, &iv, out.get_mut(start..).unwrap_or_default());
            iv = next_iv(&iv, transport.salt(), transport.info());
            iv = next_iv(&iv, transport.salt(), transport.info());
        }
        out
    }

    /// Decrypts every block and checks the header's MAC and every block's: the plaintext, without its padding. A
    /// mismatch is an [`Error::Mac`] naming the header or the block.
    pub fn decrypt_verified(&self, transport: &TransportKeys, keys: &UsageKeys) -> Result<Vec<u8>> {
        if !self.header_authentic(keys) {
            return Err(Error::Mac {
                what: "the header".to_owned(),
            });
        }
        let cipher = Aes256::new(&Array::from(*keys.data()));
        let mut out = Vec::with_capacity(self.padded_len());
        let mut iv = self.iv;
        for (index, (block, mac)) in self.slots().enumerate() {
            out.extend_from_slice(block);
            let start = out.len() - block.len();
            let plain = out.get_mut(start..).unwrap_or_default();
            cbc_decrypt(&cipher, &iv, plain);
            iv = next_iv(&iv, transport.salt(), transport.info());
            let mut stored = [0u8; MAC_LEN];
            stored.copy_from_slice(mac);
            cbc_decrypt(&cipher, &iv, &mut stored);
            if cmac(keys.mac(), plain) != stored {
                return Err(Error::Mac {
                    what: format!("block {index}"),
                });
            }
            iv = next_iv(&iv, transport.salt(), transport.info());
        }
        out.truncate(self.plaintext_len());
        Ok(out)
    }

    /// Each block's ciphertext and the MAC after it.
    fn slots(&self) -> impl Iterator<Item = (&'a [u8], &'a [u8])> + use<'a> {
        let block = self.block_size.bytes();
        self.body
            .chunks_exact(block + MAC_LEN)
            .map(move |slot| slot.split_at(block))
    }
}

/// AES-CBC decryption in place of whole 16-byte blocks (`data`'s length is a multiple of 16) under `iv`.
fn cbc_decrypt(cipher: &Aes256, iv: &[u8; 16], data: &mut [u8]) {
    let mut previous = *iv;
    for chunk in data.as_chunks_mut::<16>().0 {
        let mut ciphertext = [0u8; 16];
        ciphertext.copy_from_slice(chunk);
        let mut block = Array::from(ciphertext);
        cipher.decrypt_block(&mut block);
        for ((out, plain), prev) in chunk.iter_mut().zip(block.iter()).zip(previous.iter()) {
            *out = plain ^ prev;
        }
        previous = ciphertext;
    }
}

/// The IV after `iv`: the first 16 bytes of HKDF-SHA256 with `iv` as input keying material.
fn next_iv(iv: &[u8], salt: &[u8], info: &[u8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    // 16 bytes is far below HKDF-SHA256's 8160-byte limit, so expand cannot fail
    let _ = hkdf::Hkdf::<sha2::Sha256>::new(Some(salt), iv).expand(info, &mut out);
    out
}

/// AES-128-CMAC (RFC 4493) of `message` under `key`.
fn cmac(key: &[u8; 16], message: &[u8]) -> [u8; 16] {
    use cmac::{KeyInit as _, Mac as _};
    let mut mac = cmac::Cmac::<Aes128>::new(&Array::from(*key));
    mac.update(message);
    mac.finalize().into_bytes().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// RFC 4493 section 4: the AES-128 CMAC examples (empty, one block, 40 bytes, four blocks).
    #[test]
    fn cmac_matches_rfc_4493() {
        let key: [u8; 16] = hex("2b7e151628aed2a6abf7158809cf4f3c").try_into().unwrap();
        let message = hex("6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e51\
             30c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710");
        for (len, tag) in [
            (0, "bb1d6929e95937287fa37d129b756746"),
            (16, "070a16b46b4d4144f79bdd9dd04a287c"),
            (40, "dfa66747de9ae63030ca32611497c827"),
            (64, "51f0bebf7e3b9d92fc49741779363cfe"),
        ] {
            assert_eq!(cmac(&key, &message[..len]).to_vec(), hex(tag), "{len} bytes");
        }
    }

    /// RFC 5869 test case 1 (HKDF-SHA256): the first 16 bytes of its output, with the IV as input keying material,
    /// the first transport key as salt and the second as info.
    #[test]
    fn next_iv_matches_rfc_5869() {
        let ikm = [0x0bu8; 22];
        let salt = hex("000102030405060708090a0b0c");
        let info = hex("f0f1f2f3f4f5f6f7f8f9");
        assert_eq!(
            next_iv(&ikm, &salt, &info).to_vec(),
            hex("3cb25f25faacd57a90434f64d0362f2a")
        );
    }

    /// NIST SP 800-38A F.2.6, CBC-AES256.Decrypt: four blocks.
    #[test]
    fn cbc_decrypt_matches_sp_800_38a() {
        let key: [u8; 32] = hex("603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4")
            .try_into()
            .unwrap();
        let iv: [u8; 16] = hex("000102030405060708090a0b0c0d0e0f").try_into().unwrap();
        let mut data = hex("f58c4c04d6e5f1ba779eabfb5f7bfbd69cfc4e967edb808d679f777bc6702c7d\
             39f23369a9d9bacfa530e26304231461b2eb05e2c39be9fcda6c19078c6a9d1b");
        cbc_decrypt(&Aes256::new(&Array::from(key)), &iv, &mut data);
        assert_eq!(
            data,
            hex("6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e51\
                 30c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710")
        );
    }

    fn header(padding: u32) -> Vec<u8> {
        let mut h = vec![0u8; HEADER_LEN];
        h[16..20].copy_from_slice(&padding.to_le_bytes());
        h
    }

    #[test]
    fn framing_counts_blocks_and_plaintext() {
        let mut file = header(434);
        file.resize(HEADER_LEN + 2 * (0x200 + MAC_LEN), 0);
        let c = Container::parse(&file, BlockSize::Small).unwrap();
        assert_eq!(
            (c.block_count(), c.padded_len(), c.plaintext_len()),
            (2, 0x400, 0x400 - 434)
        );

        let mut large = header(113_664);
        large.resize(HEADER_LEN + 0x20000 + MAC_LEN, 0);
        let c = Container::parse(&large, BlockSize::Large).unwrap();
        assert_eq!((c.block_count(), c.plaintext_len()), (1, 0x20000 - 113_664));
        assert_eq!(c.padding(), 113_664);
    }

    #[test]
    fn framing_refusals() {
        let short = vec![0u8; HEADER_LEN - 1];
        assert!(matches!(
            Container::parse(&short, BlockSize::Small),
            Err(Error::Container(_))
        ));
        // no block at all
        assert!(matches!(
            Container::parse(&header(0), BlockSize::Small),
            Err(Error::Container(_))
        ));
        // a block without its MAC
        let mut file = header(0);
        file.resize(HEADER_LEN + 0x200, 0);
        assert!(matches!(
            Container::parse(&file, BlockSize::Small),
            Err(Error::Container(_))
        ));
        // a small-block body read as large
        file.resize(HEADER_LEN + 0x200 + MAC_LEN, 0);
        assert!(Container::parse(&file, BlockSize::Small).is_ok());
        assert!(matches!(
            Container::parse(&file, BlockSize::Large),
            Err(Error::Container(_))
        ));
        // padding of a whole block
        let mut file = header(0x200);
        file.resize(HEADER_LEN + 0x200 + MAC_LEN, 0);
        assert!(matches!(
            Container::parse(&file, BlockSize::Small),
            Err(Error::Container(_))
        ));
    }

    /// Every cut of a three-block container reads only at a block boundary, and never panics.
    #[test]
    fn every_truncation_reads_only_at_a_block_boundary() {
        let slot = 0x200 + MAC_LEN;
        let mut file = header(7);
        file.resize(HEADER_LEN + 3 * slot, 0);
        for len in 0..=file.len() {
            let whole = len > HEADER_LEN && (len - HEADER_LEN).is_multiple_of(slot);
            assert_eq!(
                Container::parse(&file[..len], BlockSize::Small).is_ok(),
                whole,
                "{len} bytes"
            );
        }
    }
}
