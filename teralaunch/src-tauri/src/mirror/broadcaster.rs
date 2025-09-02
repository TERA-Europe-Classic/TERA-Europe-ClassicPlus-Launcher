use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tauri::Manager;

use crate::mirror::{PACKET_BROADCAST_TX, BROADCASTER_TASK};

pub fn start_broadcast_server(app_handle: &tauri::AppHandle) {
    let app_handle = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let mut guard = BROADCASTER_TASK.lock().await;
        if guard.is_some() {
            return; // Already running
        }

        let tx = PACKET_BROADCAST_TX.clone();
        let app_handle_clone = app_handle.clone();
        
        let handle = tauri::async_runtime::spawn(async move {
            run_broadcast_server_loop(app_handle_clone, tx).await;
        });

        *guard = Some(handle);
    });
}

async fn run_broadcast_server_loop(
    app_handle: tauri::AppHandle,
    tx: tokio::sync::broadcast::Sender<std::sync::Arc<[u8]>>
) {
    let bind_addr = "127.0.0.1:7802";

    // Single bind attempt at startup
    let listener = match TcpListener::bind(bind_addr).await {
        Ok(listener) => {
            let _ = app_handle.emit_all(
                "log_message",
                format!("[BROADCAST-SERVER] Listening on {}", bind_addr),
            );
            listener
        }
        Err(e) => {
            let _ = app_handle.emit_all(
                "log_message",
                format!("[BROADCAST-SERVER] Bind failed on {}: {}", bind_addr, e),
            );
            return; // do not retry
        }
    };

    loop {
        let (socket, _addr) = match listener.accept().await {
            Ok((socket, addr)) => {
                let _ = app_handle.emit_all(
                    "log_message",
                    format!("[BROADCAST-SERVER] Client connected: {}", addr),
                );
                (socket, addr)
            }
            Err(e) => {
                let _ = app_handle.emit_all(
                    "log_message",
                    format!("[BROADCAST-SERVER] Accept error: {}", e),
                );
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
        };

        let rx = tx.subscribe();
        let app_handle_inner = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            handle_client_connection(socket, rx, app_handle_inner).await;
        });
    }
}

async fn handle_client_connection(
    mut socket: tokio::net::TcpStream,
    mut rx: tokio::sync::broadcast::Receiver<std::sync::Arc<[u8]>>,
    app_handle: tauri::AppHandle
) {
    loop {
        let frame = match rx.recv().await {
            Ok(frame) => frame,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
        };

        if !send_frame_to_client(&mut socket, &frame, &app_handle).await {
            break;
        }
    }
    
    let _ = app_handle.emit_all("log_message", "[BROADCAST-SERVER] Client disconnected");
}

async fn send_frame_to_client(
    socket: &mut tokio::net::TcpStream,
    frame: &[u8],
    app_handle: &tauri::AppHandle
) -> bool {
    let total_len = frame.len();
    
    if total_len > u16::MAX as usize {
        let _ = app_handle.emit_all(
            "log_message",
            format!("[BROADCAST-SERVER] Skip frame > u16: len={}", total_len),
        );
        return true;
    }

    let len_le = (total_len as u16).to_le_bytes();
    
    if let Err(e) = socket.write_all(&len_le).await {
        let _ = app_handle.emit_all(
            "log_message",
            format!("[BROADCAST-SERVER] write len failed: {}", e),
        );
        return false;
    }
    
    if let Err(e) = socket.write_all(frame).await {
        let _ = app_handle.emit_all(
            "log_message",
            format!("[BROADCAST-SERVER] write frame failed: {}", e),
        );
        return false;
    }
    
    true
}
