//! Small helpers shared by the crate.

/// Validate and normalize the `--base` url path prefix:
/// - empty, `/` or `///` → "" (serve at the root)
/// - add a leading `/`, strip the trailing one (`app/` → `/app`)
/// - reject empty segments (`//`), `.`/`..` and any character that is either
///   unsafe in a URL or meaningful in an axum route pattern (`*`, `{}`, `:`, ...)
///
/// The result is interpolated into route patterns, so it must be validated
/// strictly.
///
/// # Examples
///
/// ```
/// use webadev::base_normalize;
///
/// // The root: empty or slashes only
/// assert_eq!(base_normalize("").unwrap(), "");
/// assert_eq!(base_normalize("/").unwrap(), "");
/// assert_eq!(base_normalize("///").unwrap(), "");
///
/// // A leading slash is added, the trailing one is stripped
/// assert_eq!(base_normalize("app").unwrap(), "/app");
/// assert_eq!(base_normalize(" /app/ ").unwrap(), "/app");
/// assert_eq!(base_normalize("/a/b").unwrap(), "/a/b");
///
/// // Rejected: empty segments, dot segments, url and route metacharacters
/// assert!(base_normalize("//app").is_err());
/// assert!(base_normalize("/app/../x").is_err());
/// assert!(base_normalize("/{*x}").is_err());
/// assert!(base_normalize("/a%20b").is_err());
/// ```
pub fn base_normalize(base: &str) -> Result<String, String> {
    let base = base.trim();
    if base.is_empty() || base.chars().all(|c| c == '/') {
        return Ok(String::new());
    }

    let core = base.strip_prefix('/').unwrap_or(base);
    let core = core.strip_suffix('/').unwrap_or(core);

    for segment in core.split('/') {
        if segment.is_empty() {
            return Err("empty path segment (`//`)".into());
        }
        if segment == "." || segment == ".." {
            return Err(format!("invalid path segment `{segment}`"));
        }
        if !segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~'))
        {
            return Err(format!("invalid character in `{segment}`"));
        }
    }

    Ok(format!("/{core}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_normalize_normalizes() {
        assert_eq!(base_normalize("").unwrap(), "");
        assert_eq!(base_normalize("/").unwrap(), "");
        assert_eq!(base_normalize("///").unwrap(), "");
        assert_eq!(base_normalize("app").unwrap(), "/app");
        assert_eq!(base_normalize("app/").unwrap(), "/app");
        assert_eq!(base_normalize("/app/").unwrap(), "/app");
        assert_eq!(base_normalize(" /app ").unwrap(), "/app");
        assert_eq!(base_normalize("/a/b").unwrap(), "/a/b");
    }

    #[test]
    fn invalid_base_is_rejected() {
        for base in [
            "//app",
            "app//sub",
            "/app/../x",
            "/app/./x",
            "/app?x=1",
            "/app#x",
            "/{*x}",
            "/a:b",
            "/a b",
            "/a%20b",
            "\\app",
            "https://example.com/app",
        ] {
            assert!(base_normalize(base).is_err(), "`{base}` must be rejected");
        }
    }
}
