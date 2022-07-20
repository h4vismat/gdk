use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use crate::error::fn_err;
use aes_gcm_siv::aead::{AeadInPlace, NewAead};
use aes_gcm_siv::{Aes256GcmSiv, Key, Nonce};
use bitcoin::hashes::{sha256, Hash};
use bitcoin::util::bip32::ExtendedPubKey;
use rand::Rng;

use crate::Result;

pub trait Decryptable {
    fn decrypt(self, cipher: &Aes256GcmSiv) -> Result<Vec<u8>>;
}

impl Decryptable for &mut File {
    fn decrypt(self, cipher: &Aes256GcmSiv) -> Result<Vec<u8>> {
        let mut buf = Vec::<u8>::new();
        self.seek(SeekFrom::Start(0))?;
        self.read_to_end(&mut buf)?;
        buf.decrypt(cipher)
    }
}

impl Decryptable for Vec<u8> {
    fn decrypt(self, cipher: &Aes256GcmSiv) -> Result<Vec<u8>> {
        let mut iter = self.into_iter();

        let nonce = Nonce::from_exact_iter(iter.by_ref().take(12))
            .ok_or_else(fn_err("vector should be longer than 12 bytes"))?;
        let mut rest = iter.collect::<Vec<_>>();

        cipher.decrypt_in_place(&nonce, b"", &mut rest)?;
        Ok(rest)
    }
}

pub trait Encryptable {
    fn encrypt(self, key: &Aes256GcmSiv) -> Result<([u8; 12], Vec<u8>)>;
}

impl Encryptable for Vec<u8> {
    fn encrypt(mut self, cipher: &Aes256GcmSiv) -> Result<([u8; 12], Vec<u8>)> {
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        cipher.encrypt_in_place(nonce, b"", &mut self)?;
        Ok((nonce_bytes, self))
    }
}

/// Gets a cipher from an xpub.
pub fn to_cipher(xpub: ExtendedPubKey) -> Aes256GcmSiv {
    let mut enc_key_data = vec![];
    enc_key_data.extend(&xpub.public_key.to_bytes());
    enc_key_data.extend(&xpub.chain_code.to_bytes());
    enc_key_data.extend(&xpub.network.magic().to_be_bytes());
    let key_bytes = sha256::Hash::hash(&enc_key_data).into_inner();
    let key = Key::from_slice(&key_bytes);
    Aes256GcmSiv::new(&key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::str::FromStr;

    const XPUB: &str = "tpubD97UxEEcrMpkE8yG3NQveraWveHzTAJx3KwPsUycx9ABfxRjMtiwfm6BtrY5yhF9yF2eyMg2hyDtGDYXx6gVLBox1m2Mq4u8zB2NXFhUZmm";

    fn test_data_with_cipher() -> (Vec<u8>, Aes256GcmSiv) {
        let mut data = [0u8; 64];
        rand::thread_rng().fill(&mut data);

        let xpub = ExtendedPubKey::from_str(XPUB).unwrap();
        let cipher = to_cipher(xpub);

        (data.to_vec(), cipher)
    }

    #[test]
    fn test_bytes_encryption() {
        let (data, cipher) = test_data_with_cipher();

        let (nonce, rest) = data.clone().encrypt(&cipher).unwrap();
        let encrypted = nonce.iter().map(|byte| *byte).chain(rest.into_iter()).collect::<Vec<_>>();

        let decrypted = encrypted.decrypt(&cipher).unwrap();
        assert_eq!(data, decrypted);
    }

    #[test]
    fn test_file_encryption() {
        let (data, cipher) = test_data_with_cipher();
        let mut file = tempfile::tempfile().unwrap();

        let (nonce, rest) = data.clone().encrypt(&cipher).unwrap();
        let encrypted = nonce.iter().map(|byte| *byte).chain(rest.into_iter()).collect::<Vec<_>>();

        file.write_all(&encrypted).unwrap();

        let decrypted = file.decrypt(&cipher).unwrap();
        assert_eq!(data, decrypted)
    }
}