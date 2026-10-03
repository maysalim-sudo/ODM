use super::playlist::Key;
use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};

/// `key.iv` when present, else the segment sequence number as a 128-bit big-endian value.
pub fn iv_for(key: &Key, seq: u64) -> [u8; 16] {
    if let Some(iv) = key.iv {
        return iv;
    }
    let mut iv = [0u8; 16];
    iv[8..].copy_from_slice(&seq.to_be_bytes());
    iv
}

/// AES-128-CBC with PKCS7 padding.
pub fn decrypt_aes128(key: &[u8; 16], iv: &[u8; 16], data: &[u8]) -> Result<Vec<u8>, String> {
    cbc::Decryptor::<aes::Aes128>::new(key.into(), iv.into())
        .decrypt_padded_vec_mut::<Pkcs7>(data)
        .map_err(|e| format!("decryption failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hls::playlist::KeyMethod;
    use aes::cipher::BlockEncryptMut;

    #[test]
    fn decrypt_round_trip() {
        let key = [7u8; 16];
        let iv = [9u8; 16];
        let original: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        let enc = cbc::Encryptor::<aes::Aes128>::new((&key).into(), (&iv).into())
            .encrypt_padded_vec_mut::<Pkcs7>(&original);
        assert_ne!(enc, original);
        assert_eq!(decrypt_aes128(&key, &iv, &enc).unwrap(), original);
        let wrong = [8u8; 16];
        match decrypt_aes128(&wrong, &iv, &enc) {
            Err(_) => {}
            Ok(bytes) => assert_ne!(bytes, original),
        }
    }

    #[test]
    fn iv_from_sequence() {
        let k = Key {
            method: KeyMethod::Aes128,
            uri: None,
            iv: None,
        };
        let mut want = [0u8; 16];
        want[15] = 5;
        assert_eq!(iv_for(&k, 5), want);
        let explicit = Key {
            iv: Some([3u8; 16]),
            ..k
        };
        assert_eq!(iv_for(&explicit, 5), [3u8; 16]);
    }
}
