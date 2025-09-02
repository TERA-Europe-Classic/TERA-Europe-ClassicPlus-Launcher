use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::GLOBAL_AUTH_INFO;
use crate::mirror::{PACKET_BROADCAST_TX, CLIENT_TASK, crypto::{derive_mirror_key, MirrorCrypto}};


#[tauri::command]
pub async fn start_mirror_client(window: tauri::Window, host: String, port: u16) -> Result<(), String> {
    let addr = format!("{}:{}", host, port);

    // Abort any previous task
    if let Some(old) = CLIENT_TASK.lock().await.take() {
        old.abort();
    }

    let tx = PACKET_BROADCAST_TX.clone();
    let handle = tokio::spawn(async move {
        let _ = window.emit("log_message", format!("Starting mirror client for {}", addr));

        match connect_and_process(&window, &addr, &tx).await {
            Ok(_) => {
                let _ = window.emit("log_message", "Mirror connection ended normally");
            }
            Err(e) => {
                let _ = window.emit("log_message", format!("Mirror connection failed: {}", e));
            }
        }
    });

    *CLIENT_TASK.lock().await = Some(handle);
    Ok(())
}

async fn connect_and_process(
    window: &tauri::Window,
    addr: &str,
    tx: &tokio::sync::broadcast::Sender<std::sync::Arc<[u8]>>
) -> Result<(), String> {
    // Connect to proxy server
    let mut stream = TcpStream::connect(addr).await.map_err(|e| format!("connect error: {}", e))?;
    let _ = window.emit("log_message", format!("Connected to mirror server {}", addr));

    // ===== Handshake: AGNIMIRR + JSON line =====
    stream.write_all(b"AGNIMIRR").await.map_err(|e| format!("hello error: {}", e))?;

    // Load ticket
    let ticket = GLOBAL_AUTH_INFO.read().unwrap().auth_key.clone();
    if ticket.trim().is_empty() {
        return Err("No ticket available for mirror handshake".to_string());
    }

    // Build client hello (obfuscated helper)
    let (hello_json, cnonce) = super::crypto::make_client_hello(&ticket)?;

    // Send JSON line
    stream.write_all(hello_json.as_bytes()).await.map_err(|e| format!("write hello: {}", e))?;

    // Read ServerHello line with snonce
    let mut line = Vec::new();
    loop {
        let mut b = [0u8; 1];
        let n = stream.read(&mut b).await.map_err(|e| format!("read: {}", e))?;
        if n == 0 { return Err("eof before hello".into()); }
        line.push(b[0]);
        if b[0] == b'\n' { break; }
    }

    // Parse server hello (obfuscated helper)
    let snonce8 = super::crypto::parse_server_hello(&line)?;

    // Derive encryption key using obfuscated crypto module
    let key = derive_mirror_key(&ticket, &cnonce, &snonce8).map_err(|e| e.to_string())?;
    let mut crypto = MirrorCrypto::new(key, snonce8);

    let _ = window.emit("log_message", "Mirror handshake completed, reading encrypted frames");

    // ===== Encrypted frame loop =====
    let mut buf = vec![0u8; 65536];
    let mut acc = Vec::new();
    let mut _last_activity = std::time::Instant::now();
    let _activity_timeout = Duration::from_secs(60);
    let mut _packet_count = 0u64;

    loop {
        let read_result = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf)).await;
        match read_result {
            Ok(Ok(0)) => return Err("Connection closed by server".to_string()),
            Ok(Ok(n)) => {
                _last_activity = std::time::Instant::now();
                acc.extend_from_slice(&buf[..n]);

                // Frame header is 13 bytes: 'MRV1'(4) | type(1) | seq(4, LE) | len(4, LE)
                let mut pos = 0usize;
                while acc.len() >= pos + 13 {
                    if &acc[pos..pos+4] != b"MRV1" { return Err("Bad magic".into()); }
                    let ftype = acc[pos+4];
                    if ftype != 0x01 { return Err("Unsupported frame type".into()); }
                    let _seq = u32::from_le_bytes([acc[pos+5], acc[pos+6], acc[pos+7], acc[pos+8]]);
                    let pt_len = u32::from_le_bytes([acc[pos+9], acc[pos+10], acc[pos+11], acc[pos+12]]) as usize;
                    let need = 13 + pt_len + 16; // header + ciphertext + tag
                    if acc.len() < pos + need { break; }

                    // Encrypted frame data INCLUDING tag (AES-GCM expects tag appended)
                    let frame_data = &acc[pos+13 .. pos+need];
                    let frame_header = &acc[pos .. pos+13];
                    
                    // Decrypt frame using obfuscated crypto module
                    let plaintext = crypto.decrypt_frame(frame_data, frame_header).map_err(|e| e.to_string())?;

                    // 'plaintext' now contains the plaintext frame (dir|len|opcode|payload)
                    _packet_count += 1;
                    let _ = tx.send(plaintext.into());
                    pos += need;
                }
                if pos > 0 { acc.drain(0..pos); }
                if acc.len() > 1<<20 { acc.clear(); } // 1MB safety
            }
            Ok(Err(e)) => return Err(format!("Read error: {}", e)),
            Err(_) => {
                // No read within the timeout window; keep the connection open and try again.
                continue;
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
