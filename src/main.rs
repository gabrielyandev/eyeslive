#![windows_subsystem = "windows"]

//! EyesLive — Native Rust Remote Desktop Application
//! Created & Signed by Gabriel Yan (@gabrielyandev)
//! GitHub: https://github.com/gabrielyandev/eyeslive
//!
//! Features:
//! - Zero account login & Zero session time limits
//! - Multi-tab simultaneous connections to multiple machines
//! - Direct IP:Port (LAN/VPN) + Optional Self-Hosted Relay by 9-digit Desk ID
//! - Real-time Screen Sharing, Mouse/Keyboard Injection, Clipboard Sync & File Transfer

mod host;
mod protocol;
mod relay;
mod updater;
mod viewer;

use eframe::egui;
use host::{start_host_listener, HostConfig};
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

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("EyesLive — Remote Desktop [by Gabriel Yan @gabrielyandev]")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };

    eframe::run_native(
        "EyesLive [@gabrielyandev]",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(EyesLiveApp::new(rt)))
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
    active_incoming: Arc<Mutex<usize>>,

    // Connection bar
    target_input: String,
    target_pin: String,
    relay_input: String,
    use_relay: bool,

    // Multi-tab concurrent sessions (0 = Home Dashboard, 1..N = Active Remote Sessions)
    sessions: Vec<RemoteSession>,
    active_tab: usize,
    next_session_id: usize,

    // Auto-update system
    update_status: Arc<Mutex<UpdateStatus>>,
    show_update_banner: bool,
}

impl EyesLiveApp {
    fn new(rt: tokio::runtime::Runtime) -> Self {
        let mut rng = rand::thread_rng();
        let p1: u32 = rng.gen_range(100..999);
        let p2: u32 = rng.gen_range(100..999);
        let p3: u32 = rng.gen_range(100..999);
        let my_desk_id = format!("{}-{}-{}", p1, p2, p3);

        let allow_control = Arc::new(Mutex::new(true));
        let active_incoming = Arc::new(Mutex::new(0usize));
        let my_pin = Arc::new(Mutex::new(String::new()));
        let bound_port = Arc::new(Mutex::new(DEFAULT_HOST_PORT));
        let host_status = Arc::new(Mutex::new(format!(
            "Iniciando na porta {}...",
            DEFAULT_HOST_PORT
        )));

        let host_cfg = HostConfig {
            desk_id: my_desk_id.clone(),
            host_name: std::env::var("COMPUTERNAME").unwrap_or_else(|_| "EyesLive-PC".into()),
            pin: my_pin.clone(),
            allow_control: allow_control.clone(),
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

        Self {
            rt,
            my_desk_id,
            local_ip: detect_local_ip(),
            my_pin,
            bound_port,
            host_status,
            allow_control,
            active_incoming,
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
}

impl eframe::App for EyesLiveApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(16));

        // Auto-update banner notification
        let cur_update = self.update_status.lock().unwrap().clone();
        match cur_update {
            UpdateStatus::Available(ref info) if self.show_update_banner => {
                egui::TopBottomPanel::top("update_notification_banner").show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "🎉 Nova versão disponível: {} (versão atual: v{})!",
                                info.latest_version, CURRENT_VERSION
                            ))
                            .color(egui::Color32::from_rgb(250, 204, 21))
                            .strong(),
                        );

