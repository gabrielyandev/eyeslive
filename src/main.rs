#![windows_subsystem = "windows"]

//! EyesLive — Native Rust Remote Desktop Application
//! Created & Signed by Gabriel Yan (@gabrielyandev)
//! GitHub: https://github.com/gabrielyandev/eyeslive
//!
//! Features:
//! - Zero account login & Zero session time limits
//! - Interactive Access Consent Modal on Host (Full Control / View-Only / Reject)
//! - Native System Installer, Windows Autostart & System Tray Background Mode
//! - Multi-tab simultaneous connections to multiple machines
//! - Direct IP:Port (LAN/VPN) + Integrated Cloud Relay & Optional Private Relay
//! - Real-time Screen Sharing, Mouse/Keyboard Injection, Clipboard Sync, File Transfer & Auto-Updater

mod host;
mod installer;
mod protocol;
mod relay;
mod tray;
mod updater;
mod viewer;

use eframe::egui::{
    self, Align2, Color32, Frame, Id, Margin, Order, RichText, Rounding, Stroke, Vec2,
};
use host::{
    start_host_listener, start_relay_registration_loop, ConsentDecision, HostConfig, PendingConsent,
};
use installer::{
    get_installed_exe_path, install_application, is_autostart_enabled, is_installed, set_autostart,
    uninstall_application,
};
use protocol::{
    MouseBtn, Packet, AUTHOR_NAME, AUTHOR_SIGNATURE, DEFAULT_HOST_PORT, DEFAULT_RELAY_PORT,
    DEFAULT_RELAY_SERVER, GITHUB_PROFILE_URL, GITHUB_REPO_URL,
};
use rand::Rng;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use updater::{
    apply_update_and_restart, check_for_updates_sync, download_update_sync, UpdateStatus,
    CURRENT_VERSION, GITHUB_RELEASES_PAGE,
};
use viewer::RemoteSession;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiThemeMode {
    DarkSlate,
    CleanLight,
}

#[derive(Debug, Clone, Copy)]
struct ThemePalette {
    bg_base: Color32,
    bg_panel: Color32,
    bg_card: Color32,
    bg_elevated: Color32,
    bg_input: Color32,
    bg_button: Color32,
    bg_button_hover: Color32,
    border_subtle: Color32,
    border_strong: Color32,
    text_primary: Color32,
    text_secondary: Color32,
    text_muted: Color32,
    accent_primary: Color32,
    accent_bright: Color32,
    status_emerald: Color32,
    status_emerald_text: Color32,
    status_amber: Color32,
    status_red: Color32,
    status_red_text: Color32,
}

impl ThemePalette {
    fn for_mode(mode: UiThemeMode) -> Self {
        match mode {
            UiThemeMode::DarkSlate => Self {
                // Soft, elevated slate-graphite palette with high WCAG contrast (never pitch-black)
                bg_base: Color32::from_rgb(24, 28, 37),
                bg_panel: Color32::from_rgb(31, 37, 49),
                bg_card: Color32::from_rgb(37, 44, 58),
                bg_elevated: Color32::from_rgb(46, 55, 72),
                bg_input: Color32::from_rgb(20, 24, 33),
                bg_button: Color32::from_rgb(52, 62, 82),
                bg_button_hover: Color32::from_rgb(66, 79, 104),
                border_subtle: Color32::from_rgb(64, 76, 99),
                border_strong: Color32::from_rgb(88, 104, 134),
                text_primary: Color32::from_rgb(248, 250, 252),
                text_secondary: Color32::from_rgb(219, 228, 240),
                text_muted: Color32::from_rgb(172, 186, 206),
                accent_primary: Color32::from_rgb(59, 130, 246),
                accent_bright: Color32::from_rgb(96, 175, 255),
                status_emerald: Color32::from_rgb(16, 185, 129),
                status_emerald_text: Color32::from_rgb(52, 211, 153),
                status_amber: Color32::from_rgb(251, 191, 36),
                status_red: Color32::from_rgb(220, 53, 69),
                status_red_text: Color32::from_rgb(248, 113, 113),
            },
            UiThemeMode::CleanLight => Self {
                // Crisp, modern desktop light palette
                bg_base: Color32::from_rgb(241, 245, 249),
                bg_panel: Color32::from_rgb(255, 255, 255),
                bg_card: Color32::from_rgb(255, 255, 255),
                bg_elevated: Color32::from_rgb(248, 250, 252),
                bg_input: Color32::from_rgb(255, 255, 255),
                bg_button: Color32::from_rgb(236, 242, 249),
                bg_button_hover: Color32::from_rgb(220, 230, 242),
                border_subtle: Color32::from_rgb(203, 213, 225),
                border_strong: Color32::from_rgb(148, 163, 184),
                text_primary: Color32::from_rgb(15, 23, 42),
                text_secondary: Color32::from_rgb(51, 65, 85),
                text_muted: Color32::from_rgb(100, 116, 139),
                accent_primary: Color32::from_rgb(37, 99, 235),
                accent_bright: Color32::from_rgb(2, 132, 199),
                status_emerald: Color32::from_rgb(5, 150, 105),
                status_emerald_text: Color32::from_rgb(4, 120, 87),
                status_amber: Color32::from_rgb(217, 119, 6),
                status_red: Color32::from_rgb(220, 38, 38),
                status_red_text: Color32::from_rgb(185, 28, 28),
            },
        }
    }
}

fn detect_local_ip() -> String {
    if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = sock.local_addr() {
                return addr.ip().to_string();
            }
        }
    }
    "127.0.0.1".into()
}

fn open_browser_url(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd").args(["/C", "start", "", url]).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(url).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
}

/// Applies the theme palette across ALL egui internal style slots (both Dark and Light)
/// so Windows OS theme notifications never corrupt widget backgrounds or text contrast.
fn apply_theme_to_context(ctx: &egui::Context, mode: UiThemeMode) {
    let p = ThemePalette::for_mode(mode);

    ctx.options_mut(|opt| {
        opt.theme_preference = match mode {
            UiThemeMode::DarkSlate => egui::ThemePreference::Dark,
            UiThemeMode::CleanLight => egui::ThemePreference::Light,
        };
    });

    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 7.0);
        style.spacing.window_margin = Margin::same(14.0);
        style.spacing.interact_size = Vec2::new(40.0, 26.0);

        let mut visuals = match mode {
            UiThemeMode::DarkSlate => egui::Visuals::dark(),
            UiThemeMode::CleanLight => egui::Visuals::light(),
        };

        visuals.dark_mode = mode == UiThemeMode::DarkSlate;
        visuals.panel_fill = p.bg_base;
        visuals.window_fill = p.bg_card;
        visuals.extreme_bg_color = p.bg_input;
        visuals.faint_bg_color = p.bg_elevated;
        visuals.code_bg_color = p.bg_input;
        visuals.override_text_color = Some(p.text_primary);

        let rounding = Rounding::same(6.0);

        visuals.widgets.noninteractive.bg_fill = p.bg_card;
        visuals.widgets.noninteractive.weak_bg_fill = p.bg_card;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border_subtle);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.2_f32, p.text_primary);
        visuals.widgets.noninteractive.rounding = rounding;

        visuals.widgets.inactive.bg_fill = p.bg_button;
        visuals.widgets.inactive.weak_bg_fill = p.bg_button;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, p.border_strong);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.3_f32, p.text_primary);
        visuals.widgets.inactive.rounding = rounding;

        visuals.widgets.hovered.bg_fill = p.bg_button_hover;
        visuals.widgets.hovered.weak_bg_fill = p.bg_button_hover;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.2_f32, p.accent_bright);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, p.text_primary);
        visuals.widgets.hovered.rounding = rounding;

        visuals.widgets.active.bg_fill = p.accent_primary;
        visuals.widgets.active.weak_bg_fill = p.accent_primary;
        visuals.widgets.active.bg_stroke = Stroke::new(1.2_f32, p.accent_bright);
        visuals.widgets.active.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
        visuals.widgets.active.rounding = rounding;

        visuals.widgets.open.bg_fill = p.bg_elevated;
        visuals.widgets.open.weak_bg_fill = p.bg_elevated;
        visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, p.accent_bright);
        visuals.widgets.open.fg_stroke = Stroke::new(1.3_f32, p.text_primary);
        visuals.widgets.open.rounding = rounding;

        visuals.selection.bg_fill = p.accent_primary;
        visuals.selection.stroke = Stroke::new(1.5_f32, Color32::WHITE);

        style.visuals = visuals;
    });
}

