# rskit-process

Process and subprocess execution with timeout and signal handling.

This crate provides functionality to execute external processes with:

- **Timeout support** with configurable grace period
- **SIGTERM → SIGKILL escalation** for graceful shutdown
- **Process group isolation** to ensure child processes are properly terminated
- **Stdout/stderr capture**
- **Environment variable control**
- **Working directory configuration**
- **Stdin piping**

## Features

- Async/await interface powered by Tokio
- Process group management for reliable process termination
- Graceful shutdown: SIGTERM → wait → SIGKILL
- Comprehensive error handling via `rskit-errors`
- Full logging support with `tracing`

## Usage

```rust
use rskit_process::{Command, ProcessConfig, run_with_cancel};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = Command::new("echo")
        .arg("hello")
        .arg("world");

    let config = ProcessConfig {
        timeout: Some(Duration::from_secs(30)),
        grace_period: Duration::from_secs(5),
        capture_output: true,
        inherit_env: true,
        max_output_bytes: Some(rskit_process::DEFAULT_MAX_OUTPUT_BYTES),
    };

    let result = run_with_cancel(&cmd, &config, CancellationToken::new()).await?;
    println!("Output: {}", result.stdout);
    println!("Exit code: {:?}", result.exit_code);
    println!("Timed out: {}", result.timed_out);
    
    result.check()?;  // Verify exit code is 0
    Ok(())
}
```

## Examples

### Simple command execution

```rust
let cmd = Command::new("ls").arg("-la").arg("/tmp");
let result = run_with_cancel(&cmd, &ProcessConfig::default(), CancellationToken::new()).await?;
println!("{}", result.stdout);
```

### With custom working directory

```rust
let cmd = Command::new("cargo")
    .args(vec!["build", "--release"])
    .dir("/path/to/project");
    
let result = run_with_cancel(&cmd, &ProcessConfig::default(), CancellationToken::new()).await?;
```

### With environment variables

```rust
let cmd = Command::new("sh")
    .arg("-c")
    .arg("echo $MY_VAR")
    .env("MY_VAR", "my_value");
    
let result = run_with_cancel(&cmd, &ProcessConfig::default(), CancellationToken::new()).await?;
```

### With stdin

```rust
let cmd = Command::new("cat")
    .stdin(b"hello world".to_vec());
    
let result = run_with_cancel(&cmd, &ProcessConfig::default(), CancellationToken::new()).await?;
println!("{}", result.stdout);  // "hello world"
```

### With timeout

```rust
let config = ProcessConfig {
    timeout: Some(Duration::from_secs(10)),
    grace_period: Duration::from_secs(2),
    capture_output: true,
    inherit_env: true,
    max_output_bytes: Some(rskit_process::DEFAULT_MAX_OUTPUT_BYTES),
};

let cmd = Command::new("sleep").arg("5");
let result = run_with_cancel(&cmd, &config, CancellationToken::new()).await?;

if result.timed_out {
    eprintln!("Process was killed due to timeout");
}
```

## Implementation Details

### Process Group Management

The crate uses Unix process groups to ensure reliable process termination:

- Spawns processes in their own process group using `setpgid(0, 0)`
- Sends signals to the entire process group using negative PID: `-pid`
- Ensures all child processes are terminated even if they fork

### Timeout Handling

When a timeout occurs:

1. SIGTERM is sent to the process group
2. Wait for the grace period (default 5 seconds)
3. If still running, send SIGKILL to the process group
4. Return result with `timed_out: true`

## Related

- [gokit/process](https://github.com/skillsenselab/gokit) - Go implementation
- [pykit-process](https://github.com/skillsenselab/pykit) - Python implementation
