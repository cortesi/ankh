#![warn(missing_docs)]

//! Shared namespace name validation and reserved-name policy composition.

/// Names that may not be used as a namespace in any Ankh consumer.
pub const SHARED_RESERVED_NAMES: &[&str] = &[
    "api",
    "admin",
    "healthz",
    "assets",
    "static",
    "dev",
    "login",
    "logout",
    "signup",
    "signin",
    "signout",
    "register",
    "verify-email",
    "forgot-password",
    "reset-password",
    "waitlist",
    "dashboard",
    "settings",
    "account",
    "profile",
    "console",
    "org",
    "orgs",
    "organization",
    "organizations",
    "team",
    "teams",
    "user",
    "users",
    "member",
    "members",
    "new",
    "create",
    "edit",
    "delete",
    "remove",
    "www",
    "mail",
    "ftp",
    "smtp",
    "imap",
    "pop",
    "support",
    "help",
    "info",
    "contact",
    "billing",
    "sales",
    "root",
    "system",
    "null",
    "undefined",
    "anonymous",
];

/// Product-specific extension to the shared namespace reservation policy.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NamePolicy {
    /// Product-specific names reserved in addition to the shared policy.
    product_reserved: Vec<String>,
}

impl NamePolicy {
    /// Create a name policy with no product-specific reserved names.
    #[must_use]
    pub fn shared() -> Self {
        Self {
            product_reserved: Vec::new(),
        }
    }

    /// Create a name policy with product-specific reserved names.
    #[must_use]
    pub fn with_product_reserved(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let product_reserved = names
            .into_iter()
            .map(|name| normalize_name(name.into().as_str()))
            .collect();
        Self { product_reserved }
    }

    /// Return whether a namespace name is reserved by either shared or product
    /// policy.
    #[must_use]
    pub fn is_reserved_namespace_name(&self, name: &str) -> bool {
        let normalized = normalize_name(name);
        SHARED_RESERVED_NAMES.contains(&normalized.as_str())
            || self
                .product_reserved
                .iter()
                .any(|reserved| reserved == &normalized)
    }

    /// Validate a namespace name with this policy.
    pub fn validate_namespace_name(&self, name: &str) -> Result<(), &'static str> {
        validate_name_format(name)?;
        if self.is_reserved_namespace_name(name) {
            return Err("this name is reserved");
        }
        Ok(())
    }

    /// Return product-specific reserved names.
    #[must_use]
    pub fn product_reserved(&self) -> &[String] {
        self.product_reserved.as_slice()
    }
}

impl Default for NamePolicy {
    fn default() -> Self {
        Self::shared()
    }
}

/// Normalizes a name for storage and comparison.
#[must_use]
pub fn normalize_name(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Validates the shared format rules for namespace-like names.
pub fn validate_name_format(name: &str) -> Result<(), &'static str> {
    if name.len() < 3 {
        return Err("must be at least 3 characters");
    }
    if name.len() > 39 {
        return Err("must be at most 39 characters");
    }

    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("must contain only lowercase letters, numbers, and hyphens");
    }

    let first = name.chars().next().expect("nonempty after length check");
    let last = name.chars().last().expect("nonempty after length check");
    if !first.is_ascii_alphanumeric() || !last.is_ascii_alphanumeric() {
        return Err("must start and end with a letter or number");
    }

    if name.contains("--") {
        return Err("cannot contain consecutive hyphens");
    }

    Ok(())
}

/// Validates a namespace name with the shared reserved-word policy.
///
/// Module-level convenience for the default (no product extension) policy; use
/// [`NamePolicy::validate_namespace_name`] when a product adds reserved names.
pub fn validate_namespace_name(name: &str) -> Result<(), &'static str> {
    NamePolicy::shared().validate_namespace_name(name)
}

/// Return whether a name is reserved by the shared namespace policy.
///
/// Module-level convenience for the default (no product extension) policy; use
/// [`NamePolicy::is_reserved_namespace_name`] when a product adds reserved
/// names.
#[must_use]
pub fn is_reserved_namespace_name(name: &str) -> bool {
    NamePolicy::shared().is_reserved_namespace_name(name)
}

#[cfg(test)]
mod tests {
    //! Tests for shared name validation and policy composition.

    use std::{fs, path::PathBuf};

    use super::{
        NamePolicy, SHARED_RESERVED_NAMES, is_reserved_namespace_name, normalize_name,
        validate_name_format, validate_namespace_name,
    };

