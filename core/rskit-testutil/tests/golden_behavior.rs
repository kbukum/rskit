use rskit_fs::TempDir;
use rskit_testutil::{BLESS_ENV, Golden, GoldenMode, GoldenOutcome, Match, Normalizer, Rule};

fn duration_and_root_rules(root: &str) -> Vec<Rule> {
    vec![
        Rule::literal(root, "<ROOT>"),
        Rule::pattern(r"\d+\.\d+s", "<DUR>").unwrap(),
        Rule::pattern(r"\b[0-9a-f]{40}\b", "<HASH>").unwrap(),
    ]
}

// --- Normalizer -------------------------------------------------------------

#[test]
fn normalizer_applies_rules_in_order() {
    let normalizer = Normalizer::new(duration_and_root_rules("/tmp/work/repo-1"));

    let raw = "built /tmp/work/repo-1/src in 1.23s at 0123456789abcdef0123456789abcdef01234567";
    assert_eq!(normalizer.apply(raw), "built <ROOT>/src in <DUR> at <HASH>");
}

#[test]
fn normalizer_earlier_rule_wins_over_later() {
    // The first rule rewrites the span; the second no longer sees it.
    let normalizer = Normalizer::new(vec![
        Rule::pattern(r"\d+\.\d+s", "<DUR>").unwrap(),
        Rule::pattern(r"1\.23s", "<NEVER>").unwrap(),
    ]);

    assert_eq!(normalizer.apply("took 1.23s"), "took <DUR>");
}

#[test]
fn rule_rejects_invalid_regex() {
    let err = Rule::pattern("[unclosed", "<X>").unwrap_err();
    assert!(err.to_string().contains("pattern"));
}

// --- Match tiers ------------------------------------------------------------

#[test]
fn exact_passes_on_identical_and_fails_on_any_difference() {
    Match::Exact.verify("a\nb\n", "a\nb\n").unwrap();

    let err = Match::Exact.verify("a\nb\n", "a\nc\n").unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("b"),
        "diff names the expected line: {message}"
    );
    assert!(
        message.contains("c"),
        "diff names the actual line: {message}"
    );
}

#[test]
fn normalized_passes_iff_equal_after_rules() {
    let matcher = Match::Normalized(Normalizer::new(duration_and_root_rules("/tmp/w")));

    matcher
        .verify("done <ROOT>/x in <DUR>\n", "done /tmp/w/x in 9.87s\n")
        .unwrap();

    let err = matcher
        .verify("done <ROOT>/x in <DUR>\n", "failed /tmp/w/x in 9.87s\n")
        .unwrap_err();
    assert!(err.to_string().contains("done"));
}

#[test]
fn line_set_accepts_reordered_middle_band() {
    let matcher = Match::LineSet {
        frame_prefix: 1,
        frame_suffix: 1,
    };

    matcher
        .verify("PLAN\nunit b\nunit a\nOK\n", "PLAN\nunit a\nunit b\nOK\n")
        .unwrap();
}

#[test]
fn line_set_rejects_missing_extra_and_changed_frame() {
    let matcher = Match::LineSet {
        frame_prefix: 1,
        frame_suffix: 1,
    };

    // Equal length, one line substituted: the multiset names both sides.
    let err = matcher
        .verify("PLAN\nunit a\nunit b\nOK\n", "PLAN\nunit a\nunit c\nOK\n")
        .unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("unit b"),
        "names the missing line: {message}"
    );
    assert!(
        message.contains("unit c"),
        "names the extra line: {message}"
    );

    // Differing line counts fail before any set comparison.
    let err = matcher
        .verify("PLAN\nunit a\nunit b\nOK\n", "PLAN\nunit a\nOK\n")
        .unwrap_err();
    assert!(err.to_string().contains("lines"), "reports the count drift");

    // Changed frame line is positional, not a set member.
    let err = matcher
        .verify("PLAN\nunit a\nOK\n", "APPLY\nunit a\nOK\n")
        .unwrap_err();
    assert!(err.to_string().contains("PLAN"));
}

