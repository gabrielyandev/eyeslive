#![windows_subsystem = "windows"]

//! EyesLive — Native Rust Remote Desktop Application
//! Created & Signed by Gabriel Yan (@gabrielyandev)
//! GitHub: https://github.com/gabrielyandev/eyeslive
//!
//! Features:
//! - Zero account login & Zero session time limits
//! - Interactive Access Consent Modal on Host (Full Control / View-Only / Reject)
//! - Native System Installer, Windows Autostart & Background Execution Mode
//! - Multi-tab simultaneous connections to multiple machines
//! - Direct IP:Port (LAN/VPN) + Optional Self-Hosted Relay by 9-digit Desk ID
//! - Real-time Screen Sharing, Mouse/Keyboard Injection, Clipboard Sync, File Transfer & Auto-Updater

mod host;
mod installer;
mod protocol;
mod relay;
mod updater;
mod viewer;

use eframe::egui::{self, Align2, Color32, Frame, Id, Margin, Order, RichText, Rounding, Stroke, Vec2};
use host::{start_host_listener, ConsentDecision, HostConfig, PendingConsent};
use installer::{
    get_installed_exe_path, install_application, is_autostart_enabled, is_installed, set_autostart,
    uninstall_application,
};
use protocol::{
    MouseBtn, Packet, AUTHOR_NAME, AUTHOR_SIGNATURE, DEFAULT_HOST_PORT, DEFAULT_RELAY_PORT,
    GITHUB_PROFILE_URL, GITHUB_REPO_URL,
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

// Modern Dark Palette Constants
const BG_BASE: Color32 = Color32::from_rgb(9, 13, 22);
const BG_PANEL: Color32 = Color32::from_rgb(13, 19, 31);
const BG_CARD: Color32 = Color32::from_rgb(17, 24, 39);
const BG_INPUT: Color32 = Color32::from_rgb(24, 34, 53);
const BORDER_SUBTLE: Color32 = Color32::from_rgb(38, 52, 77);
const ACCENT_BLUE: Color32 = Color32::from_rgb(59, 130, 246);
const ACCENT_CYAN: Color32 = Color32::from_rgb(56, 189, 248);
const STATUS_EMERALD: Color32 = Color32::from_rgb(16, 185, 129);
const STATUS_AMBER: Color32 = Color32::from_rgb(245, 158, 11);
const STATUS_RED: Color32 = Color32::from_rgb(239, 68, 68);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(241, 245, 249);
const TEXT_MUTED: Color32 = Color32::from_rgb(148, 163, 184);

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

fn configure_modern_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(10.0, 10.0);
    style.spacing.button_padding = Vec2::new(12.0, 7.0);
    style.spacing.window_margin = Margin::same(16.0);

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG_BASE;
    visuals.window_fill = BG_CARD;
    visuals.extreme_bg_color = BG_INPUT;
    visuals.faint_bg_color = BG_PANEL;
    visuals.override_text_color = Some(TEXT_PRIMARY);

    let rounding = Rounding::same(6.0);
    visuals.widgets.noninteractive.bg_fill = BG_CARD;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER_SUBTLE);
    visuals.widgets.noninteractive.rounding = rounding;

    visuals.widgets.inactive.bg_fill = BG_INPUT;
    visuals.widgets.inactive.weak_bg_fill = BG_INPUT;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_SUBTLE);
    visuals.widgets.inactive.rounding = rounding;

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(36, 50, 76);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(36, 50, 76);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT_CYAN);
    visuals.widgets.hovered.rounding = rounding;

    visuals.widgets.active.bg_fill = ACCENT_BLUE;
    visuals.widgets.active.weak_bg_fill = ACCENT_BLUE;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT_CYAN);
    visuals.widgets.active.rounding = rounding;

    visuals.selection.bg_fill = ACCENT_BLUE;
    visuals.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);

    style.visuals = visuals;
    ctx.set_style(style);
}

fn card_frame() -> Frame {
    Frame::none()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .rounding(Rounding::same(10.0))
        .inner_margin(Margin::same(20.0))
}

fn pill_frame(fill: Color32, border: Color32) -> Frame {
    Frame::none()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, border))
        .rounding(Rounding::same(6.0))
        .inner_margin(Margin::symmetric(10.0, 5.0))
}

fn draw_status_dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 8.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
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

    let viewport = egui::ViewportBuilder::default()
        .with_title("EyesLive — Remote Desktop [by Gabriel Yan @gabrielyandev]")
        .with_inner_size([1340.0, 860.0])
        .with_min_inner_size([980.0, 660.0]);

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "EyesLive [@gabrielyandev]",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            configure_modern_theme(&cc.egui_ctx);
            if start_in_background {
                cc.egui_ctx
                    .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            Ok(Box::new(EyesLiveApp::new(rt, start_in_background)))
        }),
    )
}

struct EyesLiveApp {
    rt: tokio::runtime::Runtime,
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
    force_quit: bool,
    install_feedback: Option<(String, Color32)>,

    // Connection form
    target_input: String,
    target_pin: String,
    relay_input: String,
    use_relay: bool,

