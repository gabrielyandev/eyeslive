//! EyesLive Windows System Tray (Notification Area) Integration — by Gabriel Yan (@gabrielyandev)
//! GitHub: https://github.com/gabrielyandev/eyeslive
//! Places EyesLive in the Windows System Tray (near the clock) when minimized or closed,
//! hiding it from the taskbar while keeping the Host server active in the background.

use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};

pub const MAIN_WINDOW_TITLE: &str = "EyesLive — Remote Desktop [by Gabriel Yan @gabrielyandev]";

static TRAY_RESTORE_REQUESTED: AtomicBool = AtomicBool::new(false);
static TRAY_HIDE_REQUESTED: AtomicBool = AtomicBool::new(false);
static TRAY_QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn take_restore_request() -> bool {
    TRAY_RESTORE_REQUESTED.swap(false, Ordering::SeqCst)
}

pub fn take_hide_request() -> bool {
    TRAY_HIDE_REQUESTED.swap(false, Ordering::SeqCst)
}

pub fn take_quit_request() -> bool {
    TRAY_QUIT_REQUESTED.swap(false, Ordering::SeqCst)
}

/// Generates a modern 32x32 RGBA icon (electric blue rounded badge with cyan lens core)
pub fn generate_rgba_icon() -> (Vec<u8>, u32, u32) {
    let w = 32u32;
    let h = 32u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let cx = 15.5_f32;
    let cy = 15.5_f32;

    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = ((y * w + x) * 4) as usize;

            if dist <= 14.5 {
                if dist <= 4.2 {
                    // Inner glowing cyan core
                    rgba[idx] = 56;
                    rgba[idx + 1] = 189;
                    rgba[idx + 2] = 248;
                    rgba[idx + 3] = 255;
                } else if dist <= 7.8 {
                    // Dark slate iris ring
                    rgba[idx] = 15;
                    rgba[idx + 1] = 23;
                    rgba[idx + 2] = 42;
                    rgba[idx + 3] = 255;
                } else if dist <= 11.2 {
                    // Electric blue outer ring
                    rgba[idx] = 59;
                    rgba[idx + 1] = 130;
                    rgba[idx + 2] = 246;
                    rgba[idx + 3] = 255;
                } else {
                    // Deep slate border badge
                    rgba[idx] = 24;
                    rgba[idx + 1] = 34;
                    rgba[idx + 2] = 53;
                    rgba[idx + 3] = 255;
                }
            }
        }
    }

    (rgba, w, h)
}

pub fn build_egui_icon() -> egui::IconData {
    let (rgba, width, height) = generate_rgba_icon();
    egui::IconData {
        rgba,
        width,
        height,
    }
}

