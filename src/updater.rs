//! EyesLive Auto-Updater & GitHub Release Checker — by Gabriel Yan (@gabrielyandev)
//! GitHub: https://github.com/gabrielyandev/eyeslive

use serde::Deserialize;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GITHUB_RELEASES_API: &str =
    "https://api.github.com/repos/gabrielyandev/eyeslive/releases/latest";
pub const GITHUB_RELEASES_PAGE: &str =
    "https://github.com/gabrielyandev/eyeslive/releases";

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub html_url: String,
    #[serde(default)]
    pub assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateInfo {
    pub latest_version: String,
    pub title: String,
    pub notes: String,
    pub release_url: String,
    pub exe_download_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateStatus {
    Idle,
    Checking,
    UpToDate,
    Available(UpdateInfo),
    Downloading(f32),
    ReadyToRestart(String),
    Error(String),
}

pub fn parse_version(raw: &str) -> Vec<u32> {
    raw.trim()
        .trim_start_matches('v')
        .split('.')
        .filter_map(|s| s.split('-').next())
        .filter_map(|s| s.parse::<u32>().ok())
        .collect()
}

pub fn is_newer_version(latest: &str, current: &str) -> bool {
    let lat = parse_version(latest);
    let cur = parse_version(current);
    lat > cur
}

pub fn check_for_updates_sync() -> anyhow::Result<Option<UpdateInfo>> {
    let resp: GitHubRelease = ureq::get(GITHUB_RELEASES_API)
        .set("User-Agent", &format!("EyesLive-Desktop/{}", CURRENT_VERSION))
        .set("Accept", "application/vnd.github.v3+json")
        .timeout(Duration::from_secs(8))
        .call()?
        .into_json()?;

    if is_newer_version(&resp.tag_name, CURRENT_VERSION) {
        let exe_asset = resp
            .assets
            .iter()
            .find(|a| a.name.ends_with(".exe"))
            .map(|a| a.browser_download_url.clone());

        Ok(Some(UpdateInfo {
            latest_version: resp.tag_name,
            title: resp.name.unwrap_or_else(|| "Nova Versão do EyesLive".into()),
            notes: resp.body.unwrap_or_default(),
            release_url: resp.html_url,
            exe_download_url: exe_asset,
        }))
    } else {
        Ok(None)
    }
}

pub fn download_update_sync(url: &str, status: Arc<Mutex<UpdateStatus>>) -> anyhow::Result<()> {
    *status.lock().unwrap() = UpdateStatus::Downloading(0.01);
    let resp = ureq::get(url)
        .set("User-Agent", "EyesLive-Updater")
        .timeout(Duration::from_secs(120))
        .call()?;

    let content_len = resp
        .header("content-length")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);

    let mut reader = resp.into_reader();
    let mut bytes = Vec::new();
    let mut buf = [0u8; 32768];
    let mut downloaded = 0;

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        downloaded += n;
        if content_len > 0 {
            let progress = (downloaded as f32) / (content_len as f32);
            *status.lock().unwrap() = UpdateStatus::Downloading(progress.clamp(0.0, 0.99));
        }
    }

    let current_exe = std::env::current_exe()?;
    let current_dir = current_exe
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let new_exe = current_dir.join("eyeslive_new.exe");
    std::fs::write(&new_exe, &bytes)?;

    *status.lock().unwrap() = UpdateStatus::ReadyToRestart(new_exe.display().to_string());
    Ok(())
}

pub fn apply_update_and_restart(new_exe_path: &str) -> anyhow::Result<()> {
    let current_exe = std::env::current_exe()?;
    let old_exe = current_exe.with_extension("exe.old");
    let _ = std::fs::remove_file(&old_exe);

    std::fs::rename(&current_exe, &old_exe)?;
    std::fs::rename(new_exe_path, &current_exe)?;

    let _ = std::process::Command::new(&current_exe).spawn()?;
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_compare() {
        assert!(is_newer_version("v1.0.1", "1.0.0"));
        assert!(is_newer_version("1.1.0", "1.0.0"));
        assert!(is_newer_version("2.0.0", "1.9.9"));
        assert!(!is_newer_version("1.0.0", "1.0.0"));
        assert!(!is_newer_version("v1.0.0", "1.0.0"));
        assert!(!is_newer_version("0.9.9", "1.0.0"));
    }
}
