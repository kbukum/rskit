# rskit-process

Process and subprocess execution with explicit I/O modes, timeout handling,
and process-tree termination.

## Mode selection

| Mode | Intended use | Guarantees | Non-guarantees |
| --- | --- | --- | --- |
| `ProcessIo::Captured` | Deterministic non-interactive execution | Captures stdout/stderr separately with bounded retention, timeout, cancellation, and predefined stdin | No terminal behavior and no exact cross-stream ordering |
| `ProcessIo::Observed` | Live output observation with optional capture | Raw-byte and line callbacks for stdout/stderr; optional bounded capture | Not a TTY and no exact cross-stream ordering |
| `ProcessIo::Inherited` | Normal terminal commands | Child inherits parent stdout/stderr and, by default, stdin; process-group isolation is disabled so terminal job-control behavior follows OS defaults | No structured output capture or descendant termination |

PTY-backed terminal fidelity and live parent-stdin forwarding are intentionally not exposed until those modes are implemented with documented platform guarantees.

Line observers split deterministically on `\n`, `\r`, and `\r\n`.
Invalid UTF-8 is passed to line observers lossily; use raw-byte observers for binary output.

## Capturing output

```rust
use rskit_process::{ProcessConfig, ProcessSpec, run_with_cancel};
use tokio_util::sync::CancellationToken;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let spec = ProcessSpec::new("echo").arg("hello");
let config = ProcessConfig::default().with_max_output_bytes(1024 * 1024);

let result = run_with_cancel(&spec, &config, CancellationToken::new()).await?;
result.check()?;
println!("{}", result.stdout);
# Ok(())
# }
```

## Observing output

```rust
use rskit_process::{
    ObservedIo, OutputObserver, OutputPolicy, ProcessConfig, ProcessIo, ProcessSpec,
    run_with_cancel,
};
use tokio_util::sync::CancellationToken;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let spec = ProcessSpec::new("printf").arg("hello\n");
let observer = OutputObserver::new().with_stdout_line(|line| {
    eprintln!("child stdout: {line}");
});
let config = ProcessConfig::default().with_io(ProcessIo::observed(
    ObservedIo::new(observer).with_output(OutputPolicy::observe_only()),
));

let result = run_with_cancel(&spec, &config, CancellationToken::new()).await?;
result.check()?;
# Ok(())
# }
```

## Predefined stdin

```rust
use rskit_process::{InputPolicy, ProcessConfig, ProcessSpec, run_with_cancel};
use tokio_util::sync::CancellationToken;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let spec = ProcessSpec::new("cat");
let config = ProcessConfig::default().with_input(InputPolicy::Bytes(b"hello".to_vec()));

let result = run_with_cancel(&spec, &config, CancellationToken::new()).await?;
assert_eq!(result.stdout, "hello");
# Ok(())
# }
```

## Inherited terminal stdio

```rust
use rskit_process::{InheritedIo, ProcessConfig, ProcessIo, ProcessSpec, run_with_cancel};
use tokio_util::sync::CancellationToken;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let spec = ProcessSpec::new("printf").args(["%s\n", "terminal output"]);
let config = ProcessConfig::default().with_io(ProcessIo::inherited(InheritedIo::new()));

let result = run_with_cancel(&spec, &config, CancellationToken::new()).await?;
result.check()?;
assert!(result.stdout.is_empty());
# Ok(())
# }
```

## Timeout and process groups

By default, rskit-process creates an isolated process group where the platform supports it. On timeout or cancellation, it sends a graceful termination signal, waits for `LifecyclePolicy::grace_period`, then escalates to kill. `Inherited` mode is the exception: it does not create a new process group, because terminal-native commands should remain in the parent's foreground terminal context unless a future terminal-control mode provides stronger guarantees. Isolated children are also registered with a `ProcessSupervisor`; normal completion unregisters them on reap, supervisor shutdown fans out termination to every tracked group, and dropping a supervisor or armed child scope best-effort kills anything still live. Cleanup relies on Rust-side drops and explicit supervision: no Unix platform can prevent children from surviving an uncatchable hard kill of the parent (which runs no destructors), so callers still need an external process manager for that residual case.

Separate stdout and stderr pipes are read concurrently, so each stream is ordered internally, but exact ordering across streams is not guaranteed.

Process start logs redact secret-looking argument values, but argv is still visible to operating-system process inspection on many platforms. Prefer stdin, files with restricted permissions, or environment-specific secret mechanisms for sensitive values instead of command-line arguments.

Custom secret-bearing argument names can be added to the spawn-log redaction policy:

```rust
use rskit_process::{ArgRedaction, ProcessConfig};

let config = ProcessConfig::default()
    .with_arg_redaction(ArgRedaction::default().with_name("license-key"));
```

## Supervised child lifetime

`run` and `run_with_cancel` spawn through a throwaway per-call supervisor. To reap children on a process-wide shutdown — even while a blocking call is still waiting on one — share a `ProcessSupervisor` and spawn through `run_supervised` / `run_with_cancel_supervised`. A single `ProcessSupervisor::shutdown` then fans out termination to every tracked group.

```rust
use rskit_process::{ProcessConfig, ProcessSpec, ProcessSupervisor, run_supervised};

# fn example() -> Result<(), Box<dyn std::error::Error>> {
let supervisor = ProcessSupervisor::new(ProcessConfig::default().lifecycle);
let spec = ProcessSpec::new("echo").arg("hello");

let result = run_supervised(&supervisor, &spec, &ProcessConfig::default())?;
result.check()?;
# Ok(())
# }
```

Each child is tracked through an owned, reuse-proof identity (a Linux pidfd where available, otherwise the pid plus process-group id), so a delayed escalation always targets the original process and never a pid the OS recycled. Normal completion unregisters through a guard; the supervisor's own `Drop` is a backstop that force-kills anything still registered. Full guarantees hold on Unix; on other platforms `shutdown` is best-effort and returns an error for a registered child it can neither signal nor own rather than reporting a false success.
