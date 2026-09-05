//! Shape checks for the operator-supplied identity fields.
//!
//! Each of these values is either interpolated into a `bash -c` chroot script
//! (timezone, locale, user) or written verbatim into a file the target boots
//! from (hostname, keymap), and each arrives from an untrusted source: a CLI
//! flag, the process environment, or `/etc/appsynergy/machine.env`. Quoting at
//! the call site is not a defence — a value containing `'` escapes it. The
//! character sets below are allow-lists, so no shell metacharacter is
//! representable and the call sites need no quoting discipline at all.
//!
//! Pure and dependency-free by design: the one filesystem check
//! ([`check_timezone_exists`]) is a separate call so the unit tests can stay
//! independent of the host they run on.

use anyhow::{bail, Result};
use std::path::Path;

const ZONEINFO: &str = "/usr/share/zoneinfo";

/// One-line rejection: field, the value as rejected, and the shape required.
/// `{value:?}` so an embedded newline or control byte is visible in the log.
fn reject(field: &str, value: &str, shape: &str) -> anyhow::Error {
    anyhow::anyhow!("invalid {field} {value:?}: {shape}")
}

/// First char `[a-z_]`, rest `[a-z0-9_-]`, 1..=32 chars, never `root`.
pub fn validate_username(value: &str) -> Result<()> {
    const SHAPE: &str = "expected 1..=32 chars, first [a-z_] then [a-z0-9_-]";
    let ok = (1..=32).contains(&value.len())
        && value.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if !ok {
        return Err(reject("username", value, SHAPE));
    }
    // `root` already exists with uid 0: `useradd` would fail mid-chroot, and the
    // `chown -R {u}:{u} /home/{u}` steps would then run against a home directory
    // that is not root's, silently giving the wrong account the operator's keys.
    if value == "root" {
        return Err(reject("username", value, "must not be root"));
    }
    Ok(())
}

/// RFC1123 single label: 1..=63 chars of `[a-z0-9-]`, alphanumeric at both ends.
pub fn validate_hostname(value: &str) -> Result<()> {
    const SHAPE: &str =
        "expected an RFC1123 label: 1..=63 chars of [a-z0-9-], alphanumeric at both ends";
    let alnum = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    let ok = (1..=63).contains(&value.len())
        && value.starts_with(alnum)
        && value.ends_with(alnum)
        && value.chars().all(|c| alnum(c) || c == '-');
    if !ok {
        return Err(reject("hostname", value, SHAPE));
    }
    Ok(())
}

/// 1..=3 `/`-separated components, each 1..=32 chars of `[A-Za-z0-9_+-]`
/// starting alphanumeric. Shape only — see [`check_timezone_exists`].
pub fn validate_timezone(value: &str) -> Result<()> {
    const SHAPE: &str = "expected 1..=3 '/'-separated components of 1..=32 chars \
                         from [A-Za-z0-9_+-], each starting alphanumeric";
    // `.` is outside the set, so `..` is unrepresentable rather than filtered:
    // the value cannot escape /usr/share/zoneinfo however it is joined. A
    // leading `-` is refused for the same class of reason — it would read as an
    // option to whatever consumes the value, not as data.
    let parts: Vec<&str> = value.split('/').collect();
    let ok = (1..=3).contains(&parts.len())
        && parts.iter().all(|p| {
            (1..=32).contains(&p.len())
                && p.starts_with(|c: char| c.is_ascii_alphanumeric())
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-'))
        });
    if !ok {
        return Err(reject("timezone", value, SHAPE));
    }
    Ok(())
}

