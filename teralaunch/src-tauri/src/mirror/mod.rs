pub mod client;
pub mod crypto;
pub mod broadcaster;

use lazy_static::lazy_static;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

lazy_static! {
    // Broadcast channel for packet frames from game client to local subscribers (Arc<[u8]>) to minimize cloning
    pub static ref PACKET_BROADCAST_TX: broadcast::Sender<Arc<[u8]>> = {
        let (tx, _rx) = broadcast::channel(1024);
        tx
    };
    // Holds the running game client connection task so we can abort/stop it
    pub static ref CLIENT_TASK: Mutex<Option<tokio::task::JoinHandle<()>>> = Mutex::new(None);
    // Holds the running local broadcaster server task (spawned via tauri::async_runtime)
    pub static ref BROADCASTER_TASK: Mutex<Option<tauri::async_runtime::JoinHandle<()>>> = Mutex::new(None);
    // Preferred mirror target (host, port) provided by UI or integration
    pub static ref MIRROR_TARGET: Mutex<Option<(String, u16)>> = Mutex::new(None);
}