fn card_frame(p: &ThemePalette) -> Frame {
    Frame::none()
        .fill(p.bg_card)
        .stroke(Stroke::new(1.0_f32, p.border_subtle))
        .rounding(Rounding::same(10.0))
        .inner_margin(Margin::same(18.0))
}

fn pill_frame(fill: Color32, border: Color32) -> Frame {
    Frame::none()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, border))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::symmetric(12.0, 8.0))
}

fn draw_status_dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(9.0, 9.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
}

fn styled_button<'a>(
    label: &str,
    text_size: f32,
    text_color: Color32,
    fill_color: Color32,
    border_color: Color32,
) -> egui::Button<'a> {
    egui::Button::new(
        RichText::new(label)
            .size(text_size)
            .strong()
            .color(text_color),
    )
    .fill(fill_color)
    .stroke(Stroke::new(1.0_f32, border_color))
    .rounding(Rounding::same(6.0))
}

fn main() -> eframe::Result<()> {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }

    let args: Vec<String> = std::env::args().collect();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to initialize Tokio runtime");

    // Optional CLI mode: `eyeslive --relay 0.0.0.0:9091`
    if args.iter().any(|a| a == "--relay") {
        let bind = args
            .iter()
            .skip_while(|a| *a != "--relay")
            .nth(1)
            .cloned()
            .unwrap_or_else(|| format!("0.0.0.0:{}", DEFAULT_RELAY_PORT));
        rt.block_on(async {
            if let Err(e) = relay::run_relay_server(&bind).await {
                eprintln!("Relay error: {}", e);
            }
        });
        return Ok(());
    }

    // Optional CLI mode: `eyeslive --install`
    if args.iter().any(|a| a == "--install") {
        let _ = install_application(true);
    }

    let start_in_background = args.iter().any(|a| a == "--background");

    tray::init_system_tray();

    let viewport = egui::ViewportBuilder::default()
        .with_title(tray::MAIN_WINDOW_TITLE)
        .with_icon(std::sync::Arc::new(tray::build_egui_icon()))
        .with_inner_size([1180.0, 760.0])
        .with_min_inner_size([640.0, 480.0]);

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "EyesLive [@gabrielyandev]",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            let default_mode = UiThemeMode::DarkSlate;
            apply_theme_to_context(&cc.egui_ctx, default_mode);
            Ok(Box::new(EyesLiveApp::new(rt, start_in_background, default_mode)))
        }),
    )
}

struct EyesLiveApp {
    rt: tokio::runtime::Runtime,
    theme_mode: UiThemeMode,
    my_desk_id: String,
    local_ip: String,
    my_pin: Arc<Mutex<String>>,
    bound_port: Arc<Mutex<u16>>,
    host_status: Arc<Mutex<String>>,
    allow_control: Arc<Mutex<bool>>,
    require_consent: Arc<Mutex<bool>>,
    pending_consents: Arc<Mutex<Vec<PendingConsent>>>,
    active_incoming: Arc<Mutex<usize>>,
    last_consent_count: usize,

    // Installer & Background execution state
    installed_status: bool,
    autostart_enabled: bool,
    keep_in_background: bool,
    pending_initial_hide: bool,
    force_quit: bool,
    install_feedback: Option<(String, bool)>,

    // Connection form (Built-in relay is used transparently; custom relay is optional & hidden by default)
    target_input: String,
    target_pin: String,
    use_custom_relay: bool,
    custom_relay_input: String,
    relay_server: Arc<Mutex<String>>,
    relay_connected: Arc<Mutex<bool>>,

    // Multi-tab concurrent sessions (0 = Home Workspace, 1..N = Active Remote Sessions)
    sessions: Vec<RemoteSession>,
    active_tab: usize,
    next_session_id: usize,

    // Auto-update system
    update_status: Arc<Mutex<UpdateStatus>>,
    show_update_banner: bool,
}

impl EyesLiveApp {
    fn new(
        rt: tokio::runtime::Runtime,
        started_in_background: bool,
        theme_mode: UiThemeMode,
    ) -> Self {
        let mut rng = rand::thread_rng();
        let p1: u32 = rng.gen_range(100..999);
        let p2: u32 = rng.gen_range(100..999);
        let p3: u32 = rng.gen_range(100..999);
        let my_desk_id = format!("{}-{}-{}", p1, p2, p3);

        let default_relay = DEFAULT_RELAY_SERVER.to_string();
        let allow_control = Arc::new(Mutex::new(true));
        let require_consent = Arc::new(Mutex::new(true));
        let pending_consents = Arc::new(Mutex::new(Vec::new()));
        let active_incoming = Arc::new(Mutex::new(0usize));
        let my_pin = Arc::new(Mutex::new(String::new()));
        let bound_port = Arc::new(Mutex::new(DEFAULT_HOST_PORT));
        let relay_server = Arc::new(Mutex::new(default_relay));
        let relay_connected = Arc::new(Mutex::new(false));
        let host_status = Arc::new(Mutex::new(format!(
            "Inicializando porta {}...",
            DEFAULT_HOST_PORT
        )));

        let host_cfg = HostConfig {
            desk_id: my_desk_id.clone(),
            host_name: std::env::var("COMPUTERNAME").unwrap_or_else(|_| "EyesLive-PC".into()),
            pin: my_pin.clone(),
            allow_control: allow_control.clone(),
            require_consent: require_consent.clone(),
            pending_consents: pending_consents.clone(),
            active_clients: active_incoming.clone(),
            bound_port: bound_port.clone(),
            host_status: host_status.clone(),
            relay_server: relay_server.clone(),
            relay_connected: relay_connected.clone(),
            download_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        };

        let host_cfg_relay = host_cfg.clone();
        rt.spawn(async move {
            let _ = start_host_listener(DEFAULT_HOST_PORT, host_cfg).await;
        });
        rt.spawn(async move {
            start_relay_registration_loop(host_cfg_relay).await;
        });

        // Background update check on startup
        let update_status = Arc::new(Mutex::new(UpdateStatus::Idle));
        let checker_clone = update_status.clone();
        rt.spawn(async move {
            *checker_clone.lock().unwrap() = UpdateStatus::Checking;
            match tokio::task::spawn_blocking(check_for_updates_sync).await {
                Ok(Ok(Some(info))) => {
                    *checker_clone.lock().unwrap() = UpdateStatus::Available(info);
                }
                Ok(Ok(None)) => {
                    *checker_clone.lock().unwrap() = UpdateStatus::UpToDate;
                }
                Ok(Err(e)) => {
                    *checker_clone.lock().unwrap() = UpdateStatus::Error(e.to_string());
                }
                Err(e) => {
                    *checker_clone.lock().unwrap() = UpdateStatus::Error(e.to_string());
                }
            }
        });

        let installed_status = is_installed();
        let autostart_enabled = is_autostart_enabled();

        Self {
            rt,
            theme_mode,
            my_desk_id,
            local_ip: detect_local_ip(),
            my_pin,
            bound_port,
            host_status,
            allow_control,
            require_consent,
            pending_consents,
            active_incoming,
            last_consent_count: 0,
            installed_status,
            autostart_enabled,
            keep_in_background: true,
            pending_initial_hide: started_in_background,
            force_quit: false,
            install_feedback: if started_in_background {
                Some((
                    "Iniciado na Bandeja do Sistema (Host ativo em segundo plano).".into(),
                    true,
                ))
            } else {
                None
            },
            target_input: String::new(),
            target_pin: String::new(),
            use_custom_relay: false,
            custom_relay_input: String::new(),
            relay_server,
            relay_connected,
            sessions: Vec::new(),
            active_tab: 0,
            next_session_id: 1,
            update_status,
            show_update_banner: true,
        }
    }

