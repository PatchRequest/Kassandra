use crate::transport;

pub fn exit(task: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let command = task.get("command").unwrap().as_str().unwrap();
    let parameters = task.get("parameters").unwrap().as_str().unwrap();
    let timestamp = task.get("timestamp").unwrap().as_f64().unwrap();
    let id = task.get("id").unwrap().as_str().unwrap();

    let response_json = serde_json::json!({
        "action": "post_response",
        "responses": [
            {
                "task_id": id,
                "user_output": "Exiting",
                "timestamp": timestamp,
                "status": "success",
                "completed": true,
            }
        ]
    });

    let response_value = serde_json::to_string(&response_json)?;
    // Send the response back to the server
    transport::send_request(&response_value)?;

    // Exit the program
    std::process::exit(0);
}