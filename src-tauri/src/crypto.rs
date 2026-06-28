//! 加密原语：Argon2id 密钥派生 + XChaCha20-Poly1305 认证加密(AEAD) + 两层密钥(DEK/KEK)。
//!
//! 设计要点（详见仓库根目录 FORMAT.md）：
//! - 密码不直接当密钥，而是经 Argon2id + 随机 salt 派生出 KEK；
//! - 真正加密日记的是随机数据密钥 DEK，DEK 被 KEK 加密(包裹)后存于 meta.json；
//! - 所有密文都用 XChaCha20-Poly1305（带认证标签），密码错/数据损坏时解密会失败。

use anyhow::{anyhow, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};

pub const SALT_LEN: usize = 16;
pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 24;

/// Argon2id 参数（OWASP 推荐基线：内存 19 MiB，迭代 2，并行 1）。
#[derive(Clone, Serialize, Deserialize)]
pub struct KdfParams {
    /// 内存开销，单位 KiB
    pub m_cost: u32,
    /// 迭代次数
    pub t_cost: u32,
    /// 并行度
    pub p_cost: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        KdfParams {
            m_cost: 19_456,
            t_cost: 2,
            p_cost: 1,
        }
    }
}

/// 一个加密块（nonce + 密文），以 base64 字符串形式存放于 JSON 字段中。
#[derive(Clone, Serialize, Deserialize)]
pub struct Sealed {
    /// base64(nonce)
    pub nonce: String,
    /// base64(ciphertext+tag)
    pub ct: String,
}

/// 生成随机 salt。
pub fn random_salt() -> [u8; SALT_LEN] {
    let mut s = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut s);
    s
}

/// 生成随机数据密钥 DEK。
pub fn random_dek() -> [u8; KEY_LEN] {
    let mut k = [0u8; KEY_LEN];
    OsRng.fill_bytes(&mut k);
    k
}

/// 用 Argon2id 从密码 + salt 派生 32 字节密钥。
pub fn derive_key(password: &[u8], salt: &[u8], params: &KdfParams) -> Result<[u8; KEY_LEN]> {
    let p = Params::new(params.m_cost, params.t_cost, params.p_cost, Some(KEY_LEN))
        .map_err(|e| anyhow!("argon2 参数错误: {e}"))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, p);
    let mut key = [0u8; KEY_LEN];
    argon
        .hash_password_into(password, salt, &mut key)
        .map_err(|e| anyhow!("密钥派生失败: {e}"))?;
    Ok(key)
}

/// 用 key 加密明文，返回 (nonce, ciphertext) 原始字节。
pub fn encrypt_raw(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| anyhow!("密钥长度错误"))?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| anyhow!("加密失败"))?;
    Ok((nonce_bytes.to_vec(), ct))
}

/// 用 key 解密。认证失败（密码错误或数据损坏）返回 Err。
pub fn decrypt_raw(key: &[u8; KEY_LEN], nonce: &[u8], ct: &[u8]) -> Result<Vec<u8>> {
    if nonce.len() != NONCE_LEN {
        return Err(anyhow!("nonce 长度非法"));
    }
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| anyhow!("密钥长度错误"))?;
    let nonce = XNonce::from_slice(nonce);
    cipher
        .decrypt(nonce, ct)
        .map_err(|_| anyhow!("解密失败（密码错误或数据损坏）"))
}

