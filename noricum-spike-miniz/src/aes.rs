//! WinZip AES-256 decryption for ZIP entries (APPNOTE.TXT 7.4).
//!
//! Implements the AE-1 / AE-2 schemes:
//!   1. Salt + password-verification + encrypted data + 10-byte HMAC
//!      prefix/suffix layout, where SALT_LEN depends on AES strength
//!      (8 / 12 / 16 bytes for AES-128 / 192 / 256).
//!   2. PBKDF2-HMAC-SHA1(password, salt, 1000, 2*key_len + 2) key derivation.
//!      The derived material splits into encryption key + MAC key + 2 bytes
//!      of password verification.
//!   3. Password verification: compare the 2 derived verification bytes to
//!      the 2 bytes stored after the salt in the archive.
//!   4. Authentication: HMAC-SHA1(mac_key, ciphertext) truncated to 10 bytes,
//!      compared against the 10 bytes after the ciphertext in constant time.
//!   5. Decryption: AES-CTR mode with a ZIP-specific counter convention —
//!      the counter starts at 1, increments per 16-byte block, is LE-encoded
//!      in the low 4 bytes of a 16-byte counter block, with the upper 12
//!      bytes zero. Note that this differs from NIST AES-CTR which typically
//!      starts at 0; WinZip's 1-based start is the key gotcha.
//!
//! The output of this module is the raw bytes that would be stored under
//! the "actual compression method" recorded in the AES extra field. Those
//! bytes then pass through the normal Stored / Deflated path.

use aes::Aes128;
use aes::Aes192;
use aes::Aes256;
use aes::cipher::{BlockEncrypt, KeyInit};
use aes::cipher::generic_array::GenericArray;
use hmac::{Hmac, Mac};
use sha1::Sha1;

use crate::contract::{ZipError, ZipResult};
use crate::reader::{read_u16_le, ZIP64_EXTRA_HEADER_ID};

/// Header ID for the WinZip AES-256 extra field.
pub const AES_EXTRA_HEADER_ID: u16 = 0x9901;
/// Compression method code that marks an entry as AES-encrypted. The real
/// compression method (Stored / Deflated / etc.) is stored inside the AES
/// extra field.
pub const AES_COMPRESSION_METHOD: u16 = 99;

/// Password verification length (APPNOTE 7.4.2).
pub const PWD_VERIFY_LENGTH: usize = 2;
/// Authentication code length — HMAC-SHA1 truncated to the first 10 bytes.
pub const AUTH_CODE_LENGTH: usize = 10;
/// PBKDF2 iterations per WinZip AES spec.
pub const PBKDF2_ITERATIONS: u32 = 1000;

/// Supported AES key strengths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AesStrength {
    /// AES-128: 8-byte salt, 16-byte key.
    Aes128,
    /// AES-192: 12-byte salt, 24-byte key.
    Aes192,
    /// AES-256: 16-byte salt, 32-byte key.
    Aes256,
}

impl AesStrength {
    pub fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(AesStrength::Aes128),
            2 => Some(AesStrength::Aes192),
            3 => Some(AesStrength::Aes256),
            _ => None,
        }
    }

    pub fn key_len(self) -> usize {
        match self {
            AesStrength::Aes128 => 16,
            AesStrength::Aes192 => 24,
            AesStrength::Aes256 => 32,
        }
    }

    pub fn salt_len(self) -> usize {
        // Per APPNOTE, salt length is half the key length.
        self.key_len() / 2
    }
}

/// Parsed AES extra field. The real compression method sits inside the
/// AES extra; the entry's "top-level" compression_method is the marker 99.
#[derive(Clone, Copy, Debug)]
pub struct AesExtraInfo {
    pub version: u16,
    pub vendor: [u8; 2],
    pub strength: AesStrength,
    pub real_compression_method: u16,
}

/// Walk an entry's extra field looking for the AES extra (header ID 0x9901).
/// Returns `None` if no AES extra is present (entry is not AES encrypted).
pub fn find_aes_extra(extra: &[u8]) -> Option<AesExtraInfo> {
    let mut i = 0usize;
    while i + 4 <= extra.len() {
        let header_id = read_u16_le(extra, i);
        let data_size = read_u16_le(extra, i + 2) as usize;
        if i + 4 + data_size > extra.len() {
            return None;
        }
        if header_id == AES_EXTRA_HEADER_ID && data_size >= 7 {
            let off = i + 4;
            let version = read_u16_le(extra, off);
            let vendor = [extra[off + 2], extra[off + 3]];
            let strength = AesStrength::from_wire(extra[off + 4])?;
            let real_method = read_u16_le(extra, off + 5);
            return Some(AesExtraInfo {
                version,
                vendor,
                strength,
                real_compression_method: real_method,
            });
        }
        // Skip past zip64 extras and anything else.
        let _ = ZIP64_EXTRA_HEADER_ID;
        i += 4 + data_size;
    }
    None
}

