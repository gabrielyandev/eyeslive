//! EyesLive System Installer & Background Service Manager — by Gabriel Yan (@gabrielyandev)
//! GitHub: https://github.com/gabrielyandev/eyeslive
//! Handles one-click installation to %LOCALAPPDATA%\EyesLive, Desktop/Start Menu shortcuts,
//! Windows startup registration (HKCU Run), and background execution management.

use std::path::PathBuf;
use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn get_install_dir() -> PathBuf {
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        PathBuf::from(local_app_data).join("EyesLive")
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

pub fn get_installed_exe_path() -> PathBuf {
    get_install_dir().join("eyeslive.exe")
}

pub fn is_installed() -> bool {
    get_installed_exe_path().exists()
}

pub fn is_running_from_install_dir() -> bool {
    if let Ok(cur) = std::env::current_exe() {
        let installed = get_installed_exe_path();
        cur.to_string_lossy().to_lowercase() == installed.to_string_lossy().to_lowercase()
    } else {
        false
    }
}

pub fn is_autostart_enabled() -> bool {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("reg")
            .args([
                "query",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "EyesLive",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        if let Ok(out) = output {
            return out.status.success();
        }
        false
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

pub fn set_autostart(enable: bool) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let exe_path = if is_installed() {
            get_installed_exe_path()
        } else {
            std::env::current_exe()?
        };

        if enable {
            let cmd_val = format!("\"{}\" --background", exe_path.display());
            let status = Command::new("reg")
                .args([
                    "add",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                    "/v",
                    "EyesLive",
                    "/t",
                    "REG_SZ",
                    "/d",
                    &cmd_val,
                    "/f",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .status()?;
            if !status.success() {
                anyhow::bail!("Falha ao registrar inicializacao automatica no Registro");
            }
        } else {
            let _ = Command::new("reg")
                .args([
                    "delete",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                    "/v",
                    "EyesLive",
                    "/f",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .status();
        }
    }
    Ok(())
}

pub fn install_application(enable_autostart: bool) -> anyhow::Result<String> {
    let current_exe = std::env::current_exe()?;
    let install_dir = get_install_dir();
    std::fs::create_dir_all(&install_dir)?;

    let target_exe = get_installed_exe_path();
    if current_exe.to_string_lossy().to_lowercase()
        != target_exe.to_string_lossy().to_lowercase()
    {
        std::fs::copy(&current_exe, &target_exe)?;
    }

    #[cfg(target_os = "windows")]
    {
        create_windows_shortcuts(&target_exe, &install_dir);
    }

    set_autostart(enable_autostart)?;

    Ok(target_exe.display().to_string())
}

pub fn uninstall_application() -> anyhow::Result<()> {
    let _ = set_autostart(false);

    #[cfg(target_os = "windows")]
    {
        if let Ok(user_profile) = std::env::var("USERPROFILE") {
            let desktop_lnk = PathBuf::from(user_profile)
                .join("Desktop")
                .join("EyesLive.lnk");
            let _ = std::fs::remove_file(desktop_lnk);
        }
        if let Ok(app_data) = std::env::var("APPDATA") {
            let start_lnk = PathBuf::from(app_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("EyesLive.lnk");
            let _ = std::fs::remove_file(start_lnk);
        }
    }

    let target_exe = get_installed_exe_path();
    if !is_running_from_install_dir() && target_exe.exists() {
        let _ = std::fs::remove_file(&target_exe);
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn create_windows_shortcuts(target_exe: &PathBuf, working_dir: &PathBuf) {
    let exe_str = target_exe.display().to_string().replace('\'', "''");
    let dir_str = working_dir.display().to_string().replace('\'', "''");

    let ps_script = format!(
        r#"$ws = New-Object -ComObject WScript.Shell;
$desktop = [Environment]::GetFolderPath('Desktop');
$s1 = $ws.CreateShortcut((Join-Path $desktop 'EyesLive.lnk'));
$s1.TargetPath = '{exe}';
$s1.WorkingDirectory = '{dir}';
$s1.Description = 'EyesLive Remote Desktop — by Gabriel Yan (@gabrielyandev)';
$s1.Save();
$programs = [Environment]::GetFolderPath('Programs');
$s2 = $ws.CreateShortcut((Join-Path $programs 'EyesLive.lnk'));
$s2.TargetPath = '{exe}';
$s2.WorkingDirectory = '{dir}';
$s2.Description = 'EyesLive Remote Desktop — by Gabriel Yan (@gabrielyandev)';
$s2.Save();"#,
        exe = exe_str,
        dir = dir_str
    );

    let _ = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &ps_script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}
