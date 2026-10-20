//! Harman AIR SWF encryption (lowercase `fWS`/`cWS`/`zWS` signatures).

use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, BlockEncryptMut, KeyIvInit};

use crate::error::{Error, Result};

const GLOBAL_KEY: &str = "Adobe AIR SDK (c) 2021 HARMAN Internation Industries Incorporated";

type Enc = cbc::Encryptor<aes::Aes256>;
type Dec = cbc::Decryptor<aes::Aes256>;

pub fn is_encrypted_signature(sig: u8) -> bool {
    matches!(sig, b'f' | b'c' | b'z')
}

fn key_for(header: &[u8; 8]) -> u32 {
    let dsum: i32 = header.iter().map(|&b| b as i32).sum();
    let gk = GLOBAL_KEY.as_bytes();
    let dmod = (dsum as usize) % gk.len();
    let mut s = Vec::new();
    s.extend_from_slice(&gk[dmod..]);
    s.extend_from_slice(&gk[..dmod]);
    s.extend_from_slice(b" EncryptSWF ");
    s.extend_from_slice(dsum.to_string().as_bytes());
    let mut ret: i64 = 0;
    for &c in &s {
        ret = ret.wrapping_mul(31).wrapping_add(c as i64);
    }
    ret as u32
}

fn iv_for(header: &[u8; 8], enc_len: [u8; 4], key: u32) -> [u8; 16] {
    let mut iv = [0u8; 16];
    iv[..8].copy_from_slice(header);
    iv[8..12].copy_from_slice(&enc_len);
    iv[12..].copy_from_slice(&key.to_le_bytes());
    for (b, k) in iv.iter_mut().zip(GLOBAL_KEY.bytes()) {
        *b ^= k;
    }
    iv
}

fn mix_key(src: &[u8], key: u32, decrypting: bool) -> [u8; 32] {
    let mut out = [0u8; 32];
    for i in (0..32).step_by(4) {
        let v = u32::from_le_bytes(src[i..i + 4].try_into().unwrap());
        let add = (i & 4 == 4) != decrypting;
        let v = if add { v.wrapping_add(key) } else { v.wrapping_sub(key) };
        out[i..i + 4].copy_from_slice(&v.to_le_bytes());
    }
    out
}

/// Decrypts the payload following an encrypted 8-byte header. Returns the header with
/// an uppercased signature and the decrypted payload.
pub fn decrypt(header: &[u8; 8], payload: &[u8]) -> Result<([u8; 8], Vec<u8>)> {
    let mut header = *header;
    header[0] = header[0].wrapping_sub(32);
    let key = key_for(&header);
    if payload.len() < 4 {
        return Err(Error::Decrypt("payload too short".into()));
    }
    let enc_len: [u8; 4] = payload[..4].try_into().unwrap();
    let dec_len = (u32::from_le_bytes(enc_len) ^ key) as usize;
    let padded = (dec_len + 0x1f) & !0x1f;
    if payload.len() < 4 + padded + 32 {
        return Err(Error::Decrypt("payload truncated".into()));
    }
    let mut data = payload[4..4 + padded].to_vec();
    let aes_key = mix_key(&payload[4 + padded..4 + padded + 32], key, true);
    let iv = iv_for(&header, enc_len, key);
    Dec::new(&aes_key.into(), &iv.into())
        .decrypt_padded_mut::<NoPadding>(&mut data)
        .map_err(|e| Error::Decrypt(e.to_string()))?;
    data.truncate(dec_len);
    Ok((header, data))
}

/// Encrypts a payload for an uppercase 8-byte header; returns the whole encrypted file.
pub fn encrypt(header: &[u8; 8], payload: &[u8]) -> Result<Vec<u8>> {
    let key = key_for(header);
    let enc_len = ((payload.len() as u32) ^ key).to_le_bytes();
    let padded = (payload.len() + 0x1f) & !0x1f;
    let mut data = payload.to_vec();
    data.resize(padded, 0);
    let mut aes_key = [0u8; 32];
    getrandom::fill(&mut aes_key).map_err(|e| Error::Other(e.to_string()))?;
    let iv = iv_for(header, enc_len, key);
    Enc::new(&aes_key.into(), &iv.into())
        .encrypt_padded_mut::<NoPadding>(&mut data, padded)
        .map_err(|e| Error::Other(e.to_string()))?;
    let mut out = Vec::with_capacity(8 + 4 + padded + 32);
    out.extend_from_slice(header);
    out[0] = out[0].wrapping_add(32);
    out.extend_from_slice(&enc_len);
    out.extend_from_slice(&data);
    out.extend_from_slice(&mix_key(&aes_key, key, false));
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn roundtrip() {
        let header = *b"CWS\x32\x01\x02\x03\x04";
        let payload = b"ABC\r\n";
        let enc = super::encrypt(&header, payload).unwrap();
        assert_eq!(enc[0], b'c');
        let (h, dec) = super::decrypt(enc[..8].try_into().unwrap(), &enc[8..]).unwrap();
        assert_eq!(h, header);
        assert_eq!(dec, payload);
    }
}
