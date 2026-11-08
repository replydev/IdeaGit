//! Commit hashes.
//!
//! Ported from `platform/vcs-api/shared/src/com/intellij/vcs/log/Hash.java` and
//! `platform/vcs-impl/shared/src/com/intellij/vcs/log/impl/HashImpl.java`.
//!
//! Like `HashImpl`, a hash is stored as bytes: a parity byte (1 for an odd number of hex digits),
//! then two hex digits per byte, so any non-empty hex string round-trips (`a`, `a0`, full SHA-1 or
//! SHA-256). `as_string` is lowercase.

use std::fmt;

/// `VcsUtil.SHORT_HASH_LENGTH`.
pub const SHORT_HASH_LENGTH: usize = 8;

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash {
    data: Box<[u8]>,
}

/// The input of [`Hash::build`] was empty or contained a non-hex character.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("bad hash string: {0}")]
pub struct BadHashError(pub String);

impl Hash {
    /// `HashImpl.build`.
    ///
    /// # Panics
    /// If `input` is empty or not hexadecimal (upstream: `assert` / `IllegalArgumentException`).
    pub fn build(input: &str) -> Hash {
        Hash::try_build(input).unwrap_or_else(|e| panic!("{e}"))
    }

    /// `HashImpl.build` returning an error instead of panicking.
    pub fn try_build(input: &str) -> Result<Hash, BadHashError> {
        let bytes = input.as_bytes();
        if bytes.is_empty() {
            return Err(BadHashError(input.to_string()));
        }
        let digit = |c: u8| {
            (c as char)
                .to_digit(16)
                .map(|d| d as u8)
                .ok_or_else(|| BadHashError(input.to_string()))
        };
        let odd = bytes.len() % 2;
        let mut data = Vec::with_capacity(bytes.len() / 2 + 1 + odd);
        data.push(odd as u8);
        for pair in bytes.chunks(2) {
            if pair.len() == 2 {
                data.push(digit(pair[0])? * 16 + digit(pair[1])?);
            } else {
                data.push(digit(pair[0])?);
            }
        }
        Ok(Hash {
            data: data.into_boxed_slice(),
        })
    }

    /// `asString`: lowercase hex.
    pub fn as_string(&self) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let odd = self.data[0] == 1;
        let mut s = String::with_capacity((self.data.len() - 1) * 2);
        for (i, &b) in self.data.iter().enumerate().skip(1) {
            if odd && i == self.data.len() - 1 {
                s.push(DIGITS[(b & 0xf) as usize] as char);
            } else {
                s.push(DIGITS[(b >> 4) as usize] as char);
                s.push(DIGITS[(b & 0xf) as usize] as char);
            }
        }
        s
    }

    /// `toShortString`: the first [`SHORT_HASH_LENGTH`] hex digits.
    pub fn to_short_string(&self) -> String {
        let mut s = self.as_string();
        s.truncate(SHORT_HASH_LENGTH);
        s
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_string())
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({})", self.as_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_even_odd_and_full_hashes() {
        for s in [
            "a",
            "a0",
            "e100001",
            "0e1d130689bc52f140c5c374aa9cc2b8916c0ad7",
        ] {
            assert_eq!(Hash::build(s).as_string(), s);
        }
        assert_eq!(Hash::build("ABCDEF").as_string(), "abcdef");
        assert_eq!(Hash::build("ABCDEF"), Hash::build("abcdef"));
        assert_ne!(Hash::build("a"), Hash::build("0a"));
        assert_eq!(
            Hash::build("0e1d130689bc52f1").to_short_string(),
            "0e1d1306"
        );
    }

    #[test]
    fn rejects_empty_and_non_hex() {
        assert!(Hash::try_build("").is_err());
        assert!(Hash::try_build("xyz").is_err());
        assert!(Hash::try_build("ref: refs/heads/main").is_err());
    }
}
