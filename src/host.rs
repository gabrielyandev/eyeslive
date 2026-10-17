//! GabrielDesk Host Engine — by @gabrielyandev
//! Handles unlimited simultaneous incoming remote desktop sessions, real-time JPEG screen
//! streaming, mouse/keyboard injection, clipboard synchronization, and file reception.

use crate::protocol::{recv_packet, send_packet, MouseBtn, Packet, AUTHOR_SIGNATURE};
use arboard::Clipboard;
use enigo::{Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use image::codecs::jpeg::JpegEncoder;
use image::ColorType;
use screenshots::Screen;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::unbounded_channel;

#[derive(Clone)]
pub struct HostConfig {
    pub desk_id: String,
    pub host_name: String,
    pub pin: Arc<Mutex<String>>,
    pub allow_control: Arc<Mutex<bool>>,
    pub active_clients: Arc<Mutex<usize>>,
    pub bound_port: Arc<Mutex<u16>>,
    pub host_status: Arc<Mutex<String>>,
    pub download_dir: PathBuf,
}

pub async fn start_host_listener(base_port: u16, config: HostConfig) -> anyhow::Result<()> {
    let mut listener = None;
    let mut chosen_port = base_port;

    for port in base_port..(base_port + 20) {
        let addr = format!("0.0.0.0:{}", port);
        if let Ok(l) = TcpListener::bind(&addr).await {
            listener = Some(l);
            chosen_port = port;
            break;
        }
    }

    let listener = match listener {
        Some(l) => {
            *config.bound_port.lock().unwrap() = chosen_port;
            *config.host_status.lock().unwrap() = format!("Ativo na porta {}", chosen_port);
            l
        }
        None => {
            *config.host_status.lock().unwrap() = "Erro ao abrir porta TCP".into();
            anyhow::bail!("Could not bind any port in range");
        }
    };

    loop {
        let (stream, _peer) = listener.accept().await?;
        let _ = stream.set_nodelay(true);
        let cfg = config.clone();
        tokio::spawn(async move {
            let _ = handle_client_session(stream, cfg).await;
        });
    }
}

pub async fn handle_client_session(mut stream: TcpStream, config: HostConfig) -> anyhow::Result<()> {
    // 1. Handshake (No account login required; optional PIN check only if configured)
    let hello = recv_packet(&mut stream).await?;
    let expected_pin = config.pin.lock().unwrap().trim().to_string();
    let pin_ok = match hello {
        Packet::Hello { pin, .. } => {
            if expected_pin.is_empty() {
                true
            } else {
                pin.as_deref() == Some(expected_pin.as_str())
            }
        }
        _ => anyhow::bail!("Expected Hello packet"),
    };

    let (scr_w, scr_h) = get_primary_screen_size().unwrap_or((1920, 1080));

    if !pin_ok {
        send_packet(
            &mut stream,
            &Packet::Welcome {
                host_name: config.host_name,
                desk_id: config.desk_id,
                screen_width: scr_w,
                screen_height: scr_h,
                accepted: false,
                reason: Some("PIN de acesso incorreto".into()),
                signature: AUTHOR_SIGNATURE.into(),
            },
        )
        .await?;
        return Ok(());
    }

    send_packet(
        &mut stream,
        &Packet::Welcome {
            host_name: config.host_name.clone(),
            desk_id: config.desk_id.clone(),
            screen_width: scr_w,
            screen_height: scr_h,
            accepted: true,
            reason: None,
            signature: AUTHOR_SIGNATURE.into(),
        },
    )
    .await?;

    *config.active_clients.lock().unwrap() += 1;

    let (mut reader, mut writer) = stream.into_split();
    let (tx_out, mut rx_out) = unbounded_channel::<Packet>();

    let fps = Arc::new(Mutex::new(20u8));
    let quality = Arc::new(Mutex::new(65u8));
    let view_only = Arc::new(Mutex::new(false));

    // Dedicated cancel-safe reader task for client input/files
    let fps_r = fps.clone();
    let quality_r = quality.clone();
    let view_only_r = view_only.clone();
    let allow_ctrl = config.allow_control.clone();
    let download_dir = config.download_dir.clone();
    let tx_ack = tx_out.clone();

    let reader_handle = tokio::spawn(async move {
        while let Ok(pkt) = recv_packet(&mut reader).await {
            match pkt {
                Packet::StreamConfig {
                    fps: new_fps,
                    jpeg_quality,
                    view_only: vo,
                } => {
                    *fps_r.lock().unwrap() = new_fps.clamp(1, 60);
                    *quality_r.lock().unwrap() = jpeg_quality.clamp(10, 95);
                    *view_only_r.lock().unwrap() = vo;
                }
                Packet::MouseMove { norm_x, norm_y } => {
                    if *allow_ctrl.lock().unwrap() && !*view_only_r.lock().unwrap() {
                        inject_mouse_move(norm_x, norm_y, scr_w, scr_h);
                    }
                }
                Packet::MouseButton {
                    button,
                    pressed,
                    norm_x,
                    norm_y,
                } => {
                    if *allow_ctrl.lock().unwrap() && !*view_only_r.lock().unwrap() {
                        inject_mouse_button(button, pressed, norm_x, norm_y, scr_w, scr_h);
                    }
                }
                Packet::MouseScroll {
                    delta_x: _,
                    delta_y,
                } => {
                    if *allow_ctrl.lock().unwrap() && !*view_only_r.lock().unwrap() {
                        inject_mouse_scroll(delta_y);
                    }
                }
                Packet::KeyEvent { key, pressed } => {
                    if *allow_ctrl.lock().unwrap() && !*view_only_r.lock().unwrap() {
                        inject_key(&key, pressed);
                    }
                }
                Packet::TextInput { text } => {
                    if *allow_ctrl.lock().unwrap() && !*view_only_r.lock().unwrap() {
                        if let Ok(mut enigo) = Enigo::new(&Settings::default()) {
                            let _ = enigo.text(&text);
                        }
                    }
                }
                Packet::ClipboardText { text } => {
                    if let Ok(mut cb) = Clipboard::new() {
                        let _ = cb.set_text(text);
                    }
                }
                Packet::FileChunk {
                    file_name,
                    offset,
                    data,
                    eof,
                } => {
                    let safe_name = std::path::Path::new(&file_name)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("transfer.bin");
                    let dest = download_dir.join(safe_name);
                    if let Ok(mut file) = OpenOptions::new().create(true).write(true).open(&dest) {
                        let _ = file.seek(SeekFrom::Start(offset));
                        let _ = file.write_all(&data);
                        if eof {
                            let _ = tx_ack.send(Packet::FileAck {
                                file_name,
                                saved_path: dest.display().to_string(),
                            });
                        }
                    }
                }
                Packet::Ping(ts) => {
                    let _ = tx_ack.send(Packet::Pong(ts));
                }
                _ => {}
            }
        }
    });

    // Dedicated screen capture producer
    let tx_frames = tx_out.clone();
    let capture_handle = tokio::spawn(async move {
        let mut last_clip = String::new();
        loop {
            let cur_fps = (*fps.lock().unwrap()).max(1) as u64;
            let q = *quality.lock().unwrap();
            let frame_res = tokio::task::spawn_blocking(move || capture_screen_jpeg(q)).await;
            if let Ok(Ok((w, h, jpeg_bytes))) = frame_res {
                let now_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                if tx_frames
                    .send(Packet::VideoFrame {
                        width: w,
                        height: h,
                        jpeg_bytes,
                        timestamp_ms: now_ms,
                    })
                    .is_err()
                {
                    break;
                }
            }

            if let Ok(mut cb) = Clipboard::new() {
                if let Ok(text) = cb.get_text() {
                    if !text.is_empty() && text != last_clip {
                        last_clip = text.clone();
                        let _ = tx_frames.send(Packet::ClipboardText { text });
                    }
                }
            }

            tokio::time::sleep(Duration::from_millis((1000 / cur_fps).max(16))).await;
        }
    });

    while let Some(pkt) = rx_out.recv().await {
        if send_packet(&mut writer, &pkt).await.is_err() {
            break;
        }
    }

    reader_handle.abort();
    capture_handle.abort();

    let mut count = config.active_clients.lock().unwrap();
    *count = count.saturating_sub(1);
    Ok(())
}

fn get_primary_screen_size() -> anyhow::Result<(u32, u32)> {
    let screens = Screen::all()?;
    let primary = screens
        .first()
        .ok_or_else(|| anyhow::anyhow!("No screen found"))?;
    Ok((primary.display_info.width, primary.display_info.height))
}

fn capture_screen_jpeg(quality: u8) -> anyhow::Result<(u32, u32, Vec<u8>)> {
    let screens = Screen::all()?;
    let primary = screens
        .first()
        .ok_or_else(|| anyhow::anyhow!("No screen found"))?;
    let image = primary.capture()?;
    let w = image.width();
    let h = image.height();
    let rgba = image.into_raw();
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for chunk in rgba.chunks_exact(4) {
        rgb.push(chunk[0]);
        rgb.push(chunk[1]);
        rgb.push(chunk[2]);
    }
    let mut jpeg_bytes = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut jpeg_bytes, quality);
    encoder.encode(&rgb, w, h, ColorType::Rgb8.into())?;
    Ok((w, h, jpeg_bytes))
}

