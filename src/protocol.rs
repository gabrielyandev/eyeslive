//! EyesLive Wire Protocol — Created by Gabriel Yan (@gabrielyandev)
//! GitHub: https://github.com/gabrielyandev/eyeslive
//! Supports direct TCP P2P connections and optional Relay routing by Desk ID,
//! with zero account login, zero time limits, and unlimited concurrent sessions.

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const DEFAULT_HOST_PORT: u16 = 9090;
pub const DEFAULT_RELAY_PORT: u16 = 9091;
pub const AUTHOR_NAME: &str = "Gabriel Yan";
pub const AUTHOR_SIGNATURE: &str = "@gabrielyandev";
pub const GITHUB_PROFILE_URL: &str = "https://github.com/gabrielyandev";
pub const GITHUB_REPO_URL: &str = "https://github.com/gabrielyandev/eyeslive";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Packet {
    // Relay & Handshake
    RegisterHost {
        desk_id: String,
    },
    ConnectToHost {
        target_desk_id: String,
    },
    RelayStatus {
        ok: bool,
        message: String,
    },
    Hello {
        client_name: String,
        pin: Option<String>,
        signature: String,
    },
    Welcome {
        host_name: String,
        desk_id: String,
        screen_width: u32,
        screen_height: u32,
        accepted: bool,
        reason: Option<String>,
        signature: String,
    },

    // Real-time Screen Streaming
    VideoFrame {
        width: u32,
        height: u32,
        jpeg_bytes: Vec<u8>,
        timestamp_ms: u64,
    },
    StreamConfig {
        fps: u8,
        jpeg_quality: u8,
        view_only: bool,
    },

    // Remote Input Injection
    MouseMove {
        norm_x: f32,
        norm_y: f32,
    },
    MouseButton {
        button: MouseBtn,
        pressed: bool,
        norm_x: f32,
        norm_y: f32,
    },
    MouseScroll {
        delta_x: i32,
        delta_y: i32,
    },
    KeyEvent {
        key: String,
        pressed: bool,
    },
    TextInput {
        text: String,
    },

    // Clipboard Sync
    ClipboardText {
        text: String,
    },

    // File Transfer
    FileOffer {
        file_name: String,
        total_bytes: u64,
    },
    FileChunk {
        file_name: String,
        offset: u64,
        data: Vec<u8>,
        eof: bool,
    },
    FileAck {
        file_name: String,
        saved_path: String,
    },

    // Keepalive
    Ping(u64),
    Pong(u64),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MouseBtn {
    Left,
    Right,
    Middle,
}

/// Framed async packet writer (4-byte little-endian length prefix + bincode payload)
pub async fn send_packet<W: AsyncWrite + Unpin>(writer: &mut W, packet: &Packet) -> anyhow::Result<()> {
    let payload = bincode::serialize(packet)?;
    let len = payload.len() as u32;
    writer.write_u32_le(len).await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

/// Framed async packet reader (bounded to 32 MiB max frame size)
pub async fn recv_packet<R: AsyncRead + Unpin>(reader: &mut R) -> anyhow::Result<Packet> {
    let len = reader.read_u32_le().await? as usize;
    if len > 32 * 1024 * 1024 {
        anyhow::bail!("Packet exceeds 32 MiB limit: {} bytes", len);
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    let pkt: Packet = bincode::deserialize(&buf)?;
    Ok(pkt)
}
