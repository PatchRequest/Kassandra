use base64::engine::general_purpose;
use base64::Engine;

pub struct BeaconPack {
    buffer: Vec<u8>,
    size: u32,
}

impl BeaconPack {
    pub fn new() -> Self {
        Self { buffer: Vec::new(), size: 0 }
    }

    pub fn get_buffer(&self) -> Vec<u8> {
        let mut result = self.size.to_le_bytes().to_vec();
        result.extend(&self.buffer);
        result
    }

    pub fn add_short(&mut self, val: i16) {
        self.buffer.extend_from_slice(&val.to_le_bytes());
        self.size += 2;
    }

    pub fn add_int(&mut self, val: i32) {
        self.buffer.extend_from_slice(&val.to_le_bytes());
        self.size += 4;
    }

    pub fn add_str(&mut self, s: &str) {
        let bytes = s.as_bytes();
        self.buffer.extend_from_slice(&((bytes.len() + 1) as u32).to_le_bytes());
        self.buffer.extend_from_slice(bytes);
        self.buffer.push(0);
        self.size += (bytes.len() + 1) as u32 + 4;
    }

    pub fn add_wstr(&mut self, s: &str) {
        let wide: Vec<u16> = s.encode_utf16().collect();
        let byte_len = (wide.len() * 2 + 2) as u32;
        self.buffer.extend_from_slice(&byte_len.to_le_bytes());
        for c in &wide {
            self.buffer.extend_from_slice(&c.to_le_bytes());
        }
        self.buffer.extend_from_slice(&0u16.to_le_bytes());
        self.size += byte_len + 4;
    }

    pub fn add_bin(&mut self, bin: &[u8]) {
        self.buffer.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        self.buffer.extend_from_slice(bin);
        self.size += bin.len() as u32 + 4;
    }
}

pub fn pack_args(params_str: &str) -> Result<Vec<u8>, String> {
    if params_str.is_empty() {
        return Ok(Vec::new());
    }

    let mut pack = BeaconPack::new();

    for arg in params_str.split_whitespace() {
        if let Some(val) = arg.strip_prefix("int:") {
            let v: i32 = val.parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
            pack.add_int(v);
        } else if let Some(val) = arg.strip_prefix("short:") {
            let v: i16 = val.parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
            pack.add_short(v);
        } else if let Some(val) = arg.strip_prefix("wstr:") {
            pack.add_wstr(val);
        } else if let Some(val) = arg.strip_prefix("bin:") {
            let bytes = general_purpose::STANDARD.decode(val)
                .map_err(|e| e.to_string())?;
            pack.add_bin(&bytes);
        } else if let Some(val) = arg.strip_prefix("str:") {
            pack.add_str(val);
        } else {
            pack.add_str(arg);
        }
    }

    Ok(pack.get_buffer())
}
