//! Tests for process specification, I/O modes, and execution policy.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

#[cfg(unix)]
use crate::pty::PtyIo;
use crate::runner::OutputObserver;

use super::*;

#[test]
fn process_spec_builders_set_command_environment_and_directory() {
    let spec = command("tool")
        .arg("--one")
        .args([OsString::from("two"), OsString::from("three")])
        .dir("work")
        .env("TOKEN", "secret")
        .envs([("MODE", "test"), ("REGION", "local")])
        .env_policy(EnvPolicy::Inherit)
        .empty_env();

    assert_eq!(spec.program, PathBuf::from("tool"));
    assert_eq!(
        spec.args,
        [
            OsString::from("--one"),
            OsString::from("two"),
            OsString::from("three")
        ]
    );
    assert_eq!(spec.dir, Some(PathBuf::from("work")));
    assert_eq!(spec.env.get("TOKEN").map(String::as_str), Some("secret"));
    assert_eq!(spec.env.get("MODE").map(String::as_str), Some("test"));
    assert_eq!(spec.env.get("REGION").map(String::as_str), Some("local"));
    assert_eq!(spec.env_policy, EnvPolicy::Empty);
}

#[test]
fn io_policy_builders_update_nested_fields() {
    let bounded = OutputPolicy::captured().with_max_output_bytes(128);
    assert!(bounded.capture_stdout);
    assert!(bounded.capture_stderr);
    assert_eq!(bounded.max_output_bytes, Some(128));
    assert_eq!(
        OutputPolicy::observe_only()
            .with_unbounded_output()
            .max_output_bytes,
        None
    );

    let captured = CapturedIo::new()
        .with_input(InputPolicy::Bytes(b"stdin".to_vec()))
        .with_output(OutputPolicy::observe_only());
    assert_eq!(captured.input, InputPolicy::Bytes(b"stdin".to_vec()));
    assert!(!captured.output.capture_stdout);

    let observed = ObservedIo::new(OutputObserver::new())
        .with_input(InputPolicy::Closed)
        .with_output(OutputPolicy::captured().with_max_output_bytes(7));
    assert_eq!(observed.input, InputPolicy::Closed);
    assert_eq!(observed.output.max_output_bytes, Some(7));

    let inherited = InheritedIo::new().with_input(InputPolicy::Closed);
    assert_eq!(inherited.input, InputPolicy::Closed);
    assert!(matches!(
        ProcessIo::captured(captured),
        ProcessIo::Captured(_)
    ));
    assert!(matches!(
        ProcessIo::observed(observed),
        ProcessIo::Observed(_)
    ));
    assert!(matches!(
        ProcessIo::inherited(inherited),
        ProcessIo::Inherited(_)
    ));
}

#[test]
fn redaction_signal_and_config_builders_are_chainable() {
    let redaction = ArgRedaction::new()
        .with_name("token")
        .with_names(["password", "client-secret"]);
    assert!(redaction.is_sensitive_arg_name("--token"));
    assert!(redaction.is_sensitive_arg_name("password"));
    assert!(redaction.is_sensitive_arg_name("client-secret"));

    let replacement = ArgRedaction::from_names(["api-key"]);
    assert!(replacement.is_sensitive_arg_name("api-key"));
    assert!(!replacement.is_sensitive_arg_name("token"));

    let signal = SignalPolicy::default()
        .with_grace_period(Duration::from_millis(25))
        .with_create_process_group(false)
        .with_terminate_descendants(false);
    assert_eq!(signal.grace_period, Duration::from_millis(25));
    assert!(!signal.create_process_group);
    assert!(!signal.terminate_descendants);

    let captured = ProcessConfig::default()
        .with_timeout(None)
        .with_arg_redaction(redaction)
        .with_sensitive_arg_name("session")
        .with_signal_policy(signal)
        .with_max_output_bytes(3)
        .with_unbounded_output()
        .with_input(InputPolicy::Bytes(vec![1, 2, 3]));
    assert_eq!(captured.timeout, None);
    assert!(captured.arg_redaction.is_sensitive_arg_name("session"));
    assert_eq!(captured.signal, signal);
    match captured.io {
        ProcessIo::Captured(io) => {
            assert_eq!(io.input, InputPolicy::Bytes(vec![1, 2, 3]));
            assert_eq!(io.output.max_output_bytes, None);
        }
        ProcessIo::Observed(_) | ProcessIo::Inherited(_) => {
            panic!("default process config should use captured I/O")
        }
        #[cfg(unix)]
        ProcessIo::Pty(_) => panic!("default process config should use captured I/O"),
    }
}

#[test]
fn config_builders_update_observed_and_inherited_io_modes() {
    let observed = ProcessConfig::default()
        .with_io(ProcessIo::observed(ObservedIo::default()))
        .with_max_output_bytes(11)
        .with_unbounded_output()
        .with_input(InputPolicy::Bytes(b"observed".to_vec()));
    match observed.io {
        ProcessIo::Observed(io) => {
            assert_eq!(io.input, InputPolicy::Bytes(b"observed".to_vec()));
            assert_eq!(io.output.max_output_bytes, None);
        }
        ProcessIo::Captured(_) | ProcessIo::Inherited(_) => {
            panic!("expected observed I/O")
        }
        #[cfg(unix)]
        ProcessIo::Pty(_) => panic!("expected observed I/O"),
    }

    let inherited = ProcessConfig::default()
        .with_io(ProcessIo::inherited(InheritedIo::default()))
        .with_max_output_bytes(5)
        .with_unbounded_output()
        .with_input(InputPolicy::Closed);
    match inherited.io {
        ProcessIo::Inherited(io) => assert_eq!(io.input, InputPolicy::Closed),
        ProcessIo::Captured(_) | ProcessIo::Observed(_) => {
            panic!("expected inherited I/O")
        }
        #[cfg(unix)]
        ProcessIo::Pty(_) => panic!("expected inherited I/O"),
    }
}

#[cfg(unix)]
#[test]
fn config_builders_update_pty_io_mode() {
    let config = ProcessConfig::default()
        .with_io(ProcessIo::pty(PtyIo::default()))
        .with_max_output_bytes(9)
        .with_unbounded_output()
        .with_input(InputPolicy::Bytes(b"pty".to_vec()));

    match config.io {
        ProcessIo::Pty(io) => {
            assert_eq!(io.input, InputPolicy::Bytes(b"pty".to_vec()));
            assert_eq!(io.output.max_output_bytes, None);
        }
        ProcessIo::Captured(_) | ProcessIo::Observed(_) | ProcessIo::Inherited(_) => {
            panic!("expected pty I/O")
        }
    }
}