/// `C`, `POSIX`, `C.UTF-8`, or `ll[l][_CC][.codeset][@modifier]`; <=32 chars.
pub fn validate_locale(value: &str) -> Result<()> {
    const SHAPE: &str = "expected C, POSIX, C.UTF-8 or ll[l][_CC][.codeset][@modifier], \
                         1..=32 chars from [A-Za-z0-9._@-]";
    if !(1..=32).contains(&value.len())
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '@' | '-'))
    {
        return Err(reject("locale", value, SHAPE));
    }
    if matches!(value, "C" | "POSIX" | "C.UTF-8") {
        return Ok(());
    }
    let (head, modifier) = match value.split_once('@') {
        Some((h, m)) => (h, Some(m)),
        None => (value, None),
    };
    let (head, codeset) = match head.split_once('.') {
        Some((h, c)) => (h, Some(c)),
        None => (head, None),
    };
    let (lang, territory) = match head.split_once('_') {
        Some((l, t)) => (l, Some(t)),
        None => (head, None),
    };
    let ok = (2..=3).contains(&lang.len())
        && lang.chars().all(|c| c.is_ascii_lowercase())
        && territory.is_none_or(|t| t.len() == 2 && t.chars().all(|c| c.is_ascii_uppercase()))
        && codeset.is_none_or(|c| {
            (1..=16).contains(&c.len()) && c.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
        && modifier.is_none_or(|m| {
            (1..=16).contains(&m.len()) && m.chars().all(|c| c.is_ascii_alphanumeric())
        });
    if !ok {
        return Err(reject("locale", value, SHAPE));
    }
    Ok(())
}

/// 1..=64 chars of `[A-Za-z0-9._-]`, starting alphanumeric.
pub fn validate_keymap(value: &str) -> Result<()> {
    const SHAPE: &str = "expected 1..=64 chars of [A-Za-z0-9._-], starting alphanumeric";
    let ok = (1..=64).contains(&value.len())
        && value.starts_with(|c: char| c.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !ok {
        return Err(reject("keymap", value, SHAPE));
    }
    Ok(())
}

/// Runtime-only companion to [`validate_timezone`]: the zone must exist in the
/// live system's database. Kept out of the shape check because the unit tests
/// must not depend on the host carrying `tzdata`; where the database is absent
/// entirely there is nothing to check against, so this passes.
pub fn check_timezone_exists(value: &str) -> Result<()> {
    let root = Path::new(ZONEINFO);
    if !root.is_dir() {
        return Ok(());
    }
    if !root.join(value).exists() {
        bail!("invalid timezone {value:?}: no such zone under {ZONEINFO}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Payloads that must never survive validation, whichever field they enter.
    /// The first four are shell injection into `cmd::arch_chroot`'s `bash -c`;
    /// the rest are path traversal, option injection, and length abuse.
    fn rejected() -> Vec<String> {
        let mut v: Vec<String> = [
            "'; rm -rf / #",
            "$(reboot)",
            "`reboot`",
            "a b",
            "a\nb",
            "../../etc",
            "-x",
            "",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        v.push("a".repeat(100));
        v
    }

    fn assert_all_rejected(f: fn(&str) -> Result<()>, field: &str) {
        for bad in rejected() {
            assert!(f(&bad).is_err(), "{field} must reject {bad:?}");
        }
    }

    #[test]
    fn username_rejects_corpus() {
        assert_all_rejected(validate_username, "username");
        assert!(validate_username("root").is_err());
        assert!(
            validate_username("Imma").is_err(),
            "uppercase is not a valid login"
        );
        assert!(
            validate_username("1abc").is_err(),
            "must not start with a digit"
        );
        assert!(validate_username(&"a".repeat(33)).is_err());
    }

    #[test]
    fn username_accepts_real_logins() {
        for good in ["imma", "_svc", "appsynergy-server", &"a".repeat(32)] {
            assert!(
                validate_username(good).is_ok(),
                "username must accept {good:?}"
            );
        }
    }

    #[test]
    fn hostname_rejects_corpus() {
        assert_all_rejected(validate_hostname, "hostname");
        assert!(
            validate_hostname("nuc-").is_err(),
            "must not end with a hyphen"
        );
        assert!(validate_hostname("_nuc").is_err());
        assert!(validate_hostname("NUC").is_err());
        assert!(validate_hostname(&"a".repeat(64)).is_err());
    }

    #[test]
    fn hostname_accepts_real_labels() {
        for good in [
            "appsynergy-server",
            "nuc",
            "appsynergy",
            "n1",
            &"a".repeat(63),
        ] {
            assert!(
                validate_hostname(good).is_ok(),
                "hostname must accept {good:?}"
            );
        }
    }

    #[test]
    fn timezone_rejects_corpus() {
        assert_all_rejected(validate_timezone, "timezone");
        assert!(validate_timezone("/UTC").is_err(), "no leading slash");
        assert!(validate_timezone("UTC/").is_err(), "no trailing slash");
        assert!(
            validate_timezone("America//Sao_Paulo").is_err(),
            "no empty component"
        );
        assert!(
            validate_timezone("a/b/c/d").is_err(),
            "at most 3 components"
        );
        assert!(validate_timezone("../zoneinfo/UTC").is_err());
    }

    #[test]
    fn timezone_accepts_real_zones() {
        for good in [
            "America/Sao_Paulo",
            "UTC",
            "Etc/GMT+3",
            "Europe/London",
            "America/Argentina/Buenos_Aires",
        ] {
            assert!(
                validate_timezone(good).is_ok(),
                "timezone must accept {good:?}"
            );
        }
    }

    #[test]
    fn locale_rejects_corpus() {
        assert_all_rejected(validate_locale, "locale");
        assert!(
            validate_locale("en_us.UTF-8").is_err(),
            "territory is uppercase"
        );
        assert!(
            validate_locale("e_US.UTF-8").is_err(),
            "language is 2..=3 letters"
        );
        assert!(
            validate_locale("en_USA.UTF-8").is_err(),
            "territory is 2 letters"
        );
        assert!(validate_locale("en_US.UTF-8@").is_err(), "empty modifier");
        assert!(validate_locale(&format!("en_US.{}", "a".repeat(30))).is_err());
    }

    #[test]
    fn locale_accepts_real_locales() {
        for good in [
            "en_US.UTF-8",
            "C.UTF-8",
            "C",
            "POSIX",
            "pt_BR.UTF-8",
            "ca_ES@valencia",
            "de_DE.ISO-8859-1",
            "en",
        ] {
            assert!(validate_locale(good).is_ok(), "locale must accept {good:?}");
        }
    }

    #[test]
    fn keymap_rejects_corpus() {
        assert_all_rejected(validate_keymap, "keymap");
        assert!(validate_keymap("us/br").is_err());
        assert!(validate_keymap(&"a".repeat(65)).is_err());
    }

    #[test]
    fn keymap_accepts_real_keymaps() {
        for good in ["us", "br-abnt2", "dvorak", "uk.map", &"a".repeat(64)] {
            assert!(validate_keymap(good).is_ok(), "keymap must accept {good:?}");
        }
    }

    #[test]
    fn zoneinfo_existence_is_runtime_state_not_shape() {
        // Shape is what the tests own; existence depends on the host's tzdata,
        // hence the split into two calls.
        let bogus = "Nowhere/Nothing";
        assert!(
            validate_timezone(bogus).is_ok(),
            "bogus zone is still well-shaped"
        );
        if Path::new(ZONEINFO).is_dir() {
            assert!(check_timezone_exists(bogus).is_err());
        } else {
            assert!(check_timezone_exists(bogus).is_ok());
        }
    }
}
