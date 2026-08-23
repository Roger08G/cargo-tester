use std::{
    io::{self, Read},
    process::{Command, ExitStatus, Stdio},
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use command_group::{CommandGroup, GroupChild};

static CANCELLED: AtomicBool = AtomicBool::new(false);
static INTERRUPT_HANDLER: OnceLock<std::result::Result<(), String>> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
pub(crate) struct Cancellation;

impl Cancellation {
    pub(crate) fn install() -> Result<Self> {
        CANCELLED.store(false, Ordering::SeqCst);
        let result = INTERRUPT_HANDLER.get_or_init(|| {
            ctrlc::set_handler(|| CANCELLED.store(true, Ordering::SeqCst))
                .map_err(|error| error.to_string())
        });
        if let Err(error) = result {
            bail!("failed to install interrupt handler: {error}");
        }
        Ok(Self)
    }

    pub(crate) fn passive() -> Self {
        Self
    }

    pub(crate) fn is_cancelled(self) -> bool {
        CANCELLED.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Termination {
    Completed,
    TimedOut,
    Cancelled,
}

#[derive(Debug)]
pub(crate) struct ProcessOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub termination: Termination,
}

impl ProcessOutput {
    pub(crate) fn success(&self) -> bool {
        self.termination == Termination::Completed && self.status.success()
    }

    pub(crate) fn exit_code(&self) -> Option<i32> {
        self.status.code()
    }
}

pub(crate) fn run_command(
    command: &mut Command,
    timeout: Duration,
    max_output_bytes: usize,
    cancellation: Cancellation,
    description: &str,
) -> Result<ProcessOutput> {
    if cancellation.is_cancelled() {
        bail!("{description} cancelled");
    }

    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .group_spawn()
        .with_context(|| format!("failed to start {description}"))?;

    let stdout = child
        .inner()
        .stdout
        .take()
        .context("child stdout was not captured")?;
    let stderr = child
        .inner()
        .stderr
        .take()
        .context("child stderr was not captured")?;
    let stdout_reader = thread::spawn(move || read_bounded(stdout, max_output_bytes));
    let stderr_reader = thread::spawn(move || read_bounded(stderr, max_output_bytes));

    let started = Instant::now();
    let (status, termination) = loop {
        if cancellation.is_cancelled() {
            break (
                terminate(&mut child).with_context(|| format!("failed to cancel {description}"))?,
                Termination::Cancelled,
            );
        }
        if started.elapsed() >= timeout {
            break (
                terminate(&mut child)
                    .with_context(|| format!("failed to terminate timed out {description}"))?,
                Termination::TimedOut,
            );
        }

        match child
            .try_wait()
            .with_context(|| format!("failed while waiting for {description}"))?
        {
            Some(status) => break (status, Termination::Completed),
            None => thread::sleep(Duration::from_millis(10)),
        }
    };

    let stdout = join_reader(stdout_reader, "stdout", description)?;
    let stderr = join_reader(stderr_reader, "stderr", description)?;

    Ok(ProcessOutput {
        status,
        stdout: String::from_utf8_lossy(&stdout.bytes).into_owned(),
        stderr: String::from_utf8_lossy(&stderr.bytes).into_owned(),
        stdout_truncated: stdout.truncated,
        stderr_truncated: stderr.truncated,
        termination,
    })
}

fn terminate(child: &mut GroupChild) -> io::Result<ExitStatus> {
    if let Some(status) = child.try_wait()? {
        return Ok(status);
    }
    if let Err(error) = child.kill() {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        return Err(error);
    }
    child.wait()
}

struct BoundedOutput {
    bytes: Vec<u8>,
    truncated: bool,
}

fn read_bounded(mut reader: impl Read, max_bytes: usize) -> io::Result<BoundedOutput> {
    let mut bytes = Vec::with_capacity(max_bytes.min(8 * 1024));
    let mut buffer = [0_u8; 8 * 1024];
    let mut truncated = false;

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = max_bytes.saturating_sub(bytes.len());
        let retained = read.min(remaining);
        bytes.extend_from_slice(&buffer[..retained]);
        truncated |= retained < read;
    }

    Ok(BoundedOutput { bytes, truncated })
}

fn join_reader(
    reader: thread::JoinHandle<io::Result<BoundedOutput>>,
    stream: &str,
    description: &str,
) -> Result<BoundedOutput> {
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("{stream} reader panicked for {description}"))?
        .with_context(|| format!("failed to capture {stream} for {description}"))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn bounded_reader_drains_input_and_marks_truncation() {
        let output = read_bounded(Cursor::new(vec![b'x'; 64]), 8).unwrap();

        assert_eq!(output.bytes, vec![b'x'; 8]);
        assert!(output.truncated);
    }
}
