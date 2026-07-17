use once_cell::sync::Lazy;
use std::sync::RwLock;

static BOF_LOADER_CACHE: Lazy<RwLock<Vec<u8>>> = Lazy::new(|| RwLock::new(Vec::new()));
static DOT_LOADER_CACHE: Lazy<RwLock<Vec<u8>>> = Lazy::new(|| RwLock::new(Vec::new()));

const LOADER_KEY: &[u8; 32] = b"\x4b\x61\x73\x73\x41\x6e\x44\x72\x61\x4c\x6f\x41\x64\x45\x72\x4b\x33\x79\x5f\x52\x30\x74\x41\x74\x31\x6f\x4e\x5f\x32\x30\x32\x36";

pub enum LoaderKind {
    Bof,
    Dot,
}

fn cache_for(kind: &LoaderKind) -> &'static RwLock<Vec<u8>> {
    match kind {
        LoaderKind::Bof => &BOF_LOADER_CACHE,
        LoaderKind::Dot => &DOT_LOADER_CACHE,
    }
}

fn xor_with_key(data: &[u8]) -> Vec<u8> {
    data.iter()
        .enumerate()
        .map(|(i, b)| b ^ LOADER_KEY[i % LOADER_KEY.len()])
        .collect()
}

pub fn is_cached(kind: &LoaderKind) -> bool {
    let cache = cache_for(kind);
    let data = cache.read().unwrap();
    !data.is_empty()
}

pub fn store(kind: &LoaderKind, raw_bytes: Vec<u8>) {
    let encrypted = xor_with_key(&raw_bytes);
    let cache = cache_for(kind);
    let mut data = cache.write().unwrap();
    *data = encrypted;
}

pub fn get(kind: &LoaderKind) -> Result<Vec<u8>, &'static str> {
    let cache = cache_for(kind);
    let data = cache.read().unwrap();
    if data.is_empty() {
        return Err("loader not cached");
    }
    Ok(xor_with_key(&data))
}
