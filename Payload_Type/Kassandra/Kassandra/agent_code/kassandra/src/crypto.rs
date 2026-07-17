use aes::Aes256;
use aes::cipher::{BlockEncrypt, BlockDecrypt, KeyInit, generic_array::GenericArray};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

fn new_hmac(key: &[u8]) -> HmacSha256 {
    <HmacSha256 as Mac>::new_from_slice(key).unwrap()
}

fn derive_keys(key: &[u8]) -> ([u8; 32], [u8; 32]) {
    let mut h = new_hmac(key);
    h.update(b"s3c2-enc");
    let enc_key: [u8; 32] = h.finalize().into_bytes().into();

    let mut h = new_hmac(key);
    h.update(b"s3c2-mac");
    let mac_key: [u8; 32] = h.finalize().into_bytes().into();

    (enc_key, mac_key)
}

fn pkcs7_pad(data: &[u8]) -> Vec<u8> {
    let pad_len = 16 - (data.len() % 16);
    let mut out = data.to_vec();
    out.extend(vec![pad_len as u8; pad_len]);
    out
}

fn pkcs7_unpad(data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() || data.len() % 16 != 0 { return None; }
    let pad = *data.last()? as usize;
    if pad < 1 || pad > 16 || pad > data.len() { return None; }
    if data[data.len()-pad..].iter().any(|&b| b as usize != pad) { return None; }
    Some(data[..data.len()-pad].to_vec())
}

pub fn encrypt_message(key: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let (enc_key, mac_key) = derive_keys(key);
    let cipher = Aes256::new(GenericArray::from_slice(&enc_key));

    let mut iv = [0u8; 16];
    getrandom::getrandom(&mut iv).expect("failed to generate random IV");

    let padded = pkcs7_pad(plaintext);
    let mut ciphertext = Vec::with_capacity(padded.len());
    let mut prev = iv;

    for chunk in padded.chunks(16) {
        let mut xored = [0u8; 16];
        for i in 0..16 { xored[i] = chunk[i] ^ prev[i]; }
        let mut block = GenericArray::clone_from_slice(&xored);
        cipher.encrypt_block(&mut block);
        ciphertext.extend_from_slice(block.as_slice());
        prev.copy_from_slice(block.as_slice());
    }

    let tag: [u8; 32] = {
        let mut mac = new_hmac(&mac_key);
        mac.update(&iv);
        mac.update(&ciphertext);
        mac.finalize().into_bytes().into()
    };

    let mut out = Vec::with_capacity(16 + ciphertext.len() + 32);
    out.extend_from_slice(&iv);
    out.extend_from_slice(&ciphertext);
    out.extend_from_slice(&tag);

    crate::helpers::churn(&tag);

    out
}

pub fn decrypt_message(key: &[u8], data: &[u8]) -> Result<Vec<u8>, &'static str> {
    if data.len() < 48 { return Err("data too short"); }

    let (enc_key, mac_key) = derive_keys(key);
    let iv = &data[..16];
    let tag = &data[data.len()-32..];
    let ct = &data[16..data.len()-32];

    if ct.len() % 16 != 0 { return Err("invalid ciphertext length"); }

    // Verify HMAC (constant-time via hmac crate)
    let mut mac = new_hmac(&mac_key);
    mac.update(iv);
    mac.update(ct);
    mac.verify_slice(tag).map_err(|_| "authentication failed")?;

    // AES-256-CBC decrypt
    let cipher = Aes256::new(GenericArray::from_slice(&enc_key));
    let mut plaintext = Vec::with_capacity(ct.len());
    let mut prev: Vec<u8> = iv.to_vec();

    for chunk in ct.chunks(16) {
        let mut block = GenericArray::clone_from_slice(chunk);
        cipher.decrypt_block(&mut block);
        for i in 0..16 { plaintext.push(block[i] ^ prev[i]); }
        prev = chunk.to_vec();
    }

    let result = pkcs7_unpad(&plaintext).ok_or("bad padding")?;
    crate::helpers::churn(iv);
    Ok(result)
}
