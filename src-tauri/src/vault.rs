//! Vault 存储层：负责磁盘上的文件布局与（反）序列化。
//!
//! 目录布局（位于 app_local_data_dir()/vault）：
//! ```
//! vault/
//!   meta.json            明文(非敏感): version/kdf/salt/hint ；密文: wrapped_dek
//!   index.enc            加密索引: 条目清单(id/date/title/时间)
//!   entries/<id>.enc     每条日记 = nonce++密文，明文是 Markdown
//!   blobs/<id>.enc       每张图片 = nonce++密文，明文是图片原始字节
//! ```

use crate::crypto::{self, KdfParams, Sealed, KEY_LEN};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const FORMAT_VERSION: u32 = 1;

/// meta.json 中的 KDF 描述（salt 与参数明文存放，非机密）。
#[derive(Serialize, Deserialize)]
pub struct KdfMeta {
    pub algo: String, // "argon2id"
    #[serde(flatten)]
    pub params: KdfParams,
    pub salt: String, // base64
}

/// meta.json 顶层结构。
#[derive(Serialize, Deserialize)]
pub struct Meta {
    pub version: u32,
    pub cipher: String, // "xchacha20poly1305"
    pub kdf: KdfMeta,
    pub hint: String,
    pub wrapped_dek: Sealed,
    pub created_at: u64,
}

/// 索引中的单条记录（加密存放于 index.enc）。
#[derive(Serialize, Deserialize, Clone)]
pub struct IndexEntry {
    pub id: String,
    pub date: String, // YYYY-MM-DD
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Index {
    pub entries: Vec<IndexEntry>,
}

pub struct Vault {
    pub dir: PathBuf,
}

impl Vault {
    pub fn new(dir: PathBuf) -> Self {
        Vault { dir }
    }

    fn meta_path(&self) -> PathBuf {
        self.dir.join("meta.json")
    }
    fn index_path(&self) -> PathBuf {
        self.dir.join("index.enc")
    }
    fn entries_dir(&self) -> PathBuf {
        self.dir.join("entries")
    }
    fn blobs_dir(&self) -> PathBuf {
        self.dir.join("blobs")
    }
    fn entry_path(&self, id: &str) -> PathBuf {
        self.entries_dir().join(format!("{id}.enc"))
    }
    fn blob_path(&self, id: &str) -> PathBuf {
        self.blobs_dir().join(format!("{id}.enc"))
    }

    /// 是否已初始化（存在 meta.json）。
    pub fn exists(&self) -> bool {
        self.meta_path().exists()
    }

    pub fn read_meta(&self) -> Result<Meta> {
        let s = fs::read_to_string(self.meta_path())?;
        Ok(serde_json::from_str(&s)?)
    }

    pub fn write_meta(&self, m: &Meta) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        fs::write(self.meta_path(), serde_json::to_string_pretty(m)?)?;
        Ok(())
    }

    pub fn read_index(&self, dek: &[u8; KEY_LEN]) -> Result<Index> {
        if !self.index_path().exists() {
            return Ok(Index::default());
        }
        let data = fs::read(self.index_path())?;
        let pt = crypto::decrypt_file_bytes(dek, &data)?;
        Ok(serde_json::from_slice(&pt)?)
    }