    // Multi-tab concurrent sessions (0 = Home Workspace, 1..N = Active Remote Sessions)
    sessions: Vec<RemoteSession>,
    active_tab: usize,
    next_session_id: usize,

    // Auto-update system
    update_status: Arc<Mutex<UpdateStatus>>,
    show_update_banner: bool,
}

impl EyesLiveApp {
    fn new(rt: tokio::runtime::Runtime, started_in_background: bool) -> Self {
        let mut rng = rand::thread_rng();
        let p1: u32 = rng.gen_range(100..999);
        let p2: u32 = rng.gen_range(100..999);
        let p3: u32 = rng.gen_range(100..999);
        let my_desk_id = format!("{}-{}-{}", p1, p2, p3);

        let allow_control = Arc::new(Mutex::new(true));
        let require_consent = Arc::new(Mutex::new(true));
        let pending_consents = Arc::new(Mutex::new(Vec::new()));
        let active_incoming = Arc::new(Mutex::new(0usize));
        let my_pin = Arc::new(Mutex::new(String::new()));
        let bound_port = Arc::new(Mutex::new(DEFAULT_HOST_PORT));
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
            download_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        };

        rt.spawn(async move {
            let _ = start_host_listener(DEFAULT_HOST_PORT, host_cfg).await;
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
            force_quit: false,
            install_feedback: if started_in_background {
                Some((
                    "Iniciado em modo de segundo plano (Host ativo).".into(),
                    STATUS_EMERALD,
                ))
            } else {
                None
            },
            target_input: format!("127.0.0.1:{}", DEFAULT_HOST_PORT),
            target_pin: String::new(),
            relay_input: format!("127.0.0.1:{}", DEFAULT_RELAY_PORT),
            use_relay: false,
            sessions: Vec::new(),
            active_tab: 0,
            next_session_id: 1,
            update_status,
            show_update_banner: true,
        }
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

        let id = self.next_session_id;
        self.next_session_id += 1;
        let relay = if self.use_relay {
            Some(self.relay_input.trim().to_string())
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

    fn render_consent_modal(&mut self, ctx: &egui::Context) {
        let mut guard = self.pending_consents.lock().unwrap();
        // Clean up any expired requests (> 60s)
        guard.retain(|req| req.requested_at.elapsed() < Duration::from_secs(60));
        let count = guard.len();

        if count > self.last_consent_count {
            // New incoming connection request arrived! Bring window from background to front
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Critical,
            ));
        }
        self.last_consent_count = count;

        if guard.is_empty() {
            return;
        }

        // Dim background layer
        let screen_rect = ctx.screen_rect();
        egui::Area::new(Id::new("consent_modal_backdrop"))
            .order(Order::Foreground)
            .fixed_pos(screen_rect.min)
            .show(ctx, |ui| {
                ui.painter().rect_filled(
                    screen_rect,
                    Rounding::ZERO,
                    Color32::from_black_alpha(185),
                );
            });

        let mut resolved_index: Option<(usize, ConsentDecision)> = None;

        if let Some(req) = guard.first() {
            let remaining_secs = 60u64.saturating_sub(req.requested_at.elapsed().as_secs());

            egui::Window::new("SOLICITACAO DE ACESSO REMOTO")
                .order(Order::Tooltip)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .fixed_size(Vec2::new(500.0, 290.0))
                .frame(
                    Frame::none()
                        .fill(BG_CARD)
                        .stroke(Stroke::new(2.0_f32, ACCENT_BLUE))
                        .rounding(Rounding::same(12.0))
                        .inner_margin(Margin::same(24.0)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        draw_status_dot(ui, STATUS_AMBER);
                        ui.label(
                            RichText::new("AUTORIZACAO DE SEGURANCA NECESSARIA")
                                .size(12.0)
                                .strong()
                                .color(STATUS_AMBER)
                                .monospace(),
                        );
                    });
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "Um dispositivo remoto esta solicitando permissao para acessar este computador agora:",
                        )
                        .size(13.5)
                        .color(TEXT_PRIMARY),
                    );

