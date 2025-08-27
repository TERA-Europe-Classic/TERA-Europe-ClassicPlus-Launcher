pub mod client;
pub mod broadcaster;

use lazy_static::lazy_static;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

lazy_static! {
    pub static ref PACKET_BROADCAST_TX: broadcast::Sender<Arc<[u8]>> = {
        let (tx, _rx) = broadcast::channel(1024);
        tx
    };
    pub static ref CLIENT_TASK: Mutex<Option<tokio::task::JoinHandle<()>>> = Mutex::new(None);
    pub static ref BROADCASTER_TASK: Mutex<Option<tauri::async_runtime::JoinHandle<()>>> = Mutex::new(None);
}
