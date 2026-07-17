use super::*;

#[test]
fn version_info_has_version() {
    let info = get_version_info();
    assert!(!info.version.is_empty(), "version must not be empty");
}

#[test]
fn version_info_has_rust_version() {
    let info = get_version_info();
    assert!(
        info.rust_version.contains("rustc"),
        "rust_version should contain 'rustc', got: {}",
        info.rust_version
    );
}

#[test]
fn version_info_has_build_time() {
    let info = get_version_info();
    assert!(
        info.build_time.contains('T'),
        "build_time should be RFC 3339, got: {}",
        info.build_time
    );
}

#[test]
fn short_version_contains_version() {
    let sv = get_short_version();
    let info = get_version_info();
    assert!(
        sv.starts_with(&info.version),
        "short version should start with crate version: {sv}"
    );
}

#[test]
fn full_version_contains_built() {
    let fv = get_full_version();
    assert!(
        fv.contains("built"),
        "full version should contain 'built': {fv}"
    );
}

#[test]
fn is_release_reflects_version() {
    // Pre-release versions are still release builds; only dev/dirty builds are not.
    assert!(is_release());
}

#[test]
fn display_matches_full_version() {
    let info = get_version_info();
    assert_eq!(info.to_string(), get_full_version());
}

#[test]
fn package_version_parses_as_semver() {
    let version = package_semver().expect("workspace package version should be semver");
    assert_eq!(version.major, 0);
    assert!(get_version_info().semver().is_some());
}

#[test]
fn version_info_matches_semver_requirement() {
    let info = get_version_info();
    assert_eq!(info.matches_requirement(">=0.2.0-alpha.1"), Some(true));
    assert_eq!(info.matches_requirement(">=0.1.0"), Some(false));
    assert_eq!(info.matches_requirement("not a requirement"), None);
}