#[cfg(target_os = "windows")]
mod win32_tray {
    use super::*;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreateIcon, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon,
        DestroyMenu, DispatchMessageW, FindWindowW, GetCursorPos, GetMessageW, PostQuitMessage,
        RegisterClassW, SetForegroundWindow, ShowWindow, TrackPopupMenu, TranslateMessage,
        CW_USEDEFAULT, HICON, MF_SEPARATOR, MF_STRING, MSG, SW_HIDE, SW_RESTORE, SW_SHOW,
        TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP,
        WM_RBUTTONUP, WM_USER, WNDCLASSW, WS_OVERLAPPEDWINDOW,
    };

    const WM_TRAYICON: u32 = WM_USER + 101;
    const ID_TRAY_OPEN: usize = 1001;
    const ID_TRAY_HIDE: usize = 1002;
    const ID_TRAY_EXIT: usize = 1003;

    fn wide_null(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn find_main_hwnd() -> Option<HWND> {
        let title = wide_null(MAIN_WINDOW_TITLE);
        unsafe {
            let hwnd = FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())).ok()?;
            if hwnd.0.is_null() {
                None
            } else {
                Some(hwnd)
            }
        }
    }

    pub fn hide_window() {
        if let Some(hwnd) = find_main_hwnd() {
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }

    pub fn restore_window() {
        if let Some(hwnd) = find_main_hwnd() {
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }

    unsafe fn create_hicon() -> Option<HICON> {
        let (rgba, w, h) = generate_rgba_icon();
        let mut bgra = vec![0u8; rgba.len()];
        for (src, dst) in rgba.chunks_exact(4).zip(bgra.chunks_exact_mut(4)) {
            dst[0] = src[2]; // B
            dst[1] = src[1]; // G
            dst[2] = src[0]; // R
            dst[3] = src[3]; // A
        }
        let and_mask = vec![0u8; ((w * h) / 8) as usize];
        let hinstance = GetModuleHandleW(PCWSTR::null()).ok()?;
        CreateIcon(
            hinstance,
            w as i32,
            h as i32,
            1,
            32,
            and_mask.as_ptr(),
            bgra.as_ptr(),
        )
        .ok()
    }

    unsafe extern "system" fn tray_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_TRAYICON => {
                let event = (lparam.0 as u32) & 0xFFFF;
                if event == WM_LBUTTONUP || event == WM_LBUTTONDBLCLK {
                    restore_window();
                    TRAY_RESTORE_REQUESTED.store(true, Ordering::SeqCst);
                } else if event == WM_RBUTTONUP {
                    if let Ok(hmenu) = CreatePopupMenu() {
                        let _ = AppendMenuW(hmenu, MF_STRING, ID_TRAY_OPEN, w!("Abrir EyesLive"));
                        let _ = AppendMenuW(
                            hmenu,
                            MF_STRING,
                            ID_TRAY_HIDE,
                            w!("Ocultar para a Bandeja"),
                        );
                        let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
                        let _ = AppendMenuW(
                            hmenu,
                            MF_STRING,
                            ID_TRAY_EXIT,
                            w!("Encerrar EyesLive Completamente"),
                        );

                        let mut pt = POINT::default();
                        let _ = GetCursorPos(&mut pt);
                        let _ = SetForegroundWindow(hwnd);

                        let cmd = TrackPopupMenu(
                            hmenu,
                            TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RETURNCMD,
                            pt.x,
                            pt.y,
                            0,
                            hwnd,
                            None,
                        );
                        let _ = DestroyMenu(hmenu);

                        match cmd.0 as usize {
                            ID_TRAY_OPEN => {
                                restore_window();
                                TRAY_RESTORE_REQUESTED.store(true, Ordering::SeqCst);
                            }
                            ID_TRAY_HIDE => {
                                hide_window();
                                TRAY_HIDE_REQUESTED.store(true, Ordering::SeqCst);
                            }
                            ID_TRAY_EXIT => {
                                TRAY_QUIT_REQUESTED.store(true, Ordering::SeqCst);
                                restore_window();
                                std::process::exit(0);
                            }
                            _ => {}
                        }
                    }
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    pub fn spawn_tray_thread() {
        std::thread::spawn(|| unsafe {
            let Ok(hinstance) = GetModuleHandleW(PCWSTR::null()) else {
                return;
            };
            let class_name = w!("EyesLiveSystemTrayClass");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(tray_wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: class_name,
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);

            let Ok(hwnd) = CreateWindowExW(
                Default::default(),
                class_name,
                w!("EyesLiveTrayHelper"),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                hinstance,
                None,
            ) else {
                return;
            };

            let hicon = create_hicon().unwrap_or_default();

            let mut nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
                uCallbackMessage: WM_TRAYICON,
                hIcon: hicon,
                ..Default::default()
            };

            let tip = "EyesLive Remote Desktop — Em Segundo Plano (@gabrielyandev)";
            for (i, ch) in tip.encode_utf16().take(nid.szTip.len() - 1).enumerate() {
                nid.szTip[i] = ch;
            }

            let _ = Shell_NotifyIconW(NIM_ADD, &nid);

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            if !hicon.0.is_null() {
                let _ = DestroyIcon(hicon);
            }
        });
    }
}

pub fn init_system_tray() {
    #[cfg(target_os = "windows")]
    win32_tray::spawn_tray_thread();
}

pub fn hide_to_system_tray(ctx: &egui::Context) {
    #[cfg(target_os = "windows")]
    win32_tray::hide_window();

    #[cfg(not(target_os = "windows"))]
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));

    let _ = ctx;
}

pub fn restore_from_system_tray(ctx: &egui::Context) {
    #[cfg(target_os = "windows")]
    win32_tray::restore_window();

    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
}