    /// Expected error for short names.
    const TOO_SHORT: &str = "must be at least 3 characters";
    /// Expected error for long names.
    const TOO_LONG: &str = "must be at most 39 characters";
    /// Expected error for invalid characters.
    const BAD_CHARS: &str = "must contain only lowercase letters, numbers, and hyphens";
    /// Expected error for invalid edge characters.
    const BAD_EDGE: &str = "must start and end with a letter or number";
    /// Expected error for consecutive hyphens.
    const DOUBLE_HYPHEN: &str = "cannot contain consecutive hyphens";
    /// Expected error for reserved words.
    const RESERVED: &str = "this name is reserved";
    /// Name at the maximum accepted length.
    const MAX_LEN: &str = "a23456789012345678901234567890123456789";
    /// Name above the maximum accepted length.
    const OVER_LEN: &str = "a234567890123456789012345678901234567890";

    /// Proves names are trimmed and lowercased.
    #[test]
    fn normalize_trims_and_lowercases() {
        assert_eq!(normalize_name("  Hello  "), "hello");
        assert_eq!(normalize_name("FooBar"), "foobar");
        assert_eq!(normalize_name("foo-bar"), "foo-bar");
    }

    /// Proves valid shared name formats are accepted.
    #[test]
    fn name_format_accepts_valid() {
        for ok in [
            "abc",
            "foo-bar",
            "foo-bar-baz",
            "a1b2c3",
            "123",
            "a-1",
            "1-a",
            MAX_LEN,
        ] {
            assert!(validate_name_format(ok).is_ok(), "expected ok: {ok}");
        }
    }

    /// Proves invalid shared name formats are rejected with stable messages.
    #[test]
    fn name_format_rejects_invalid() {
        let cases: &[(&str, &str)] = &[
            ("", TOO_SHORT),
            ("a", TOO_SHORT),
            ("ab", TOO_SHORT),
            (OVER_LEN, TOO_LONG),
            ("Foo", BAD_CHARS),
            ("foo_bar", BAD_CHARS),
            ("foo.bar", BAD_CHARS),
            ("foo bar", BAD_CHARS),
            ("foo@bar", BAD_CHARS),
            ("-foo", BAD_EDGE),
            ("foo-", BAD_EDGE),
            ("-foo-", BAD_EDGE),
            ("foo--bar", DOUBLE_HYPHEN),
            ("a--b", DOUBLE_HYPHEN),
        ];

        for (input, expected) in cases {
            assert_eq!(
                validate_name_format(input),
                Err(*expected),
                "input: {input}"
            );
        }
    }

    /// Proves the shared policy rejects common route and system terms.
    #[test]
    fn namespace_rejects_shared_reserved_words() {
        for reserved in ["api", "admin", "login", "dashboard", "org", "user", "new"] {
            assert!(is_reserved_namespace_name(reserved), "reserved: {reserved}");
            assert_eq!(validate_namespace_name(reserved), Err(RESERVED));
        }
    }

    /// Proves product-specific reserved words compose with the shared policy.
    #[test]
    fn namespace_policy_composes_product_reserved_words() {
        let policy = NamePolicy::with_product_reserved(["restless", "soundscape"]);

        assert!(policy.is_reserved_namespace_name("restless"));
        assert!(policy.is_reserved_namespace_name("api"));
        assert_eq!(policy.validate_namespace_name("soundscape"), Err(RESERVED));
        assert!(policy.validate_namespace_name("example").is_ok());
    }

    /// Path of the TypeScript copy of the shared names, from this crate.
    const TYPESCRIPT_NAMES_PATH: &str = "../../frontend/packages/auth-react/src/generated-names.ts";

    /// Render the TypeScript module of the shared reserved names, which
    /// `@ankh/auth-react` validates against.
    fn typescript_names_module() -> String {
        let names: String = SHARED_RESERVED_NAMES
            .iter()
            .map(|name| format!("  \"{name}\",\n"))
            .collect();
        format!(
            "// Generated from ankh-names by its export_typescript_names test. Do not edit by \
             hand.\n\n\
             /** Namespace names that every Ankh product reserves. */\n\
             export const SHARED_RESERVED_NAMES: readonly string[] = [\n{names}];\n"
        )
    }

    fn typescript_names_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(TYPESCRIPT_NAMES_PATH)
    }

    #[test]
    #[ignore = "writes frontend/packages/auth-react/src/generated-names.ts"]
    fn export_typescript_names() {
        fs::write(typescript_names_path(), typescript_names_module())
            .expect("write generated-names.ts");
    }

    #[test]
    fn generated_typescript_names_are_current() {
        let current = fs::read_to_string(typescript_names_path()).expect("read generated-names.ts");
        assert!(
            current == typescript_names_module(),
            "frontend/packages/auth-react/src/generated-names.ts is stale; regenerate it with \
             `cargo test -p ankh-names -- --ignored tests::export_typescript_names --exact`"
        );
    }
}