    fn sync_effective_relay(&mut self) {
        let effective = if self.use_custom_relay {
            let trimmed = self.custom_relay_input.trim();
            if trimmed.is_empty() {
                DEFAULT_RELAY_SERVER.to_string()
            } else if trimmed.contains(':') {
                trimmed.to_string()
            } else {
                format!("{}:{}", trimmed, DEFAULT_RELAY_PORT)
            }
        } else {
            DEFAULT_RELAY_SERVER.to_string()
        };
        *self.relay_server.lock().unwrap() = effective;
    }

    fn trigger_manual_update_check(&self) {
        let checker = self.update_status.clone();
        self.rt.spawn(async move {
            *checker.lock().unwrap() = UpdateStatus::Checking;
            match tokio::task::spawn_blocking(check_for_updates_sync).await {
                Ok(Ok(Some(info))) => {
                    *checker.lock().unwrap() = UpdateStatus::Available(info);
                }
                Ok(Ok(None)) => {
                    *checker.lock().unwrap() = UpdateStatus::UpToDate;
                }
                Ok(Err(e)) => {
                    *checker.lock().unwrap() = UpdateStatus::Error(e.to_string());
                }
                Err(e) => {
                    *checker.lock().unwrap() = UpdateStatus::Error(e.to_string());
                }
            }
        });
    }

    fn spawn_new_session(&mut self) {
        let mut target = self.target_input.trim().to_string();
        if target.is_empty() {
            return;
        }
        if target == "127.0.0.1:7070" {
            let p = *self.bound_port.lock().unwrap();
            target = format!("127.0.0.1:{}", p);
            self.target_input = target.clone();
        }

        self.sync_effective_relay();
        let is_id_only = !target.contains('.') && !target.contains(':');
        let id = self.next_session_id;
        self.next_session_id += 1;

        let relay = if is_id_only || self.use_custom_relay {
            Some(self.relay_server.lock().unwrap().clone())
        } else {
            None
        };

        let pin = if self.target_pin.trim().is_empty() {
            None
        } else {
            Some(self.target_pin.trim().to_string())
        };

        let handle = self.rt.handle().clone();
        let session = RemoteSession::connect(id, target, relay, pin, &handle);
        self.sessions.push(session);
        self.active_tab = id;
    }

