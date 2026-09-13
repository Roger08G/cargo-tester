use std::{
    collections::VecDeque,
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
    // Protocol parsing must not depend on the user-facing capture limit.
    pub stdout_tail: String,
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
    let mut group = command.group();
    #[cfg(windows)]
    group.kill_on_drop(true);
    let mut child = ChildGuard(
        group
            .spawn()
            .with_context(|| format!("failed to start {description}"))?,
    );

    let stdout = child
        .0
        .inner()
        .stdout
        .take()
        .context("child stdout was not captured")?;
    let stderr = child
        .0
        .inner()
        .stderr
        .take()
        .context("child stderr was not captured")?;
    let stdout_reader = thread::Builder::new()
        .name("tester-stdout".into())
        .spawn(move || read_bounded(stdout, max_output_bytes, 1024))?;
    let stderr_reader = thread::Builder::new()
        .name("tester-stderr".into())
        .spawn(move || read_bounded(stderr, max_output_bytes, 0))?;

    let started = Instant::now();
    let (status, termination) = loop {
        if cancellation.is_cancelled() {
            break (
                terminate(&mut child.0)
                    .with_context(|| format!("failed to cancel {description}"))?,
                Termination::Cancelled,
            );
        }
        if started.elapsed() >= timeout {
            break (
                terminate(&mut child.0)
                    .with_context(|| format!("failed to terminate timed out {description}"))?,
                Termination::TimedOut,
            );
        }

        match child
            .0
            .try_wait()
            .with_context(|| format!("failed while waiting for {description}"))?
        {
            Some(status) => break (status, Termination::Completed),
            None => thread::sleep(Duration::from_millis(10)),
        }
    };

    // A successfully exited leader can leave descendants holding its pipes open.
    // Always clean the group before draining, including normal completion.
    let _ = child.0.kill();
    let drain_started = Instant::now();
    while !stdout_reader.is_finished() || !stderr_reader.is_finished() {
        if drain_started.elapsed() >= Duration::from_secs(1) {
            bail!("{description}: output pipes remained open after process-group cleanup");
        }
        thread::sleep(Duration::from_millis(2));
    }
    let stdout = join_reader(stdout_reader, "stdout", description)?;
    let stderr = join_reader(stderr_reader, "stderr", description)?;
    let (stdout_text, stdout_decoding_truncated) = decode_bounded(&stdout.bytes, max_output_bytes);
    let (stderr_text, stderr_decoding_truncated) = decode_bounded(&stderr.bytes, max_output_bytes);

    Ok(ProcessOutput {
        status,
        stdout: stdout_text,
        stdout_tail: String::from_utf8_lossy(&stdout.tail).into_owned(),
        stderr: stderr_text,
        stdout_truncated: stdout.truncated || stdout_decoding_truncated,
        stderr_truncated: stderr.truncated || stderr_decoding_truncated,
        termination,
    })
}

fn terminate(child: &mut GroupChild) -> io::Result<ExitStatus> {
    if let Err(error) = child.kill() {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        return Err(error);
    }
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if started.elapsed() >= Duration::from_secs(1) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "process did not exit after group termination",
            ));
        }
        thread::sleep(Duration::from_millis(2));
    }
}

struct ChildGuard(GroupChild);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        // Also runs on capture/setup/wait errors. Do not wait indefinitely in Drop.
        let _ = self.0.kill();
        let _ = self.0.try_wait();
    }
}

struct BoundedOutput {
    bytes: Vec<u8>,
    tail: Vec<u8>,
    truncated: bool,
}

fn read_bounded(
    mut reader: impl Read,
    max_bytes: usize,
    tail_bytes: usize,
) -> io::Result<BoundedOutput> {
    let mut bytes = Vec::with_capacity(max_bytes.min(8 * 1024));
    let mut tail = VecDeque::with_capacity(tail_bytes);
    let mut buffer = [0_u8; 8 * 1024];
    let mut truncated = false;

    loop {
        let read = match reader.read(&mut buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if read == 0 {
            break;
        }
        let remaining = max_bytes.saturating_sub(bytes.len());
        let retained = read.min(remaining);
        bytes.extend_from_slice(&buffer[..retained]);
        truncated |= retained < read;
        if tail_bytes > 0 {
            let chunk = &buffer[read.saturating_sub(tail_bytes)..read];
            let excess = (tail.len() + chunk.len()).saturating_sub(tail_bytes);
            tail.drain(..excess);
            tail.extend(chunk);
        }
    }

    Ok(BoundedOutput {
        bytes,
        tail: tail.into(),
        truncated,
    })
}

fn decode_bounded(bytes: &[u8], max_bytes: usize) -> (String, bool) {
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    let truncated = text.len() > max_bytes;
    if truncated {
        let mut end = max_bytes;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    (text, truncated)
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
        let output = read_bounded(Cursor::new(vec![b'x'; 64]), 8, 4).unwrap();

        assert_eq!(output.bytes, vec![b'x'; 8]);
        assert!(output.truncated);
        assert_eq!(output.tail, vec![b'x'; 4]);
    }

    #[test]
    fn tail_retains_protocol_after_capture_limit() {
        let output = read_bounded(Cursor::new(b"0123456789abcdef"), 1, 4).unwrap();
        assert_eq!(output.bytes, b"0");
        assert_eq!(output.tail, b"cdef");
    }

    #[test]
    fn invalid_utf8_cannot_expand_past_capture_limit() {
        let (text, truncated) = decode_bounded(&[255; 8], 8);
        assert!(text.len() <= 8);
        assert!(truncated);
    }
}