                    ui.add_space(12.0);
                    pill_frame(BG_PANEL, BORDER_SUBTLE).show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("IDENTIFICACAO DO CLIENTE:")
                                        .size(11.0)
                                        .color(TEXT_MUTED)
                                        .monospace(),
                                );
                                ui.label(
                                    RichText::new(&req.client_name)
                                        .size(14.0)
                                        .strong()
                                        .color(TEXT_PRIMARY)
                                        .monospace(),
                                );
                            });
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("ENDERECO IP DE ORIGEM:")
                                        .size(11.0)
                                        .color(TEXT_MUTED)
                                        .monospace(),
                                );
                                ui.label(
                                    RichText::new(&req.peer_addr)
                                        .size(13.5)
                                        .strong()
                                        .color(ACCENT_CYAN)
                                        .monospace(),
                                );
                            });
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("TEMPO RESTANTE PARA RESPOSTA:")
                                        .size(11.0)
                                        .color(TEXT_MUTED)
                                        .monospace(),
                                );
                                ui.label(
                                    RichText::new(format!("{} segundos", remaining_secs))
                                        .size(12.0)
                                        .strong()
                                        .color(STATUS_AMBER)
                                        .monospace(),
                                );
                            });
                        });
                    });

                    ui.add_space(16.0);

                    let full_btn = egui::Button::new(
                        RichText::new("PERMITIR ACESSO COMPLETO (TELA + MOUSE E TECLADO)")
                            .size(12.5)
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(STATUS_EMERALD)
                    .min_size(Vec2::new(ui.available_width(), 38.0));

                    if ui.add(full_btn).clicked() {
                        resolved_index = Some((0, ConsentDecision::AllowFullControl));
                    }

                    ui.add_space(6.0);

                    ui.columns(2, |cols| {
                        let view_btn = egui::Button::new(
                            RichText::new("SOMENTE VISUALIZACAO")
                                .size(12.0)
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(ACCENT_BLUE)
                        .min_size(Vec2::new(cols[0].available_width(), 36.0));

                        if cols[0].add(view_btn).clicked() {
                            resolved_index = Some((0, ConsentDecision::AllowViewOnly));
                        }

                        let reject_btn = egui::Button::new(
                            RichText::new("RECUSAR CONEXAO")
                                .size(12.0)
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(STATUS_RED)
                        .min_size(Vec2::new(cols[1].available_width(), 36.0));

                        if cols[1].add(reject_btn).clicked() {
                            resolved_index = Some((0, ConsentDecision::Reject));
                        }
                    });
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
}

impl eframe::App for EyesLiveApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Intercept window close (X) to keep running in background if enabled
        if ctx.input(|i| i.viewport().close_requested()) && self.keep_in_background && !self.force_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }

        ctx.request_repaint_after(Duration::from_millis(16));

        // Render incoming connection authorization modal if any client is waiting
        self.render_consent_modal(ctx);

        // Auto-update banner notification
        let cur_update = self.update_status.lock().unwrap().clone();
        match cur_update {
            UpdateStatus::Available(ref info) if self.show_update_banner => {
                egui::TopBottomPanel::top("update_notification_banner")
                    .frame(
                        Frame::none()
                            .fill(BG_CARD)
                            .stroke(Stroke::new(1.0_f32, STATUS_AMBER))
                            .inner_margin(Margin::symmetric(20.0, 8.0)),
                    )
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            draw_status_dot(ui, STATUS_AMBER);
                            ui.label(
                                RichText::new(format!(
                                    "NOVA VERSAO DISPONIVEL: {} (ATUAL: v{})",
                                    info.latest_version, CURRENT_VERSION
                                ))
                                .color(STATUS_AMBER)
                                .size(12.0)
                                .strong()
                                .monospace(),
                            );

                            if let Some(exe_url) = info.exe_download_url.clone() {
                                let status_clone = self.update_status.clone();
                                let rt_handle = self.rt.handle().clone();
                                if ui
                                    .button(
                                        RichText::new("BAIXAR E ATUALIZAR AUTOMATICAMENTE (.EXE)")
                                            .color(STATUS_EMERALD)
                                            .size(11.5)
                                            .strong(),
                                    )
                                    .clicked()
                                {
                                    rt_handle.spawn(async move {
                                        let _ = tokio::task::spawn_blocking(move || {
                                            download_update_sync(&exe_url, status_clone)
                                        })
                                        .await;
                                    });
                                }
                            }

                            if ui
                                .button(RichText::new("NOTAS DA VERSAO").size(11.5))
                                .clicked()
                            {
                                open_browser_url(&info.release_url);
                            }

                            if ui.small_button(RichText::new("X").size(11.0)).clicked() {
                                self.show_update_banner = false;
                            }
                        });
                    });
            }
            UpdateStatus::Downloading(progress) => {
                egui::TopBottomPanel::top("download_progress_banner")
                    .frame(
                        Frame::none()
                            .fill(BG_CARD)
                            .stroke(Stroke::new(1.0_f32, ACCENT_BLUE))
                            .inner_margin(Margin::symmetric(20.0, 8.0)),
                    )
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(
                                RichText::new(format!(
                                    "BAIXANDO ATUALIZACAO DO EYESLIVE: {:.0}%",
                                    progress * 100.0
                                ))
                                .size(12.0)
                                .strong()
                                .monospace(),
                            );
                            ui.add(egui::ProgressBar::new(progress).desired_width(200.0));
                        });
                    });
            }
            UpdateStatus::ReadyToRestart(ref new_exe) => {
                let exe_to_run = new_exe.clone();
                egui::TopBottomPanel::top("ready_restart_banner")
                    .frame(
                        Frame::none()
                            .fill(BG_CARD)
                            .stroke(Stroke::new(1.0_f32, STATUS_EMERALD))
                            .inner_margin(Margin::symmetric(20.0, 8.0)),
                    )
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            draw_status_dot(ui, STATUS_EMERALD);
                            ui.label(
                                RichText::new("ATUALIZACAO CONCLUIDA COM SUCESSO")
                                    .color(STATUS_EMERALD)
                                    .size(12.0)
                                    .strong()
                                    .monospace(),
                            );
                            if ui
                                .button(
                                    RichText::new("REINICIAR AGORA")
                                        .color(Color32::WHITE)
                                        .size(11.5)
                                        .strong(),
                                )
                                .clicked()
                            {
                                let _ = apply_update_and_restart(&exe_to_run);
                            }
                        });
                    });
            }
            _ => {}
        }

        // Top Navigation Header
        egui::TopBottomPanel::top("top_nav_bar")
            .frame(
                Frame::none()
                    .fill(BG_PANEL)
                    .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                    .inner_margin(Margin::symmetric(20.0, 12.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Brand Identity
                    draw_status_dot(ui, ACCENT_BLUE);
                    ui.label(
                        RichText::new("EYESLIVE")
                            .size(18.0)
                            .strong()
                            .color(TEXT_PRIMARY),
                    );
                    ui.label(RichText::new("//").color(BORDER_SUBTLE));
                    if ui
                        .button(
                            RichText::new(format!("{} ({})", AUTHOR_NAME, AUTHOR_SIGNATURE))
                                .size(12.5)
                                .color(ACCENT_CYAN)
                                .monospace(),
                        )
                        .on_hover_text(format!("Abrir GitHub: {}", GITHUB_PROFILE_URL))
                        .clicked()
                    {
                        open_browser_url(GITHUB_PROFILE_URL);
                    }

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Workspace / Dashboard Tab
                    let home_selected = self.active_tab == 0;
                    if ui
                        .selectable_label(
                            home_selected,
                            RichText::new("PAINEL DE CONTROLE")
                                .size(12.5)
                                .strong()
                                .color(if home_selected {
                                    Color32::WHITE
                                } else {
                                    TEXT_MUTED
                                }),
                        )
                        .clicked()
                    {
                        self.active_tab = 0;
                    }

                    // Active Remote Session Tabs
                    let mut close_id = None;
                    for s in &self.sessions {
                        let st = s.state.lock().unwrap();
                        let is_online = st.connected;
                        let label = format!("SESSAO #{}: {}", s.id, st.host_name);
                        drop(st);

                        let is_active = self.active_tab == s.id;
                        pill_frame(
                            if is_active { BG_INPUT } else { BG_CARD },
                            if is_active { ACCENT_BLUE } else { BORDER_SUBTLE },
                        )
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                draw_status_dot(
                                    ui,
                                    if is_online {
                                        STATUS_EMERALD
                                    } else {
                                        STATUS_AMBER
                                    },
                                );
                                if ui
                                    .selectable_label(
                                        is_active,
                                        RichText::new(label).size(12.0).strong(),
                                    )
                                    .clicked()
                                {
                                    self.active_tab = s.id;
                                }
                                if ui
                                    .small_button(RichText::new("X").size(11.0).color(TEXT_MUTED))
                                    .on_hover_text("Encerrar sessao")
                                    .clicked()
                                {
                                    close_id = Some(s.id);
                                }
                            });
                        });
                    }

                    if let Some(cid) = close_id {
                        self.sessions.retain(|s| s.id != cid);
                        if self.active_tab == cid {
                            self.active_tab = 0;
                        }
                    }

                    // Right-aligned Telemetry Badges
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        pill_frame(BG_CARD, BORDER_SUBTLE).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                draw_status_dot(ui, STATUS_EMERALD);
                                ui.label(
                                    RichText::new("SEM LOGIN  |  TEMPO ILIMITADO  |  MULTI-SESSAO")
                                        .size(11.0)
                                        .color(TEXT_MUTED)
                                        .monospace(),
                                );
                            });
                        });
                    });
                });
            });

        // Bottom Status & Developer Credits Bar
        egui::TopBottomPanel::bottom("bottom_status_bar")
            .frame(
                Frame::none()
                    .fill(BG_PANEL)
                    .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                    .inner_margin(Margin::symmetric(20.0, 10.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let inc = *self.active_incoming.lock().unwrap();
                    let hstatus = self.host_status.lock().unwrap().clone();
                    let port = *self.bound_port.lock().unwrap();

                    draw_status_dot(ui, STATUS_EMERALD);
                    ui.label(
                        RichText::new(format!(
                            "HOST: {}  |  IP: {}:{}  |  CLIENTES: {}  |  ABAS: {}",
                            hstatus.to_uppercase(),
                            self.local_ip,
                            port,
                            inc,
                            self.sessions.len()
                        ))
                        .size(11.5)
                        .color(TEXT_MUTED)
                        .monospace(),
                    );

                    ui.separator();

                    let cur_up = self.update_status.lock().unwrap().clone();
                    match cur_up {
                        UpdateStatus::Checking => {
                            ui.spinner();
                            ui.label(
                                RichText::new("VERIFICANDO ATUALIZACOES...")
                                    .size(11.0)
                                    .color(TEXT_MUTED)
                                    .monospace(),
                            );
                        }
                        UpdateStatus::Available(ref info) => {
                            if ui
                                .small_button(
                                    RichText::new(format!(
                                        "ATUALIZACAO {} DISPONIVEL",
                                        info.latest_version
                                    ))
                                    .color(STATUS_AMBER)
                                    .size(11.0)
                                    .strong()
                                    .monospace(),
                                )
                                .on_hover_text("Clique para exibir a barra de atualizacao")
                                .clicked()
                            {
                                self.show_update_banner = true;
                            }
                        }
                        UpdateStatus::UpToDate => {
                            if ui
                                .small_button(
                                    RichText::new(format!("v{} (ATUALIZADO)", CURRENT_VERSION))
                                        .size(11.0)
                                        .color(STATUS_EMERALD)
                                        .monospace(),
                                )
                                .on_hover_text("Versao mais recente. Clique para verificar novamente.")
                                .clicked()
                            {
                                self.trigger_manual_update_check();
                            }
                        }
                        UpdateStatus::Downloading(p) => {
                            ui.label(
                                RichText::new(format!("BAIXANDO: {:.0}%", p * 100.0))
                                    .size(11.0)
                                    .color(ACCENT_CYAN)
                                    .monospace(),
                            );
                        }
                        UpdateStatus::ReadyToRestart(_) => {
                            ui.label(
                                RichText::new("PRONTO PARA REINICIAR")
                                    .size(11.0)
                                    .color(STATUS_EMERALD)
                                    .monospace(),
                            );
                        }
                        _ => {
                            if ui
                                .small_button(
                                    RichText::new(format!(
                                        "v{}  |  CHECAR ATUALIZACOES",
                                        CURRENT_VERSION
                                    ))
                                    .size(11.0)
                                    .color(TEXT_MUTED)
                                    .monospace(),
                                )
                                .on_hover_text("Verificar se ha nova versao no GitHub")
                                .clicked()
                            {
                                self.trigger_manual_update_check();
                            }
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(
                                RichText::new("ENCERRAR PROCESSO")
                                    .size(11.0)
                                    .strong()
                                    .color(STATUS_RED),
                            )
                            .on_hover_text("Fechar o EyesLive completamente (parar segundo plano)")
                            .clicked()
                        {
                            self.force_quit = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui
                            .button(
                                RichText::new("GITHUB PROFILE")
                                    .size(11.0)
                                    .strong()
                                    .color(ACCENT_CYAN),
                            )
                            .on_hover_text(GITHUB_PROFILE_URL)
                            .clicked()
                        {
                            open_browser_url(GITHUB_PROFILE_URL);
                        }
                        if ui
                            .button(
                                RichText::new("REPOSITORIO EYESLIVE")
                                    .size(11.0)
                                    .strong()
                                    .color(TEXT_PRIMARY),
                            )
                            .on_hover_text(GITHUB_REPO_URL)
                            .clicked()
                        {
                            open_browser_url(GITHUB_REPO_URL);
                        }
                        ui.label(
                            RichText::new(format!(
                                "CREDITOS: {} ({})",
                                AUTHOR_NAME.to_uppercase(),
                                AUTHOR_SIGNATURE
                            ))
                            .size(11.5)
                            .color(TEXT_MUTED)
                            .monospace(),
                        );
                    });
                });
            });

        // Central Workspace
        egui::CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(BG_BASE)
                    .inner_margin(Margin::symmetric(28.0, 20.0)),
            )
            .show(ctx, |ui| {
                if self.active_tab == 0 {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.render_dashboard(ctx, ui);
                    });
                } else {
                    let rt_handle = self.rt.handle().clone();
                    if let Some(session) =
                        self.sessions.iter_mut().find(|s| s.id == self.active_tab)
                    {
                        Self::render_remote_session(ctx, ui, session, &rt_handle);
                    } else {
                        self.active_tab = 0;
                    }
                }
            });
    }
}

