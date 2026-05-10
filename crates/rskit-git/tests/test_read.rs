mod helpers;

use std::time::{Duration, SystemTime};

use rskit_git::{
    BlameOptions, Blamer, DescribeOptions, Differ, EntryKind, EntryState, FileStatus, GrepOptions,
    Inspector, LogOptions, LogReader, Repository, TreeReader, open,
};

#[test]
fn test_diff_added() {
    let repo = helpers::TestRepo::init();
    repo.create_tag("v1");
    repo.commit_file("new.txt", "hello", "add new file");
    let r = open(repo.path()).unwrap();

    let entries = r.diff("v1", "HEAD").unwrap();
    assert!(!entries.is_empty());
    assert!(
        entries
            .iter()
            .any(|e| e.path == "new.txt" && e.status == FileStatus::Added)
    );
}

#[test]
fn test_diff_modified() {
    let repo = helpers::TestRepo::init();
    repo.create_tag("v1");
    repo.commit_file("README.md", "updated", "update readme");
    let r = open(repo.path()).unwrap();

    let entries = r.diff("v1", "HEAD").unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.path == "README.md" && e.status == FileStatus::Modified)
    );
}

#[test]
fn test_diff_stats() {
    let repo = helpers::TestRepo::init();
    repo.create_tag("v1");
    repo.commit_file("a.txt", "line1\nline2\n", "add a");
    repo.commit_file("b.txt", "line1\n", "add b");
    let r = open(repo.path()).unwrap();

    let stats = r.diff_stats("v1", "HEAD").unwrap();
    assert!(stats.files_changed >= 2);
    assert!(stats.additions >= 3);
}

#[test]
fn test_status_clean() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();
    let entries = r.status().unwrap();
    assert!(entries.is_empty());
}

#[test]
fn test_status_dirty() {
    let repo = helpers::TestRepo::init();
    repo.make_untracked("untracked.txt");
    repo.make_dirty("README.md");
    let r = open(repo.path()).unwrap();

    let entries = r.status().unwrap();
    assert!(entries.len() >= 2);
    assert!(
        entries
            .iter()
            .any(|e| e.path == "untracked.txt" && e.state == EntryState::Untracked)
    );
}

#[test]
fn test_file_at() {
    let repo = helpers::TestRepo::init();
    repo.commit_file("hello.txt", "hello world", "add hello");
    let r = open(repo.path()).unwrap();

    let content = r.file_at("HEAD", "hello.txt").unwrap();
    assert_eq!(content, b"hello world");
}

#[test]
fn test_file_at_not_found() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();
    assert!(r.file_at("HEAD", "nonexistent.txt").is_err());
}

#[test]
fn test_list_entries() {
    let repo = helpers::TestRepo::init();
    repo.commit_file("a.txt", "a", "add a");
    repo.commit_file("sub/b.txt", "b", "add b");
    let r = open(repo.path()).unwrap();

    let entries = r.list_entries("HEAD", "").unwrap();
    assert!(entries.len() >= 2);
    assert!(entries.iter().any(|e| e.kind == EntryKind::Blob));
    assert!(entries.iter().any(|e| e.kind == EntryKind::Tree));
}

#[test]
fn test_list_entries_subdir() {
    let repo = helpers::TestRepo::init();
    repo.commit_file("sub/file.txt", "content", "add sub/file");
    let r = open(repo.path()).unwrap();

    let entries = r.list_entries("HEAD", "sub").unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "file.txt");
}

#[test]
fn test_tree_hash() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();
    let hash = r.tree_hash("HEAD", "").unwrap();
    assert!(!hash.is_zero());
}

#[test]
fn test_tree_hash_changes() {
    let repo = helpers::TestRepo::init();
    repo.create_tag("v1");
    let r = open(repo.path()).unwrap();
    let hash1 = r.tree_hash("v1", "").unwrap();

    repo.commit_file("new.txt", "content", "add file");
    let r = open(repo.path()).unwrap();
    let hash2 = r.tree_hash("HEAD", "").unwrap();

    assert_ne!(hash1, hash2);
}

#[test]
fn test_log_with_options() {
    let repo = helpers::TestRepo::init();
    repo.commit_file(
        "foo.txt", "one
", "add foo",
    );
    repo.commit_file(
        "bar.txt", "bar
", "add bar",
    );
    repo.commit_file(
        "foo.txt",
        "one
two
",
        "update foo",
    );
    let r = open(repo.path()).unwrap();

    let commits = r
        .log(Some(&LogOptions {
            max_count: Some(2),
            ..Default::default()
        }))
        .unwrap();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].message, "update foo");
    assert_eq!(commits[1].message, "add bar");

    let foo_commits = r
        .log(Some(&LogOptions {
            path_filter: Some("foo.txt".to_string()),
            ..Default::default()
        }))
        .unwrap();
    assert_eq!(foo_commits.len(), 2);
    assert!(
        foo_commits
            .iter()
            .all(|commit| commit.author.name == "Test User")
    );

    let none_by_author = r
        .log(Some(&LogOptions {
            author_filter: Some("nobody@example.com".to_string()),
            ..Default::default()
        }))
        .unwrap();
    assert!(none_by_author.is_empty());

    let none_since = r
        .log(Some(&LogOptions {
            since: Some(SystemTime::now() + Duration::from_secs(60)),
            ..Default::default()
        }))
        .unwrap();
    assert!(none_since.is_empty());

    let none_until = r
        .log(Some(&LogOptions {
            until: Some(SystemTime::UNIX_EPOCH),
            ..Default::default()
        }))
        .unwrap();
    assert!(none_until.is_empty());
}

