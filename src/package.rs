/// A package identity derived from its `.dist-info` directory name.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Package {
    /// Normalized package name.
    pub name: String,
    /// Package version, if known.
    pub version: Option<String>,
}

/// Parse a dist-info directory name into a package identity.
///
/// uv names these `{name}-{version}.dist-info`, where the name is normalized
/// (PEP 503: `-` is replaced with `_`), so the first `-` separates the two.
impl From<&str> for Package {
    fn from(dist_info_name: &str) -> Self {
        match dist_info_name.split_once('-') {
            Some((name, version)) => Package {
                name: name.to_string(),
                version: Some(version.to_string()),
            },
            // No `-` at all — treat the whole name as the package name.
            None => Package {
                name: dist_info_name.to_string(),
                version: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dist_info_name() {
        let pkg = Package::from("anyio-4.0.0");
        assert_eq!(pkg.name, "anyio");
        assert_eq!(pkg.version.as_deref(), Some("4.0.0"));

        // No version separator — treat the whole name as the package name.
        let odd = Package::from("odd");
        assert_eq!(odd.name, "odd");
        assert_eq!(odd.version, None);
    }
}