                        if let Some(exe_url) = info.exe_download_url.clone() {
                            let status_clone = self.update_status.clone();
                            let rt_handle = self.rt.handle().clone();
                            if ui
                                .button(
                                    egui::RichText::new("⚡ Baixar e Atualizar Automaticamente (.exe)")
                                        .color(egui::Color32::from_rgb(74, 222, 128))
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

                        if ui.button("🌐 Ver Notas da Versão").clicked() {
                            open_browser_url(&info.release_url);
                        }

                        if ui.small_button("✖").clicked() {
                            self.show_update_banner = false;
                        }
                    });
                });
            }
            UpdateStatus::Downloading(progress) => {
                egui::TopBottomPanel::top("download_progress_banner").show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            egui::RichText::new(format!(
                                "Baixando nova versão do EyesLive: {:.0}%...",
                                progress * 100.0
                            ))
                            .strong(),
                        );
                        ui.add(egui::ProgressBar::new(progress).desired_width(200.0));
                    });
                });
            }
            UpdateStatus::ReadyToRestart(ref new_exe) => {
                let exe_to_run = new_exe.clone();
                egui::TopBottomPanel::top("ready_restart_banner").show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("✅ Nova versão baixada com sucesso!")
                                .color(egui::Color32::from_rgb(74, 222, 128))
                                .strong(),
                        );
                        if ui
                            .button(
                                egui::RichText::new("🚀 Reiniciar Agora com a Nova Versão")
                                    .color(egui::Color32::GREEN)
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

        // Top Bar with Multi-Tab Navigation & Clickable @gabrielyandev GitHub Link
        egui::TopBottomPanel::top("top_tab_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(
                    egui::RichText::new("👁 EyesLive")
                        .color(egui::Color32::from_rgb(239, 68, 68))
                        .strong(),
                );
                if ui
                    .button(
                        egui::RichText::new(format!("🔗 {} ({})", AUTHOR_NAME, AUTHOR_SIGNATURE))
                            .color(egui::Color32::from_rgb(56, 189, 248))
                            .monospace(),
                    )
                    .on_hover_text(format!("Abrir GitHub: {}", GITHUB_PROFILE_URL))
                    .clicked()
                {
                    open_browser_url(GITHUB_PROFILE_URL);
                }
                ui.separator();

                if ui
                    .selectable_label(self.active_tab == 0, "🏠 Painel Principal")
                    .clicked()
                {
                    self.active_tab = 0;
                }

                let mut close_id = None;
                for s in &self.sessions {
                    let st = s.state.lock().unwrap();
                    let label = format!("🖥 {} (#{})", st.host_name, s.id);
                    drop(st);
                    ui.group(|ui| {
                        if ui.selectable_label(self.active_tab == s.id, label).clicked() {
                            self.active_tab = s.id;
                        }
                        if ui.small_button("✖").clicked() {
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

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("🔓 Sem Login • ♾ Sem Limite de Tempo • Multi-Sessão")
                            .color(egui::Color32::from_rgb(74, 222, 128)),
                    );
                });
            });
        });

        // Bottom Status Bar with Version, Update Indicator & Clickable GitHub Buttons
        egui::TopBottomPanel::bottom("bottom_status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let inc = *self.active_incoming.lock().unwrap();
                let hstatus = self.host_status.lock().unwrap().clone();
                ui.label(format!(
                    "Host: {} (IP: {}:{}) | Clientes: {} | Abas: {}",
                    hstatus,
                    self.local_ip,
                    *self.bound_port.lock().unwrap(),
                    inc,
                    self.sessions.len()
                ));

                ui.separator();

                let cur_up = self.update_status.lock().unwrap().clone();
                match cur_up {
                    UpdateStatus::Checking => {
                        ui.spinner();
                        ui.label("Verificando atualizações...");
                    }
                    UpdateStatus::Available(ref info) => {
                        if ui
                            .small_button(
                                egui::RichText::new(format!("🔔 Atualização {} Disponível!", info.latest_version))
                                    .color(egui::Color32::from_rgb(250, 204, 21))
                                    .strong(),
                            )
                            .on_hover_text("Clique para reabrir o aviso ou baixar")
                            .clicked()
                        {
                            self.show_update_banner = true;
                        }
                    }
                    UpdateStatus::UpToDate => {
                        if ui
                            .small_button(format!("✅ v{} (Atualizado)", CURRENT_VERSION))
                            .on_hover_text("Versão mais recente. Clique para rechecar.")
                            .clicked()
                        {
                            self.trigger_manual_update_check();
                        }
                    }
                    UpdateStatus::Downloading(p) => {
                        ui.label(format!("⏳ Baixando: {:.0}%", p * 100.0));
                    }
                    UpdateStatus::ReadyToRestart(_) => {
                        ui.label(egui::RichText::new("🚀 Pronto para reiniciar!").color(egui::Color32::GREEN));
                    }
                    _ => {
                        if ui
                            .small_button(format!("v{} • Checar Atualizações", CURRENT_VERSION))
                            .on_hover_text("Clique para verificar se há nova versão no GitHub")
                            .clicked()
                        {
                            self.trigger_manual_update_check();
                        }
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("🌐 GitHub @gabrielyandev")
                        .on_hover_text(GITHUB_PROFILE_URL)
                        .clicked()
                    {
                        open_browser_url(GITHUB_PROFILE_URL);
                    }
                    if ui
                        .small_button("⭐ Repositório EyesLive")
                        .on_hover_text(GITHUB_REPO_URL)
                        .clicked()
                    {
                        open_browser_url(GITHUB_REPO_URL);
                    }
                    ui.label(format!(
                        "Créditos: {} ({})",
                        AUTHOR_NAME, AUTHOR_SIGNATURE
                    ));
                });
            });
        });

        // Main Area
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.active_tab == 0 {
                self.render_dashboard(ui);
            } else {
                let rt_handle = self.rt.handle().clone();
                if let Some(session) = self.sessions.iter_mut().find(|s| s.id == self.active_tab) {
                    Self::render_remote_session(ctx, ui, session, &rt_handle);
                } else {
                    self.active_tab = 0;
                }
            }
        });
    }
}

