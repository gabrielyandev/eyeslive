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

use eframe::egui::{
    self, Align2, Color32, Frame, Id, Margin, Order, RichText, Rounding, Stroke, Vec2,
};
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
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(10.0, 6.0);
    style.spacing.window_margin = Margin::same(14.0);

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
        .inner_margin(Margin::same(16.0))
}

fn pill_frame(fill: Color32, border: Color32) -> Frame {
    Frame::none()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, border))
        .rounding(Rounding::same(6.0))
        .inner_margin(Margin::symmetric(10.0, 6.0))
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
        guard.retain(|req| req.requested_at.elapsed() < Duration::from_secs(60));
        let count = guard.len();

        if count > self.last_consent_count {
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
            let modal_w = (screen_rect.width() - 40.0).clamp(300.0, 480.0);

            egui::Window::new("SOLICITACAO DE ACESSO REMOTO")
                .order(Order::Tooltip)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .default_width(modal_w)
                .max_width(modal_w)
                .frame(
                    Frame::none()
                        .fill(BG_CARD)
                        .stroke(Stroke::new(2.0_f32, ACCENT_BLUE))
                        .rounding(Rounding::same(12.0))
                        .inner_margin(Margin::same(20.0)),
                )
                .show(ctx, |ui| {
                    ui.set_width(modal_w - 40.0);
                    ui.horizontal_wrapped(|ui| {
                        draw_status_dot(ui, STATUS_AMBER);
                        ui.label(
                            RichText::new("AUTORIZACAO DE SEGURANCA NECESSARIA")
                                .size(12.0)
                                .strong()
                                .color(STATUS_AMBER)
                                .monospace(),
                        );
                    });
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "Um dispositivo remoto esta solicitando permissao para acessar este computador:",
                        )
                        .size(13.0)
                        .color(TEXT_PRIMARY),
                    );

                    ui.add_space(10.0);
                    pill_frame(BG_PANEL, BORDER_SUBTLE).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.vertical(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new("CLIENTE:")
                                        .size(11.0)
                                        .color(TEXT_MUTED)
                                        .monospace(),
                                );
                                ui.label(
                                    RichText::new(&req.client_name)
                                        .size(13.5)
                                        .strong()
                                        .color(TEXT_PRIMARY)
                                        .monospace(),
                                );
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new("ENDERECO IP:")
                                        .size(11.0)
                                        .color(TEXT_MUTED)
                                        .monospace(),
                                );
                                ui.label(
                                    RichText::new(&req.peer_addr)
                                        .size(13.0)
                                        .strong()
                                        .color(ACCENT_CYAN)
                                        .monospace(),
                                );
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new("EXPIRA EM:")
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

                    ui.add_space(14.0);

                    let w = ui.available_width();
                    let full_btn = egui::Button::new(
                        RichText::new("PERMITIR ACESSO COMPLETO (TELA + CONTROLE)")
                            .size(12.0)
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(STATUS_EMERALD)
                    .min_size(Vec2::new(w, 36.0));

                    if ui.add(full_btn).clicked() {
                        resolved_index = Some((0, ConsentDecision::AllowFullControl));
                    }

                    ui.add_space(6.0);

                    let view_btn = egui::Button::new(
                        RichText::new("PERMITIR SOMENTE VISUALIZACAO")
                            .size(12.0)
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(ACCENT_BLUE)
                    .min_size(Vec2::new(w, 34.0));

                    if ui.add(view_btn).clicked() {
                        resolved_index = Some((0, ConsentDecision::AllowViewOnly));
                    }

                    ui.add_space(6.0);

                    let reject_btn = egui::Button::new(
                        RichText::new("RECUSAR CONEXAO")
                            .size(12.0)
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(STATUS_RED)
                    .min_size(Vec2::new(w, 34.0));

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

    fn render_host_card(&mut self, ui: &mut egui::Ui, lan_endpoint: &str) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                draw_status_dot(ui, STATUS_EMERALD);
                ui.label(
                    RichText::new("ESTE DISPOSITIVO  //  SERVIDOR HOST")
                        .size(12.0)
                        .strong()
                        .color(ACCENT_CYAN)
                        .monospace(),
                );
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Compartilhe seu ID ou IP direto para receber conexoes remotas sem limite de tempo.",
                )
                .size(12.5)
                .color(TEXT_MUTED),
            );
            ui.add_space(12.0);

            // Desk ID Box
            pill_frame(BG_PANEL, BORDER_SUBTLE).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("EYESLIVE ID")
                                .size(10.5)
                                .color(TEXT_MUTED)
                                .monospace(),
                        );
                        ui.label(
                            RichText::new(&self.my_desk_id)
                                .size(24.0)
                                .strong()
                                .color(TEXT_PRIMARY)
                                .monospace(),
                        );
                    });
                    ui.add_space(8.0);
                    if ui
                        .button(RichText::new("COPIAR ID").size(11.0).monospace())
                        .clicked()
                    {
                        if let Ok(mut cb) = arboard::Clipboard::new() {
                            let _ = cb.set_text(self.my_desk_id.clone());
                        }
                    }
                });
            });

            ui.add_space(8.0);

            // LAN Endpoint Box
            pill_frame(BG_PANEL, BORDER_SUBTLE).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("ENDERECO DIRETO (LAN / VPN / TAILSCALE)")
                                .size(10.5)
                                .color(TEXT_MUTED)
                                .monospace(),
                        );
                        ui.label(
                            RichText::new(lan_endpoint)
                                .size(16.5)
                                .strong()
                                .color(ACCENT_CYAN)
                                .monospace(),
                        );
                    });
                    ui.add_space(8.0);
                    if ui
                        .button(RichText::new("COPIAR IP").size(11.0).monospace())
                        .clicked()
                    {
                        if let Ok(mut cb) = arboard::Clipboard::new() {
                            let _ = cb.set_text(lan_endpoint.to_string());
                        }
                    }
                });
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            let mut req_consent = *self.require_consent.lock().unwrap();
            if ui
                .checkbox(
                    &mut req_consent,
                    RichText::new("Solicitar autorizacao na tela ao receber conexao")
                        .size(12.5)
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
                    RichText::new("Permitir controle remoto de mouse e teclado").size(12.5),
                )
                .changed()
            {
                *self.allow_control.lock().unwrap() = allow;
            }

            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("PIN de Seguranca (opcional):")
                        .size(12.5)
                        .color(TEXT_MUTED),
                );
                let mut pin_guard = self.my_pin.lock().unwrap();
                ui.add(
                    egui::TextEdit::singleline(&mut *pin_guard)
                        .desired_width(130.0)
                        .hint_text("Sem PIN"),
                );
            });
        });
    }

    fn render_client_card(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                draw_status_dot(ui, ACCENT_BLUE);
                ui.label(
                    RichText::new("NOVA CONEXAO  //  CLIENTE MULTI-SESSAO")
                        .size(12.0)
                        .strong()
                        .color(ACCENT_CYAN)
                        .monospace(),
                );
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Conecte-se simultaneamente a multiplas maquinas em abas independentes.",
                )
                .size(12.5)
                .color(TEXT_MUTED),
            );
            ui.add_space(12.0);

            let field_w = ui.available_width();

            ui.label(
                RichText::new("DESTINO REMOTO (IP:PORTA OU EYESLIVE ID)")
                    .size(10.5)
                    .color(TEXT_MUTED)
                    .monospace(),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.target_input)
                    .desired_width(field_w)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("Ex: 192.168.1.50:9090 ou 123-456-789"),
            );

            ui.add_space(8.0);

            ui.label(
                RichText::new("PIN DE AUTENTICACAO (OPCIONAL)")
                    .size(10.5)
                    .color(TEXT_MUTED)
                    .monospace(),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.target_pin)
                    .desired_width(field_w)
                    .password(true)
                    .hint_text("Deixe vazio se o Host nao exigir PIN"),
            );

            ui.add_space(8.0);
            ui.checkbox(
                &mut self.use_relay,
                RichText::new("Roteamento via Servidor Relay por ID").size(12.5),
            );

            if self.use_relay {
                ui.add_space(4.0);
                ui.label(
                    RichText::new("ENDERECO DO SERVIDOR RELAY")
                        .size(10.5)
                        .color(TEXT_MUTED)
                        .monospace(),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.relay_input)
                        .desired_width(field_w)
                        .font(egui::TextStyle::Monospace),
                );
            }

            ui.add_space(14.0);
            let btn = egui::Button::new(
                RichText::new("INICIAR SESSAO REMOTA")
                    .size(13.5)
                    .strong()
                    .color(Color32::WHITE),
            )
            .fill(ACCENT_BLUE)
            .min_size(Vec2::new(ui.available_width(), 40.0));

            if ui.add(btn).clicked() {
                self.spawn_new_session();
            }
        });
    }
}