    fn render_consent_modal(&mut self, ctx: &egui::Context, p: &ThemePalette) {
        let mut guard = self.pending_consents.lock().unwrap();
        guard.retain(|req| req.requested_at.elapsed() < Duration::from_secs(60));
        let count = guard.len();

        if count > self.last_consent_count {
            tray::restore_from_system_tray(ctx);
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Critical,
            ));
        }
        self.last_consent_count = count;

        if guard.is_empty() {
            return;
        }

        let screen_rect = ctx.screen_rect();
        egui::Area::new(Id::new("consent_modal_backdrop"))
            .order(Order::Foreground)
            .fixed_pos(screen_rect.min)
            .show(ctx, |ui| {
                ui.painter().rect_filled(
                    screen_rect,
                    Rounding::ZERO,
                    Color32::from_black_alpha(180),
                );
            });

        let mut resolved_index: Option<(usize, ConsentDecision)> = None;

        if let Some(req) = guard.first() {
            let remaining_secs = 60u64.saturating_sub(req.requested_at.elapsed().as_secs());
            let modal_w = (screen_rect.width() - 40.0).clamp(320.0, 500.0);

            egui::Window::new("SOLICITAÇÃO DE ACESSO REMOTO")
                .order(Order::Tooltip)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .default_width(modal_w)
                .max_width(modal_w)
                .frame(
                    Frame::none()
                        .fill(p.bg_card)
                        .stroke(Stroke::new(2.0_f32, p.accent_primary))
                        .rounding(Rounding::same(12.0))
                        .inner_margin(Margin::same(22.0)),
                )
                .show(ctx, |ui| {
                    ui.set_width(modal_w - 44.0);
                    ui.horizontal_wrapped(|ui| {
                        draw_status_dot(ui, p.status_amber);
                        ui.label(
                            RichText::new("AUTORIZAÇÃO DE SEGURANÇA NECESSÁRIA")
                                .size(13.0)
                                .strong()
                                .color(p.status_amber),
                        );
                    });
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "Um dispositivo remoto está solicitando permissão para acessar este computador:",
                        )
                        .size(13.5)
                        .color(p.text_primary),
                    );

                    ui.add_space(10.0);
                    pill_frame(p.bg_elevated, p.border_subtle).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.vertical(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new("DISPOSITIVO:")
                                        .size(12.0)
                                        .strong()
                                        .color(p.text_muted),
                                );
                                ui.label(
                                    RichText::new(&req.client_name)
                                        .size(14.0)
                                        .strong()
                                        .color(p.text_primary)
                                        .monospace(),
                                );
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new("ORIGEM:")
                                        .size(12.0)
                                        .strong()
                                        .color(p.text_muted),
                                );
                                ui.label(
                                    RichText::new(&req.peer_addr)
                                        .size(13.5)
                                        .strong()
                                        .color(p.accent_bright)
                                        .monospace(),
                                );
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new("TEMPO RESTANTE:")
                                        .size(12.0)
                                        .strong()
                                        .color(p.text_muted),
                                );
                                ui.label(
                                    RichText::new(format!("{} segundos", remaining_secs))
                                        .size(13.0)
                                        .strong()
                                        .color(p.status_amber),
                                );
                            });
                        });
                    });

                    ui.add_space(14.0);

                    let w = ui.available_width();
                    let full_btn = styled_button(
                        "PERMITIR ACESSO COMPLETO (TELA + CONTROLE)",
                        12.5,
                        Color32::WHITE,
                        p.status_emerald,
                        p.status_emerald,
                    )
                    .min_size(Vec2::new(w, 38.0));

                    if ui.add(full_btn).clicked() {
                        resolved_index = Some((0, ConsentDecision::AllowFullControl));
                    }

                    ui.add_space(6.0);

                    let view_btn = styled_button(
                        "PERMITIR SOMENTE VISUALIZAÇÃO",
                        12.5,
                        Color32::WHITE,
                        p.accent_primary,
                        p.accent_primary,
                    )
                    .min_size(Vec2::new(w, 36.0));

                    if ui.add(view_btn).clicked() {
                        resolved_index = Some((0, ConsentDecision::AllowViewOnly));
                    }

                    ui.add_space(6.0);

                    let reject_btn = styled_button(
                        "RECUSAR CONEXÃO",
                        12.5,
                        Color32::WHITE,
                        p.status_red,
                        p.status_red,
                    )
                    .min_size(Vec2::new(w, 36.0));

                    if ui.add(reject_btn).clicked() {
                        resolved_index = Some((0, ConsentDecision::Reject));
                    }
                });
        }

        if let Some((idx, decision)) = resolved_index {
            if idx < guard.len() {
                let mut item = guard.remove(idx);
                if let Some(tx) = item.response_tx.take() {
                    let _ = tx.send(decision);
                }
            }
            self.last_consent_count = guard.len();
        }
    }

    fn render_host_card(&mut self, ui: &mut egui::Ui, lan_endpoint: &str, p: &ThemePalette) {
        card_frame(p).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                draw_status_dot(ui, p.status_emerald);
                ui.label(
                    RichText::new("ESTE DISPOSITIVO  •  RECEBER ACESSO (HOST)")
                        .size(13.0)
                        .strong()
                        .color(p.accent_bright),
                );
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Compartilhe seu EyesLive ID ou IP direto para permitir conexões remotas sem limite de tempo.",
                )
                .size(13.5)
                .color(p.text_secondary),
            );
            ui.add_space(12.0);

            // Desk ID Box
            let relay_ok = *self.relay_connected.lock().unwrap();
            pill_frame(p.bg_elevated, p.border_subtle).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new("SEU EYESLIVE ID")
                                    .size(11.5)
                                    .strong()
                                    .color(p.text_muted),
                            );
                            draw_status_dot(
                                ui,
                                if relay_ok {
                                    p.status_emerald
                                } else {
                                    p.status_amber
                                },
                            );
                            ui.label(
                                RichText::new(if relay_ok {
                                    "Online para acesso externo"
                                } else {
                                    "Sincronizando rede..."
                                })
                                .size(11.5)
                                .strong()
                                .color(if relay_ok {
                                    p.status_emerald_text
                                } else {
                                    p.status_amber
                                }),
                            );
                        });
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(&self.my_desk_id)
                                .size(26.0)
                                .strong()
                                .color(p.text_primary)
                                .monospace(),
                        );
                    });
                    ui.add_space(10.0);
                    let copy_id_btn = styled_button(
                        "COPIAR ID",
                        12.0,
                        p.text_primary,
                        p.bg_button,
                        p.border_strong,
                    );
                    if ui.add(copy_id_btn).clicked() {
                        if let Ok(mut cb) = arboard::Clipboard::new() {
                            let _ = cb.set_text(self.my_desk_id.clone());
                        }
                    }
                });
            });

            ui.add_space(10.0);

            // LAN Endpoint Box
            pill_frame(p.bg_elevated, p.border_subtle).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("ENDEREÇO DIRETO (REDE LOCAL / VPN / TAILSCALE)")
                                .size(11.5)
                                .strong()
                                .color(p.text_muted),
                        );
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(lan_endpoint)
                                .size(18.0)
                                .strong()
                                .color(p.accent_bright)
                                .monospace(),
                        );
                    });
                    ui.add_space(10.0);
                    let copy_ip_btn = styled_button(
                        "COPIAR IP",
                        12.0,
                        p.text_primary,
                        p.bg_button,
                        p.border_strong,
                    );
                    if ui.add(copy_ip_btn).clicked() {
                        if let Ok(mut cb) = arboard::Clipboard::new() {
                            let _ = cb.set_text(lan_endpoint.to_string());
                        }
                    }
                });
            });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(6.0);

            let mut req_consent = *self.require_consent.lock().unwrap();
            if ui
                .checkbox(
                    &mut req_consent,
                    RichText::new("Solicitar autorização na tela ao receber conexão")
                        .size(13.5)
                        .strong()
                        .color(p.text_primary),
                )
                .changed()
            {
                *self.require_consent.lock().unwrap() = req_consent;
            }

            let mut allow = *self.allow_control.lock().unwrap();
            if ui
                .checkbox(
                    &mut allow,
                    RichText::new("Permitir controle remoto de mouse e teclado")
                        .size(13.5)
                        .color(p.text_primary),
                )
                .changed()
            {
                *self.allow_control.lock().unwrap() = allow;
            }

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("PIN de Segurança (opcional):")
                        .size(13.0)
                        .strong()
                        .color(p.text_secondary),
                );
                let mut pin_guard = self.my_pin.lock().unwrap();
                ui.add(
                    egui::TextEdit::singleline(&mut *pin_guard)
                        .desired_width(150.0)
                        .text_color(p.text_primary)
                        .hint_text("Sem PIN"),
                );
            });
        });
    }

    fn render_client_card(&mut self, ui: &mut egui::Ui, p: &ThemePalette) {
        card_frame(p).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                draw_status_dot(ui, p.accent_primary);
                ui.label(
                    RichText::new("NOVA CONEXÃO  •  ACESSAR OUTRO COMPUTADOR")
                        .size(13.0)
                        .strong()
                        .color(p.accent_bright),
                );
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Digite o EyesLive ID ou o IP:Porta do computador remoto para iniciar uma sessão em nova aba.",
                )
                .size(13.5)
                .color(p.text_secondary),
            );
            ui.add_space(14.0);

            let field_w = ui.available_width();

            ui.label(
                RichText::new("DESTINO REMOTO (EYESLIVE ID OU IP:PORTA)")
                    .size(12.0)
                    .strong()
                    .color(p.text_secondary),
            );
            ui.add_space(2.0);
            let resp_target = ui.add(
                egui::TextEdit::singleline(&mut self.target_input)
                    .desired_width(field_w)
                    .font(egui::TextStyle::Monospace)
                    .text_color(p.text_primary)
                    .margin(Margin::symmetric(10.0, 8.0))
                    .hint_text("Ex: 462-152-868 ou 192.168.1.50:9090"),
            );
            if resp_target.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.spawn_new_session();
            }

            ui.add_space(10.0);

            ui.label(
                RichText::new("PIN DE AUTENTICAÇÃO (OPCIONAL)")
                    .size(12.0)
                    .strong()
                    .color(p.text_secondary),
            );
            ui.add_space(2.0);
            let resp_pin = ui.add(
                egui::TextEdit::singleline(&mut self.target_pin)
                    .desired_width(field_w)
                    .password(true)
                    .text_color(p.text_primary)
                    .margin(Margin::symmetric(10.0, 8.0))
                    .hint_text("Deixe vazio se o computador remoto não exigir PIN"),
            );
            if resp_pin.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.spawn_new_session();
            }

            ui.add_space(10.0);

            // Optional private self-hosted relay toggle (built-in cloud relay is used automatically and never exposes its IP)
            if ui
                .checkbox(
                    &mut self.use_custom_relay,
                    RichText::new("Usar servidor Relay privado próprio (avançado)")
                        .size(13.0)
                        .color(p.text_secondary),
                )
                .changed()
            {
                self.sync_effective_relay();
            }

            if self.use_custom_relay {
                ui.add_space(4.0);
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut self.custom_relay_input)
                            .desired_width(field_w)
                            .font(egui::TextStyle::Monospace)
                            .text_color(p.text_primary)
                            .margin(Margin::symmetric(10.0, 8.0))
                            .hint_text("Ex: relay.suaempresa.com:9091"),
                    )
                    .changed()
                {
                    self.sync_effective_relay();
                }
            }

            ui.add_space(16.0);
            let btn = styled_button(
                "CONECTAR E INICIAR SESSÃO REMOTA",
                14.0,
                Color32::WHITE,
                p.accent_primary,
                p.accent_bright,
            )
            .min_size(Vec2::new(ui.available_width(), 42.0));

            if ui.add(btn).clicked() {
                self.spawn_new_session();
            }
        });
    }
}

