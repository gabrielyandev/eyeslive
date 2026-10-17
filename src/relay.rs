//! EyesLive Self-Hosted Relay Server — Created by Gabriel Yan (@gabrielyandev)
//! GitHub: https://github.com/gabrielyandev/eyeslive
//! Routes connections by 9-digit Desk ID (`XXX-XXX-XXX`) without accounts or time limits.

use crate::protocol::{recv_packet, send_packet, Packet, AUTHOR_NAME, AUTHOR_SIGNATURE, GITHUB_PROFILE_URL};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

type WaitingHosts = Arc<Mutex<HashMap<String, TcpStream>>>;

pub async fn run_relay_server(bind_addr: &str) -> anyhow::Result<()> {
    let listener = TcpListener::bind(bind_addr).await?;
    println!(
        "[EyesLive Relay — by {} ({}) | {}] Listening on {} (Unlimited sessions, No login)",
        AUTHOR_NAME,
        AUTHOR_SIGNATURE,
        GITHUB_PROFILE_URL,
        bind_addr
    );

    let hosts: WaitingHosts = Arc::new(Mutex::new(HashMap::new()));

    loop {
        let (mut stream, peer) = listener.accept().await?;
        let hosts = hosts.clone();
        tokio::spawn(async move {
            match recv_packet(&mut stream).await {
                Ok(Packet::RegisterHost { desk_id }) => {
                    let clean_id = normalize_id(&desk_id);
                    println!("[EyesLive Relay] Host registered: {} from {}", clean_id, peer);
                    let _ = send_packet(
                        &mut stream,
                        &Packet::RelayStatus {
                            ok: true,
                            message: format!("Registered ID {} on EyesLive relay", clean_id),
                        },
                    )
                    .await;
                    hosts.lock().await.insert(clean_id, stream);
                }
                Ok(Packet::ConnectToHost { target_desk_id }) => {
                    let clean_id = normalize_id(&target_desk_id);
                    let maybe_host = hosts.lock().await.remove(&clean_id);
                    if let Some(mut host_stream) = maybe_host {
                        let _ = send_packet(
                            &mut stream,
                            &Packet::RelayStatus {
                                ok: true,
                                message: "Connected to host via EyesLive Relay".into(),
                            },
                        )
                        .await;
                        let _ = send_packet(
                            &mut host_stream,
                            &Packet::RelayStatus {
                                ok: true,
                                message: "Viewer connected via EyesLive Relay".into(),
                            },
                        )
                        .await;
                        let _ = copy_bidirectional(&mut stream, &mut host_stream).await;
                    } else {
                        let _ = send_packet(
                            &mut stream,
                            &Packet::RelayStatus {
                                ok: false,
                                message: format!("Host ID {} is not online on this relay", clean_id),
                            },
                        )
                        .await;
                    }
                }
                _ => {}
            }
        });
    }
}

pub fn normalize_id(raw: &str) -> String {
    raw.chars().filter(|c| c.is_ascii_digit()).collect()
}
