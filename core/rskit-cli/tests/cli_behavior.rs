use std::time::Duration;

use rskit_cli::{
    CancellationToken, ErrorRenderer, ExitCode, MultiProgress, OutputFormat, OutputKV, OutputTable,
    ProgressBar, ProgressStyle,
};
use rskit_errors::{AppError, ErrorCode};

#[test]
fn exit_code_mapping_covers_public_error_categories() {
    let cases = [
        (ErrorCode::InvalidInput, ExitCode::Usage),
        (ErrorCode::InvalidFormat, ExitCode::Usage),
        (ErrorCode::MissingField, ExitCode::Usage),
        (ErrorCode::Unauthorized, ExitCode::Permission),
        (ErrorCode::Forbidden, ExitCode::Permission),
        (ErrorCode::TokenExpired, ExitCode::Permission),
        (ErrorCode::InvalidToken, ExitCode::Permission),
        (ErrorCode::NotFound, ExitCode::NotFound),
        (ErrorCode::Conflict, ExitCode::Conflict),
        (ErrorCode::AlreadyExists, ExitCode::Conflict),
        (ErrorCode::ServiceUnavailable, ExitCode::Unavailable),
        (ErrorCode::ConnectionFailed, ExitCode::Unavailable),
        (ErrorCode::ExternalService, ExitCode::Unavailable),
        (ErrorCode::RateLimited, ExitCode::RateLimited),
        (ErrorCode::Timeout, ExitCode::Timeout),
        (ErrorCode::Cancelled, ExitCode::Cancelled),
        (ErrorCode::Internal, ExitCode::Failure),
    ];

    for (error_code, exit_code) in cases {
        assert_eq!(ExitCode::from(error_code), exit_code);
        assert_eq!(exit_code.as_i32(), exit_code as i32);
    }
}

#[test]
fn error_renderer_outputs_text_json_and_yaml_with_details() {
    let error = AppError::new(ErrorCode::RateLimited, "too many requests")
        .with_detail("bucket", "search")
        .retryable(true);

    let (text, text_code) = ErrorRenderer::default().render(&error);
    assert_eq!(text_code, ExitCode::RateLimited);
    assert_eq!(text, "error[RATE_LIMITED]: too many requests");

    let (json, json_code) = ErrorRenderer::new(OutputFormat::Json).render(&error);
    assert_eq!(json_code, ExitCode::RateLimited);
    let payload: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(payload["code"], "RATE_LIMITED");
    assert_eq!(payload["message"], "too many requests");
    assert_eq!(payload["retryable"], true);
    assert_eq!(payload["exit_code"], 75);
    assert_eq!(payload["details"]["bucket"], "search");

    let (yaml, yaml_code) = ErrorRenderer::new(OutputFormat::Yaml).render(&error);
    assert_eq!(yaml_code, ExitCode::RateLimited);
    assert!(yaml.contains("code: RATE_LIMITED"));
    assert!(yaml.contains("bucket: search"));
}

#[test]
fn table_and_key_value_outputs_handle_empty_and_wide_rows() {
    let empty = OutputTable::new(vec!["Name", "Status"])
        .with_title("Release readiness")
        .to_string();
    assert!(empty.contains("Release readiness"));
    assert!(empty.contains("Name"));
    assert!(empty.contains("Status"));

    let mut table = OutputTable::new(vec!["Name"]);
    table.add_row(vec!["short", "extra cell ignored by width calculation"]);
    let output = table.to_string();
    assert!(output.contains("short"));
    assert!(output.contains("extra cell ignored by width calculation"));

    assert_eq!(OutputKV::default().to_string(), "");
    let mut kv = OutputKV::new();
    kv.add("Short", "yes").add("MuchLongerKey", "aligned");
    let kv_output = kv.to_string();
    assert!(kv_output.contains("        Short:  yes"));
    assert!(kv_output.contains("MuchLongerKey:  aligned"));
}

#[test]
fn progress_bar_wrapper_updates_position_length_message_and_finish_state() {
    let bar = ProgressBar::new(10, ProgressStyle::Bar);
    assert_eq!(bar.inner().length(), Some(10));

    bar.set_prefix("download");
    bar.set_message("starting");
    bar.inc(3);
    bar.tick();
    assert_eq!(bar.inner().position(), 3);
    assert_eq!(bar.inner().prefix(), "download");
    assert_eq!(bar.inner().message(), "starting");

    bar.set_style(ProgressStyle::Download);
    bar.enable_steady_tick(Duration::from_millis(1));
    bar.set_length(20);
    bar.set_position(12);
    assert_eq!(bar.inner().length(), Some(20));
    assert_eq!(bar.inner().position(), 12);

    bar.finish_with_message("done");
    assert!(bar.inner().is_finished());
    assert_eq!(bar.inner().message(), "done");

    bar.reset();
    assert_eq!(bar.inner().position(), 0);
    bar.finish_and_clear();
}

#[test]
fn spinner_and_multi_progress_cover_ordering_static_lines_and_removal() {
    let spinner = ProgressBar::spinner();
    spinner.set_prefix("spin");
    spinner.set_style(ProgressStyle::Spinner);
    spinner.finish();
    assert!(spinner.inner().is_finished());

    let multi = MultiProgress::new();
    multi.println("starting").unwrap();
    let first = multi.add_bar("first", 2);
    let second = multi.insert_bar_after(&first, "second", 3);
    let spinner = multi.add_spinner("waiting");
    let static_line = multi.add_static_line("ok", "completed");

    first.inc(1);
    second.set_position(2);
    spinner.tick();

    assert_eq!(first.inner().position(), 1);
    assert_eq!(second.inner().position(), 2);
    assert!(static_line.inner().is_finished());
    assert_eq!(static_line.inner().message(), "completed");

    multi.remove(&spinner);
    multi.clear().unwrap();

    let raw = rskit_cli::progress::RawMultiProgress::new();
    let wrapped = MultiProgress::from_raw(raw);
    wrapped.clear().unwrap();
    let _ = wrapped.raw();
}

#[tokio::test]
async fn cancellation_token_default_clone_and_waiter_observe_cancel() {
    let token = CancellationToken::default();
    let waiter = token.clone();
    let task = tokio::spawn(async move {
        waiter.cancelled().await;
        waiter.is_cancelled()
    });

    assert!(!token.is_cancelled());
    token.cancel();
    assert!(task.await.unwrap());
}