/// 把明文加密成 Sealed（用于 JSON 字段，如包裹后的 DEK）。
pub fn seal(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<Sealed> {
    let (nonce, ct) = encrypt_raw(key, plaintext)?;
    Ok(Sealed {
        nonce: B64.encode(nonce),
        ct: B64.encode(ct),
    })
}

/// 解密一个 Sealed。
pub fn unseal(key: &[u8; KEY_LEN], s: &Sealed) -> Result<Vec<u8>> {
    let nonce = B64.decode(s.nonce.as_bytes()).map_err(|e| anyhow!("base64: {e}"))?;
    let ct = B64.decode(s.ct.as_bytes()).map_err(|e| anyhow!("base64: {e}"))?;
    decrypt_raw(key, &nonce, &ct)
}

/// 把明文加密成 .enc 文件字节：布局为 [nonce(24)] ++ [ciphertext+tag]。
pub fn encrypt_file_bytes(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<Vec<u8>> {
    let (nonce, ct) = encrypt_raw(key, plaintext)?;
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// 解密一个 .enc 文件字节（前 24 字节为 nonce）。
pub fn decrypt_file_bytes(key: &[u8; KEY_LEN], data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < NONCE_LEN {
        return Err(anyhow!("加密文件过短"));
    }
    let (nonce, ct) = data.split_at(NONCE_LEN);
    decrypt_raw(key, nonce, ct)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 测试用的轻量 KDF 参数，加速单测。
    fn fast_params() -> KdfParams {
        KdfParams {
            m_cost: 256,
            t_cost: 1,
            p_cost: 1,
        }
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = random_dek();
        let msg = "今天写了第一篇加密日记 hello".as_bytes();
        let (nonce, ct) = encrypt_raw(&key, msg).unwrap();
        assert_ne!(ct.as_slice(), msg, "密文不应等于明文");
        let pt = decrypt_raw(&key, &nonce, &ct).unwrap();
        assert_eq!(pt.as_slice(), msg);
    }

    #[test]
    fn wrong_password_fails() {
        let salt = random_salt();
        let key_ok = derive_key(b"correct horse", &salt, &fast_params()).unwrap();
        let key_bad = derive_key(b"Tr0ub4dor", &salt, &fast_params()).unwrap();
        let (nonce, ct) = encrypt_raw(&key_ok, b"secret").unwrap();
        // 错误密钥解密必须失败（认证标签不通过）
        assert!(decrypt_raw(&key_bad, &nonce, &ct).is_err());
        // 正确密钥仍可解
        assert_eq!(decrypt_raw(&key_ok, &nonce, &ct).unwrap(), b"secret");
    }

    #[test]
    fn two_tier_change_password_keeps_data() {
        let params = fast_params();
        // 初始：用旧密码包裹 DEK
        let salt_old = random_salt();
        let kek_old = derive_key(b"old-pass", &salt_old, &params).unwrap();
        let dek = random_dek();
        let wrapped = seal(&kek_old, &dek).unwrap();

        // 用 DEK 加密一段日记
        let entry = encrypt_file_bytes(&dek, "# 标题\n正文".as_bytes()).unwrap();

        // 改密码：用旧密码解出 DEK，再用新密码 + 新 salt 重新包裹
        let recovered = unseal(&kek_old, &wrapped).unwrap();
        assert_eq!(recovered, dek.to_vec());
        let salt_new = random_salt();
        let kek_new = derive_key(b"new-pass", &salt_new, &params).unwrap();
        let mut dek2 = [0u8; KEY_LEN];
        dek2.copy_from_slice(&recovered);
        let rewrapped = seal(&kek_new, &dek2).unwrap();

        // 旧密码不再能解新包裹；新密码能解，且老日记仍可用同一个 DEK 解开
        assert!(unseal(&kek_old, &rewrapped).is_err());
        let dek_after = unseal(&kek_new, &rewrapped).unwrap();
        assert_eq!(dek_after, dek.to_vec());
        let mut dek3 = [0u8; KEY_LEN];
        dek3.copy_from_slice(&dek_after);
        assert_eq!(
            String::from_utf8(decrypt_file_bytes(&dek3, &entry).unwrap()).unwrap(),
            "# 标题\n正文"
        );
    }

    #[test]
    fn file_bytes_roundtrip_and_tamper_detection() {
        let key = random_dek();
        let mut data = encrypt_file_bytes(&key, b"image-bytes").unwrap();
        assert_eq!(decrypt_file_bytes(&key, &data).unwrap(), b"image-bytes");
        // 篡改最后一个字节 -> 认证失败
        let last = data.len() - 1;
        data[last] ^= 0xff;
        assert!(decrypt_file_bytes(&key, &data).is_err());
    }
}