#[test]
fn test_merge_base_and_is_ancestor() {
    let repo = helpers::TestRepo::init();
    repo.commit_file(
        "base.txt", "base
", "base",
    );
    let base = repo.rev_parse("HEAD");
    repo.create_branch("feature");
    let main_branch = repo.current_branch();

    repo.checkout("feature");
    repo.commit_file(
        "feature.txt",
        "feature
",
        "feature work",
    );
    let feature_head = repo.rev_parse("HEAD");

    repo.checkout(&main_branch);
    repo.commit_file(
        "main.txt",
        "main
",
        "main work",
    );
    let r = open(repo.path()).unwrap();

    let merge_base = r.merge_base("HEAD", &feature_head).unwrap();
    assert_eq!(merge_base.to_string(), base);
    assert!(r.is_ancestor(&base, "HEAD").unwrap());
    assert!(r.is_ancestor(&base, &feature_head).unwrap());
    assert!(!r.is_ancestor("HEAD", &feature_head).unwrap());
    assert!(!r.head().unwrap().target.is_zero());
}

#[test]
fn test_blame_with_multiple_commits() {
    let repo = helpers::TestRepo::init();
    repo.commit_file("notes.txt", "line 1\nline 2\nline 3\n", "add notes");
    let add_notes = repo.rev_parse("HEAD");
    repo.commit_file(
        "notes.txt",
        "line 1\nline 2 updated\nline 3\n",
        "update line 2",
    );
    let update_line_2 = repo.rev_parse("HEAD");
    repo.commit_file(
        "notes.txt",
        "line 1\nline 2 updated\nline 3 updated\n",
        "update line 3",
    );
    let update_line_3 = repo.rev_parse("HEAD");
    let r = open(repo.path()).unwrap();

    let lines = r.blame("HEAD", "notes.txt", None).unwrap();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].line, 1);
    assert_eq!(lines[0].content, "line 1");
    assert_eq!(lines[0].commit_oid.to_string(), add_notes);
    assert_eq!(lines[1].content, "line 2 updated");
    assert_eq!(lines[1].commit_oid.to_string(), update_line_2);
    assert_eq!(lines[2].content, "line 3 updated");
    assert_eq!(lines[2].commit_oid.to_string(), update_line_3);

    let subset = r
        .blame(
            "HEAD",
            "notes.txt",
            Some(&BlameOptions {
                start_line: Some(2),
                end_line: Some(3),
                ignore_whitespace: false,
                ..Default::default()
            }),
        )
        .unwrap();
    assert_eq!(subset.len(), 2);
    assert_eq!(subset[0].line, 2);
    assert_eq!(subset[1].line, 3);
}

#[test]
fn test_rev_parse_describe_grep_and_show() {
    let repo = helpers::TestRepo::init();
    repo.create_annotated_tag("v0.1.0", "annotated release");
    repo.commit_file("docs/notes.txt", "Hello World\nsecond line\n", "add notes");
    repo.create_tag("v0.2.0");
    let r = open(repo.path()).unwrap();

    let head = r.rev_parse("HEAD").unwrap();
    assert_eq!(head.to_string(), repo.rev_parse("HEAD"));

    let describe_default = r.describe(None).unwrap();
    assert_eq!(describe_default, "v0.2.0");

    let describe_annotated = r
        .describe(Some(&DescribeOptions {
            annotated_tags_only: true,
            ..Default::default()
        }))
        .unwrap();
    assert!(describe_annotated.starts_with("v0.1.0-1-g"));

    let describe_long = r
        .describe(Some(&DescribeOptions {
            long: true,
            ..Default::default()
        }))
        .unwrap();
    assert!(describe_long.contains("-0-g"));

    let matches = r
        .grep(
            "hello world",
            "HEAD",
            Some(&GrepOptions {
                ignore_case: true,
                line_numbers: true,
                pathspecs: vec!["docs".to_string()],
                ..Default::default()
            }),
        )
        .unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].path, "docs/notes.txt");
    assert_eq!(matches[0].line_number, Some(1));
    assert_eq!(matches[0].line, "Hello World");

    let shown = r.show("HEAD:docs/notes.txt").unwrap();
    assert_eq!(shown, b"Hello World\nsecond line\n");
}
