use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use cryptify;
use chamox::obfuscate;
use log::info;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json;
use getrandom;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::Zeroize;

#[cfg(has_mirror_cfg)]
mod mirror_cfg {
    include!(concat!(env!("OUT_DIR"), "/mirror_cfg_gen.rs"));
}

#[cold]
#[inline(never)]
pub fn make_client_hello(ticket: &str) -> Result<(String, [u8; 16]), String> {
    cryptify::flow_stmt!();
    if ticket.trim().is_empty() { return Err("empty ticket".into()); }
    let mut cnonce = [0u8; 16];
    getrandom::getrandom(&mut cnonce).map_err(|e| format!("rng: {}", e))?;
    let cnonce_b64 = B64.encode(&cnonce);
    let hmac_hex = compute_mirror_hmac(ticket, &cnonce).map_err(|e| e.to_string())?;
    let hello = format!(
        "{{\"hello\":{{\"v\":1,\"ticket\":\"{}\",\"cnonce\":\"{}\",\"hmac\":\"{}\"}}}}\n",
        ticket, cnonce_b64, hmac_hex
    );
    Ok((hello, cnonce))
}

#[cold]
#[inline(never)]
pub fn parse_server_hello(line: &[u8]) -> Result<[u8; 8], String> {
    cryptify::flow_stmt!();
    let resp: serde_json::Value = serde_json::from_slice(line).map_err(|e| format!("serverhello parse: {}", e))?;
    let snonce_b64 = resp["hello"]["snonce"].as_str().ok_or("missing snonce")?;
    let sn = B64.decode(snonce_b64.as_bytes()).map_err(|e| e.to_string())?;
    if sn.len() != 8 { return Err("snonce must be 8 bytes".into()); }
    let mut snonce8 = [0u8; 8];
    snonce8.copy_from_slice(&sn);
    Ok(snonce8)
}

static OBF_GUARD_SEED: u32 = 0;

#[cold]
#[inline(never)]
pub fn get_mirror_psk() -> Option<[u8; 32]> {
    cryptify::flow_stmt!();
    let g = unsafe { std::ptr::read_volatile(&OBF_GUARD_SEED) };
    
    if g == 1 {
        std::hint::black_box(());
    }
    
    #[cfg(has_mirror_cfg)]
    {
        if mirror_cfg::GEN_MIRROR_PSK_SET {
            let mut psk = mirror_cfg::GEN_MIRROR_PSK;
            // Deobfuscate with XOR mask
            for b in &mut psk { *b ^= 0xB3; }
            Some(psk)
        } else {
            None
        }
    }
    #[cfg(not(has_mirror_cfg))]
    {
        None
    }
}

#[cold]
#[inline(never)]
#[obfuscate]
pub fn compute_mirror_hmac(ticket: &str, cnonce: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
    cryptify::flow_stmt!();
    let g = unsafe { std::ptr::read_volatile(&OBF_GUARD_SEED) };
    
    if g == 2 {
        std::hint::black_box(());
    }
    
    let psk = get_mirror_psk().ok_or("No PSK available")?;
    let mut mac = <Hmac<Sha256> as hmac::Mac>::new_from_slice(&psk).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    
    // Mirror handshake HMAC prefix matches Enterance
    mac.update(b"MIRR-V1");
    mac.update(ticket.as_bytes());
    mac.update(cnonce);
    
    let result = hex::encode(mac.finalize().into_bytes());
    Ok(result)
}

#[cold]
#[inline(never)]
#[obfuscate]
pub fn derive_mirror_key(ticket: &str, cnonce: &[u8], snonce: &[u8]) -> Result<[u8; 32], String> {
    cryptify::flow_stmt!();
    let g = unsafe { std::ptr::read_volatile(&OBF_GUARD_SEED) };
    
    if g == 3 {
        std::hint::black_box(());
    }
    
    let psk = get_mirror_psk().ok_or("No PSK available")?;
    
    // HKDF key derivation (must match server): salt = "MIRR-V1-salt"
    let salt = b"MIRR-V1-salt";
    let hk = Hkdf::<Sha256>::new(Some(salt), &psk);
    
    let mut info = Vec::new();
    // info = "MIRR-V1-KDF" || ticket || cnonce || snonce
    info.extend_from_slice(b"MIRR-V1-KDF");
    info.extend_from_slice(ticket.as_bytes());
    info.extend_from_slice(cnonce);
    info.extend_from_slice(snonce);
    
    let mut key = [0u8; 32];
    hk.expand(&info, &mut key).map_err(|e| format!("HKDF expand failed: {:?}", e))?;
    // Log a short fingerprint of the key for diagnostics
    info!("MirrorCrypto: derived key fpr={}", &hex::encode(key)[..8]);
    
    Ok(key)
}

pub struct MirrorCrypto {
    cipher: Aes256Gcm,
    snonce: [u8; 8],
    sequence: u32,
}

impl MirrorCrypto {
    #[cold]
    #[inline(never)]
    #[obfuscate]
    pub fn new(key: [u8; 32], snonce: [u8; 8]) -> Self {
        cryptify::flow_stmt!();
        let g = unsafe { std::ptr::read_volatile(&OBF_GUARD_SEED) };
        
        if g == 4 {
            std::hint::black_box(());
        }
        
        let cipher_key = Key::<Aes256Gcm>::from_slice(&key);
        let cipher = Aes256Gcm::new(cipher_key);
        
        Self {
            cipher,
            snonce,
            sequence: 0,
        }
    }
    
    #[obfuscate]
    pub fn decrypt_frame(&mut self, frame_data: &[u8], frame_header: &[u8]) -> Result<Vec<u8>, String> {
        cryptify::flow_stmt!();
        let g = unsafe { std::ptr::read_volatile(&OBF_GUARD_SEED) };
        
        if g == 5 {
            std::hint::black_box(());
        }
        
        if frame_data.len() < 16 {
            return Err("Frame too short".to_string());
        }
        
        // Extract sequence from header: header layout 'MRV1'(4) | type(1) | seq(4 LE) | len(4 LE)
        if frame_header.len() < 13 { return Err("Header too short".into()); }
        let sequence = u32::from_le_bytes([
            frame_header[5], frame_header[6], frame_header[7], frame_header[8]
        ]);
        
        // Construct nonce: snonce(8) + sequence(4)
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[0..8].copy_from_slice(&self.snonce);
        nonce_bytes[8..12].copy_from_slice(&sequence.to_le_bytes());
        
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = frame_data; // includes tag (last 16 bytes)
        if frame_header.len() >= 13 {
            let fpr = if ciphertext.len() >= 4 { &hex::encode(&ciphertext[ciphertext.len().saturating_sub(4)..])[..] } else { "" };
            info!(
                "MirrorCrypto: dec seq={} hdr_len={} ct_len={} tag_tail={}",
                sequence,
                frame_header.len(),
                ciphertext.len(),
                fpr
            );
        }
        
        // Decrypt with AAD = frame header
        let plaintext = self.cipher.decrypt(nonce, aes_gcm::aead::Payload {
            msg: ciphertext,
            aad: frame_header,
        }).map_err(|e| format!("AES-GCM decrypt failed: {:?}", e))?;
        
        self.sequence = sequence.wrapping_add(1);
        Ok(plaintext)
    }
}

impl Drop for MirrorCrypto {
    fn drop(&mut self) {
        cryptify::flow_stmt!();
        self.snonce.zeroize();
    }
}