impl EyesLiveApp {
    fn render_dashboard(&mut self, ui: &mut egui::Ui) {
        let cur_port = *self.bound_port.lock().unwrap();
        ui.add_space(12.0);
        ui.columns(2, |cols| {
            // Left Column: This Desk (Host Info)
            cols[0].group(|ui| {
                ui.heading("📍 Este Computador (Host EyesLive)");
                ui.add_space(8.0);
                ui.label("Compartilhe seu ID ou IP:Porta para receber acesso remoto ilimitado:");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Seu EyesLive ID:");
                    ui.label(
                        egui::RichText::new(&self.my_desk_id)
                            .size(28.0)
                            .strong()
                            .color(egui::Color32::from_rgb(248, 113, 113)),
                    );
                });
                ui.label(
                    egui::RichText::new(format!(
                        "Endereço Direto na Rede (LAN/VPN): {}:{}",
                        self.local_ip, cur_port
                    ))
                    .color(egui::Color32::from_rgb(56, 189, 248))
                    .strong(),
                );
                ui.add_space(8.0);

                let mut allow = *self.allow_control.lock().unwrap();
                if ui
                    .checkbox(&mut allow, "Permitir controle de mouse e teclado")
                    .changed()
                {
                    *self.allow_control.lock().unwrap() = allow;
                }

                ui.horizontal(|ui| {
                    ui.label("PIN Opcional (vazio = livre):");
                    let mut pin_guard = self.my_pin.lock().unwrap();
                    ui.text_edit_singleline(&mut *pin_guard);
                });
            });

            // Right Column: Connect to Remote Desk (Multi-Session)
            cols[1].group(|ui| {
                ui.heading("🚀 Conectar a Outra Máquina");
                ui.add_space(8.0);
                ui.label("Abra quantas conexões simultâneas quiser em abas independentes:");
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label("IP:Porta ou ID:");
                    ui.text_edit_singleline(&mut self.target_input);
                });
                ui.horizontal(|ui| {
                    ui.label("PIN Remoto (se houver):");
                    ui.text_edit_singleline(&mut self.target_pin);
                });
                ui.checkbox(&mut self.use_relay, "Usar Servidor Relay por ID");
                if self.use_relay {
                    ui.horizontal(|ui| {
                        ui.label("Endereço do Relay:");
                        ui.text_edit_singleline(&mut self.relay_input);
                    });
                }

                ui.add_space(10.0);
                if ui
                    .button(
                        egui::RichText::new("➕ Nova Sessão Remota Simultânea")
                            .size(18.0)
                            .strong(),
                    )
                    .clicked()
                {
                    self.spawn_new_session();
                }
            });
        });

        ui.add_space(16.0);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("🔄 EyesLive Desktop v{}", CURRENT_VERSION))
                        .strong(),
                );
                ui.separator();
                if ui
                    .button("🔍 Checar Atualizações no Repositório")
                    .on_hover_text("Consulta se há novas versões ou alterações publicadas no GitHub")
                    .clicked()
                {
                    self.trigger_manual_update_check();
                }
                if ui
                    .button(
                        egui::RichText::new("⬇ Baixar Executável Portátil (.exe)")
                            .color(egui::Color32::from_rgb(56, 189, 248)),
                    )
                    .on_hover_text("Abrir página de download direto do executável no GitHub Releases")
                    .clicked()
                {
                    open_browser_url(GITHUB_RELEASES_PAGE);
                }
            });
        });

        ui.add_space(8.0);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "👨‍💻 Desenvolvido por {} ({})",
                        AUTHOR_NAME, AUTHOR_SIGNATURE
                    ))
                    .strong(),
                );
                if ui
                    .button("🌐 Acessar Perfil no GitHub (github.com/gabrielyandev)")
                    .clicked()
                {
                    open_browser_url(GITHUB_PROFILE_URL);
                }
                if ui
                    .button("📦 Ver Projeto EyesLive no GitHub")
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
        let (status, remote_w, remote_h, remote_clip, file_ack, frames_rx, ping_rtt) = {
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

        // Session Toolbar
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&status).strong());

            if let Some(rtt) = ping_rtt {
                let (color, icon) = if rtt < 40 {
                    (egui::Color32::from_rgb(74, 222, 128), "🟢")
                } else if rtt < 100 {
                    (egui::Color32::from_rgb(250, 204, 21), "🟡")
                } else {
                    (egui::Color32::from_rgb(248, 113, 113), "🔴")
                };
                ui.label(
                    egui::RichText::new(format!("{} Ping: {}ms", icon, rtt))
                        .color(color)
                        .strong(),
                );
            }

            ui.label(format!("| Quadros: {}", frames_rx));
            ui.separator();

            let mut changed = false;

            // One-click performance presets
            if ui
                .small_button("⚡ Modo Rápido")
                .on_hover_text("Máxima velocidade e menor latência (30 FPS, JPEG 50, Escala 75%)")
                .clicked()
            {
                session.fps_setting = 30;
                session.quality_setting = 50;
                session.scale_setting = 75;
                changed = true;
            }
            if ui
                .small_button("⚖ Equilibrado")
                .on_hover_text("Boa imagem e baixa latência (25 FPS, JPEG 65, Escala 75%)")
                .clicked()
            {
                session.fps_setting = 25;
                session.quality_setting = 65;
                session.scale_setting = 75;
                changed = true;
            }
            if ui
                .small_button("🎨 Alta Resolução")
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
                    egui::Slider::new(&mut session.quality_setting, 20..=95).text("Qualidade JPEG"),
                )
                .changed();
            changed |= ui
                .add(egui::Slider::new(&mut session.scale_setting, 40..=100).text("Escala %"))
                .changed();
            changed |= ui
                .checkbox(&mut session.view_only, "Somente Visualização")
                .changed();

            if changed {
                let _ = session.tx.send(Packet::StreamConfig {
                    fps: session.fps_setting,
                    jpeg_quality: session.quality_setting,
                    view_only: session.view_only,
                    scale_pct: session.scale_setting,
                });
            }

            if ui.button("📁 Enviar Arquivo").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    session.send_file(path, rt);
                }
            }

            if ui.button("📋 Sincronizar Clipboard").clicked() {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    if let Ok(txt) = cb.get_text() {
                        let _ = session.tx.send(Packet::ClipboardText { text: txt });
                    }
                }
            }

            if let Some(ack) = file_ack {
                ui.label(egui::RichText::new(ack).color(egui::Color32::LIGHT_GREEN));
            }
            if !remote_clip.is_empty() {
                let preview: String = remote_clip.chars().take(24).collect();
                ui.label(format!("📋 Clip: {}", preview));
            }
        });

        ui.separator();

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
                egui::Color32::WHITE,
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
            ui.centered_and_justified(|ui| {
                ui.label("Aguardando primeiro quadro de vídeo do Host...");
            });
        }
    }
}
