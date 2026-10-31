//! The keys a container is decrypted with, which you supply: no key ships with this crate.
//!
//! [`Keys::parse`] reads them from a short text file, one `name = hexadecimal` per line (`#` starts a comment):
//!
//! ```text
//! transport.salt = <64 hex digits>     # HKDF salt of the IV chain, shared by every container
//! transport.info = <64 hex digits>     # HKDF info of the IV chain
//! GameDB.data    = <64 hex digits>     # a usage's AES-256 data key
//! GameDB.mac     = <32 hex digits>     # the same usage's AES-128 CMAC key
//! General.data   = ...                 # as many usages as you have, each with both keys
//! General.mac    = ...
//! gamedb.scramble = <8 hex digits>     # the game database's keystream constant (crate::gamedb)
//! ```
//!
//! A usage is a set of keys for a kind of file; its name is yours to choose (letters, digits, `_` and `-`).
//! [`Keys::usage_for`] finds which usage a container was written with from its header's MAC.
use crate::container::Container;
use crate::{Error, Result};
use std::collections::BTreeMap;

/// The two keys of the IV chain, shared by every container: HKDF-SHA256's salt and info.
#[derive(Clone, PartialEq, Eq)]
pub struct TransportKeys {
    salt: [u8; 32],
    info: [u8; 32],
}

impl TransportKeys {
    /// The transport keys from their bytes.
    pub fn new(salt: [u8; 32], info: [u8; 32]) -> Self {
        Self { salt, info }
    }
    /// HKDF's salt.
    pub fn salt(&self) -> &[u8; 32] {
        &self.salt
    }
    /// HKDF's info.
    pub fn info(&self) -> &[u8; 32] {
        &self.info
    }
}

impl std::fmt::Debug for TransportKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TransportKeys(..)")
    }
}

/// A usage's two keys: the data key (AES-256) and the MAC key (AES-128 CMAC).
#[derive(Clone, PartialEq, Eq)]
pub struct UsageKeys {
    data: [u8; 32],
    mac: [u8; 16],
}

impl UsageKeys {
    /// A usage's keys from their bytes.
    pub fn new(data: [u8; 32], mac: [u8; 16]) -> Self {
        Self { data, mac }
    }
    /// The AES-256 key of the blocks and their MACs.
    pub fn data(&self) -> &[u8; 32] {
        &self.data
    }
    /// The AES-128 CMAC key.
    pub fn mac(&self) -> &[u8; 16] {
        &self.mac
    }
}

impl std::fmt::Debug for UsageKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UsageKeys(..)")
    }
}

/// The keys a keys text supplies. What it does not supply is `None`; its `Debug` names what is there, never a key.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Keys {
    transport: Option<TransportKeys>,
    usages: BTreeMap<String, UsageKeys>,
    gamedb_scramble: Option<u32>,
}

impl std::fmt::Debug for Keys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keys")
            .field("transport", &self.transport.is_some())
            .field("usages", &self.usages.keys().collect::<Vec<_>>())
            .field("gamedb_scramble", &self.gamedb_scramble.is_some())
            .finish()
    }
}

