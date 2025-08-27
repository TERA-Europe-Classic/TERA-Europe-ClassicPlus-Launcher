use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use log::error;

use crate::mirror::{PACKET_BROADCAST_TX, BROADCASTER_TASK};

pub fn start_broadcast_server(_app_handle: &tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut guard = BROADCASTER_TASK.lock().await;
        if guard.is_some() {
            return; // Already running
        }

        let tx = PACKET_BROADCAST_TX.clone();
        
        let handle = tauri::async_runtime::spawn(async move {
            run_broadcast_server_loop(tx).await;
        });

        *guard = Some(handle);
    });
}

async fn run_broadcast_server_loop(
    tx: tokio::sync::broadcast::Sender<std::sync::Arc<[u8]>>
) {
    let bind_addr = "127.0.0.1:7802";
    
    loop {
        let listener = match TcpListener::bind(bind_addr).await {
            Ok(listener) => listener,
            Err(e) => {
                error!("Broadcast server bind failed on {}: {}. Retrying in 3s...", bind_addr, e);
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue;
            }
        };

        loop {
            let (socket, _addr) = match listener.accept().await {
                Ok((socket, addr)) => (socket, addr),
                Err(e) => {
                    error!("Broadcast server accept error: {}", e);
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    continue;
                }
            };

            let rx = tx.subscribe();
            tauri::async_runtime::spawn(async move {
                handle_client_connection(socket, rx).await;
            });
        }
    }
}

async fn handle_client_connection(
    mut socket: tokio::net::TcpStream,
    mut rx: tokio::sync::broadcast::Receiver<std::sync::Arc<[u8]>>,
) {
    loop {
        let frame = match rx.recv().await {
            Ok(frame) => frame,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
        };

        if !send_frame_to_client(&mut socket, &frame).await {
            break;
        }
    }
}

async fn send_frame_to_client(
    socket: &mut tokio::net::TcpStream,
    frame: &[u8],
) -> bool {
    let total_len = frame.len();
    
    if total_len > u16::MAX as usize {
        error!("Broadcast server: skip frame > u16: len={}", total_len);
        return true;
    }

    let len_le = (total_len as u16).to_le_bytes();
    
    if let Err(e) = socket.write_all(&len_le).await {
        error!("Broadcast server: write len failed: {}", e);
        return false;
    }
    
    if let Err(e) = socket.write_all(frame).await {
        error!("Broadcast server: write frame failed: {}", e);
        return false;
    }
    
    true
}
