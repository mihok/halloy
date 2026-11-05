#[cfg(not(target_arch = "wasm32"))]
use std::process::Stdio;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use data::platform::time;
#[cfg(not(target_arch = "wasm32"))]
use tokio::process::Command;

// https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Browser: there's no shell to run commands in.
#[cfg(target_arch = "wasm32")]
pub async fn run(
    _command: String,
    _timeout_secs: u64,
    _max_output_bytes: usize,
) -> Result<String, String> {
    Err("exec isn't supported in the browser".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn run(
    command: String,
    timeout_secs: u64,
    max_output_bytes: usize,
) -> Result<String, String> {
    let output = time::timeout(Duration::from_secs(timeout_secs), async move {
        let mut process = if cfg!(target_os = "windows") {
            let mut process = Command::new("cmd");
            #[cfg(target_os = "windows")]
            process.creation_flags(CREATE_NO_WINDOW);
            process.arg("/C").arg(command);
            process
        } else {
            let mut process = Command::new("sh");
            process.arg("-c").arg(command);
            process
        };

        process
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output()
            .await
    })
    .await
    .map_err(|_| format!("exec timed out after {timeout_secs} seconds"))?
    .map_err(|error| format!("exec failed: {error}"))?;

    if output.stdout.len() > max_output_bytes {
        return Err(format!("exec output exceeds {max_output_bytes} bytes"));
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();

        return Err(if stderr.is_empty() {
            format!("exec exited with {}", output.status)
        } else {
            format!("exec failed: {stderr}")
        });
    }

    first_non_empty_line(&output.stdout)
}

#[cfg(any(not(target_arch = "wasm32"), test))]
fn first_non_empty_line(output: &[u8]) -> Result<String, String> {
    String::from_utf8_lossy(output)
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .find(|line| !line.trim().is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| String::from("exec produced no output"))
}

#[cfg(test)]
mod tests {
    use super::first_non_empty_line;

    #[test]
    fn returns_first_non_empty_line() {
        assert_eq!(
            first_non_empty_line(b"\n  \r\n/me hello\nignored").unwrap(),
            "/me hello"
        );
    }

    #[test]
    fn trims_carriage_returns() {
        assert_eq!(first_non_empty_line(b"hello\r\n").unwrap(), "hello");
    }

    #[test]
    fn rejects_empty_output() {
        assert_eq!(
            first_non_empty_line(b"\n \r\n\t").unwrap_err(),
            "exec produced no output"
        );
    }
}
