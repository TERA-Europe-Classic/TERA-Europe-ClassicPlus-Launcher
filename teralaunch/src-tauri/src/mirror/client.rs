use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
 

use crate::GLOBAL_AUTH_INFO;
use crate::mirror::{PACKET_BROADCAST_TX, CLIENT_TASK};

#[tauri::command]
pub async fn start_mirror_client(window: tauri::Window, host: String, port: u16) -> Result<(), String> {
    let addr = format!("{}:{}", host, port);

    if let Some(old) = CLIENT_TASK.lock().await.take() {
        old.abort();
    }

    let tx = PACKET_BROADCAST_TX.clone();
    let handle = tokio::spawn(async move {
        let _ = connect_and_process(&window, &addr, &tx).await;
    });

    *CLIENT_TASK.lock().await = Some(handle);
    Ok(())
}

async fn connect_and_process(
    _window: &tauri::Window,
    addr: &str,
    tx: &tokio::sync::broadcast::Sender<std::sync::Arc<[u8]>>
) -> Result<(), String> {
    let mut stream = TcpStream::connect(addr).await.map_err(|e| format!("connect error: {}", e))?;
    stream.write_all(b"AGNIMIRR").await.map_err(|e| format!("hello error: {}", e))?;

    let ticket = GLOBAL_AUTH_INFO.read().unwrap().auth_key.clone();
    if !ticket.is_empty() {
        let bind = format!("{{\"version\":1,\"bind\":{{\"ticket\":\"{}\"}}}}\n", ticket);
        stream.write_all(bind.as_bytes()).await.map_err(|e| format!("auth error: {}", e))?;
    }
    let mut buf = vec![0u8; 65536];
    let mut acc = Vec::new();
    let mut last_activity = std::time::Instant::now();
    let activity_timeout = Duration::from_secs(60);

    loop {
        let read_result = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf)).await;
        
        match read_result {
            Ok(Ok(0)) => {
                return Err("Connection closed by server".to_string());
            }
            Ok(Ok(n)) => {
                last_activity = std::time::Instant::now();
                acc.extend_from_slice(&buf[..n]);

                let mut pos = 0;
                while pos + 5 <= acc.len() {
                    let dir = acc[pos];
                    let tera_len = u16::from_le_bytes([acc[pos + 1], acc[pos + 2]]);
                    let payload_len = tera_len.saturating_sub(4) as usize;
                    let frame_total = 1 + 2 + 2 + payload_len;

                    if frame_total > 65536 {
                        acc.clear();
                        break;
                    }

                    if pos + frame_total > acc.len() { break; } // Incomplete frame

                    let opcode = u16::from_le_bytes([acc[pos + 3], acc[pos + 4]]);
                    let payload = &acc[pos + 5..pos + 5 + payload_len];

                    let mut out = Vec::with_capacity(frame_total);
                    out.push(dir);
                    out.extend_from_slice(&tera_len.to_le_bytes());
                    out.extend_from_slice(&opcode.to_le_bytes());
                    out.extend_from_slice(payload);

                    let _ = tx.send(out.into());
                    pos += frame_total;
                }
                if pos > 0 { 
                    acc.drain(0..pos); 
                }
                
                if acc.len() > 65536 { // 64KB limit for incomplete packets
                    acc.clear();
                }
            }
            Ok(Err(e)) => {
                return Err(format!("Read error: {}", e));
            }
            Err(_) => {
                if last_activity.elapsed() > activity_timeout {
                    return Err("Connection timeout - no activity".to_string());
                }
            }
        }
    }
}

#[tauri::command]
pub async fn stop_mirror_client() -> Result<(), String> {
    if let Some(handle) = CLIENT_TASK.lock().await.take() {
        handle.abort();
    }
    Ok(())
}
