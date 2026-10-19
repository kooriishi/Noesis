use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::AppState;

/// H4: 前端界面设置（主题 / 字体 / 字号 / 行高等）。
///
/// 全部字段都带默认值，方便旧版 `settings.json` 或缺失字段的平滑升级。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSettings {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_editor_font_size")]
    pub editor_font_size: u32,
    #[serde(default = "default_ui_font_size")]
    pub ui_font_size: u32,
    #[serde(default = "default_font_family")]
    pub font_family: String,
    #[serde(default = "default_ui_font_family")]
    pub ui_font_family: String,
    #[serde(default)]
    pub word_wrap: bool,
    #[serde(default = "default_tab_size")]
    pub tab_size: u32,
    #[serde(default = "default_line_height")]
    pub line_height: f32,
    /// H2-1: 最近一次打开的项目绝对路径，启动时用它恢复工作区。
    #[serde(default)]
    pub last_project: Option<String>,
    /// H2-1: 最近项目列表（最新的在前）。
    #[serde(default)]
    pub recent_projects: Vec<String>,
}

fn default_theme() -> String {
    "catppuccin".to_string()
}
fn default_editor_font_size() -> u32 {
    14
}
fn default_ui_font_size() -> u32 {
    13
}
fn default_font_family() -> String {
    "\"JetBrains Mono\", \"Fira Code\", Consolas, monospace".to_string()
}
fn default_ui_font_family() -> String {
    "-apple-system, BlinkMacSystemFont, \"Segoe UI\", \"PingFang SC\", \"Microsoft YaHei\", sans-serif"
        .to_string()
}
fn default_tab_size() -> u32 {
    4
}
fn default_line_height() -> f32 {
    1.6
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            editor_font_size: default_editor_font_size(),
            ui_font_size: default_ui_font_size(),
            font_family: default_font_family(),
            ui_font_family: default_ui_font_family(),
            word_wrap: false,
            tab_size: default_tab_size(),
            line_height: default_line_height(),
            last_project: None,
            recent_projects: Vec::new(),
        }
    }
}

fn settings_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("settings.json")
}

fn read_settings(app_data_dir: &Path) -> UiSettings {
    let path = settings_path(app_data_dir);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str::<UiSettings>(&text).unwrap_or_default(),
        Err(_) => UiSettings::default(),
    }
}

fn write_settings(app_data_dir: &Path, settings: &UiSettings) -> Result<(), String> {
    std::fs::create_dir_all(app_data_dir).map_err(|e| e.to_string())?;
    let path = settings_path(app_data_dir);
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

/// H4-4: 从后端读取界面设置。前端启动时优先用它，读失败才回退 localStorage。
pub async fn get_ui_settings(state: &AppState) -> Result<UiSettings, String> {
    Ok(read_settings(&state.app_data_dir))
}

/// H4-4: 写入界面设置到应用数据目录下的 `settings.json`。
pub async fn set_ui_settings(state: &AppState, settings: UiSettings) -> Result<UiSettings, String> {
    write_settings(&state.app_data_dir, &settings)?;
    Ok(settings)
}

/// H2-1: 记录「最近一次打开的项目」，供下次启动恢复。
pub async fn remember_project(state: &AppState, path: &str) -> Result<UiSettings, String> {
    let mut settings = read_settings(&state.app_data_dir);
    let normalized = normalize_project_path(path);
    settings.last_project = Some(normalized.clone());
    settings.recent_projects.retain(|p| p != &normalized);
    settings.recent_projects.insert(0, normalized);
    settings.recent_projects.truncate(12);
    write_settings(&state.app_data_dir, &settings)?;
    Ok(settings)
}

/// 去掉 Windows canonicalize 产生的 `\\?\` 前缀，统一用 `/` 分隔，避免欢迎页显示 `//?/F:/...`。
fn normalize_project_path(path: &str) -> String {
    let trimmed = path.trim();
    let without_prefix = trimmed
        .strip_prefix("\\\\?\\")
        .or_else(|| trimmed.strip_prefix("//?/"))
        .unwrap_or(trimmed);
    without_prefix.replace('\\', "/")
}

/// H2-1: 最近项目列表（启动欢迎页使用）。
pub async fn get_recent_projects(state: &AppState) -> Result<Vec<String>, String> {
    let settings = read_settings(&state.app_data_dir);
    Ok(settings.recent_projects)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_settings_roundtrip_with_defaults() {
        let dir = std::env::temp_dir().join(format!("noesis-ui-settings-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        // 缺字段的旧文件应该能读出来并补默认值
        std::fs::write(dir.join("settings.json"), r#"{"theme":"dracula"}"#).unwrap();
        let loaded = read_settings(&dir);
        assert_eq!(loaded.theme, "dracula");
        assert_eq!(loaded.editor_font_size, 14);
        assert_eq!(loaded.tab_size, 4);

        let mut updated = loaded;
        updated.last_project = Some("/tmp/demo".to_string());
        write_settings(&dir, &updated).unwrap();
        let again = read_settings(&dir);
        assert_eq!(again.last_project.as_deref(), Some("/tmp/demo"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn normalizes_windows_verbatim_prefix() {
        assert_eq!(normalize_project_path("\\\\?\\F:\\Projects\\Noesis"), "F:/Projects/Noesis");
        assert_eq!(normalize_project_path("//?/F:/Projects/Noesis"), "F:/Projects/Noesis");
        assert_eq!(normalize_project_path("F:/Projects/Noesis"), "F:/Projects/Noesis");
        assert_eq!(normalize_project_path("/home/me/proj"), "/home/me/proj");
    }
}
