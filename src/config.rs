use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Device {
    pub id: u64,
    pub name: String,
    pub host: String,
    pub port: u16,
    /// 关闭时向 scrcpy 传 --no-audio
    pub audio: bool,
    pub always_on_top: bool,
    pub fullscreen: bool,
    pub max_size: Option<u32>,
    pub bit_rate: Option<String>,
    pub max_fps: Option<u32>,
    pub window_title: Option<String>,
    /// 追加到 scrcpy 命令行的自定义参数
    pub extra_args: String,
}

impl Default for Device {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            host: String::new(),
            port: 5555,
            audio: true,
            always_on_top: false,
            fullscreen: false,
            max_size: None,
            bit_rate: None,
            max_fps: None,
            window_title: None,
            extra_args: String::new(),
        }
    }
}

impl Device {
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn tags(&self) -> Vec<String> {
        let mut tags = Vec::new();
        if !self.audio {
            tags.push("无音频".to_string());
        }
        if self.always_on_top {
            tags.push("置顶".to_string());
        }
        if self.fullscreen {
            tags.push("全屏".to_string());
        }
        if let Some(size) = self.max_size {
            tags.push(format!("{size}px"));
        }
        if let Some(bit_rate) = &self.bit_rate
            && !bit_rate.is_empty()
        {
            tags.push(format!("码率 {bit_rate}"));
        }
        if let Some(fps) = self.max_fps {
            tags.push(format!("{fps}fps"));
        }
        if !self.extra_args.trim().is_empty() {
            tags.push("自定义参数".to_string());
        }
        tags
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub devices: Vec<Device>,
    pub next_id: u64,
}

fn app_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let dir = base.join("scrcpy-desktop");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn config_path() -> PathBuf {
    app_dir().join("config.json")
}

/// scrcpy 运行时必须与程序同级放在 `scrcpy/` 目录里（即发布压缩包解压后的结构）。
pub fn scrcpy_exe() -> PathBuf {
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        let candidate = dir.join("scrcpy").join("scrcpy.exe");
        if candidate.exists() {
            return candidate;
        }
    }
    // 开发构建的 exe 在 target/<profile>/ 下，回落到仓库里的 scrcpy/
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scrcpy")
        .join("scrcpy.exe")
}

pub fn load() -> AppConfig {
    let path = config_path();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    serde_json::from_str::<AppConfig>(&text).unwrap_or_default()
}

pub fn save(config: &AppConfig) -> Result<(), std::io::Error> {
    let text = serde_json::to_string_pretty(config).expect("config serializable");
    std::fs::write(config_path(), text)
}

#[cfg(test)]
mod tests {
    /// 测试 exe 位于 target/debug/deps，没有同级 scrcpy/，因此应回落到仓库目录。
    #[test]
    fn scrcpy_resolved_from_sibling_directory_only() {
        let exe = super::scrcpy_exe();
        assert!(exe.ends_with("scrcpy/scrcpy.exe"), "{}", exe.display());
        assert!(exe.exists(), "找不到 {}", exe.display());
    }
}
