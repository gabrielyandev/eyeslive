//! EyesLive Multi-Session Viewer Client — Created by Gabriel Yan (@gabrielyandev)
//! GitHub: https://github.com/gabrielyandev/eyeslive
//! Manages concurrent remote desktop sessions with independent tabs, no session time limits,
//! bidirectional clipboard sync, and chunked file transfers.

use crate::protocol::{recv_packet, send_packet, Packet, AUTHOR_SIGNATURE};
use egui::ColorImage;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

pub struct RemoteSession {
    pub id: usize,
    #[allow(dead_code)]
    pub target: String,
    pub state: Arc<Mutex<SessionState>>,
    pub tx: UnboundedSender<Packet>,
    pub texture: Option<egui::TextureHandle>,
    pub fps_setting: u8,
    pub quality_setting: u8,
    pub scale_setting: u8,
    pub view_only: bool,
    pub last_mouse_pos: (f32, f32),
    pub last_mouse_sent: Instant,
    pub last_ping_sent: Instant,
}

pub struct SessionState {
    pub connected: bool,
    pub status_text: String,
    pub host_name: String,
    pub desk_id: String,
    pub remote_width: u32,
    pub remote_height: u32,
    pub latest_frame: Option<ColorImage>,
    pub frame_dirty: bool,
    pub frames_received: u64,
    pub remote_clipboard: String,
    pub last_file_ack: Option<String>,
    pub ping_rtt_ms: Option<u64>,
}

impl RemoteSession {
    pub fn connect(
        id: usize,
        target: String,
        relay_addr: Option<String>,
        pin: Option<String>,
        rt: &tokio::runtime::Handle,
    ) -> Self {
        let (tx, rx) = unbounded_channel::<Packet>();
        let is_loopback = target.starts_with("127.0.0.1") || target.starts_with("localhost");

        let state = Arc::new(Mutex::new(SessionState {
            connected: false,
            status_text: format!("Conectando a {}...", target),
            host_name: target.clone(),
            desk_id: String::new(),
            remote_width: 1920,
            remote_height: 1080,
            latest_frame: None,
            frame_dirty: false,
            frames_received: 0,
            remote_clipboard: String::new(),
            last_file_ack: None,
            ping_rtt_ms: None,
        }));

        let state_clone = state.clone();
        let target_clone = target.clone();
        rt.spawn(async move {
            if let Err(e) =
                run_viewer_connection(target_clone, relay_addr, pin, state_clone.clone(), rx).await
            {
                let mut st = state_clone.lock().unwrap();
                st.connected = false;
                st.status_text = format!("Desconectado: {}", e);
            }
        });

        Self {
            id,
            target,
            state,
            tx,
            texture: None,
            fps_setting: 30,
            quality_setting: 55,
            scale_setting: 75,
            view_only: is_loopback,
            last_mouse_pos: (-1.0, -1.0),
            last_mouse_sent: Instant::now(),
            last_ping_sent: Instant::now(),
        }
    }

    pub fn send_file(&self, path: PathBuf, rt: &tokio::runtime::Handle) {
        let tx = self.tx.clone();
        rt.spawn(async move {
            if let Ok(bytes) = tokio::fs::read(&path).await {
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("upload.bin")
                    .to_string();
                let total_bytes = bytes.len() as u64;
                let _ = tx.send(Packet::FileOffer {
                    file_name: file_name.clone(),
                    total_bytes,
                });
                let chunk_size = 64 * 1024;
                let mut offset = 0u64;
                for chunk in bytes.chunks(chunk_size) {
                    let eof = (offset + chunk.len() as u64) >= total_bytes;
                    let _ = tx.send(Packet::FileChunk {
                        file_name: file_name.clone(),
                        offset,
                        data: chunk.to_vec(),
                        eof,
                    });
                    offset += chunk.len() as u64;
                }
            }
        });
    }
}