    pub fn write_index(&self, dek: &[u8; KEY_LEN], idx: &Index) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        let pt = serde_json::to_vec(idx)?;
        let data = crypto::encrypt_file_bytes(dek, &pt)?;
        fs::write(self.index_path(), data)?;
        Ok(())
    }

    pub fn read_entry(&self, dek: &[u8; KEY_LEN], id: &str) -> Result<String> {
        let data = fs::read(self.entry_path(id))?;
        let pt = crypto::decrypt_file_bytes(dek, &data)?;
        Ok(String::from_utf8(pt)?)
    }

    pub fn write_entry(&self, dek: &[u8; KEY_LEN], id: &str, markdown: &str) -> Result<()> {
        fs::create_dir_all(self.entries_dir())?;
        let data = crypto::encrypt_file_bytes(dek, markdown.as_bytes())?;
        fs::write(self.entry_path(id), data)?;
        Ok(())
    }

    pub fn delete_entry_file(&self, id: &str) -> Result<()> {
        let p = self.entry_path(id);
        if p.exists() {
            fs::remove_file(p)?;
        }
        Ok(())
    }

    pub fn write_blob(&self, dek: &[u8; KEY_LEN], id: &str, bytes: &[u8]) -> Result<()> {
        fs::create_dir_all(self.blobs_dir())?;
        let data = crypto::encrypt_file_bytes(dek, bytes)?;
        fs::write(self.blob_path(id), data)?;
        Ok(())
    }

    pub fn read_blob(&self, dek: &[u8; KEY_LEN], id: &str) -> Result<Vec<u8>> {
        let data = fs::read(self.blob_path(id))?;
        crypto::decrypt_file_bytes(dek, &data)
    }

    // ── 云同步相关 ──

    fn webdav_path(&self) -> PathBuf {
        self.dir.join("webdav.enc")
    }

    pub fn has_webdav(&self) -> bool {
        self.webdav_path().exists()
    }

    /// 读取（解密）WebDAV 配置。未配置返回 None。
    pub fn read_webdav_config(&self, dek: &[u8; KEY_LEN]) -> Result<Option<WebDavConfig>> {
        if !self.webdav_path().exists() {
            return Ok(None);
        }
        let data = fs::read(self.webdav_path())?;
        let pt = crypto::decrypt_file_bytes(dek, &data)?;
        Ok(Some(serde_json::from_slice(&pt)?))
    }

    /// 加密保存 WebDAV 配置（含坚果云应用密码）。
    pub fn write_webdav_config(&self, dek: &[u8; KEY_LEN], cfg: &WebDavConfig) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        let pt = serde_json::to_vec(cfg)?;
        let data = crypto::encrypt_file_bytes(dek, &pt)?;
        fs::write(self.webdav_path(), data)?;
        Ok(())
    }

    /// 由相对路径（用 / 分隔）得到绝对路径。
    pub fn abs(&self, rel: &str) -> PathBuf {
        let mut p = self.dir.clone();
        for seg in rel.split('/') {
            if !seg.is_empty() {
                p.push(seg);
            }
        }
        p
    }

    /// 列出需要参与同步的本地文件（相对路径）：
    /// meta.json、index.enc、entries/*.enc、blobs/*.enc。
    /// 刻意排除 webdav.enc（含云端凭据）与 sync_state.json。
    pub fn local_sync_files(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.meta_path().exists() {
            out.push("meta.json".to_string());
        }
        if self.index_path().exists() {
            out.push("index.enc".to_string());
        }
        for (sub, dir) in [("entries", self.entries_dir()), ("blobs", self.blobs_dir())] {
            if let Ok(rd) = fs::read_dir(&dir) {
                for ent in rd.flatten() {
                    if let Some(name) = ent.file_name().to_str() {
                        if name.ends_with(".enc") {
                            out.push(format!("{sub}/{name}"));
                        }
                    }
                }
            }
        }
        out
    }
}

/// 坚果云 / WebDAV 配置（加密存于 vault/webdav.enc）。
#[derive(Serialize, Deserialize, Clone)]
pub struct WebDavConfig {
    pub url: String,
    pub username: String,
    pub password: String,
    pub remote_dir: String,
}

fn default_true() -> bool {
    true
}

/// 非机密的应用偏好（明文存于 vault/settings.json）。
#[derive(Serialize, Deserialize, Clone)]
pub struct Settings {
    /// 解锁后自动拉取
    #[serde(default = "default_true")]
    pub sync_pull_on_unlock: bool,
    /// 退出时自动推送
    #[serde(default = "default_true")]
    pub sync_push_on_exit: bool,
    /// 定时同步分钟数（0=关闭）
    #[serde(default)]
    pub sync_interval_min: u32,
    /// 保存后自动同步
    #[serde(default)]
    pub sync_on_save: bool,
    /// 启动时自动检查更新
    #[serde(default)]
    pub auto_update_check: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            sync_pull_on_unlock: true,
            sync_push_on_exit: true,
            sync_interval_min: 0,
            sync_on_save: false,
            auto_update_check: false,
        }
    }
}

impl Vault {
    fn settings_path(&self) -> PathBuf {
        self.dir.join("settings.json")
    }

    pub fn read_settings(&self) -> Result<Settings> {
        match fs::read(self.settings_path()) {
            Ok(b) => Ok(serde_json::from_slice(&b).unwrap_or_default()),
            Err(_) => Ok(Settings::default()),
        }
    }

    pub fn write_settings(&self, s: &Settings) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        fs::write(self.settings_path(), serde_json::to_vec_pretty(s)?)?;
        Ok(())
    }
}