impl EyesLiveApp {
    fn render_dashboard(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let cur_port = *self.bound_port.lock().unwrap();
        let lan_endpoint = format!("{}:{}", self.local_ip, cur_port);

        // Hero Banner
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("ESTACAO DE ACESSO REMOTO")
                        .size(11.5)
                        .color(ACCENT_CYAN)
                        .monospace(),
                );
                ui.label(
                    RichText::new("EyesLive Remote Desktop")
                        .size(25.0)
                        .strong()
                        .color(TEXT_PRIMARY),
                );
            });
        });

        ui.add_space(14.0);

        ui.columns(2, |cols| {
            // Left Card: Local Host Station
            card_frame().show(&mut cols[0], |ui| {
                ui.set_min_height(325.0);
                ui.horizontal(|ui| {
                    draw_status_dot(ui, STATUS_EMERALD);
                    ui.label(
                        RichText::new("ESTE DISPOSITIVO  //  SERVIDOR HOST")
                            .size(12.0)
                            .strong()
                            .color(ACCENT_CYAN)
                            .monospace(),
                    );
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "Compartilhe seu identificador ou endereco IP direto para permitir conexoes remotas sem restricao de tempo.",
                    )
                    .size(13.0)
                    .color(TEXT_MUTED),
                );
                ui.add_space(14.0);

                // Desk ID Box
                pill_frame(BG_PANEL, BORDER_SUBTLE).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new("EYESLIVE ID")
                                    .size(10.5)
                                    .color(TEXT_MUTED)
                                    .monospace(),
                            );
                            ui.label(
                                RichText::new(&self.my_desk_id)
                                    .size(26.0)
                                    .strong()
                                    .color(TEXT_PRIMARY)
                                    .monospace(),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button(RichText::new("COPIAR ID").size(11.5).monospace())
                                .clicked()
                            {
                                if let Ok(mut cb) = arboard::Clipboard::new() {
                                    let _ = cb.set_text(self.my_desk_id.clone());
                                }
                            }
                        });
                    });
                });

                ui.add_space(8.0);

                // LAN Endpoint Box
                pill_frame(BG_PANEL, BORDER_SUBTLE).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new("ENDERECO DIRETO (LAN / VPN / TAILSCALE)")
                                    .size(10.5)
                                    .color(TEXT_MUTED)
                                    .monospace(),
                            );
                            ui.label(
                                RichText::new(&lan_endpoint)
                                    .size(17.5)
                                    .strong()
                                    .color(ACCENT_CYAN)
                                    .monospace(),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button(RichText::new("COPIAR IP").size(11.5).monospace())
                                .clicked()
                            {
                                if let Ok(mut cb) = arboard::Clipboard::new() {
                                    let _ = cb.set_text(lan_endpoint.clone());
                                }
                            }
                        });
                    });
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                let mut req_consent = *self.require_consent.lock().unwrap();
                if ui
                    .checkbox(
                        &mut req_consent,
                        RichText::new("Solicitar autorizacao na tela antes de permitir o acesso")
                            .size(13.0)
                            .strong(),
                    )
                    .changed()
                {
                    *self.require_consent.lock().unwrap() = req_consent;
                }

                let mut allow = *self.allow_control.lock().unwrap();
                if ui
                    .checkbox(
                        &mut allow,
                        RichText::new("Permitir injecao remota de mouse e teclado").size(13.0),
                    )
                    .changed()
                {
                    *self.allow_control.lock().unwrap() = allow;
                }

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("PIN de Seguranca (vazio = livre):")
                            .size(13.0)
                            .color(TEXT_MUTED),
                    );
                    let mut pin_guard = self.my_pin.lock().unwrap();
                    ui.add(
                        egui::TextEdit::singleline(&mut *pin_guard)
                            .desired_width(140.0)
                            .hint_text("Sem PIN"),
                    );
                });
            });

            // Right Card: Remote Client Launcher
            card_frame().show(&mut cols[1], |ui| {
                ui.set_min_height(325.0);
                ui.horizontal(|ui| {
                    draw_status_dot(ui, ACCENT_BLUE);
                    ui.label(
                        RichText::new("NOVA CONEXAO  //  CLIENTE MULTI-SESSAO")
                            .size(12.0)
                            .strong()
                            .color(ACCENT_CYAN)
                            .monospace(),
                    );
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "Conecte-se simultaneamente a multiplas maquinas em abas independentes usando IP:Porta ou ID.",
                    )
                    .size(13.0)
                    .color(TEXT_MUTED),
                );
                ui.add_space(14.0);

                ui.label(
                    RichText::new("DESTINO REMOTO (IP:PORTA OU EYESLIVE ID)")
                        .size(11.0)
                        .color(TEXT_MUTED)
                        .monospace(),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.target_input)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace)
                        .hint_text("Ex: 192.168.1.50:9090 ou 123-456-789"),
                );

                ui.add_space(10.0);

                ui.label(
                    RichText::new("PIN DE AUTENTICACAO (OPCIONAL)")
                        .size(11.0)
                        .color(TEXT_MUTED)
                        .monospace(),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.target_pin)
                        .desired_width(f32::INFINITY)
                        .password(true)
                        .hint_text("Deixe em branco se o Host nao exigir PIN"),
                );

                ui.add_space(10.0);
                ui.checkbox(
                    &mut self.use_relay,
                    RichText::new("Roteamento via Servidor Relay por ID").size(13.0),
                );

                if self.use_relay {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Servidor Relay:")
                                .size(12.5)
                                .color(TEXT_MUTED),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.relay_input)
                                .desired_width(f32::INFINITY)
                                .font(egui::TextStyle::Monospace),
                        );
                    });
                }

                ui.add_space(18.0);
                let btn = egui::Button::new(
                    RichText::new("INICIAR SESSAO REMOTA")
                        .size(14.0)
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(ACCENT_BLUE)
                .min_size(Vec2::new(ui.available_width(), 42.0));

                if ui.add(btn).clicked() {
                    self.spawn_new_session();
                }
            });
        });

        ui.add_space(14.0);

        // System Installation & Background Service Card
        card_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                draw_status_dot(
                    ui,
                    if self.installed_status {
                        STATUS_EMERALD
                    } else {
                        ACCENT_BLUE
                    },
                );
                ui.label(
                    RichText::new("INSTALACAO NO SISTEMA E EXECUCAO EM SEGUNDO PLANO")
                        .size(12.0)
                        .strong()
                        .color(ACCENT_CYAN)
                        .monospace(),
                );
                ui.separator();
                ui.label(
                    RichText::new(if self.installed_status {
                        format!(
                            "STATUS: INSTALADO ({})",
                            get_installed_exe_path().display()
                        )
                    } else {
                        "STATUS: MODO PORTATIL (NAO INSTALADO NO SISTEMA)".to_string()
                    })
                    .size(11.5)
                    .color(if self.installed_status {
                        STATUS_EMERALD
                    } else {
                        TEXT_MUTED
                    })
                    .monospace(),
                );
            });

            ui.add_space(8.0);

            ui.horizontal(|ui| {
                let install_label = if self.installed_status {
                    "ATUALIZAR INSTALACAO LOCAL E ATALHOS"
                } else {
                    "INSTALAR EYESLIVE E ATIVAR SEGUNDO PLANO"
                };

                let install_btn = egui::Button::new(
                    RichText::new(install_label)
                        .size(12.0)
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(STATUS_EMERALD)
                .min_size(Vec2::new(280.0, 34.0));

                if ui
                    .add(install_btn)
                    .on_hover_text(
                        "Copia o executavel para %LOCALAPPDATA%\\EyesLive, cria atalhos na Area de Trabalho e Menu Iniciar e configura execucao em segundo plano",
                    )
                    .clicked()
                {
                    match install_application(true) {
                        Ok(path) => {
                            self.installed_status = true;
                            self.autostart_enabled = true;
                            self.keep_in_background = true;
                            self.install_feedback = Some((
                                format!("Instalado com sucesso em: {}", path),
                                STATUS_EMERALD,
                            ));
                        }
                        Err(e) => {
                            self.install_feedback = Some((
                                format!("Erro ao instalar: {}", e),
                                STATUS_RED,
                            ));
                        }
                    }
                }

                if ui
                    .button(
                        RichText::new("MINIMIZAR PARA SEGUNDO PLANO AGORA")
                            .size(11.5)
                            .strong()
                            .color(ACCENT_CYAN),
                    )
                    .on_hover_text(
                        "Oculta a janela mantendo o servidor Host ativo; a janela reabre automaticamente se alguem solicitar acesso",
                    )
                    .clicked()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }

                if self.installed_status {
                    if ui
                        .button(
                            RichText::new("DESINSTALAR DO SISTEMA")
                                .size(11.5)
                                .color(STATUS_RED),
                        )
                        .clicked()
                    {
                        let _ = uninstall_application();
                        self.installed_status = is_installed();
                        self.autostart_enabled = is_autostart_enabled();
                        self.install_feedback = Some((
                            "Atalhos e inicializacao automatica removidos.".into(),
                            STATUS_AMBER,
                        ));
                    }
                }
            });

            ui.add_space(6.0);

            ui.horizontal(|ui| {
                if ui
                    .checkbox(
                        &mut self.autostart_enabled,
                        RichText::new("Iniciar automaticamente em segundo plano junto com o Windows")
                            .size(12.5),
                    )
                    .changed()
                {
                    let _ = set_autostart(self.autostart_enabled);
                }

                ui.add_space(16.0);

                ui.checkbox(
                    &mut self.keep_in_background,
                    RichText::new(
                        "Continuar rodando em segundo plano ao fechar a janela no botao (X)",
                    )
                    .size(12.5),
                );
            });

            if let Some((ref msg, color)) = self.install_feedback {
                ui.add_space(4.0);
                ui.label(RichText::new(msg).size(11.5).color(color).monospace());
            }
        });

        ui.add_space(14.0);

        // Release & Portable Executable Card
        card_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                draw_status_dot(ui, ACCENT_CYAN);
                ui.label(
                    RichText::new(format!("EYESLIVE DESKTOP v{}", CURRENT_VERSION))
                        .size(13.0)
                        .strong()
                        .monospace(),
                );
                ui.separator();
                if ui
                    .button(
                        RichText::new("VERIFICAR ATUALIZACOES NO REPOSITORIO")
                            .size(11.5)
                            .strong(),
                    )
                    .on_hover_text("Consulta se ha novas versoes publicadas no GitHub")
                    .clicked()
                {
                    self.trigger_manual_update_check();
                }
                if ui
                    .button(
                        RichText::new("BAIXAR EXECUTAVEL PORTATIL (.EXE)")
                            .size(11.5)
                            .strong()
                            .color(ACCENT_CYAN),
                    )
                    .on_hover_text("Abrir pagina de download direto do executavel no GitHub Releases")
                    .clicked()
                {
                    open_browser_url(GITHUB_RELEASES_PAGE);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let repo_btn = egui::Button::new(
                        RichText::new("REPOSITORIO NO GITHUB")
                            .size(11.5)
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(ACCENT_BLUE)
                    .min_size(Vec2::new(170.0, 32.0));

                    if ui.add(repo_btn).on_hover_text(GITHUB_REPO_URL).clicked() {
                        open_browser_url(GITHUB_REPO_URL);
                    }

                    let profile_btn = egui::Button::new(
                        RichText::new(format!("AUTOR: {} ({})", AUTHOR_NAME, AUTHOR_SIGNATURE))
                            .size(11.5)
                            .strong()
                            .color(ACCENT_CYAN),
                    )
                    .min_size(Vec2::new(200.0, 32.0));

                    if ui
                        .add(profile_btn)
                        .on_hover_text(GITHUB_PROFILE_URL)
                        .clicked()
                    {
                        open_browser_url(GITHUB_PROFILE_URL);
                    }
                });
            });
        });
    }

    fn render_remote_session(
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        session: &mut RemoteSession,
        rt: &tokio::runtime::Handle,
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

        // Session Control Bar
        pill_frame(BG_CARD, BORDER_SUBTLE).show(ui, |ui| {
            ui.horizontal(|ui| {
                draw_status_dot(
                    ui,
                    if connected {
                        STATUS_EMERALD
                    } else {
                        STATUS_AMBER
                    },
                );
                ui.label(RichText::new(&status).size(12.0).strong());

                if let Some(rtt) = ping_rtt {
                    let color = if rtt < 40 {
                        STATUS_EMERALD
                    } else if rtt < 100 {
                        STATUS_AMBER
                    } else {
                        STATUS_RED
                    };
                    ui.separator();
                    draw_status_dot(ui, color);
                    ui.label(
                        RichText::new(format!("PING: {}ms", rtt))
                            .size(11.5)
                            .color(color)
                            .strong()
                            .monospace(),
                    );
                }

                ui.label(
                    RichText::new(format!("|  QUADROS: {}", frames_rx))
                        .size(11.5)
                        .color(TEXT_MUTED)
                        .monospace(),
                );
                ui.separator();

                let mut changed = false;

                // One-click performance presets
                if ui
                    .small_button(RichText::new("MODO RAPIDO").size(11.0).strong())
                    .on_hover_text("Maxima velocidade e menor latencia (30 FPS, JPEG 50, Escala 75%)")
                    .clicked()
                {
                    session.fps_setting = 30;
                    session.quality_setting = 50;
                    session.scale_setting = 75;
                    changed = true;
                }
                if ui
                    .small_button(RichText::new("EQUILIBRADO").size(11.0).strong())
                    .on_hover_text("Boa imagem e baixa latencia (25 FPS, JPEG 65, Escala 75%)")
                    .clicked()
                {
                    session.fps_setting = 25;
                    session.quality_setting = 65;
                    session.scale_setting = 75;
                    changed = true;
                }
                if ui
                    .small_button(RichText::new("ALTA RESOLUCAO").size(11.0).strong())
                    .on_hover_text("100% Nativo sem escala e mais detalhes (20 FPS, JPEG 80, Escala 100%)")
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
                    .add(
                        egui::Slider::new(&mut session.quality_setting, 20..=95)
                            .text("JPEG"),
                    )
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut session.scale_setting, 40..=100).text("Escala %"))
                    .changed();
                changed |= ui
                    .checkbox(&mut session.view_only, "Somente Leitura")
                    .changed();

                if changed {
                    let _ = session.tx.send(Packet::StreamConfig {
                        fps: session.fps_setting,
                        jpeg_quality: session.quality_setting,
                        view_only: session.view_only,
                        scale_pct: session.scale_setting,
                    });
                }

                ui.separator();

                if ui
                    .button(RichText::new("ENVIAR ARQUIVO").size(11.0).strong())
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        session.send_file(path, rt);
                    }
                }

                if ui
                    .button(RichText::new("CLIPBOARD").size(11.0).strong())
                    .clicked()
                {
                    if let Ok(mut cb) = arboard::Clipboard::new() {
                        if let Ok(txt) = cb.get_text() {
                            let _ = session.tx.send(Packet::ClipboardText { text: txt });
                        }
                    }
                }

                if let Some(ack) = file_ack {
                    ui.label(RichText::new(ack).size(11.0).color(STATUS_EMERALD));
                }
                if !remote_clip.is_empty() {
                    let preview: String = remote_clip.chars().take(18).collect();
                    ui.label(
                        RichText::new(format!("CLIP: {}", preview))
                            .size(11.0)
                            .color(TEXT_MUTED)
                            .monospace(),
                    );
                }
            });
        });

        ui.add_space(10.0);

        // Interactive Remote Viewport
        if let Some(tex) = &session.texture {
            let avail = ui.available_size();
            let aspect = (remote_w.max(1) as f32) / (remote_h.max(1) as f32);
            let mut draw_w = avail.x;
            let mut draw_h = draw_w / aspect;
            if draw_h > avail.y {
                draw_h = avail.y;
                draw_w = draw_h * aspect;
            }

            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(draw_w, draw_h), egui::Sense::click_and_drag());
            ui.painter().image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            ui.painter().rect_stroke(
                rect,
                Rounding::same(4.0),
                Stroke::new(1.0_f32, BORDER_SUBTLE),
            );

            if !session.view_only {
                if let Some(pos) = response.hover_pos() {
                    let nx = ((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
                    let ny = ((pos.y - rect.min.y) / rect.height()).clamp(0.0, 1.0);

                    let moved = (nx - session.last_mouse_pos.0).abs() > 0.001
                        || (ny - session.last_mouse_pos.1).abs() > 0.001;
                    if moved && session.last_mouse_sent.elapsed() >= Duration::from_millis(16) {
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
                                        egui::PointerButton::Secondary => MouseBtn::Right,
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
        } else {
            card_frame().show(ui, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new("AGUARDANDO STREAM DE VIDEO DO HOST REMOTO...")
                            .size(13.0)
                            .color(TEXT_MUTED)
                            .monospace(),
                    );
                });
            });
        }
    }
}