#[test]
fn line_set_rejects_golden_smaller_than_its_frame() {
    let matcher = Match::LineSet {
        frame_prefix: 1,
        frame_suffix: 1,
    };

    let err = matcher.verify("PLAN\n", "PLAN\n").unwrap_err();
    assert!(err.to_string().contains("frame"), "names the frame: {err}");
}

#[test]
fn exact_reports_trailing_newline_only_difference() {
    let err = Match::Exact.verify("a\n", "a").unwrap_err();
    assert!(
        err.to_string().contains("trailing newline"),
        "a bare-newline drift must not render an empty diff: {err}"
    );
}

#[test]
fn subset_passes_on_superset_and_requires_order() {
    Match::Subset
        .verify(
            "compiled\ntests passed\n",
            "warming up\ncompiled ok\nnoise\nall tests passed\n",
        )
        .unwrap();

    // Missing required line.
    let err = Match::Subset
        .verify("compiled\nlinked\n", "compiled\n")
        .unwrap_err();
    assert!(err.to_string().contains("linked"));

    // Present but out of order.
    let err = Match::Subset
        .verify("linked\ncompiled\n", "compiled\nlinked ok\n")
        .unwrap_err();
    assert!(err.to_string().contains("compiled"));
}

// --- Golden file ------------------------------------------------------------

#[test]
fn golden_verifies_against_existing_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.write_file("case.stdout", b"hello\n").unwrap();

    let golden = Golden::new(&path, Match::Exact);
    let outcome = golden.run("hello\n", GoldenMode::Verify).unwrap();
    assert_eq!(outcome, GoldenOutcome::Verified);

    let err = golden.run("bye\n", GoldenMode::Verify).unwrap_err();
    assert!(err.to_string().contains("hello"));
}

#[test]
fn missing_golden_is_a_not_found_error() {
    let dir = TempDir::new().unwrap();
    let golden = Golden::new(dir.path().join("absent.stdout"), Match::Exact);

    let err = golden.run("anything\n", GoldenMode::Verify).unwrap_err();
    assert!(err.is_not_found(), "missing golden must be NotFound: {err}");
}

#[test]
fn bless_writes_normalized_actual_then_verify_passes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nested/case.stdout");
    let matcher = || Match::Normalized(Normalizer::new(duration_and_root_rules("/tmp/w")));

    let golden = Golden::new(&path, matcher());
    let outcome = golden
        .run("done /tmp/w/x in 1.23s\n", GoldenMode::Bless)
        .unwrap();
    assert_eq!(outcome, GoldenOutcome::Blessed);

    // The golden on disk holds the normalized text …
    assert_eq!(
        rskit_fs::sync_io::file::read_string(&path).unwrap(),
        "done <ROOT>/x in <DUR>\n"
    );
    // … and a fresh raw run now verifies against it.
    let outcome = Golden::new(&path, matcher())
        .run("done /tmp/w/x in 4.56s\n", GoldenMode::Verify)
        .unwrap();
    assert_eq!(outcome, GoldenOutcome::Verified);
}

#[test]
fn verify_reads_bless_mode_from_env() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("env.stdout");
    let golden = Golden::new(&path, Match::Exact);

    // SAFETY: this is the only test in this binary that mutates the process
    // environment, and it restores the prior value before asserting.
    let prior = std::env::var_os(BLESS_ENV);
    unsafe { std::env::set_var(BLESS_ENV, "1") };
    let outcome = golden.verify("payload\n");
    unsafe {
        match prior {
            Some(value) => std::env::set_var(BLESS_ENV, value),
            None => std::env::remove_var(BLESS_ENV),
        }
    }

    assert_eq!(outcome.unwrap(), GoldenOutcome::Blessed);
    assert_eq!(golden.verify("payload\n").unwrap(), GoldenOutcome::Verified);
}