/// 构造一个全新 vault 的 meta（首次设置密码时调用）。
pub fn build_meta(hint: String, salt_b64: String, params: KdfParams, wrapped_dek: Sealed) -> Meta {
    Meta {
        version: FORMAT_VERSION,
        cipher: "xchacha20poly1305".to_string(),
        kdf: KdfMeta {
            algo: "argon2id".to_string(),
            params,
            salt: salt_b64,
        },
        hint,
        wrapped_dek,
        created_at: crate::commands::now_ms(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto;
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("diary_vault_test_{nanos}"))
    }

    // 轻量 KDF 参数以加速测试
    fn fast() -> KdfParams {
        KdfParams {
            m_cost: 256,
            t_cost: 1,
            p_cost: 1,
        }
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    /// 模拟首次设置密码，返回 (vault, dek)。
    fn setup(dir: &PathBuf, password: &str, params: &KdfParams) -> ([u8; 32], Vault) {
        let v = Vault::new(dir.clone());
        let salt = crypto::random_salt();
        let kek = crypto::derive_key(password.as_bytes(), &salt, params).unwrap();
        let dek = crypto::random_dek();
        let wrapped = crypto::seal(&kek, &dek).unwrap();
        let meta = build_meta("我的提示".to_string(), B64.encode(salt), params.clone(), wrapped);
        v.write_meta(&meta).unwrap();
        v.write_index(&dek, &Index::default()).unwrap();
        (dek, v)
    }

    #[test]
    fn full_lifecycle_encrypts_at_rest_and_survives_password_change() {
        let dir = temp_dir();
        let params = fast();
        let password = "p@ss码123";
        let (dek, v) = setup(&dir, password, &params);

        // 保存一条带明文标记的日记
        let plaintext = "# 秘密\n只有我知道 SECRET-MARKER-XYZ";
        let id = "entry-1";
        v.write_entry(&dek, id, plaintext).unwrap();
        let mut index = v.read_index(&dek).unwrap();
        index.entries.push(IndexEntry {
            id: id.to_string(),
            date: "2026-06-28".to_string(),
            title: "秘密".to_string(),
            created_at: 0,
            updated_at: 0,
        });
        v.write_index(&dek, &index).unwrap();

        // 需求5/7：落盘必须是密文，原始文件里不能出现明文标记
        let raw = std::fs::read(dir.join("entries").join(format!("{id}.enc"))).unwrap();
        assert!(!contains(&raw, b"SECRET-MARKER-XYZ"), "日记落盘必须是密文");
        // 用 DEK 能解出原文
        assert_eq!(v.read_entry(&dek, id).unwrap(), plaintext);

        // 需求1：正确密码可还原 DEK，错误密码失败
        let meta = v.read_meta().unwrap();
        let salt = B64.decode(meta.kdf.salt.as_bytes()).unwrap();
        let kek_ok = crypto::derive_key(password.as_bytes(), &salt, &meta.kdf.params).unwrap();
        assert_eq!(crypto::unseal(&kek_ok, &meta.wrapped_dek).unwrap(), dek.to_vec());
        let kek_bad = crypto::derive_key(b"wrong", &salt, &meta.kdf.params).unwrap();
        assert!(crypto::unseal(&kek_bad, &meta.wrapped_dek).is_err());

        // 需求8相关：改密码后老日记仍可解
        let mut meta2 = v.read_meta().unwrap();
        let new_salt = crypto::random_salt();
        let new_kek = crypto::derive_key(b"newpass", &new_salt, &params).unwrap();
        meta2.wrapped_dek = crypto::seal(&new_kek, &dek).unwrap();
        meta2.kdf = KdfMeta {
            algo: "argon2id".to_string(),
            params: params.clone(),
            salt: B64.encode(new_salt),
        };
        v.write_meta(&meta2).unwrap();

        let meta3 = v.read_meta().unwrap();
        let salt3 = B64.decode(meta3.kdf.salt.as_bytes()).unwrap();
        let kek3 = crypto::derive_key(b"newpass", &salt3, &meta3.kdf.params).unwrap();
        let dek3v = crypto::unseal(&kek3, &meta3.wrapped_dek).unwrap();
        let mut dek3 = [0u8; 32];
        dek3.copy_from_slice(&dek3v);
        assert_eq!(v.read_entry(&dek3, id).unwrap(), plaintext);
        // 旧密码不再可用
        let kek_old = crypto::derive_key(password.as_bytes(), &salt3, &meta3.kdf.params).unwrap();
        assert!(crypto::unseal(&kek_old, &meta3.wrapped_dek).is_err());

        // 图片 blob：落盘密文 + 往返一致
        let img = b"\x89PNG\r\n FAKE-IMAGE-BYTES";
        v.write_blob(&dek, "img1", img).unwrap();
        let raw_blob = std::fs::read(dir.join("blobs").join("img1.enc")).unwrap();
        assert!(!contains(&raw_blob, b"FAKE-IMAGE-BYTES"), "图片落盘必须是密文");
        assert_eq!(v.read_blob(&dek, "img1").unwrap(), img);

        std::fs::remove_dir_all(&dir).ok();
    }
}