impl eframe::App for EyesLiveApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Intercept window close (X) to keep running in background if enabled
        if ctx.input(|i| i.viewport().close_requested())
            && self.keep_in_background
            && !self.force_quit
        {
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
                            .inner_margin(Margin::symmetric(16.0, 8.0)),
                    )
                    .show(ctx, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            draw_status_dot(ui, STATUS_AMBER);
                            ui.label(
                                RichText::new(format!(
                                    "NOVA VERSAO DISPONIVEL: {} (ATUAL: v{})",
                                    info.latest_version, CURRENT_VERSION
                                ))
                                .color(STATUS_AMBER)
                                .size(11.5)
                                .strong()
                                .monospace(),
                            );

                            if let Some(exe_url) = info.exe_download_url.clone() {
                                let status_clone = self.update_status.clone();
                                let rt_handle = self.rt.handle().clone();
                                if ui
                                    .button(
                                        RichText::new("BAIXAR E ATUALIZAR (.EXE)")
                                            .color(STATUS_EMERALD)
                                            .size(11.0)
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
                                .button(RichText::new("NOTAS DA VERSAO").size(11.0))
                                .clicked()
                            {
                                open_browser_url(&info.release_url);
                            }

                            if ui.small_button(RichText::new("FECHAR").size(10.5)).clicked() {
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
                            .inner_margin(Margin::symmetric(16.0, 8.0)),
                    )
                    .show(ctx, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spinner();
                            ui.label(
                                RichText::new(format!(
                                    "BAIXANDO ATUALIZACAO: {:.0}%",
                                    progress * 100.0
                                ))
                                .size(11.5)
                                .strong()
                                .monospace(),
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
                            .fill(BG_CARD)
                            .stroke(Stroke::new(1.0_f32, STATUS_EMERALD))
                            .inner_margin(Margin::symmetric(16.0, 8.0)),
                    )
                    .show(ctx, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            draw_status_dot(ui, STATUS_EMERALD);
                            ui.label(
                                RichText::new("ATUALIZACAO PRONTA PARA APLICAR")
                                    .color(STATUS_EMERALD)
                                    .size(11.5)
                                    .strong()
                                    .monospace(),
                            );
                            if ui
                                .button(
                                    RichText::new("REINICIAR AGORA")
                                        .color(Color32::WHITE)
                                        .size(11.0)
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

        // Responsive Top Navigation Header (uses horizontal_wrapped so items never collide)
        egui::TopBottomPanel::top("top_nav_bar")
            .frame(
                Frame::none()
                    .fill(BG_PANEL)
                    .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                    .inner_margin(Margin::symmetric(16.0, 10.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    draw_status_dot(ui, ACCENT_BLUE);
                    ui.label(
                        RichText::new("EYESLIVE")
                            .size(17.0)
                            .strong()
                            .color(TEXT_PRIMARY),
                    );
                    if ui
                        .button(
                            RichText::new(format!("{} ({})", AUTHOR_NAME, AUTHOR_SIGNATURE))
                                .size(11.5)
                                .color(ACCENT_CYAN)
                                .monospace(),
                        )
                        .on_hover_text(format!("Abrir GitHub: {}", GITHUB_PROFILE_URL))
                        .clicked()
                    {
                        open_browser_url(GITHUB_PROFILE_URL);
                    }

                    ui.separator();

                    // Workspace / Dashboard Tab
                    let home_selected = self.active_tab == 0;
                    if ui
                        .selectable_label(
                            home_selected,
                            RichText::new("PAINEL PRINCIPAL")
                                .size(12.0)
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
                                    RichText::new(label).size(11.5).strong(),
                                )
                                .clicked()
                            {
                                self.active_tab = s.id;
                            }
                            if ui
                                .small_button(RichText::new("X").size(10.5).color(TEXT_MUTED))
                                .on_hover_text("Encerrar sessao")
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
                });
            });

        // Responsive Bottom Status Bar (wrapped, zero collision between left and right)
        egui::TopBottomPanel::bottom("bottom_status_bar")
            .frame(
                Frame::none()
                    .fill(BG_PANEL)
                    .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                    .inner_margin(Margin::symmetric(16.0, 8.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let inc = *self.active_incoming.lock().unwrap();
                    let hstatus = self.host_status.lock().unwrap().clone();
                    let port = *self.bound_port.lock().unwrap();

                    draw_status_dot(ui, STATUS_EMERALD);
                    ui.label(
                        RichText::new(format!(
                            "HOST: {} ({}:{})  |  CLIENTES: {}  |  ABAS: {}",
                            hstatus.to_uppercase(),
                            self.local_ip,
                            port,
                            inc,
                            self.sessions.len()
                        ))
                        .size(11.0)
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
                                    .size(10.5)
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
                                    .size(10.5)
                                    .strong()
                                    .monospace(),
                                )
                                .clicked()
                            {
                                self.show_update_banner = true;
                            }
                        }
                        UpdateStatus::UpToDate => {
                            if ui
                                .small_button(
                                    RichText::new(format!("v{} ATUALIZADO", CURRENT_VERSION))
                                        .size(10.5)
                                        .color(STATUS_EMERALD)
                                        .monospace(),
                                )
                                .on_hover_text("Clique para verificar novamente")
                                .clicked()
                            {
                                self.trigger_manual_update_check();
                            }
                        }
                        UpdateStatus::Downloading(p) => {
                            ui.label(
                                RichText::new(format!("BAIXANDO: {:.0}%", p * 100.0))
                                    .size(10.5)
                                    .color(ACCENT_CYAN)
                                    .monospace(),
                            );
                        }
                        UpdateStatus::ReadyToRestart(_) => {
                            ui.label(
                                RichText::new("PRONTO PARA REINICIAR")
                                    .size(10.5)
                                    .color(STATUS_EMERALD)
                                    .monospace(),
                            );
                        }
                        _ => {
                            if ui
                                .small_button(
                                    RichText::new(format!("v{} CHECAR UPDATE", CURRENT_VERSION))
                                        .size(10.5)
                                        .color(TEXT_MUTED)
                                        .monospace(),
                                )
                                .clicked()
                            {
                                self.trigger_manual_update_check();
                            }
                        }
                    }

                    ui.separator();

                    if ui
                        .small_button(
                            RichText::new("GITHUB @GABRIELYANDEV")
                                .size(10.5)
                                .strong()
                                .color(ACCENT_CYAN),
                        )
                        .on_hover_text(GITHUB_PROFILE_URL)
                        .clicked()
                    {
                        open_browser_url(GITHUB_PROFILE_URL);
                    }
                    if ui
                        .small_button(
                            RichText::new("REPOSITORIO")
                                .size(10.5)
                                .strong()
                                .color(TEXT_PRIMARY),
                        )
                        .on_hover_text(GITHUB_REPO_URL)
                        .clicked()
                    {
                        open_browser_url(GITHUB_REPO_URL);
                    }
                    if ui
                        .small_button(
                            RichText::new("ENCERRAR PROCESSO")
                                .size(10.5)
                                .strong()
                                .color(STATUS_RED),
                        )
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
                    .fill(BG_BASE)
                    .inner_margin(Margin::symmetric(18.0, 14.0)),
            )
            .show(ctx, |ui| {
                if self.active_tab == 0 {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
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
        let avail_w = ui.available_width();

        // Header Row
        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("ESTACAO DE ACESSO REMOTO  //  SEM LOGIN  //  TEMPO ILIMITADO")
                        .size(11.0)
                        .color(ACCENT_CYAN)
                        .monospace(),
                );
                ui.label(
                    RichText::new("EyesLive Remote Desktop")
                        .size(22.0)
                        .strong()
                        .color(TEXT_PRIMARY),
                );
            });
        });

        ui.add_space(12.0);

        // Responsive Breakpoint: 2 columns on wide screens (>= 820px), stacked 1-column on narrower windows
        if avail_w >= 820.0 {
            ui.columns(2, |cols| {
                self.render_host_card(&mut cols[0], &lan_endpoint);
                self.render_client_card(&mut cols[1]);
            });
        } else {
            self.render_host_card(ui, &lan_endpoint);
            ui.add_space(12.0);
            self.render_client_card(ui);
        }

        ui.add_space(12.0);

        // System Installation & Background Service Card
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
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
                            "INSTALADO ({})",
                            get_installed_exe_path().display()
                        )
                    } else {
                        "MODO PORTATIL (NAO INSTALADO)".to_string()
                    })
                    .size(11.0)
                    .color(if self.installed_status {
                        STATUS_EMERALD
                    } else {
                        TEXT_MUTED
                    })
                    .monospace(),
                );
            });

            ui.add_space(8.0);

            ui.horizontal_wrapped(|ui| {
                let install_label = if self.installed_status {
                    "ATUALIZAR INSTALACAO LOCAL E ATALHOS"
                } else {
                    "INSTALAR EYESLIVE E ATIVAR SEGUNDO PLANO"
                };

                let install_btn = egui::Button::new(
                    RichText::new(install_label)
                        .size(11.5)
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(STATUS_EMERALD);

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
                            self.install_feedback =
                                Some((format!("Erro ao instalar: {}", e), STATUS_RED));
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

            ui.vertical(|ui| {
                if ui
                    .checkbox(
                        &mut self.autostart_enabled,
                        RichText::new(
                            "Iniciar automaticamente em segundo plano junto com o Windows",
                        )
                        .size(12.5),
                    )
                    .changed()
                {
                    let _ = set_autostart(self.autostart_enabled);
                }

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

        ui.add_space(12.0);

        // Release, Updates & Credits Card (fully wrapped for any window width)
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                draw_status_dot(ui, ACCENT_CYAN);
                ui.label(
                    RichText::new(format!(
                        "EYESLIVE v{}  //  POR {} ({})",
                        CURRENT_VERSION,
                        AUTHOR_NAME.to_uppercase(),
                        AUTHOR_SIGNATURE
                    ))
                    .size(12.0)
                    .strong()
                    .monospace(),
                );
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button(
                        RichText::new("VERIFICAR ATUALIZACOES")
                            .size(11.5)
                            .strong(),
                    )
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
                    .clicked()
                {
                    open_browser_url(GITHUB_RELEASES_PAGE);
                }
                if ui
                    .button(
                        RichText::new("PERFIL GITHUB @GABRIELYANDEV")
                            .size(11.5)
                            .strong()
                            .color(ACCENT_CYAN),
                    )
                    .clicked()
                {
                    open_browser_url(GITHUB_PROFILE_URL);
                }
                if ui
                    .button(
                        RichText::new("REPOSITORIO NO GITHUB")
                            .size(11.5)
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .clicked()
                {
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

        // Responsive Session Control Bar (wrapped cleanly so controls never clip off-screen)
        pill_frame(BG_CARD, BORDER_SUBTLE).show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Row 1: Connection Status, Ping, Actions
            ui.horizontal_wrapped(|ui| {
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
                    RichText::new(format!("|  {}x{}  |  QUADROS: {}", remote_w, remote_h, frames_rx))
                        .size(11.0)
                        .color(TEXT_MUTED)
                        .monospace(),
                );

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
                    .button(RichText::new("SINCRONIZAR CLIPBOARD").size(11.0).strong())
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

            ui.separator();

            // Row 2: Quality Presets & Stream Sliders
            ui.horizontal_wrapped(|ui| {
                let mut changed = false;

                if ui
                    .small_button(RichText::new("MODO RAPIDO").size(11.0).strong())
                    .on_hover_text("30 FPS, JPEG 50, Escala 75%")
                    .clicked()
                {
                    session.fps_setting = 30;
                    session.quality_setting = 50;
                    session.scale_setting = 75;
                    changed = true;
                }
                if ui
                    .small_button(RichText::new("EQUILIBRADO").size(11.0).strong())
                    .on_hover_text("25 FPS, JPEG 65, Escala 75%")
                    .clicked()
                {
                    session.fps_setting = 25;
                    session.quality_setting = 65;
                    session.scale_setting = 75;
                    changed = true;
                }
                if ui
                    .small_button(RichText::new("ALTA RESOLUCAO").size(11.0).strong())
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
                            Stroke::new(1.0_f32, BORDER_SUBTLE),
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
            card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
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
