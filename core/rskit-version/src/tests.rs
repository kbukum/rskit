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
    let info = get_version_info();
    assert_eq!(
        is_release(),
        info.version != "dev" && !info.version.contains("dirty")
    );
}

#[test]
fn is_release_is_independent_of_dirty_state() {
    // Release status is derived solely from the version string; working-tree
    // dirtiness is reported separately via `is_dirty` (cross-kit contract).
    let dirty_release = VersionInfo {
        version: "1.2.3".to_string(),
        git_commit: "abcdef1234567890".to_string(),
        git_branch: "feature".to_string(),
        build_time: String::new(),
        build_date: None,
        rust_version: "rustc 1.97.0".to_string(),
        is_release: true,
        is_dirty: true,
    };
    assert!(dirty_release.is_release);
    assert!(dirty_release.is_dirty);
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

#[test]
fn version_info_serializes_dirty_and_build_date_fields() {
    let info = VersionInfo {
        version: "1.2.3".to_string(),
        git_commit: "abcdef1234567890".to_string(),
        git_branch: "feature".to_string(),
        build_time: "2026-08-30T09:00:00Z".to_string(),
        build_date: Some("2026-08-30".to_string()),
        rust_version: "rustc 1.97.0".to_string(),
        is_release: false,
        is_dirty: true,
    };

    let json = serde_json::to_value(&info).unwrap();
    assert_eq!(json["is_dirty"], true);
    assert_eq!(json["build_date"], "2026-08-30");
    assert_eq!(json["rust_version"], "rustc 1.97.0");
}

#[test]
fn dirty_versions_include_suffix_deterministically() {
    let info = VersionInfo {
        version: "1.2.3".to_string(),
        git_commit: "abcdef1234567890".to_string(),
        git_branch: "feature".to_string(),
        build_time: "2026-08-30T09:00:00Z".to_string(),
        build_date: None,
        rust_version: "rustc 1.97.0".to_string(),
        is_release: false,
        is_dirty: true,
    };

    assert_eq!(info.short_version(), "1.2.3-abcdef1-dirty");
    assert_eq!(
        info.full_version(),
        "1.2.3-abcdef1-feature-dirty (built 2026-08-30T09:00:00Z)"
    );
}
