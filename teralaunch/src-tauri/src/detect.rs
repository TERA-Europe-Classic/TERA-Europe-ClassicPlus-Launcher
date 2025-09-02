use std::net::IpAddr;
use netstat2::{get_sockets_info, AddressFamilyFlags, ProtocolFlags, TcpState, ProtocolSocketInfo};

// Try to detect the game's remote host:port by inspecting TCP connections for the given PID.
// Prefer remote port 9000; otherwise pick the first Established/SynSent remote connection.
pub async fn detect_remote_by_pid(pid: u32) -> Option<(String, u16)> {
    // netstat2 is synchronous; run it on a blocking thread to avoid stalling the async runtime
    tokio::task::spawn_blocking(move || {
        let af_flags = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
        let proto_flags = ProtocolFlags::TCP;
        let sockets = match get_sockets_info(af_flags, proto_flags) {
            Ok(s) => s,
            Err(_) => return None,
        };

        // Filter sockets owned by the game process
        let owned = sockets
            .into_iter()
            .filter(|s| s.associated_pids.iter().any(|p| *p as u32 == pid));

        // Prefer connections in Established or SynSent state
        let established: Vec<_> = owned
            .filter_map(|s| {
                if let ProtocolSocketInfo::Tcp(tcp) = s.protocol_socket_info {
                    Some(tcp)
                } else {
                    None
                }
            })
            .filter(|tcp| matches!(tcp.state, TcpState::Established | TcpState::SynSent))
            .collect();

        // First try to find remote port 9000 (common for lobby/relay)
        if let Some(tcp) = established
            .iter()
            .find(|tcp| tcp.remote_port == 9000)
            .cloned()
        {
            let ip: IpAddr = tcp.remote_addr;
            return Some((ip.to_string(), tcp.remote_port));
        }

        // Otherwise pick the first with a valid remote endpoint
        for tcp in established {
            let ip: IpAddr = tcp.remote_addr;
            if tcp.remote_port > 0 {
                return Some((ip.to_string(), tcp.remote_port));
            }
        }

        None
    })
    .await
    .ok()
    .flatten()
}
