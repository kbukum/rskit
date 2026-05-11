//! Read operations for the CLI backend.

use rskit_errors::{AppError, AppResult};

use crate::options::{DescribeOptions, GrepOptions};
use crate::read::Inspector;
use crate::types::{GrepMatch, Oid};

use super::{Backend, parse_oid};

impl Inspector for Backend {
    fn describe(&self, opts: Option<&DescribeOptions>) -> AppResult<String> {
        let opts = opts.cloned().unwrap_or_default();
        let mut args = vec!["describe".to_string()];
        if !opts.annotated_tags_only {
            args.push("--tags".to_string());
        }
        if opts.long {
            args.push("--long".to_string());
        }
        args.extend(opts.extra_args);

        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let output = self.run(&refs)?;
        Ok(String::from_utf8_lossy(&output).trim().to_string())
    }

    fn rev_parse(&self, revision: &str) -> AppResult<Oid> {
        let output = self.run(&["rev-parse", revision])?;
        parse_oid(String::from_utf8_lossy(&output).trim())
    }

    fn grep(
        &self,
        pattern: &str,
        revision: &str,
        opts: Option<&GrepOptions>,
    ) -> AppResult<Vec<GrepMatch>> {
        let opts = opts.cloned().unwrap_or_default();
        let mut args = vec!["grep".to_string()];
        if opts.ignore_case {
            args.push("-i".to_string());
        }
        if opts.line_numbers {
            args.push("-n".to_string());
        }
        args.extend(opts.extra_args);
        args.push("-e".to_string());
        args.push(pattern.to_string());
        args.push(revision.to_string());
        if !opts.pathspecs.is_empty() {
            args.push("--".to_string());
            args.extend(opts.pathspecs);
        }

        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let output = self.run(&refs)?;
        let line_numbers = opts.line_numbers;
        String::from_utf8_lossy(&output)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| parse_grep_match(line, revision, line_numbers))
            .collect()
    }

    fn show(&self, object: &str) -> AppResult<Vec<u8>> {
        self.run(&["show", object])
    }
}

fn parse_grep_match(line: &str, revision: &str, line_numbers: bool) -> AppResult<GrepMatch> {
    // Strip optional revision prefix (e.g., "HEAD:" from "HEAD:path:1:content")
    let stripped = line.strip_prefix(&format!("{revision}:")).unwrap_or(line);

    let path;
    let line_number;
    let content;

    if line_numbers {
        let invalid_match = || AppError::invalid_format("grep match", "<path>:<line>:<content>");
        let mut parts = stripped.splitn(3, ':');
        path = parts.next().ok_or_else(invalid_match)?.to_string();
        line_number = Some(
            parts
                .next()
                .ok_or_else(invalid_match)?
                .parse::<usize>()
                .map_err(|_| invalid_match())?,
        );
        content = parts.next().ok_or_else(invalid_match)?.to_string();
    } else {
        // Without -n the format is <path>:<content>.
        // Split from left: path has no colons (relative), content may.
        let invalid_match = || AppError::invalid_format("grep match", "<path>:<content>");
        let mut parts = stripped.splitn(2, ':');
        path = parts.next().ok_or_else(invalid_match)?.to_string();
        content = parts.next().ok_or_else(invalid_match)?.to_string();
        line_number = None;
    }

    Ok(GrepMatch {
        path,
        line_number,
        line: content,
    })
}

#[cfg(test)]
mod tests {
    use rskit_errors::ErrorCode;

    use super::parse_grep_match;

    #[test]
    fn parse_grep_match_reports_invalid_format() {
        let err = parse_grep_match("README.md:not-a-line", "HEAD", true).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidFormat);
        assert_eq!(
            err.message,
            "invalid format for grep match: expected <path>:<line>:<content>"
        );
    }

    #[test]
    fn parse_grep_match_with_line_numbers_preserves_colons_in_content() {
        let matched =
            parse_grep_match("README.md:12:key: value: still content", "HEAD", true).unwrap();

        assert_eq!(matched.path, "README.md");
        assert_eq!(matched.line_number, Some(12));
        assert_eq!(matched.line, "key: value: still content");
    }
}