async fn run_viewer_connection(
    target: String,
    relay_addr: Option<String>,
    pin: Option<String>,
    state: Arc<Mutex<SessionState>>,
    mut rx: UnboundedReceiver<Packet>,
) -> anyhow::Result<()> {
    let mut stream = if target.contains(':') || target.contains('.') {
        let addr = if target.contains(':') {
            target.clone()
        } else {
            format!("{}:{}", target, crate::protocol::DEFAULT_HOST_PORT)
        };
        TcpStream::connect(&addr).await?
    } else if let Some(relay) = relay_addr {
        let mut s = TcpStream::connect(&relay)
            .await
            .map_err(|_| anyhow::anyhow!("Não foi possível conectar à rede de resolução de ID"))?;
        send_packet(
            &mut s,
            &Packet::ConnectToHost {
                target_desk_id: target.clone(),
            },
        )
        .await?;
        match recv_packet(&mut s).await? {
            Packet::RelayStatus { ok: true, .. } => s,
            Packet::RelayStatus { ok: false, message } => anyhow::bail!(message),
            _ => anyhow::bail!("Resposta inesperada do servidor de conexão"),
        }
    } else {
        anyhow::bail!(
            "Destino inválido ('{}'). Informe um EyesLive ID (ex: 462-152-868) ou IP:Porta (ex: 192.168.1.50:{})",
            target,
            crate::protocol::DEFAULT_HOST_PORT
        );
    };

    let _ = stream.set_nodelay(true);

    send_packet(
        &mut stream,
        &Packet::Hello {
            client_name: std::env::var("COMPUTERNAME")
                .unwrap_or_else(|_| "EyesLive Viewer".into()),
            pin,
            signature: AUTHOR_SIGNATURE.into(),
        },
    )
    .await?;

    match recv_packet(&mut stream).await? {
        Packet::Welcome {
            host_name,
            desk_id,
            screen_width,
            screen_height,
            accepted,
            reason,
            ..
        } => {
            if !accepted {
                anyhow::bail!(reason.unwrap_or_else(|| "Conexão recusada pelo Host".into()));
            }
            let mut st = state.lock().unwrap();
            st.connected = true;
            st.host_name = host_name.clone();
            st.desk_id = desk_id.clone();
            st.remote_width = screen_width;
            st.remote_height = screen_height;
            st.status_text = format!(
                "ONLINE — Conectado a {} (ID {}) — Sessão Ilimitada",
                host_name, desk_id
            );
        }
        _ => anyhow::bail!("Handshake inválido"),
    }

    let (mut reader, mut writer) = stream.into_split();

    let writer_handle = tokio::spawn(async move {
        while let Some(out_pkt) = rx.recv().await {
            if send_packet(&mut writer, &out_pkt).await.is_err() {
                break;
            }
        }
    });

    while let Ok(in_pkt) = recv_packet(&mut reader).await {
        match in_pkt {
            Packet::VideoFrame {
                width: _,
                height: _,
                jpeg_bytes,
                timestamp_ms: _,
            } => {
                if let Ok(img) = image::load_from_memory(&jpeg_bytes) {
                    let rgba = img.to_rgba8();
                    let size = [rgba.width() as usize, rgba.height() as usize];
                    let color_img = ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                    let mut st = state.lock().unwrap();
                    st.remote_width = size[0] as u32;
                    st.remote_height = size[1] as u32;
                    st.latest_frame = Some(color_img);
                    st.frame_dirty = true;
                    st.frames_received += 1;
                }
            }
            Packet::ClipboardText { text } => {
                let mut st = state.lock().unwrap();
                st.remote_clipboard = text;
            }
            Packet::FileAck {
                file_name,
                saved_path,
            } => {
                let mut st = state.lock().unwrap();
                st.last_file_ack = Some(format!("Salvo '{}' em {}", file_name, saved_path));
            }
            Packet::Pong(sent_ts) => {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let rtt = now_ms.saturating_sub(sent_ts);
                let mut st = state.lock().unwrap();
                st.ping_rtt_ms = Some(rtt);
            }
            _ => {}
        }
    }

    writer_handle.abort();
    anyhow::bail!("Conexão encerrada pelo Host")
}
