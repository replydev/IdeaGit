//! Commit hash value. Port of the validation in `HashImpl.build` as used by `GitRefUtil.parseHash`:
//! any non-empty hexadecimal string is accepted (SHA-1 and SHA-256 alike).

use std::fmt;

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash(String);

impl Hash {
    /// Returns `None` if `value` is empty or contains a non-hex character (e.g. `ref: refs/heads/main`).
    pub fn parse(value: &str) -> Option<Hash> {
        if !value.is_empty() && value.bytes().all(|b| b.is_ascii_hexdigit()) {
            Some(Hash(value.to_ascii_lowercase()))
        } else {
            None
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({})", self.0)
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_hash_and_lowercases() {
        let h = Hash::parse("0E1D130689BC52F140C5C374AA9CC2B8916C0AD7").unwrap();
        assert_eq!(h.as_str(), "0e1d130689bc52f140c5c374aa9cc2b8916c0ad7");
    }

    #[test]
    fn rejects_symbolic_ref_and_empty() {
        assert!(Hash::parse("ref: refs/heads/master").is_none());
        assert!(Hash::parse("").is_none());
    }
}
