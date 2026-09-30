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

/// Líneas en vuelo antes de aplicar contrapresión a la salida del proceso.
/// Si se supera, los hilos de lectura se bloquean y dejan de consumir el pipe.
const RUN_OUTPUT_BUFFER: usize = 1024;

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

    let stdout = child.stdout.take().ok_or_else(|| {
        AdapterError::Io("No se pudo capturar la salida estándar del proceso hijo.".into())
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        AdapterError::Io("No se pudo capturar la salida de error del proceso hijo.".into())
    })?;
    let (sender, receiver) = mpsc::sync_channel::<Message>(RUN_OUTPUT_BUFFER);
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

    // Si algún hilo de lectura sigue bloqueado esperando EOF (por ejemplo un
    // nieto que escapó del kill), no lo esperamos: desligarlo evita que el
    // comando se cuelgue indefinidamente. Solo unimos cuando ambos terminaron.
    if finished >= 2 {
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();
    }
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
    #[cfg(windows)]
    {
        // En Windows no existe el grupo de procesos de Unix y `Child::kill`
        // solo mata al proceso directo: los nietos heredarían los pipes y
        // bloquearían a los hilos de lectura. `taskkill /T` mata el árbol.
        let pid = child.id();
        if pid > 0 {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    let _ = child.kill();
}

fn read_lines<R: Read>(
    mut reader: BufReader<R>,
    stream: Stream,
    sender: mpsc::SyncSender<Message>,
) {
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(0) => break,
            Ok(_) => {
                if buffer.last() == Some(&b'\n') {
                    buffer.pop();
                    if buffer.last() == Some(&b'\r') {
                        buffer.pop();
                    }
                }
                let line = String::from_utf8_lossy(&buffer).into_owned();
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
    fn keeps_reading_after_invalid_utf8() {
        let mut lines = Vec::new();
        run_streaming(
            &shell("printf '\\377\\376\\n'; printf 'despues\\n'"),
            Some(Duration::from_secs(5)),
            |_, line| lines.push(line),
        )
        .unwrap();

        assert!(
            lines.iter().any(|line| line == "despues"),
            "la salida se cortó tras bytes no UTF-8: {lines:?}"
        );
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

    #[test]
    fn returns_without_hanging_when_a_grandchild_keeps_the_pipe_open() {
        let spec = RunSpec {
            program: PathBuf::from("cmd"),
            args: ["/C", "start /B ping -n 30 127.0.0.1 & ping -n 30 127.0.0.1"]
                .iter()
                .map(|value| value.to_string())
                .collect(),
            cwd: std::env::temp_dir(),
            env: Vec::new(),
        };
        let started = std::time::Instant::now();
        let outcome = run_streaming(&spec, Some(Duration::from_millis(500)), |_, _| {}).unwrap();
        assert!(outcome.timed_out);
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "run_streaming no debería colgarse con un nieto vivo"
        );
    }
}
