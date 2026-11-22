//! File filter policy: parse, normalize, match.

use std::path::Path;

/// Parse one `--filter` value: trim, lowercase, and reject a leading dot or
/// a path separator.
///
/// A value matches a watched file when the file name equals it or ends with
/// `.<value>`, so `html` matches `index.html`, `tar.gz` matches `a.tar.gz`,
/// and `one-file.html` matches that file. Empty values are allowed:
/// [`crate::watch`] drops them, so an empty filter watches every file.
///
/// Used by [`normalize`].
fn parse(value: &str) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    if value.starts_with('.') {
        return Err(format!("must not start with a dot (got: `{value}`)"));
    }
    if value.contains('/') || value.contains('\\') {
        return Err(format!(
            "must not contain a path separator (got: `{value}`)"
        ));
    }
    Ok(value)
}

// Parse every item (splitting on commas), drop the empty ones.
// An empty result watches all files.
pub(crate) fn normalize(filter: &[&str]) -> Result<Vec<String>, String> {
    let mut normalized = Vec::with_capacity(filter.len());
    for item in filter {
        for part in item.split(',') {
            let part = parse(part)?;
            if !part.is_empty() {
                normalized.push(part);
            }
        }
    }
    Ok(normalized)
}

// An empty filter watches every file. An item matches when the file name
// equals it or ends with `.<item>`, case-insensitively.
pub(crate) fn matches(path: &Path, filter: &[String]) -> bool {
    if filter.is_empty() {
        return true;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    filter.iter().any(|item| {
        name == item.as_str()
            || name
                .strip_suffix(item)
                .is_some_and(|prefix| prefix.ends_with('.'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(items: &[&str]) -> Vec<String> {
        normalize(items).unwrap()
    }

    #[test]
    fn parse_trims_and_lowercases() {
        assert_eq!(parse(" HTML ").unwrap(), "html");
        assert_eq!(parse("Css").unwrap(), "css");
        assert_eq!(parse("My-File.HTML").unwrap(), "my-file.html");
    }

    #[test]
    fn parse_rejects_leading_dot() {
        for value in [".html", " .HTML", "..js"] {
            let err = parse(value).unwrap_err();
            assert!(err.contains("must not start with a dot"), "got: {err}");
        }
    }

    #[test]
    fn parse_rejects_path_separator() {
        for value in ["src/index.html", "a\\b.js", "/html"] {
            let err = parse(value).unwrap_err();
            assert!(
                err.contains("must not contain a path separator"),
                "got: {err}"
            );
        }
    }

    #[test]
    fn parse_allows_empty() {
        assert_eq!(parse("").unwrap(), "");
        assert_eq!(parse("   ").unwrap(), "");
    }

    #[test]
    fn normalize_drops_empty_items() {
        assert_eq!(filter(&["html", "", " ", "css"]), ["html", "css"]);
    }

    #[test]
    fn normalize_splits_embedded_commas() {
        assert_eq!(filter(&["html,css"]), ["html", "css"]);
    }

    #[test]
    fn normalize_empty_is_all() {
        assert!(normalize(&[]).unwrap().is_empty());
        assert!(normalize(&[""]).unwrap().is_empty());
        assert!(normalize(&[",", " "]).unwrap().is_empty());
    }

    #[test]
    fn normalize_rejects_a_dotted_item() {
        assert!(normalize(&["html", ".css"]).is_err());
    }

    #[test]
    fn matches_empty_filter_watches_all() {
        let filter: Vec<String> = vec![];
        assert!(matches(Path::new("a.txt"), &filter));
        assert!(matches(Path::new("a.toml"), &filter));
        assert!(matches(Path::new("noext"), &filter));
    }

    #[test]
    fn matches_suffix() {
        let filter = filter(&["html", "toml"]);
        assert!(matches(Path::new("a.html"), &filter));
        assert!(matches(Path::new("dir/a.toml"), &filter));
        assert!(!matches(Path::new("a.txt"), &filter));
    }

    #[test]
    fn matches_respects_dot_boundary() {
        let filter = filter(&["html"]);
        assert!(!matches(Path::new("foohtml"), &filter));
        assert!(!matches(Path::new("a.myhtml"), &filter));
    }

    #[test]
    fn matches_compound_suffix() {
        let filter = filter(&["tar.gz"]);
        assert!(matches(Path::new("a.tar.gz"), &filter));
        assert!(!matches(Path::new("a.gz"), &filter));
        assert!(!matches(Path::new("a.tar"), &filter));
    }

    #[test]
    fn matches_dotfile() {
        let filter = filter(&["env"]);
        assert!(matches(Path::new(".env"), &filter));
        assert!(!matches(Path::new(".envrc"), &filter));
    }

    #[test]
    fn matches_full_file_name() {
        let filter = filter(&["one-file.html"]);
        assert!(matches(Path::new("one-file.html"), &filter));
        assert!(matches(Path::new("dir/one-file.html"), &filter));
        assert!(!matches(Path::new("other.html"), &filter));
        assert!(!matches(Path::new("xone-file.html"), &filter));
    }

    #[test]
    fn matches_is_case_insensitive() {
        let filter = filter(&["html"]);
        assert!(matches(Path::new("a.HTML"), &filter));
        assert!(matches(Path::new("A.Html"), &filter));
    }
}
