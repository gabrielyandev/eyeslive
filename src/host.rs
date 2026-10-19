//! EyesLive Host Engine — Created by Gabriel Yan (@gabrielyandev)
//! GitHub: https://github.com/gabrielyandev/eyeslive
//! Handles unlimited simultaneous incoming remote desktop sessions, interactive user consent
//! prompts, real-time JPEG screen streaming, mouse/keyboard injection, clipboard sync, and file reception.

use crate::protocol::{recv_packet, send_packet, MouseBtn, Packet, AUTHOR_SIGNATURE};
use arboard::Clipboard;
use enigo::{Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use image::codecs::jpeg::JpegEncoder;
use image::ColorType;
use screenshots::Screen;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::oneshot;

static NEXT_CONSENT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentDecision {
    AllowFullControl,
    AllowViewOnly,
    Reject,
}

pub struct PendingConsent {
    pub id: u64,
    pub client_name: String,
    pub peer_addr: String,
    pub requested_at: Instant,
    pub response_tx: Option<oneshot::Sender<ConsentDecision>>,
}

#[derive(Clone)]
pub struct HostConfig {
    pub desk_id: String,
    pub host_name: String,
    pub pin: Arc<Mutex<String>>,
    pub allow_control: Arc<Mutex<bool>>,
    pub require_consent: Arc<Mutex<bool>>,
    pub pending_consents: Arc<Mutex<Vec<PendingConsent>>>,
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
        let (stream, peer) = listener.accept().await?;
        let _ = stream.set_nodelay(true);
        let peer_str = peer.to_string();
        let cfg = config.clone();
        tokio::spawn(async move {
            let _ = handle_client_session(stream, peer_str, cfg).await;
        });
    }
}