/// Decrypt a WinZip AES-encrypted blob. `raw` is the full blob as read from
/// the archive starting at the data offset: [salt][pwd_verify][ciphertext][auth_code].
/// Returns the plaintext (still compressed by the "real" compression method;
/// the caller must then decompress if needed).
pub fn decrypt(raw: &[u8], info: AesExtraInfo, password: &[u8]) -> ZipResult<Vec<u8>> {
    let salt_len = info.strength.salt_len();
    let key_len = info.strength.key_len();
    let prefix_len = salt_len + PWD_VERIFY_LENGTH;
    let min_len = prefix_len + AUTH_CODE_LENGTH;

    if raw.len() < min_len {
        return Err(ZipError::InvalidHeaderOrCorrupted);
    }

    let salt = &raw[..salt_len];
    let archive_pwd_verify = &raw[salt_len..prefix_len];
    let ciphertext = &raw[prefix_len..raw.len() - AUTH_CODE_LENGTH];
    let archive_auth_code = &raw[raw.len() - AUTH_CODE_LENGTH..];

    // PBKDF2-HMAC-SHA1(password, salt, 1000, 2*key_len + 2)
    let derived_len = 2 * key_len + PWD_VERIFY_LENGTH;
    let mut derived = vec![0u8; derived_len];
    pbkdf2::pbkdf2::<Hmac<Sha1>>(password, salt, PBKDF2_ITERATIONS, &mut derived)
        .map_err(|_| ZipError::InternalError)?;

    let enc_key = &derived[..key_len];
    let mac_key = &derived[key_len..2 * key_len];
    let pwd_verify = &derived[2 * key_len..];

    // Constant-time password verification.
    if !constant_time_eq::constant_time_eq(pwd_verify, archive_pwd_verify) {
        return Err(ZipError::InvalidParameter);
    }

    // Verify HMAC-SHA1 over the ciphertext.
    let mut mac = <Hmac<Sha1> as Mac>::new_from_slice(mac_key)
        .map_err(|_| ZipError::InternalError)?;
    mac.update(ciphertext);
    let computed = mac.finalize().into_bytes();
    if !constant_time_eq::constant_time_eq(&computed[..AUTH_CODE_LENGTH], archive_auth_code) {
        return Err(ZipError::CrcCheckFailed);
    }

    // AES-CTR decryption. ZIP uses a 1-based counter in the low 4 bytes of
    // the 16-byte counter block, LE-encoded, upper 12 bytes zero. We
    // implement CTR manually instead of using the `ctr` crate because the
    // counter layout isn't the standard nonce+counter form.
    let mut plaintext = ciphertext.to_vec();
    match info.strength {
        AesStrength::Aes128 => decrypt_ctr::<Aes128>(enc_key, &mut plaintext),
        AesStrength::Aes192 => decrypt_ctr::<Aes192>(enc_key, &mut plaintext),
        AesStrength::Aes256 => decrypt_ctr::<Aes256>(enc_key, &mut plaintext),
    };

    Ok(plaintext)
}

/// Apply AES-CTR using a ZIP-specific 1-based 4-byte counter.
fn decrypt_ctr<C>(key: &[u8], data: &mut [u8])
where
    C: BlockEncrypt + KeyInit,
{
    let cipher = <C as KeyInit>::new_from_slice(key).expect("key length checked by caller");
    // BlockEncrypt::block_size() is a const on the associated BlockSize type
    // but calling it through the trait requires a concrete size. For AES
    // all three variants have block size = 16, so we hard-code.
    const BLOCK: usize = 16;

    let mut counter: u32 = 1;
    let mut i = 0usize;
    while i < data.len() {
        let mut block = [0u8; BLOCK];
        // Low 4 bytes of the counter block are LE counter, rest are zero.
        block[..4].copy_from_slice(&counter.to_le_bytes());
        let mut key_stream = GenericArray::clone_from_slice(&block);
        cipher.encrypt_block(&mut key_stream);

        let take = core::cmp::min(BLOCK, data.len() - i);
        for j in 0..take {
            data[i + j] ^= key_stream[j];
        }

        counter = counter.wrapping_add(1);
        i += BLOCK;
    }
}

#[cfg(test)]
mod aes_tests {
    use super::*;

    #[test]
    fn strength_from_wire_and_lengths() {
        assert_eq!(AesStrength::from_wire(1), Some(AesStrength::Aes128));
        assert_eq!(AesStrength::from_wire(2), Some(AesStrength::Aes192));
        assert_eq!(AesStrength::from_wire(3), Some(AesStrength::Aes256));
        assert_eq!(AesStrength::from_wire(9), None);

        assert_eq!(AesStrength::Aes128.key_len(), 16);
        assert_eq!(AesStrength::Aes192.key_len(), 24);
        assert_eq!(AesStrength::Aes256.key_len(), 32);

        assert_eq!(AesStrength::Aes128.salt_len(), 8);
        assert_eq!(AesStrength::Aes192.salt_len(), 12);
        assert_eq!(AesStrength::Aes256.salt_len(), 16);
    }

    #[test]
    fn find_aes_extra_parses_header_9901() {
        // Mock extra field: [0x01 0x00 (zip64 id)][0x00 0x00 (empty data)][0x01 0x99 (AES id)][0x07 0x00 (data_size=7)][data...]
        // Data: version=2 vendor="AE" strength=3 real_method=8
        let extra = vec![
            0x01, 0x00, // header id 0x0001 (zip64)
            0x00, 0x00, // data size 0
            // no data
            0x01, 0x99, // header id 0x9901 (AES)
            0x07, 0x00, // data size 7
            0x02, 0x00, // version 2 (AE-2)
            b'A', b'E', // vendor
            0x03, // strength (AES-256)
            0x08, 0x00, // real compression method (deflated)
        ];
        let info = find_aes_extra(&extra).expect("should parse");
        assert_eq!(info.version, 2);
        assert_eq!(&info.vendor, b"AE");
        assert_eq!(info.strength, AesStrength::Aes256);
        assert_eq!(info.real_compression_method, 8);
    }

    #[test]
    fn find_aes_extra_none_when_absent() {
        let extra = vec![0x01, 0x00, 0x00, 0x00]; // just a zip64 header with no data
        assert!(find_aes_extra(&extra).is_none());
    }
}
