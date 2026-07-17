use std::io::Read;
use std::io::Cursor;
use std::time::{SystemTime, UNIX_EPOCH};
use std::path::Path;
use std::fs::File;
use serde_json::Value;
use base64::engine::general_purpose;
use base64::Engine;
use zip::ZipArchive;

pub fn run_py_worker() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).expect("worker: failed to read stdin");

    let data: Value = serde_json::from_str(&input).expect("worker: failed to parse input");
    let file_bytes = general_purpose::STANDARD
        .decode(data["file_bytes"].as_str().expect("worker: missing file_bytes"))
        .expect("worker: failed to decode file_bytes");
    let params_str = data["parameters"].as_str().unwrap_or("").trim().to_string();
    let python_embed_bytes = data.get("python_embed_bytes")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|s| general_purpose::STANDARD.decode(s).expect("worker: failed to decode python_embed_bytes"));

    let tmp_dir = std::env::temp_dir();
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    let work_dir = tmp_dir.join(format!("kassandra_py_{}", ts));
    if let Err(e) = std::fs::create_dir_all(&work_dir) {
        eprint!("PY tempdir error: {:?}", e);
        std::process::exit(1);
    }

    let script_path = work_dir.join("kassandra_exec.py");
    if let Err(e) = std::fs::write(&script_path, &file_bytes) {
        eprint!("PY write error: {:?}", e);
        std::process::exit(1);
    }

    let mut cmd = match resolve_python(&work_dir, python_embed_bytes) {
        Ok(c) => c,
        Err(e) => {
            eprint!("{}", e);
            std::process::exit(1);
        }
    };
    cmd.arg(&script_path);
    if !params_str.is_empty() {
        cmd.args(params_str.split_whitespace());
    }
    cmd.current_dir(&work_dir);

    match cmd.output() {
        Ok(res) => {
            let mut output = String::from_utf8_lossy(&res.stdout).to_string();
            if !res.status.success() {
                let stderr = String::from_utf8_lossy(&res.stderr);
                if !stderr.is_empty() {
                    output.push_str("\nstderr: ");
                    output.push_str(&stderr);
                }
                eprint!("{}", output);
                let code = res.status.code().unwrap_or(1);
                std::process::exit(code);
            }
            print!("{}", output);
        }
        Err(e) => {
            eprint!("PY exec error: {:?}", e);
            std::process::exit(1);
        }
    }
}

fn resolve_python(work_dir: &Path, python_embed_bytes: Option<Vec<u8>>) -> Result<std::process::Command, String> {
    if has_python("python") {
        return Ok(std::process::Command::new("python"));
    }
    if has_python("python3") {
        return Ok(std::process::Command::new("python3"));
    }

    if let Some(bytes) = python_embed_bytes {
        if let Err(e) = extract_zip(&bytes, work_dir) {
            return Err(format!("PY extract error: {:?}", e));
        }
        let python_path = work_dir.join("python.exe");
        if !python_path.exists() {
            return Err("PY extract error: python.exe not found in embeddable zip".to_string());
        }
        return Ok(std::process::Command::new(python_path));
    }

    Err("PY error: python not found on PATH and no embeddable runtime provided".to_string())
}

fn has_python(cmd: &str) -> bool {
    std::process::Command::new(cmd)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn extract_zip(bytes: &[u8], dest: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let reader = Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader)?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = dest.join(file.name());
        if file.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
    }
    Ok(())
}