impl eframe::App for EyesLiveApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Enforce our custom palette on every frame so OS theme changes never corrupt widget colors
        apply_theme_to_context(ctx, self.theme_mode);
        let p = ThemePalette::for_mode(self.theme_mode);

        if self.pending_initial_hide {
            self.pending_initial_hide = false;
            tray::hide_to_system_tray(ctx);
        }

        if tray::take_restore_request() {
            tray::restore_from_system_tray(ctx);
        }
        if tray::take_hide_request() {
            tray::hide_to_system_tray(ctx);
        }
        if tray::take_quit_request() {
            self.force_quit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Intercept window close (X) or minimize (_) to send directly to the Windows System Tray
        let close_req = ctx.input(|i| i.viewport().close_requested());
        let min_req = ctx.input(|i| i.viewport().minimized == Some(true));
        if (close_req || min_req) && self.keep_in_background && !self.force_quit {
            if close_req {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            tray::hide_to_system_tray(ctx);
        }

        ctx.request_repaint_after(Duration::from_millis(16));

        // Render incoming connection authorization modal if any client is waiting
        self.render_consent_modal(ctx, &p);

        // Auto-update banner notification
        let cur_update = self.update_status.lock().unwrap().clone();
        match cur_update {
            UpdateStatus::Available(ref info) if self.show_update_banner => {
                egui::TopBottomPanel::top("update_notification_banner")
                    .frame(
                        Frame::none()
                            .fill(p.bg_card)
                            .stroke(Stroke::new(1.0_f32, p.status_amber))
                            .inner_margin(Margin::symmetric(16.0, 8.0)),
                    )
                    .show(ctx, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            draw_status_dot(ui, p.status_amber);
                            ui.label(
                                RichText::new(format!(
                                    "NOVA VERSÃO DISPONÍVEL: {} (ATUAL: v{})",
                                    info.latest_version, CURRENT_VERSION
                                ))
                                .color(p.status_amber)
                                .size(12.5)
                                .strong(),
                            );

                            if let Some(exe_url) = info.exe_download_url.clone() {
                                let status_clone = self.update_status.clone();
                                let rt_handle = self.rt.handle().clone();
                                let dl_btn = styled_button(
                                    "BAIXAR E ATUALIZAR (.EXE)",
                                    12.0,
                                    Color32::WHITE,
                                    p.status_emerald,
                                    p.status_emerald,
                                );
                                if ui.add(dl_btn).clicked() {
                                    rt_handle.spawn(async move {
                                        let _ = tokio::task::spawn_blocking(move || {
                                            download_update_sync(&exe_url, status_clone)
                                        })
                                        .await;
                                    });
                                }
                            }

                            let notes_btn = styled_button(
                                "NOTAS DA VERSÃO",
                                11.5,
                                p.text_primary,
                                p.bg_button,
                                p.border_strong,
                            );
                            if ui.add(notes_btn).clicked() {
                                open_browser_url(&info.release_url);
                            }

                            let close_btn = styled_button(
                                "FECHAR",
                                11.5,
                                p.text_secondary,
                                p.bg_button,
                                p.border_subtle,
                            );
                            if ui.add(close_btn).clicked() {
                                self.show_update_banner = false;
                            }
                        });
                    });
            }
            UpdateStatus::Downloading(progress) => {
                egui::TopBottomPanel::top("download_progress_banner")
                    .frame(
                        Frame::none()
                            .fill(p.bg_card)
                            .stroke(Stroke::new(1.0_f32, p.accent_primary))
                            .inner_margin(Margin::symmetric(16.0, 8.0)),
                    )
                    .show(ctx, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spinner();
                            ui.label(
                                RichText::new(format!(
                                    "BAIXANDO ATUALIZAÇÃO: {:.0}%",
                                    progress * 100.0
                                ))
                                .size(12.5)
                                .strong()
                                .color(p.text_primary),
                            );
                            ui.add(egui::ProgressBar::new(progress).desired_width(180.0));
                        });
                    });
            }
            UpdateStatus::ReadyToRestart(ref new_exe) => {
                let exe_to_run = new_exe.clone();
                egui::TopBottomPanel::top("ready_restart_banner")
                    .frame(
                        Frame::none()
                            .fill(p.bg_card)
                            .stroke(Stroke::new(1.0_f32, p.status_emerald))
                            .inner_margin(Margin::symmetric(16.0, 8.0)),
                    )
                    .show(ctx, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            draw_status_dot(ui, p.status_emerald);
                            ui.label(
                                RichText::new("ATUALIZAÇÃO PRONTA PARA APLICAR")
                                    .color(p.status_emerald_text)
                                    .size(12.5)
                                    .strong(),
                            );
                            let restart_btn = styled_button(
                                "REINICIAR AGORA",
                                12.0,
                                Color32::WHITE,
                                p.status_emerald,
                                p.status_emerald,
                            );
                            if ui.add(restart_btn).clicked() {
                                let _ = apply_update_and_restart(&exe_to_run);
                            }
                        });
                    });
            }
            _ => {}
        }

        // Responsive Top Navigation Header
        egui::TopBottomPanel::top("top_nav_bar")
            .frame(
                Frame::none()
                    .fill(p.bg_panel)
                    .stroke(Stroke::new(1.0_f32, p.border_subtle))
                    .inner_margin(Margin::symmetric(18.0, 10.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    draw_status_dot(ui, p.accent_primary);
                    ui.label(
                        RichText::new("EYESLIVE")
                            .size(18.0)
                            .strong()
                            .color(p.text_primary),
                    );
                    let author_btn = styled_button(
                        &format!("{} ({})", AUTHOR_NAME, AUTHOR_SIGNATURE),
                        12.0,
                        p.accent_bright,
                        p.bg_elevated,
                        p.border_subtle,
                    );
                    if ui
                        .add(author_btn)
                        .on_hover_text(format!("Abrir GitHub: {}", GITHUB_PROFILE_URL))
                        .clicked()
                    {
                        open_browser_url(GITHUB_PROFILE_URL);
                    }

                    ui.separator();

                    // Workspace / Dashboard Tab
                    let home_selected = self.active_tab == 0;
                    let home_btn = styled_button(
                        "PAINEL PRINCIPAL",
                        12.5,
                        if home_selected {
                            Color32::WHITE
                        } else {
                            p.text_primary
                        },
                        if home_selected {
                            p.accent_primary
                        } else {
                            p.bg_button
                        },
                        if home_selected {
                            p.accent_bright
                        } else {
                            p.border_strong
                        },
                    );
                    if ui.add(home_btn).clicked() {
                        self.active_tab = 0;
                    }

                    // Active Remote Session Tabs
                    let mut close_id = None;
                    for s in &self.sessions {
                        let st = s.state.lock().unwrap();
                        let is_online = st.connected;
                        let label = format!("SESSÃO #{}: {}", s.id, st.host_name);
                        drop(st);

                        let is_active = self.active_tab == s.id;
                        ui.horizontal(|ui| {
                            draw_status_dot(
                                ui,
                                if is_online {
                                    p.status_emerald
                                } else {
                                    p.status_amber
                                },
                            );
                            let tab_btn = styled_button(
                                &label,
                                12.0,
                                if is_active {
                                    Color32::WHITE
                                } else {
                                    p.text_primary
                                },
                                if is_active {
                                    p.accent_primary
                                } else {
                                    p.bg_button
                                },
                                if is_active {
                                    p.accent_bright
                                } else {
                                    p.border_strong
                                },
                            );
                            if ui.add(tab_btn).clicked() {
                                self.active_tab = s.id;
                            }
                            let close_tab_btn = styled_button(
                                "X",
                                11.5,
                                Color32::WHITE,
                                p.status_red,
                                p.status_red,
                            );
                            if ui
                                .add(close_tab_btn)
                                .on_hover_text("Encerrar sessão")
                                .clicked()
                            {
                                close_id = Some(s.id);
                            }
                        });
                    }

                    if let Some(cid) = close_id {
                        self.sessions.retain(|s| s.id != cid);
                        if self.active_tab == cid {
                            self.active_tab = 0;
                        }
                    }

                    ui.separator();

                    // Theme Switcher (Dark Slate vs Clean Light)
                    let theme_label = match self.theme_mode {
                        UiThemeMode::DarkSlate => "TEMA: ESCURO SUAVE (MUDAR P/ CLARO)",
                        UiThemeMode::CleanLight => "TEMA: CLARO (MUDAR P/ ESCURO)",
                    };
                    let theme_btn = styled_button(
                        theme_label,
                        11.5,
                        p.text_primary,
                        p.bg_button,
                        p.border_strong,
                    );
                    if ui
                        .add(theme_btn)
                        .on_hover_text("Alternar entre Modo Escuro Suave e Modo Claro")
                        .clicked()
                    {
                        self.theme_mode = match self.theme_mode {
                            UiThemeMode::DarkSlate => UiThemeMode::CleanLight,
                            UiThemeMode::CleanLight => UiThemeMode::DarkSlate,
                        };
                        apply_theme_to_context(ctx, self.theme_mode);
                    }
                });
            });

        // Responsive Bottom Status Bar
        egui::TopBottomPanel::bottom("bottom_status_bar")
            .frame(
                Frame::none()
                    .fill(p.bg_panel)
                    .stroke(Stroke::new(1.0_f32, p.border_subtle))
                    .inner_margin(Margin::symmetric(18.0, 8.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let inc = *self.active_incoming.lock().unwrap();
                    let hstatus = self.host_status.lock().unwrap().clone();
                    let port = *self.bound_port.lock().unwrap();

                    draw_status_dot(ui, p.status_emerald);
                    ui.label(
                        RichText::new(format!(
                            "HOST: {} ({}:{})  |  CLIENTES ATIVOS: {}  |  ABAS: {}",
                            hstatus.to_uppercase(),
                            self.local_ip,
                            port,
                            inc,
                            self.sessions.len()
                        ))
                        .size(12.0)
                        .strong()
                        .color(p.text_secondary)
                        .monospace(),
                    );

                    ui.separator();

                    let cur_up = self.update_status.lock().unwrap().clone();
                    match cur_up {
                        UpdateStatus::Checking => {
                            ui.spinner();
                            ui.label(
                                RichText::new("VERIFICANDO ATUALIZAÇÕES...")
                                    .size(11.5)
                                    .color(p.text_secondary),
                            );
                        }
                        UpdateStatus::Available(ref info) => {
                            let avail_btn = styled_button(
                                &format!("ATUALIZAÇÃO {} DISPONÍVEL", info.latest_version),
                                11.5,
                                Color32::BLACK,
                                p.status_amber,
                                p.status_amber,
                            );
                            if ui.add(avail_btn).clicked() {
                                self.show_update_banner = true;
                            }
                        }
                        UpdateStatus::UpToDate => {
                            let up_btn = styled_button(
                                &format!("v{} ATUALIZADO", CURRENT_VERSION),
                                11.5,
                                p.status_emerald_text,
                                p.bg_elevated,
                                p.status_emerald,
                            );
                            if ui
                                .add(up_btn)
                                .on_hover_text("Clique para verificar novamente")
                                .clicked()
                            {
                                self.trigger_manual_update_check();
                            }
                        }
                        UpdateStatus::Downloading(prog) => {
                            ui.label(
                                RichText::new(format!("BAIXANDO: {:.0}%", prog * 100.0))
                                    .size(11.5)
                                    .strong()
                                    .color(p.accent_bright),
                            );
                        }
                        UpdateStatus::ReadyToRestart(_) => {
                            ui.label(
                                RichText::new("PRONTO PARA REINICIAR")
                                    .size(11.5)
                                    .strong()
                                    .color(p.status_emerald_text),
                            );
                        }
                        _ => {
                            let chk_btn = styled_button(
                                &format!("v{} VERIFICAR UPDATE", CURRENT_VERSION),
                                11.5,
                                p.text_primary,
                                p.bg_button,
                                p.border_strong,
                            );
                            if ui.add(chk_btn).clicked() {
                                self.trigger_manual_update_check();
                            }
                        }
                    }

                    ui.separator();

                    let gh_btn = styled_button(
                        "GITHUB @GABRIELYANDEV",
                        11.5,
                        p.accent_bright,
                        p.bg_elevated,
                        p.border_strong,
                    );
                    if ui.add(gh_btn).on_hover_text(GITHUB_PROFILE_URL).clicked() {
                        open_browser_url(GITHUB_PROFILE_URL);
                    }

                    let repo_btn = styled_button(
                        "REPOSITÓRIO",
                        11.5,
                        p.text_primary,
                        p.bg_button,
                        p.border_strong,
                    );
                    if ui.add(repo_btn).on_hover_text(GITHUB_REPO_URL).clicked() {
                        open_browser_url(GITHUB_REPO_URL);
                    }

                    let quit_btn = styled_button(
                        "ENCERRAR PROCESSO",
                        11.5,
                        Color32::WHITE,
                        p.status_red,
                        p.status_red,
                    );
                    if ui
                        .add(quit_btn)
                        .on_hover_text("Fechar o EyesLive completamente (parar segundo plano)")
                        .clicked()
                    {
                        self.force_quit = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });

        // Central Workspace
        egui::CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(p.bg_base)
                    .inner_margin(Margin::symmetric(20.0, 16.0)),
            )
            .show(ctx, |ui| {
                if self.active_tab == 0 {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            self.render_dashboard(ctx, ui, &p);
                        });
                } else {
                    let rt_handle = self.rt.handle().clone();
                    if let Some(session) =
                        self.sessions.iter_mut().find(|s| s.id == self.active_tab)
                    {
                        Self::render_remote_session(ctx, ui, session, &rt_handle, &p);
                    } else {
                        self.active_tab = 0;
                    }
                }
            });
    }
}

