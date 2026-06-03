//! Semantic version parsing and requirement helpers.

pub use ::semver::{Version, VersionReq};

/// Parses a semantic version string.
///
/// Returns `None` when `value` is not valid SemVer.
///
/// # Examples
///
/// ```
/// let version = rskit_version::semver::parse_version("1.2.3").unwrap();
/// assert_eq!(version.major, 1);
/// ```
#[must_use]
pub fn parse_version(value: &str) -> Option<Version> {
    Version::parse(value).ok()
}

/// Parses a semantic version requirement.
///
/// Returns `None` when `requirement` is not a valid SemVer requirement.
///
/// # Examples
///
/// ```
/// let requirement = rskit_version::semver::parse_requirement(">=1.2").unwrap();
/// let version = rskit_version::semver::parse_version("1.3.0").unwrap();
/// assert!(requirement.matches(&version));
/// ```
#[must_use]
pub fn parse_requirement(requirement: &str) -> Option<VersionReq> {
    VersionReq::parse(requirement).ok()
}

/// Returns whether a semantic version string satisfies a semantic version requirement.
///
/// Returns `None` when either input cannot be parsed.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     rskit_version::semver::matches_requirement("1.3.0", ">=1.2"),
///     Some(true)
/// );
/// ```
#[must_use]
pub fn matches_requirement(version: &str, requirement: &str) -> Option<bool> {
    let version = parse_version(version)?;
    let requirement = parse_requirement(requirement)?;
    Some(requirement.matches(&version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        let version = parse_version("1.2.3-alpha.1+build.5").expect("version should parse");
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 2);
        assert_eq!(version.patch, 3);
        assert_eq!(parse_version("1.2"), None);
    }

    #[test]
    fn parses_requirements() {
        let requirement = parse_requirement("^1.2").expect("requirement should parse");
        assert!(requirement.matches(&Version::parse("1.3.0").expect("version should parse")));
        assert_eq!(parse_requirement("not a requirement"), None);
    }

    #[test]
    fn checks_requirement_matches() {
        assert_eq!(matches_requirement("1.3.0", ">=1.2"), Some(true));
        assert_eq!(matches_requirement("1.1.0", ">=1.2"), Some(false));
        assert_eq!(matches_requirement("invalid", ">=1.2"), None);
        assert_eq!(matches_requirement("1.3.0", "invalid"), None);
    }
}