impl Keys {
    /// Reads a keys text (the module's documentation shows its lines). Refused, naming the line: a line that is not
    /// `name = value`, a name it does not know, a value of the wrong length or not hexadecimal, a name given twice,
    /// one transport key without the other, and a usage with only one of its two keys.
    pub fn parse(text: &str) -> Result<Self> {
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        let (mut salt, mut info) = (None, None);
        let mut data: BTreeMap<String, ([u8; 32], usize)> = BTreeMap::new();
        let mut mac: BTreeMap<String, ([u8; 16], usize)> = BTreeMap::new();
        let mut gamedb_scramble = None;
        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            let content = raw.split('#').next().unwrap_or_default().trim();
            if content.is_empty() {
                continue;
            }
            let err = |detail: String| Error::Keys { line, detail };
            let Some((name, value)) = content.split_once('=') else {
                return Err(err(format!("`{content}` is not `name = value`")));
            };
            let (name, value) = (name.trim(), value.trim());
            if let Some(first) = seen.insert(name.to_owned(), line) {
                return Err(err(format!("`{name}` is given again (first on line {first})")));
            }
            match name {
                "transport.salt" => salt = Some(hex::<32>(value).map_err(err)?),
                "transport.info" => info = Some(hex::<32>(value).map_err(err)?),
                "gamedb.scramble" => {
                    gamedb_scramble = Some(u32::from_be_bytes(hex::<4>(value).map_err(err)?));
                }
                _ => {
                    let Some((usage, which)) = name.rsplit_once('.') else {
                        return Err(err(format!("`{name}` is not a key this text can hold")));
                    };
                    if usage.is_empty()
                        || !usage
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                    {
                        return Err(err(format!(
                            "`{usage}` is not a usage name (letters, digits, `_`, `-`)"
                        )));
                    }
                    match which {
                        "data" => {
                            data.insert(usage.to_owned(), (hex::<32>(value).map_err(err)?, line));
                        }
                        "mac" => {
                            mac.insert(usage.to_owned(), (hex::<16>(value).map_err(err)?, line));
                        }
                        _ => return Err(err(format!("`{name}` is not a key this text can hold"))),
                    }
                }
            }
        }
        let transport = match (salt, info) {
            (Some(salt), Some(info)) => Some(TransportKeys { salt, info }),
            (None, None) => None,
            (Some(_), None) | (None, Some(_)) => {
                let (given, missing) = if salt.is_some() {
                    ("transport.salt", "transport.info")
                } else {
                    ("transport.info", "transport.salt")
                };
                let line = seen.get(given).copied().unwrap_or_default();
                return Err(Error::Keys {
                    line,
                    detail: format!("`{given}` is given without `{missing}`"),
                });
            }
        };
        for (usage, (_, line)) in &data {
            if !mac.contains_key(usage) {
                return Err(Error::Keys {
                    line: *line,
                    detail: format!("`{usage}.data` is given without `{usage}.mac`"),
                });
            }
        }
        let mut usages = BTreeMap::new();
        for (usage, (mac_key, line)) in mac {
            let Some((data_key, _)) = data.remove(&usage) else {
                return Err(Error::Keys {
                    line,
                    detail: format!("`{usage}.mac` is given without `{usage}.data`"),
                });
            };
            usages.insert(usage, UsageKeys::new(data_key, mac_key));
        }
        Ok(Self {
            transport,
            usages,
            gamedb_scramble,
        })
    }

    /// The transport keys, when the text gave them.
    pub fn transport(&self) -> Option<&TransportKeys> {
        self.transport.as_ref()
    }
    /// The keys of the usage `name`, when the text gave them.
    pub fn usage(&self, name: &str) -> Option<&UsageKeys> {
        self.usages.get(name)
    }
    /// The usages' names, in order.
    pub fn usage_names(&self) -> impl Iterator<Item = &str> {
        self.usages.keys().map(String::as_str)
    }
    /// The game database's keystream constant ([`crate::gamedb`]), when the text gave it.
    pub fn gamedb_scramble(&self) -> Option<u32> {
        self.gamedb_scramble
    }
    /// The usage whose MAC key authenticates `container`'s header ([`Container::header_authentic`]), with its keys;
    /// `None` when none does.
    pub fn usage_for(&self, container: &Container<'_>) -> Option<(&str, &UsageKeys)> {
        self.usages
            .iter()
            .find(|(_, keys)| container.header_authentic(keys))
            .map(|(name, keys)| (name.as_str(), keys))
    }
}

/// `N` bytes from `2N` hexadecimal digits (either case).
fn hex<const N: usize>(value: &str) -> std::result::Result<[u8; N], String> {
    let digits = value.as_bytes();
    if digits.len() != 2 * N {
        return Err(format!(
            "the value has {} characters; this key is {} hexadecimal digits",
            digits.len(),
            2 * N
        ));
    }
    let mut out = [0u8; N];
    for (byte, pair) in out.iter_mut().zip(digits.as_chunks::<2>().0) {
        let text = std::str::from_utf8(pair).unwrap_or("");
        *byte = u8::from_str_radix(text, 16)
            .map_err(|_| format!("`{}` is not hexadecimal", String::from_utf8_lossy(pair)))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SALT: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
    const DATA: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const MAC: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn reads_every_line_kind() {
        let text = format!(
            "# test keys\n\ntransport.salt = {SALT}\ntransport.info={SALT}  # same\nGameDB.data = {DATA}\n\
             GameDB.mac = {MAC}\nGeneral.mac = {MAC}\nGeneral.data = {DATA}\ngamedb.scramble = 0000ABCD\n"
        );
        let keys = Keys::parse(&text).unwrap();
        let transport = keys.transport().unwrap();
        assert_eq!(transport.salt()[31], 0x1f);
        assert_eq!(transport.info(), transport.salt());
        assert_eq!(keys.usage_names().collect::<Vec<_>>(), ["GameDB", "General"]);
        assert_eq!(keys.usage("GameDB").unwrap().data(), &[0xaa; 32]);
        assert_eq!(keys.usage("General").unwrap().mac()[..2], [0x01, 0x23]);
        assert_eq!(keys.gamedb_scramble(), Some(0xabcd));
        assert!(!format!("{keys:?}").contains("aa"), "Debug shows no key");
    }

    #[test]
    fn empty_text_has_no_keys() {
        let keys = Keys::parse("# nothing\n").unwrap();
        assert!(keys.transport().is_none() && keys.usage_names().next().is_none());
    }

    #[test]
    fn refusals_name_their_line() {
        let line = |text: &str| match Keys::parse(text) {
            Err(Error::Keys { line, .. }) => line,
            other => panic!("{text:?}: {other:?}"),
        };
        assert_eq!(line("\nnot a pair\n"), 2);
        assert_eq!(line(&format!("x.data = {DATA}\nx.data = {DATA}\n")), 2);
        assert_eq!(line(&format!("transport.salt = {}\n", &SALT[2..])), 1);
        assert_eq!(line(&format!("x.mac = {}zz\n", &MAC[2..])), 1);
        assert_eq!(line(&format!("x.key = {MAC}\n")), 1);
        assert_eq!(line(&format!("a b.mac = {MAC}\n")), 1);
        assert_eq!(line(&format!("transport.info = {SALT}\n")), 1);
        assert_eq!(line(&format!("\nx.data = {DATA}\n")), 2);
        assert_eq!(line(&format!("\n\nx.mac = {MAC}\n")), 3);
    }
}