impl EyesLiveApp {
    fn render_dashboard(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, p: &ThemePalette) {
        let cur_port = *self.bound_port.lock().unwrap();
        let lan_endpoint = format!("{}:{}", self.local_ip, cur_port);
        let avail_w = ui.available_width();

        // Header Row
        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("ACESSO REMOTO DIRETO E POR ID  •  SEM LOGIN  •  TEMPO ILIMITADO")
                        .size(12.0)
                        .strong()
                        .color(p.accent_bright),
                );
                ui.label(
                    RichText::new("EyesLive Remote Desktop")
                        .size(24.0)
                        .strong()
                        .color(p.text_primary),
                );
            });
        });

        ui.add_space(14.0);

        // Responsive Breakpoint: 2 columns on wide screens (>= 820px), stacked 1-column on narrower windows
        if avail_w >= 820.0 {
            ui.columns(2, |cols| {
                self.render_host_card(&mut cols[0], &lan_endpoint, p);
                self.render_client_card(&mut cols[1], p);
            });
        } else {
            self.render_host_card(ui, &lan_endpoint, p);
            ui.add_space(14.0);
            self.render_client_card(ui, p);
        }

        ui.add_space(14.0);

        // System Installation & Background Service Card
        card_frame(p).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                draw_status_dot(
                    ui,
                    if self.installed_status {
                        p.status_emerald
                    } else {
                        p.accent_primary
                    },
                );
                ui.label(
                    RichText::new("INSTALAÇÃO NO SISTEMA E EXECUÇÃO EM SEGUNDO PLANO")
                        .size(13.0)
                        .strong()
                        .color(p.accent_bright),
                );
                ui.separator();
                ui.label(
                    RichText::new(if self.installed_status {
                        format!("INSTALADO ({})", get_installed_exe_path().display())
                    } else {
                        "MODO PORTÁTIL (NÃO INSTALADO)".to_string()
                    })
                    .size(12.0)
                    .strong()
                    .color(if self.installed_status {
                        p.status_emerald_text
                    } else {
                        p.text_muted
                    }),
                );
            });

            ui.add_space(10.0);

            ui.horizontal_wrapped(|ui| {
                let install_label = if self.installed_status {
                    "ATUALIZAR INSTALAÇÃO LOCAL E ATALHOS"
                } else {
                    "INSTALAR EYESLIVE E ATIVAR SEGUNDO PLANO"
                };

                let install_btn = styled_button(
                    install_label,
                    12.5,
                    Color32::WHITE,
                    p.status_emerald,
                    p.status_emerald,
                );

                if ui
                    .add(install_btn)
                    .on_hover_text(
                        "Copia o executável para %LOCALAPPDATA%\\EyesLive, cria atalhos na Área de Trabalho e Menu Iniciar e configura execução em segundo plano",
                    )
                    .clicked()
                {
                    match install_application(true) {
                        Ok(path) => {
                            self.installed_status = true;
                            self.autostart_enabled = true;
                            self.keep_in_background = true;
                            self.install_feedback =
                                Some((format!("Instalado com sucesso em: {}", path), true));
                        }
                        Err(e) => {
                            self.install_feedback =
                                Some((format!("Erro ao instalar: {}", e), false));
                        }
                    }
                }

                let tray_btn = styled_button(
                    "OCULTAR PARA A BANDEJA DO SISTEMA AGORA",
                    12.5,
                    p.text_primary,
                    p.bg_button,
                    p.border_strong,
                );
                if ui
                    .add(tray_btn)
                    .on_hover_text(
                        "Remove da barra de tarefas e mantém na Bandeja do Sistema (perto do relógio); clique no ícone da bandeja para reabrir",
                    )
                    .clicked()
                {
                    tray::hide_to_system_tray(ctx);
                }

                if self.installed_status {
                    let uninst_btn = styled_button(
                        "DESINSTALAR DO SISTEMA",
                        12.5,
                        Color32::WHITE,
                        p.status_red,
                        p.status_red,
                    );
                    if ui.add(uninst_btn).clicked() {
                        let _ = uninstall_application();
                        self.installed_status = is_installed();
                        self.autostart_enabled = is_autostart_enabled();
                        self.install_feedback = Some((
                            "Atalhos e inicialização automática removidos.".into(),
                            false,
                        ));
                    }
                }
            });

            ui.add_space(8.0);

            ui.vertical(|ui| {
                if ui
                    .checkbox(
                        &mut self.autostart_enabled,
                        RichText::new(
                            "Iniciar automaticamente em segundo plano junto com o Windows",
                        )
                        .size(13.5)
                        .color(p.text_primary),
                    )
                    .changed()
                {
                    let _ = set_autostart(self.autostart_enabled);
                }

                ui.checkbox(
                    &mut self.keep_in_background,
                    RichText::new(
                        "Continuar rodando na Bandeja do Sistema ao fechar ou minimizar a janela",
                    )
                    .size(13.5)
                    .color(p.text_primary),
                );
            });

            if let Some((ref msg, ok)) = self.install_feedback {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(msg)
                        .size(12.5)
                        .strong()
                        .color(if ok {
                            p.status_emerald_text
                        } else {
                            p.status_amber
                        }),
                );
            }
        });

        ui.add_space(14.0);

        // Release, Updates & Credits Card
        card_frame(p).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                draw_status_dot(ui, p.accent_bright);
                ui.label(
                    RichText::new(format!(
                        "EYESLIVE v{}  •  DESENVOLVIDO POR {} ({})",
                        CURRENT_VERSION,
                        AUTHOR_NAME.to_uppercase(),
                        AUTHOR_SIGNATURE
                    ))
                    .size(13.0)
                    .strong()
                    .color(p.text_primary),
                );
            });
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                let upd_btn = styled_button(
                    "VERIFICAR ATUALIZAÇÕES",
                    12.0,
                    p.text_primary,
                    p.bg_button,
                    p.border_strong,
                );
                if ui.add(upd_btn).clicked() {
                    self.trigger_manual_update_check();
                }

                let dl_exe_btn = styled_button(
                    "BAIXAR EXECUTÁVEL PORTÁTIL (.EXE)",
                    12.0,
                    Color32::WHITE,
                    p.accent_primary,
                    p.accent_bright,
                );
                if ui.add(dl_exe_btn).clicked() {
                    open_browser_url(GITHUB_RELEASES_PAGE);
                }

                let prof_btn = styled_button(
                    "PERFIL GITHUB @GABRIELYANDEV",
                    12.0,
                    p.text_primary,
                    p.bg_button,
                    p.border_strong,
                );
                if ui.add(prof_btn).clicked() {
                    open_browser_url(GITHUB_PROFILE_URL);
                }

                let rep_btn = styled_button(
                    "REPOSITÓRIO OFICIAL NO GITHUB",
                    12.0,
                    p.text_primary,
                    p.bg_button,
                    p.border_strong,
                );
                if ui.add(rep_btn).clicked() {
                    open_browser_url(GITHUB_REPO_URL);
                }
            });
        });
    }

    fn render_remote_session(
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        session: &mut RemoteSession,
        rt: &tokio::runtime::Handle,
        p: &ThemePalette,
    ) {
        let (connected, status, remote_w, remote_h, remote_clip, file_ack, frames_rx, ping_rtt) = {
            let mut st = session.state.lock().unwrap();
            if st.frame_dirty {
                if let Some(img) = st.latest_frame.take() {
                    if let Some(tex) = &mut session.texture {
                        tex.set(img, egui::TextureOptions::LINEAR);
                    } else {
                        session.texture = Some(ctx.load_texture(
                            format!("remote_screen_{}", session.id),
                            img,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                }
                st.frame_dirty = false;
            }
            (
                st.connected,
                st.status_text.clone(),
                st.remote_width,
                st.remote_height,
                st.remote_clipboard.clone(),
                st.last_file_ack.clone(),
                st.frames_received,
                st.ping_rtt_ms,
            )
        };

        // Automatic periodic keepalive ping to gauge latency
        if session.last_ping_sent.elapsed() >= Duration::from_millis(1500) {
            session.last_ping_sent = Instant::now();
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let _ = session.tx.send(Packet::Ping(now_ms));
        }

        // Responsive Session Control Bar
        pill_frame(p.bg_card, p.border_subtle).show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Row 1: Connection Status, Ping, Actions
            ui.horizontal_wrapped(|ui| {
                draw_status_dot(
                    ui,
                    if connected {
                        p.status_emerald
                    } else {
                        p.status_amber
                    },
                );
                ui.label(
                    RichText::new(&status)
                        .size(13.0)
                        .strong()
                        .color(p.text_primary),
                );

                if let Some(rtt) = ping_rtt {
                    let color = if rtt < 40 {
                        p.status_emerald_text
                    } else if rtt < 100 {
                        p.status_amber
                    } else {
                        p.status_red_text
                    };
                    ui.separator();
                    draw_status_dot(ui, color);
                    ui.label(
                        RichText::new(format!("PING: {}ms", rtt))
                            .size(12.0)
                            .color(color)
                            .strong()
                            .monospace(),
                    );
                }

                ui.label(
                    RichText::new(format!(
                        "|  {}x{}  |  QUADROS: {}",
                        remote_w, remote_h, frames_rx
                    ))
                    .size(12.0)
                    .color(p.text_secondary)
                    .monospace(),
                );

                ui.separator();

                let send_file_btn = styled_button(
                    "ENVIAR ARQUIVO",
                    11.5,
                    p.text_primary,
                    p.bg_button,
                    p.border_strong,
                );
                if ui.add(send_file_btn).clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        session.send_file(path, rt);
                    }
                }

                let clip_btn = styled_button(
                    "SINCRONIZAR CLIPBOARD",
                    11.5,
                    p.text_primary,
                    p.bg_button,
                    p.border_strong,
                );
                if ui.add(clip_btn).clicked() {
                    if let Ok(mut cb) = arboard::Clipboard::new() {
                        if let Ok(txt) = cb.get_text() {
                            let _ = session.tx.send(Packet::ClipboardText { text: txt });
                        }
                    }
                }

                if let Some(ack) = file_ack {
                    ui.label(
                        RichText::new(ack)
                            .size(12.0)
                            .strong()
                            .color(p.status_emerald_text),
                    );
                }
                if !remote_clip.is_empty() {
                    let preview: String = remote_clip.chars().take(18).collect();
                    ui.label(
                        RichText::new(format!("CLIP: {}", preview))
                            .size(11.5)
                            .color(p.text_muted)
                            .monospace(),
                    );
                }
            });

            ui.separator();

            // Row 2: Quality Presets & Stream Sliders
            ui.horizontal_wrapped(|ui| {
                let mut changed = false;

                let fast_btn = styled_button(
                    "MODO RÁPIDO",
                    11.5,
                    p.text_primary,
                    p.bg_button,
                    p.border_strong,
                );
                if ui
                    .add(fast_btn)
                    .on_hover_text("30 FPS, JPEG 50, Escala 75%")
                    .clicked()
                {
                    session.fps_setting = 30;
                    session.quality_setting = 50;
                    session.scale_setting = 75;
                    changed = true;
                }

                let bal_btn = styled_button(
                    "EQUILIBRADO",
                    11.5,
                    p.text_primary,
                    p.bg_button,
                    p.border_strong,
                );
                if ui
                    .add(bal_btn)
                    .on_hover_text("25 FPS, JPEG 65, Escala 75%")
                    .clicked()
                {
                    session.fps_setting = 25;
                    session.quality_setting = 65;
                    session.scale_setting = 75;
                    changed = true;
                }

                let hq_btn = styled_button(
                    "ALTA RESOLUÇÃO",
                    11.5,
                    p.text_primary,
                    p.bg_button,
                    p.border_strong,
                );
                if ui
                    .add(hq_btn)
                    .on_hover_text("20 FPS, JPEG 80, Escala 100%")
                    .clicked()
                {
                    session.fps_setting = 20;
                    session.quality_setting = 80;
                    session.scale_setting = 100;
                    changed = true;
                }

                ui.separator();

                changed |= ui
                    .add(egui::Slider::new(&mut session.fps_setting, 5..=60).text("FPS"))
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut session.quality_setting, 20..=95).text("JPEG"))
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut session.scale_setting, 40..=100).text("Escala %"))
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut session.view_only,
                        RichText::new("Somente Leitura")
                            .size(12.5)
                            .color(p.text_primary),
                    )
                    .changed();

                if changed {
                    let _ = session.tx.send(Packet::StreamConfig {
                        fps: session.fps_setting,
                        jpeg_quality: session.quality_setting,
                        view_only: session.view_only,
                        scale_pct: session.scale_setting,
                    });
                }
            });
        });

        ui.add_space(8.0);

        // Interactive Remote Viewport (centered and aspect-ratio preserved)
        if let Some(tex) = &session.texture {
            let avail = ui.available_size();
            if avail.x > 10.0 && avail.y > 10.0 {
                let aspect = (remote_w.max(1) as f32) / (remote_h.max(1) as f32);
                let mut draw_w = avail.x;
                let mut draw_h = draw_w / aspect;
                if draw_h > avail.y {
                    draw_h = avail.y;
                    draw_w = draw_h * aspect;
                }

                let pad_x = ((avail.x - draw_w) * 0.5).max(0.0);
                let pad_y = ((avail.y - draw_h) * 0.5).max(0.0);

                ui.vertical(|ui| {
                    ui.add_space(pad_y);
                    ui.horizontal(|ui| {
                        ui.add_space(pad_x);
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(draw_w, draw_h),
                            egui::Sense::click_and_drag(),
                        );
                        ui.painter().image(
                            tex.id(),
                            rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                        ui.painter().rect_stroke(
                            rect,
                            Rounding::same(4.0),
                            Stroke::new(1.0_f32, p.border_subtle),
                        );

                        if !session.view_only {
                            if let Some(pos) = response.hover_pos() {
                                let nx = ((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
                                let ny = ((pos.y - rect.min.y) / rect.height()).clamp(0.0, 1.0);

                                let moved = (nx - session.last_mouse_pos.0).abs() > 0.001
                                    || (ny - session.last_mouse_pos.1).abs() > 0.001;
                                if moved
                                    && session.last_mouse_sent.elapsed()
                                        >= Duration::from_millis(16)
                                {
                                    session.last_mouse_pos = (nx, ny);
                                    session.last_mouse_sent = Instant::now();
                                    let _ = session.tx.send(Packet::MouseMove {
                                        norm_x: nx,
                                        norm_y: ny,
                                    });
                                }

                                ctx.input(|i| {
                                    for ev in &i.events {
                                        match ev {
                                            egui::Event::PointerButton {
                                                button, pressed, ..
                                            } => {
                                                let btn = match button {
                                                    egui::PointerButton::Primary => MouseBtn::Left,
                                                    egui::PointerButton::Secondary => {
                                                        MouseBtn::Right
                                                    }
                                                    _ => MouseBtn::Middle,
                                                };
                                                let _ = session.tx.send(Packet::MouseButton {
                                                    button: btn,
                                                    pressed: *pressed,
                                                    norm_x: nx,
                                                    norm_y: ny,
                                                });
                                            }
                                            egui::Event::Text(text) => {
                                                let _ = session.tx.send(Packet::TextInput {
                                                    text: text.clone(),
                                                });
                                            }
                                            egui::Event::Key { key, pressed, .. } => {
                                                let _ = session.tx.send(Packet::KeyEvent {
                                                    key: format!("{:?}", key),
                                                    pressed: *pressed,
                                                });
                                            }
                                            _ => {}
                                        }
                                    }
                                });
                            }
                        }
                    });
                });
            }
        } else {
            card_frame(p).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new("AGUARDANDO TRANSMISSÃO DE VÍDEO DO COMPUTADOR REMOTO...")
                            .size(14.0)
                            .strong()
                            .color(p.text_secondary),
                    );
                });
            });
        }
    }
}
