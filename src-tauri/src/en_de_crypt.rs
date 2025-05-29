use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rand::{TryRngCore, rngs::OsRng};
use std::str;

// 加密函数
pub(crate) fn encrypt(data: &str, key: &[u8]) -> (String, String) {
    // 确保密钥长度正确（32字节）
    let key = if key.len() < 32 {
        let mut new_key = [0u8; 32];
        new_key[..key.len()].copy_from_slice(key);
        new_key
    } else {
        key[0..32].try_into().expect("密钥长度错误")
    };

    let cipher = Aes256Gcm::new((&key).into());

    // 正确生成随机 nonce (12 字节)
    let mut nonce_bytes = [0u8; 12];
    OsRng
        .try_fill_bytes(nonce_bytes.as_mut_slice())
        .expect("无法生成随机数");
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher.encrypt(nonce, data.as_bytes()).expect("加密失败");

    (STANDARD.encode(&ciphertext), STANDARD.encode(nonce))
}

// 解密函数
pub(crate) fn decrypt(encrypted_data: &str, nonce: &str, key: &[u8]) -> String {
    // 确保密钥长度正确（32字节）
    let key = if key.len() < 32 {
        let mut new_key = [0u8; 32];
        new_key[..key.len()].copy_from_slice(key);
        new_key
    } else {
        key[0..32].try_into().expect("密钥长度错误")
    };

    let cipher = Aes256Gcm::new((&key).into());

    // 修复：先将解码结果存储在变量中
    let decoded_nonce = STANDARD.decode(nonce).expect("无效的nonce");
    let nonce = Nonce::from_slice(&decoded_nonce);

    let decoded_data = STANDARD.decode(encrypted_data).expect("无效的密文");
    let decrypted_ciphertext = cipher.decrypt(nonce, &*decoded_data).expect("解密失败");

    str::from_utf8(&decrypted_ciphertext)
        .expect("无效的UTF-8数据")
        .to_string()
}