pub async fn handle_client_session(
    mut stream: TcpStream,
    peer_addr: String,
    config: HostConfig,
) -> anyhow::Result<()> {
    // 1. Handshake (Optional PIN check + Interactive User Consent Modal)
    let hello = recv_packet(&mut stream).await?;
    let expected_pin = config.pin.lock().unwrap().trim().to_string();
    let (client_name, pin_ok) = match hello {
        Packet::Hello {
            client_name, pin, ..
        } => {
            let ok = if expected_pin.is_empty() {
                true
            } else {
                pin.as_deref() == Some(expected_pin.as_str())
            };
            (client_name, ok)
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

    // 2. Interactive User Consent Check (if require_consent is enabled on Host)
    let mut session_force_view_only = false;
    let ask_user = *config.require_consent.lock().unwrap();
    if ask_user {
        let req_id = NEXT_CONSENT_ID.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel::<ConsentDecision>();
        {
            let mut list = config.pending_consents.lock().unwrap();
            list.push(PendingConsent {
                id: req_id,
                client_name: client_name.clone(),
                peer_addr: peer_addr.clone(),
                requested_at: Instant::now(),
                response_tx: Some(tx),
            });
        }

        let decision = match tokio::time::timeout(Duration::from_secs(60), rx).await {
            Ok(Ok(d)) => d,
            _ => {
                // Remove expired request if still present
                config
                    .pending_consents
                    .lock()
                    .unwrap()
                    .retain(|item| item.id != req_id);
                ConsentDecision::Reject
            }
        };

        match decision {
            ConsentDecision::AllowFullControl => {
                session_force_view_only = false;
            }
            ConsentDecision::AllowViewOnly => {
                session_force_view_only = true;
            }
            ConsentDecision::Reject => {
                send_packet(
                    &mut stream,
                    &Packet::Welcome {
                        host_name: config.host_name,
                        desk_id: config.desk_id,
                        screen_width: scr_w,
                        screen_height: scr_h,
                        accepted: false,
                        reason: Some("Conexao recusada pelo usuario do Host".into()),
                        signature: AUTHOR_SIGNATURE.into(),
                    },
                )
                .await?;
                return Ok(());
            }
        }
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
    let (tx_ctrl, mut rx_ctrl) = unbounded_channel::<Packet>();
    let latest_video_frame = Arc::new(Mutex::new(Option::<Packet>::None));
    let frame_notify = Arc::new(tokio::sync::Notify::new());

    let fps = Arc::new(Mutex::new(30u8));
    let quality = Arc::new(Mutex::new(55u8));
    let scale_pct = Arc::new(Mutex::new(75u8));
    let view_only = Arc::new(Mutex::new(session_force_view_only));

    // Dedicated cancel-safe reader task for client input/files
    let fps_r = fps.clone();
    let quality_r = quality.clone();
    let scale_pct_r = scale_pct.clone();
    let view_only_r = view_only.clone();
    let allow_ctrl = config.allow_control.clone();
    let download_dir = config.download_dir.clone();
    let tx_ack = tx_ctrl.clone();

    let reader_handle = tokio::spawn(async move {
        let mut enigo: Option<Enigo> = Enigo::new(&Settings::default()).ok();
        while let Ok(pkt) = recv_packet(&mut reader).await {
            let can_inject = !session_force_view_only
                && *allow_ctrl.lock().unwrap()
                && !*view_only_r.lock().unwrap();

            match pkt {
                Packet::StreamConfig {
                    fps: new_fps,
                    jpeg_quality,
                    view_only: vo,
                    scale_pct: new_scale,
                } => {
                    *fps_r.lock().unwrap() = new_fps.clamp(1, 60);
                    *quality_r.lock().unwrap() = jpeg_quality.clamp(10, 95);
                    *view_only_r.lock().unwrap() = vo || session_force_view_only;
                    *scale_pct_r.lock().unwrap() = new_scale.clamp(30, 100);
                }
                Packet::MouseMove { norm_x, norm_y } => {
                    if can_inject {
                        inject_mouse_move(&mut enigo, norm_x, norm_y, scr_w, scr_h);
                    }
                }
                Packet::MouseButton {
                    button,
                    pressed,
                    norm_x,
                    norm_y,
                } => {
                    if can_inject {
                        inject_mouse_button(
                            &mut enigo, button, pressed, norm_x, norm_y, scr_w, scr_h,
                        );
                    }
                }
                Packet::MouseScroll {
                    delta_x: _,
                    delta_y,
                } => {
                    if can_inject {
                        inject_mouse_scroll(&mut enigo, delta_y);
                    }
                }
                Packet::KeyEvent { key, pressed } => {
                    if can_inject {
                        inject_key(&mut enigo, &key, pressed);
                    }
                }
                Packet::TextInput { text } => {
                    if can_inject {
                        if enigo.is_none() {
                            enigo = Enigo::new(&Settings::default()).ok();
                        }
                        if let Some(e) = enigo.as_mut() {
                            let _ = e.text(&text);
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

    // Dedicated screen capture producer with dirty checking and zero bufferbloat
    let latest_frame_producer = latest_video_frame.clone();
    let notify_producer = frame_notify.clone();
    let fps_c = fps.clone();
    let quality_c = quality.clone();
    let scale_c = scale_pct.clone();
    let tx_ctrl_c = tx_ctrl.clone();

    let capture_handle = tokio::spawn(async move {
        let mut last_clip = String::new();
        let mut last_hash = 0u64;
        let mut last_sent_time = Instant::now();

        loop {
            let cur_fps = (*fps_c.lock().unwrap()).max(1) as u64;
            let q = *quality_c.lock().unwrap();
            let scale = *scale_c.lock().unwrap();
            let force = last_sent_time.elapsed() >= Duration::from_millis(1500);

            let frame_res = tokio::task::spawn_blocking(move || {
                capture_screen_jpeg(q, scale, last_hash, force)
            })
            .await;

            match frame_res {
                Ok(Ok(Some((w, h, jpeg_bytes, new_hash)))) => {
                    last_hash = new_hash;
                    last_sent_time = Instant::now();
                    let now_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;

                    // Overwrite the slot so network writer always transmits the freshest frame
                    *latest_frame_producer.lock().unwrap() = Some(Packet::VideoFrame {
                        width: w,
                        height: h,
                        jpeg_bytes,
                        timestamp_ms: now_ms,
                    });
                    notify_producer.notify_one();
                }
                Ok(Ok(None)) => {
                    // Screen unchanged: no unnecessary encoding or network traffic!
                }
                Ok(Err(e)) => {
                    eprintln!("[Host] Falha ao capturar tela: {}", e);
                }
                Err(e) => {
                    eprintln!("[Host] Erro na thread de captura: {}", e);
                }
            }

            if let Ok(mut cb) = Clipboard::new() {
                if let Ok(text) = cb.get_text() {
                    if !text.is_empty() && text != last_clip {
                        last_clip = text.clone();
                        let _ = tx_ctrl_c.send(Packet::ClipboardText { text });
                    }
                }
            }

            tokio::time::sleep(Duration::from_millis((1000 / cur_fps).max(10))).await;
        }
    });

    // Zero-bloat writer loop: handles high-priority control packets immediately
    // and sends the freshest available video frame without unbounded queue accumulation.
    loop {
        tokio::select! {
            biased;
            Some(ctrl_pkt) = rx_ctrl.recv() => {
                if send_packet(&mut writer, &ctrl_pkt).await.is_err() {
                    break;
                }
            }
            _ = frame_notify.notified() => {
                let frame = latest_video_frame.lock().unwrap().take();
                if let Some(pkt) = frame {
                    if send_packet(&mut writer, &pkt).await.is_err() {
                        break;
                    }
                }
            }
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

#[cfg(target_os = "windows")]
fn capture_screen_gdi() -> anyhow::Result<(u32, u32, Vec<u8>)> {
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

    unsafe {
        let hdc_screen = GetDC(None);
        if hdc_screen.is_invalid() {
            anyhow::bail!("GetDC failed");
        }
        let w = GetSystemMetrics(SM_CXSCREEN);
        let h = GetSystemMetrics(SM_CYSCREEN);
        if w <= 0 || h <= 0 {
            let _ = ReleaseDC(None, hdc_screen);
            anyhow::bail!("Invalid screen metrics {}x{}", w, h);
        }

        let hdc_mem = CreateCompatibleDC(hdc_screen);
        let hbm = CreateCompatibleBitmap(hdc_screen, w, h);
        let old_bm = SelectObject(hdc_mem, hbm);

        let _ = BitBlt(hdc_mem, 0, 0, w, h, hdc_screen, 0, 0, SRCCOPY);

        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bgra = vec![0u8; (w * h * 4) as usize];
        let lines = GetDIBits(
            hdc_mem,
            hbm,
            0,
            h as u32,
            Some(bgra.as_mut_ptr() as *mut _),
            &mut bi,
            DIB_RGB_COLORS,
        );

        let _ = SelectObject(hdc_mem, old_bm);
        let _ = DeleteObject(hbm);
        let _ = DeleteDC(hdc_mem);
        let _ = ReleaseDC(None, hdc_screen);

        if lines == 0 {
            anyhow::bail!("GetDIBits failed");
        }

        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for chunk in bgra.chunks_exact(4) {
            rgb.push(chunk[2]);
            rgb.push(chunk[1]);
            rgb.push(chunk[0]);
        }
        Ok((w as u32, h as u32, rgb))
    }
}

fn capture_screen_raw() -> anyhow::Result<(u32, u32, Vec<u8>)> {
    if let Ok(screens) = Screen::all() {
        if let Some(primary) = screens.first() {
            if let Ok(image) = primary.capture() {
                let w = image.width();
                let h = image.height();
                let rgba = image.into_raw();
                let mut rgb = Vec::with_capacity((w * h * 3) as usize);
                for chunk in rgba.chunks_exact(4) {
                    rgb.push(chunk[0]);
                    rgb.push(chunk[1]);
                    rgb.push(chunk[2]);
                }
                return Ok((w, h, rgb));
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        return capture_screen_gdi();
    }

    #[cfg(not(target_os = "windows"))]
    anyhow::bail!("Screenshots capture failed")
}

fn fast_sample_hash(data: &[u8]) -> u64 {
    let len = data.len();
    if len == 0 {
        return 0;
    }
    let step = (len / 1024).max(1);
    let mut hash = 0xcbf29ce484222325u64;
    let mut idx = 0;
    while idx < len {
        hash ^= data[idx] as u64;
        hash = hash.wrapping_mul(0x100000001b3);
        idx += step;
    }
    hash
}

fn downsample_rgb(src: &[u8], src_w: u32, src_h: u32, scale_pct: u8) -> (u32, u32, Vec<u8>) {
    if scale_pct >= 95 || scale_pct == 0 {
        return (src_w, src_h, src.to_vec());
    }
    let factor = (scale_pct.clamp(30, 100) as f32) / 100.0;
    let dst_w = (((src_w as f32 * factor) as u32) & !1).max(320);
    let dst_h = (((src_h as f32 * factor) as u32) & !1).max(240);

    let mut dst = vec![0u8; (dst_w * dst_h * 3) as usize];
    for y in 0..dst_h {
        let src_y = (y * src_h) / dst_h;
        let src_row = (src_y * src_w * 3) as usize;
        let dst_row = (y * dst_w * 3) as usize;
        for x in 0..dst_w {
            let src_x = (x * src_w) / dst_w;
            let s_idx = src_row + (src_x * 3) as usize;
            let d_idx = dst_row + (x * 3) as usize;
            dst[d_idx] = src[s_idx];
            dst[d_idx + 1] = src[s_idx + 1];
            dst[d_idx + 2] = src[s_idx + 2];
        }
    }
    (dst_w, dst_h, dst)
}

fn capture_screen_jpeg(
    quality: u8,
    scale_pct: u8,
    last_hash: u64,
    force: bool,
) -> anyhow::Result<Option<(u32, u32, Vec<u8>, u64)>> {
    let (orig_w, orig_h, rgb) = capture_screen_raw()?;
    let new_hash = fast_sample_hash(&rgb);

    if !force && new_hash == last_hash {
        return Ok(None);
    }

    let (final_w, final_h, final_rgb) = downsample_rgb(&rgb, orig_w, orig_h, scale_pct);
    let mut jpeg_bytes = Vec::with_capacity((final_w * final_h / 4) as usize);
    let mut encoder = JpegEncoder::new_with_quality(&mut jpeg_bytes, quality);
    encoder.encode(&final_rgb, final_w, final_h, ColorType::Rgb8.into())?;

    Ok(Some((final_w, final_h, jpeg_bytes, new_hash)))
}

fn inject_mouse_move(enigo: &mut Option<Enigo>, norm_x: f32, norm_y: f32, w: u32, h: u32) {
    if enigo.is_none() {
        *enigo = Enigo::new(&Settings::default()).ok();
    }
    if let Some(e) = enigo.as_mut() {
        let x = (norm_x.clamp(0.0, 1.0) * w as f32) as i32;
        let y = (norm_y.clamp(0.0, 1.0) * h as f32) as i32;
        let _ = e.move_mouse(x, y, Coordinate::Abs);
    }
}

fn inject_mouse_button(
    enigo: &mut Option<Enigo>,
    btn: MouseBtn,
    pressed: bool,
    norm_x: f32,
    norm_y: f32,
    w: u32,
    h: u32,
) {
    if enigo.is_none() {
        *enigo = Enigo::new(&Settings::default()).ok();
    }
    if let Some(e) = enigo.as_mut() {
        let x = (norm_x.clamp(0.0, 1.0) * w as f32) as i32;
        let y = (norm_y.clamp(0.0, 1.0) * h as f32) as i32;
        let _ = e.move_mouse(x, y, Coordinate::Abs);
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
        let _ = e.button(b, dir);
    }
}

fn inject_mouse_scroll(enigo: &mut Option<Enigo>, delta_y: i32) {
    if enigo.is_none() {
        *enigo = Enigo::new(&Settings::default()).ok();
    }
    if let Some(e) = enigo.as_mut() {
        let _ = e.scroll(delta_y, enigo::Axis::Vertical);
    }
}

fn inject_key(enigo: &mut Option<Enigo>, key_name: &str, pressed: bool) {
    if enigo.is_none() {
        *enigo = Enigo::new(&Settings::default()).ok();
    }
    if let Some(e) = enigo.as_mut() {
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
            let _ = e.key(k, dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture() {
        let res = capture_screen_jpeg(65, 100, 0, true);
        assert!(res.is_ok());
    }
}

