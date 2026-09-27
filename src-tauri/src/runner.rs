use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::core::adapter::{AdapterError, RunSpec};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunOutcome {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
}

enum Message {
    Line(Stream, String),
    Done,
}

pub fn run_streaming<F>(
    spec: &RunSpec,
    timeout: Option<Duration>,
    mut on_line: F,
) -> Result<RunOutcome, AdapterError>
where
    F: FnMut(Stream, String),
{
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    #[cfg(unix)]
    command.process_group(0);

    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AdapterError::ToolMissing {
                tool: spec.program.to_string_lossy().into_owned(),
                adapter: "el ejecutor".into(),
                hint: "Instala el intérprete o compilador del lenguaje del proyecto y asegúrate de que esté en el PATH.".into(),
            }
        } else {
            AdapterError::Io(error.to_string())
        }
    })?;

    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let (sender, receiver) = mpsc::channel::<Message>();
    let stderr_sender = sender.clone();
    let stdout_reader =
        thread::spawn(move || read_lines(BufReader::new(stdout), Stream::Stdout, sender));
    let stderr_reader =
        thread::spawn(move || read_lines(BufReader::new(stderr), Stream::Stderr, stderr_sender));

    let mut timed_out = false;
    let deadline = timeout.map(|duration| Instant::now() + duration);
    let mut finished = 0usize;

    while finished < 2 {
        let wait = match deadline {
            Some(deadline) => deadline.saturating_duration_since(Instant::now()),
            None => Duration::from_secs(3600),
        };
        let wait = if wait.is_zero() {
            Duration::from_millis(1)
        } else {
            wait
        };

        match receiver.recv_timeout(wait) {
            Ok(Message::Line(stream, line)) => on_line(stream, line),
            Ok(Message::Done) => finished += 1,
            Err(RecvTimeoutError::Timeout) => {
                if timed_out {
                    thread::sleep(Duration::from_millis(5));
                } else {
                    timed_out = true;
                    kill_tree(&mut child);
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    let _ = stdout_reader.join();
    let _ = stderr_reader.join();
    let status = child
        .wait()
        .map_err(|error| AdapterError::Io(error.to_string()))?;

    Ok(RunOutcome {
        exit_code: status.code(),
        timed_out,
    })
}

fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        if pid > 0 {
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
    }
    let _ = child.kill();
}

fn read_lines<R: Read>(reader: BufReader<R>, stream: Stream, sender: mpsc::Sender<Message>) {
    for line in reader.lines() {
        match line {
            Ok(line) => {
                if sender.send(Message::Line(stream, line)).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let _ = sender.send(Message::Done);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn shell(script: &str) -> RunSpec {
        RunSpec {
            program: PathBuf::from("sh"),
            args: vec!["-c".into(), script.into()],
            cwd: std::env::temp_dir(),
            env: Vec::new(),
        }
    }

    #[cfg(unix)]
    #[test]
    fn streams_standard_output_and_error() {
        let mut lines = Vec::new();
        let outcome = run_streaming(
            &shell("printf 'uno\\ndos\\n'; printf 'fallo\\n' 1>&2"),
            Some(Duration::from_secs(5)),
            |stream, line| lines.push((stream, line)),
        )
        .unwrap();

        assert_eq!(outcome.exit_code, Some(0));
        assert!(!outcome.timed_out);
        assert!(lines.contains(&(Stream::Stdout, "uno".to_string())));
        assert!(lines.contains(&(Stream::Stdout, "dos".to_string())));
        assert!(lines.contains(&(Stream::Stderr, "fallo".to_string())));
    }

    #[cfg(unix)]
    #[test]
    fn reports_the_exit_code() {
        let outcome =
            run_streaming(&shell("exit 3"), Some(Duration::from_secs(5)), |_, _| {}).unwrap();
        assert_eq!(outcome.exit_code, Some(3));
        assert!(!outcome.timed_out);
    }

    #[cfg(unix)]
    #[test]
    fn kills_a_process_that_overruns_the_timeout() {
        let outcome = run_streaming(
            &shell("sleep 5"),
            Some(Duration::from_millis(200)),
            |_, _| {},
        )
        .unwrap();
        assert!(outcome.timed_out);
    }

    #[cfg(unix)]
    #[test]
    fn kills_the_whole_process_group_on_timeout() {
        let dir = std::env::temp_dir().join(format!("idecode-grupo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pidfile = dir.join("pid");

        let script = format!("sleep 30 & echo $! > {}; wait", pidfile.display());
        let outcome =
            run_streaming(&shell(&script), Some(Duration::from_millis(500)), |_, _| {}).unwrap();
        assert!(outcome.timed_out);

        let pid: i32 = std::fs::read_to_string(&pidfile)
            .unwrap()
            .trim()
            .parse()
            .expect("pid del nieto");
        let mut alive = true;
        for _ in 0..60 {
            alive = unsafe { libc::kill(pid, 0) } == 0;
            if !alive {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            !alive,
            "el proceso lanzado por el script sobrevivió al timeout"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
