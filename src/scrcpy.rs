use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use crate::config::{Device, scrcpy_exe};

pub fn build_args(device: &Device) -> Vec<String> {
    let mut args = vec![format!("--tcpip={}", device.address())];

    if !device.audio {
        args.push("--no-audio".to_string());
    }
    if device.always_on_top {
        args.push("--always-on-top".to_string());
    }
    if device.fullscreen {
        args.push("--fullscreen".to_string());
    }
    if let Some(max_size) = device.max_size {
        args.push(format!("--max-size={max_size}"));
    }
    if let Some(bit_rate) = &device.bit_rate
        && !bit_rate.is_empty()
    {
        args.push(format!("--video-bit-rate={bit_rate}"));
    }
    if let Some(max_fps) = device.max_fps {
        args.push(format!("--max-fps={max_fps}"));
    }
    let title = device
        .window_title
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| device.name.clone());
    if !title.is_empty() {
        args.push(format!("--window-title={title}"));
    }
    args.extend(device.extra_args.split_whitespace().map(str::to_string));

    args
}

#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}

#[cfg(not(windows))]
fn detach(_: &mut Command) {}

/// Where the scrcpy child is right now, derived from its own log output.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Running,
    Ready,
    Failed(String),
}

/// A running scrcpy child plus the log lines it has written so far.
pub struct Session {
    child: Arc<Mutex<Child>>,
    lines: Arc<Mutex<Vec<String>>>,
}

impl Session {
    pub fn outcome(&self) -> Outcome {
        let finished = self
            .child
            .lock()
            .ok()
            .and_then(|mut child| child.try_wait().ok().flatten())
            .is_some();
        let Some(lines) = self.lines.lock().ok() else {
            return Outcome::Running;
        };
        if lines.iter().any(|line| line.contains("INFO: Renderer")) {
            return Outcome::Ready;
        }
        if finished {
            let reason = lines
                .iter()
                .rev()
                .find(|line| line.starts_with("ERROR") || line.contains("ERROR:"))
                .map(|line| line.trim().to_string())
                .unwrap_or_else(|| "scrcpy 已退出".to_string());
            return Outcome::Failed(reason);
        }
        Outcome::Running
    }
}

fn drain(
    reader: impl std::io::Read + Send + 'static,
    sink: Arc<Mutex<Vec<String>>>,
) {
    std::thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            if let Ok(mut held) = sink.lock() {
                held.push(line);
            }
        }
    });
}

pub fn launch(device: &Device) -> Result<Session, String> {
    let exe = scrcpy_exe();
    if !exe.exists() {
        return Err(format!("找不到 scrcpy：{}", exe.display()));
    }
    let mut command = Command::new(&exe);
    command.current_dir(exe.parent().unwrap_or(std::path::Path::new(".")));
    command.args(build_args(device));
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    detach(&mut command);

    let mut child = command.spawn().map_err(|error| error.to_string())?;
    // scrcpy 的 INFO 日志走 stdout，adb 推送进度与 ERROR 走 stderr，两边都要收。
    let lines = Arc::new(Mutex::new(Vec::new()));
    if let Some(stdout) = child.stdout.take() {
        drain(stdout, Arc::clone(&lines));
    }
    if let Some(stderr) = child.stderr.take() {
        drain(stderr, Arc::clone(&lines));
    }

    Ok(Session {
        child: Arc::new(Mutex::new(child)),
        lines,
    })
}

#[cfg(test)]
mod tests {
    use super::build_args;
    use crate::config::Device;

    fn device() -> Device {
        Device {
            id: 1,
            name: "小米 14".to_string(),
            host: "192.168.1.23".to_string(),
            ..Device::default()
        }
    }

    #[test]
    fn audio_on_sends_no_flag() {
        assert_eq!(
            build_args(&device()),
            ["--tcpip=192.168.1.23:5555", "--window-title=小米 14"]
        );
    }

    #[test]
    fn audio_off_and_custom_args() {
        let mut device = device();
        device.audio = false;
        device.fullscreen = true;
        device.extra_args = "--no-window-animation --max-fps=120".to_string();
        assert_eq!(
            build_args(&device),
            [
                "--tcpip=192.168.1.23:5555",
                "--no-audio",
                "--fullscreen",
                "--window-title=小米 14",
                "--no-window-animation",
                "--max-fps=120",
            ]
        );
    }
}
