use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Discovery payloads and diagnostics are tiny; refuse to buffer more.
const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

#[derive(Clone)]
pub struct ProcessOutcome {
    /// `None` when the process was terminated by a signal.
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone)]
pub enum ProcessFailure {
    /// Could not start the process at all.
    Spawn(String),
    /// Deadline elapsed; the child was killed.
    Timeout { stderr: String },
}

pub trait ProcessRunner: Send + Sync {
    fn run(
        &self,
        program: &Path,
        args: &[&str],
        timeout: Duration,
    ) -> Result<ProcessOutcome, ProcessFailure>;
}

/// Real runner: `Command::args` only. There is no shell anywhere, so no
/// interpolation is possible by construction.
pub struct SystemProcessRunner;

impl ProcessRunner for SystemProcessRunner {
    fn run(
        &self,
        program: &Path,
        args: &[&str],
        timeout: Duration,
    ) -> Result<ProcessOutcome, ProcessFailure> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| ProcessFailure::Spawn(error.to_string()))?;

        // Drain both pipes off-thread: the child can never block on a full
        // pipe while we poll for exit, and a kill closes the pipes.
        let stdout = drain(child.stdout.take().expect("stdout piped"));
        let stderr = drain(child.stderr.take().expect("stderr piped"));

        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    // A descendant may still hold the pipes open; stay within the deadline.
                    let grace = deadline
                        .saturating_duration_since(Instant::now())
                        .max(Duration::from_secs(1));
                    let stdout = match stdout.recv_timeout(grace) {
                        Ok(text) => text,
                        Err(_) => {
                            return Err(ProcessFailure::Timeout {
                                stderr: stderr
                                    .recv_timeout(Duration::from_secs(1))
                                    .unwrap_or_default(),
                            })
                        }
                    };
                    return Ok(ProcessOutcome {
                        exit_code: status.code(),
                        stdout,
                        stderr: stderr
                            .recv_timeout(Duration::from_secs(1))
                            .unwrap_or_default(),
                    });
                }
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ProcessFailure::Timeout {
                        stderr: stderr
                            .recv_timeout(Duration::from_secs(1))
                            .unwrap_or_default(),
                    });
                }
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                Err(error) => return Err(ProcessFailure::Spawn(error.to_string())),
            }
        }
    }
}

fn drain(mut pipe: impl Read + Send + 'static) -> mpsc::Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 8192];
        while bytes.len() < MAX_OUTPUT_BYTES {
            match pipe.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => bytes.extend_from_slice(&chunk[..read]),
            }
        }
        let _ = sender.send(String::from_utf8_lossy(&bytes).into_owned());
    });
    receiver
}

pub trait EngineLocator: Send + Sync {
    fn locate(&self, executable_name: &str) -> Option<PathBuf>;
}

/// PATH lookup with no hardcoded installation path. The `which` crate
/// handles platform rules (PATHEXT on Windows, permission bits on Unix).
/// Stage 7's embedded runtime will add a second locator, not replace this.
pub struct PathEngineLocator;

impl EngineLocator for PathEngineLocator {
    fn locate(&self, executable_name: &str) -> Option<PathBuf> {
        which::which(executable_name).ok()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// The shell exits immediately but its backgrounded child holds both
    /// pipe write-ends open; the probe must fail as a timeout within its
    /// budget instead of stalling on the held pipes.
    #[test]
    fn a_grandchild_holding_the_pipe_fails_as_a_timeout() {
        match SystemProcessRunner.run(
            Path::new("sh"),
            &["-c", "sleep 2 & exit 0"],
            Duration::from_millis(500),
        ) {
            Err(ProcessFailure::Timeout { .. }) => {}
            Err(ProcessFailure::Spawn(message)) => panic!("spawn failed: {message}"),
            Ok(outcome) => panic!(
                "expected a timeout failure, got exit code {:?}",
                outcome.exit_code
            ),
        }
    }
}
