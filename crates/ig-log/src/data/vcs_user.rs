//! Commit authors and committers.
//!
//! Ported from `platform/vcs-log/api/src/com/intellij/vcs/log/VcsUser.java`,
//! `platform/vcs-impl/shared/src/com/intellij/vcs/log/impl/VcsUserImpl.kt` and the parts of
//! `platform/vcs-impl/shared/src/com/intellij/vcs/log/util/VcsUserUtil.java` used by the log data.
//!
//! Deviation: users are not interned (`VcsUserRegistryImpl`); they are small owned values.

use std::fmt;

/// A user; equal when name and email are equal (`VcsUserImpl` is a data class).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VcsUser {
    name: String,
    email: String,
}

impl VcsUser {
    /// `VcsUserUtil.createUser`: the email is lowercased.
    pub fn create_user(name: &str, email: &str) -> VcsUser {
        VcsUser {
            name: name.to_string(),
            email: email.to_lowercase(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    /// `VcsUserUtil.toExactString`: `name <email>`, or whichever part is non-empty.
    pub fn to_exact_string(&self) -> String {
        if self.name.is_empty() {
            return self.email.clone();
        }
        if self.email.is_empty() {
            return self.name.clone();
        }
        format!("{} <{}>", self.name, self.email)
    }
}

impl fmt::Display for VcsUser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_exact_string())
    }
}

impl fmt::Debug for VcsUser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VcsUser({})", self.to_exact_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_is_lowercased_and_exact_string_drops_empty_parts() {
        let u = VcsUser::create_user("John Doe", "John.Doe@Example.com");
        assert_eq!(u.email(), "john.doe@example.com");
        assert_eq!(u.to_exact_string(), "John Doe <john.doe@example.com>");
        assert_eq!(VcsUser::create_user("", "a@b").to_exact_string(), "a@b");
        assert_eq!(VcsUser::create_user("n", "").to_exact_string(), "n");
    }
}
