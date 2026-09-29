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

pub const APPIMAGE_ENV_POLLUTION: &[&str] = &["PYTHONHOME", "PYTHONPATH", "LD_LIBRARY_PATH"];

pub fn sanitize_child_env(command: &mut Command) {
    if std::env::var_os("APPDIR").is_some() {
        strip_appimage_env(command);
    }
}

fn strip_appimage_env(command: &mut Command) {
    for key in APPIMAGE_ENV_POLLUTION {
        command.env_remove(key);
    }
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
    sanitize_child_env(&mut command);
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
    let kill_deadline = deadline.map(|d| d + Duration::from_secs(5));

    while finished < 2 {
        let Some(limit) = deadline else {
            // Sin timeout: espera bloqueante real, sin ciclos de espera.
            match receiver.recv() {
                Ok(Message::Line(stream, line)) => on_line(stream, line),
                Ok(Message::Done) => finished += 1,
                Err(_) => break,
            }
            continue;
        };

        if !limit.saturating_duration_since(Instant::now()).is_zero() {
            match receiver.recv_timeout(limit.saturating_duration_since(Instant::now())) {
                Ok(Message::Line(stream, line)) => on_line(stream, line),
                Ok(Message::Done) => finished += 1,
                Err(RecvTimeoutError::Timeout) => {
                    timed_out = true;
                    kill_tree(&mut child);
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
            continue;
        }

        if !timed_out {
            timed_out = true;
            kill_tree(&mut child);
            continue;
        }

        // Tras matar el proceso, espera como máximo 5 s a que los
        // hilos de lectura terminen; después sale aunque se atasquen.
        let grace = kill_deadline
            .map(|end| end.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_millis(100));
        if grace.is_zero() {
            break;
        }
        match receiver.recv_timeout(grace) {
            Ok(Message::Line(stream, line)) => on_line(stream, line),
            Ok(Message::Done) => finished += 1,
            Err(_) => break,
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn strips_appimage_environment_for_children() {
        let mut command = Command::new("python3");
        command.env("PYTHONHOME", "/tmp/.mount_ide/usr/");
        command.env("PYTHONPATH", "/tmp/.mount_ide/usr/share/pyshared/");
        strip_appimage_env(&mut command);
        let removed: Vec<String> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .filter_map(|(key, _)| key.to_str().map(str::to_string))
            .collect();
        assert!(removed.contains(&"PYTHONHOME".to_string()));
        assert!(removed.contains(&"PYTHONPATH".to_string()));
        assert!(removed.contains(&"LD_LIBRARY_PATH".to_string()));
    }

    #[test]
    fn adapter_env_wins_over_the_sanitization() {
        let mut command = Command::new("python3");
        strip_appimage_env(&mut command);
        command.env("PYTHONPATH", "/aula/proyecto");
        let value = command
            .get_envs()
            .find(|(key, _)| key.to_str() == Some("PYTHONPATH"))
            .and_then(|(_, value)| value)
            .expect("PYTHONPATH del adaptador");
        assert_eq!(value, "/aula/proyecto");
    }

    fn shell(script: &str) -> RunSpec {
        RunSpec {
            program: PathBuf::from("sh"),
            args: vec!["-c".into(), script.into()],
            cwd: std::env::temp_dir(),
            env: Vec::new(),
        }
    }

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

    #[test]
    fn reports_the_exit_code() {
        let outcome =
            run_streaming(&shell("exit 3"), Some(Duration::from_secs(5)), |_, _| {}).unwrap();
        assert_eq!(outcome.exit_code, Some(3));
        assert!(!outcome.timed_out);
    }

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

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn streams_output_and_reports_the_exit_code() {
        let spec = RunSpec {
            program: PathBuf::from("cmd"),
            args: ["/C", "echo hola"]
                .iter()
                .map(|value| value.to_string())
                .collect(),
            cwd: std::env::temp_dir(),
            env: Vec::new(),
        };
        let mut lines = Vec::new();
        let outcome = run_streaming(&spec, Some(Duration::from_secs(15)), |stream, line| {
            if stream == Stream::Stdout {
                lines.push(line);
            }
        })
        .unwrap();
        assert_eq!(outcome.exit_code, Some(0));
        assert!(
            lines.iter().any(|line| line.contains("hola")),
            "salida inesperada: {lines:?}"
        );
    }
}