fn inject_mouse_move(norm_x: f32, norm_y: f32, w: u32, h: u32) {
    if let Ok(mut enigo) = Enigo::new(&Settings::default()) {
        let x = (norm_x.clamp(0.0, 1.0) * w as f32) as i32;
        let y = (norm_y.clamp(0.0, 1.0) * h as f32) as i32;
        let _ = enigo.move_mouse(x, y, Coordinate::Abs);
    }
}

fn inject_mouse_button(btn: MouseBtn, pressed: bool, norm_x: f32, norm_y: f32, w: u32, h: u32) {
    if let Ok(mut enigo) = Enigo::new(&Settings::default()) {
        let x = (norm_x.clamp(0.0, 1.0) * w as f32) as i32;
        let y = (norm_y.clamp(0.0, 1.0) * h as f32) as i32;
        let _ = enigo.move_mouse(x, y, Coordinate::Abs);
        let b = match btn {
            MouseBtn::Left => Button::Left,
            MouseBtn::Right => Button::Right,
            MouseBtn::Middle => Button::Middle,
        };
        let dir = if pressed {
            Direction::Press
        } else {
            Direction::Release
        };
        let _ = enigo.button(b, dir);
    }
}

fn inject_mouse_scroll(delta_y: i32) {
    if let Ok(mut enigo) = Enigo::new(&Settings::default()) {
        let _ = enigo.scroll(delta_y, enigo::Axis::Vertical);
    }
}

fn inject_key(key_name: &str, pressed: bool) {
    if let Ok(mut enigo) = Enigo::new(&Settings::default()) {
        let dir = if pressed {
            Direction::Press
        } else {
            Direction::Release
        };
        let mapped = match key_name {
            "Enter" => Some(Key::Return),
            "Backspace" => Some(Key::Backspace),
            "Tab" => Some(Key::Tab),
            "Escape" => Some(Key::Escape),
            "Space" => Some(Key::Space),
            "ArrowUp" => Some(Key::UpArrow),
            "ArrowDown" => Some(Key::DownArrow),
            "ArrowLeft" => Some(Key::LeftArrow),
            "ArrowRight" => Some(Key::RightArrow),
            "Delete" => Some(Key::Delete),
            _ => None,
        };
        if let Some(k) = mapped {
            let _ = enigo.key(k, dir);
        }
    }
}
