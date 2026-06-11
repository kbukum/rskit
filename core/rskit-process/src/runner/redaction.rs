use std::ffi::OsString;
use std::fmt;

use crate::ArgRedaction;

pub(super) struct RedactedArgs<'a> {
    args: &'a [OsString],
    redaction: &'a ArgRedaction,
}

impl<'a> RedactedArgs<'a> {
    pub(super) fn new(args: &'a [OsString], redaction: &'a ArgRedaction) -> Self {
        Self { args, redaction }
    }
}

impl fmt::Debug for RedactedArgs<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        let mut redact_next = false;
        for arg in self.args {
            let arg = arg.to_string_lossy();
            if redact_next {
                if let Some((key, _value)) = arg.split_once('=')
                    && self.redaction.is_sensitive_arg_name(key)
                {
                    let redacted = format!("{key}=<redacted>");
                    list.entry(&redacted);
                    redact_next = false;
                    continue;
                }

                if self.redaction.is_sensitive_arg_name(&arg) {
                    list.entry(&arg);
                    continue;
                }

                list.entry(&"<redacted>");
                redact_next = false;
                continue;
            }

            if let Some((key, _value)) = arg.split_once('=')
                && self.redaction.is_sensitive_arg_name(key)
            {
                let redacted = format!("{key}=<redacted>");
                list.entry(&redacted);
                continue;
            }

            if self.redaction.is_sensitive_arg_name(&arg) {
                list.entry(&arg);
                redact_next = true;
            } else {
                list.entry(&arg);
            }
        }
        list.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacted_args_masks_secret_values() {
        let args = vec![
            OsString::from("--token"),
            OsString::from("abc123"),
            OsString::from("--password=hunter2"),
            OsString::from("--auth-token=super-secret"),
            OsString::from("--author"),
            OsString::from("octocat"),
            OsString::from("--name"),
            OsString::from("visible"),
        ];

        let rendered = format!("{:?}", RedactedArgs::new(&args, &ArgRedaction::default()));

        assert!(rendered.contains("\"--token\""));
        assert!(rendered.contains("\"<redacted>\""));
        assert!(rendered.contains("\"--password=<redacted>\""));
        assert!(rendered.contains("\"--auth-token=<redacted>\""));
        assert!(rendered.contains("\"--author\""));
        assert!(rendered.contains("\"octocat\""));
        assert!(rendered.contains("\"visible\""));
        assert!(!rendered.contains("abc123"));
        assert!(!rendered.contains("hunter2"));
        assert!(!rendered.contains("super-secret"));
    }

    #[test]
    fn redacted_args_uses_custom_secret_names() {
        let args = vec![
            OsString::from("--license-key"),
            OsString::from("licensed"),
            OsString::from("--public-key"),
            OsString::from("visible"),
        ];
        let redaction = ArgRedaction::default().with_name("license-key");

        let rendered = format!("{:?}", RedactedArgs::new(&args, &redaction));

        assert!(rendered.contains("\"--license-key\""));
        assert!(rendered.contains("\"<redacted>\""));
        assert!(rendered.contains("\"--public-key\""));
        assert!(rendered.contains("\"visible\""));
        assert!(!rendered.contains("licensed"));
    }

    #[test]
    fn redacted_args_handles_adjacent_sensitive_flags() {
        let args = vec![
            OsString::from("--token"),
            OsString::from("--password"),
            OsString::from("hunter2"),
        ];

        let rendered = format!("{:?}", RedactedArgs::new(&args, &ArgRedaction::default()));

        assert!(rendered.contains("\"--token\""));
        assert!(rendered.contains("\"--password\""));
        assert!(rendered.contains("\"<redacted>\""));
        assert!(!rendered.contains("hunter2"));
    }

    #[test]
    fn redacted_args_preserves_sensitive_flag_when_next_arg_is_sensitive_assignment() {
        let args = vec![
            OsString::from("--token"),
            OsString::from("--password=hunter2"),
            OsString::from("visible"),
        ];

        let rendered = format!("{:?}", RedactedArgs::new(&args, &ArgRedaction::default()));

        assert!(rendered.contains("\"--token\""));
        assert!(rendered.contains("\"--password=<redacted>\""));
        assert!(rendered.contains("\"visible\""));
        assert!(!rendered.contains("hunter2"));
    }
}
