//! 暴露给前端的 Tauri 命令。
//!
//! 安全约束：数据密钥 DEK 只存在于后端内存（AppState），永不返回前端；
//! 前端只发“请加/解密第 X 条”，由后端用内存中的 DEK 完成。

use crate::crypto::{self, KdfParams};
use crate::sync::{self, SyncReport};
use crate::vault::{self, Index, IndexEntry, Settings, WebDavConfig};
use crate::webdav::WebDav;
use crate::AppState;
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;
use uuid::Uuid;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 从 Markdown 提取标题：第一行非空文本（去掉前导 # 与空白），截断到 60 字符。
fn derive_title(md: &str) -> String {
    for line in md.lines() {
        let t = line.trim_start_matches('#').trim();
        if !t.is_empty() {
            return t.chars().take(60).collect();
        }
    }
    "(无标题)".to_string()
}

fn sanitize_ext(ext: &str) -> String {
    let e: String = ext.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if e.is_empty() {
        "png".to_string()
    } else {
        e.to_lowercase()
    }
}

fn mime_for(ext: &str) -> String {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// 把 "<id>.<ext>" 拆成 (id, ext)。
fn split_name(name: &str) -> (String, String) {
    match name.rsplit_once('.') {
        Some((id, ext)) => (id.to_string(), ext.to_lowercase()),
        None => (name.to_string(), "png".to_string()),
    }
}

// ───────────────────────── 密码 / 会话 ─────────────────────────

#[tauri::command]
pub fn vault_exists(state: State<AppState>) -> bool {
    state.vault.exists()
}

#[tauri::command]
pub fn is_unlocked(state: State<AppState>) -> bool {
    state.is_unlocked()
}

/// 读取密码提示（无需解锁，提示本身非机密）。
#[tauri::command]
pub fn get_hint(state: State<AppState>) -> Result<String, String> {
    if !state.vault.exists() {
        return Ok(String::new());
    }
    state.vault.read_meta().map(|m| m.hint).map_err(|e| e.to_string())
}

/// 首次设置密码：创建 vault、生成 salt/DEK、包裹 DEK、写 meta 与空索引，并直接解锁。
#[tauri::command]
pub fn setup_password(state: State<AppState>, password: String, hint: String) -> Result<(), String> {
    if state.vault.exists() {
        return Err("日记库已存在，无法重复初始化".to_string());
    }
    if password.is_empty() {
        return Err("密码不能为空".to_string());
    }
    (|| -> Result<()> {
        let params = KdfParams::default();
        let salt = crypto::random_salt();
        let kek = crypto::derive_key(password.as_bytes(), &salt, &params)?;
        let dek = crypto::random_dek();
        let wrapped = crypto::seal(&kek, &dek)?;
        let meta = vault::build_meta(hint, B64.encode(salt), params, wrapped);
        state.vault.write_meta(&meta)?;
        state.vault.write_index(&dek, &Index::default())?;
        state.set_dek(dek);
        Ok(())
    })()
    .map_err(|e| e.to_string())
}

/// 用密码解锁：派生 KEK，尝试解开 wrapped_dek。失败=密码错误。
#[tauri::command]
pub fn unlock(state: State<AppState>, password: String) -> Result<(), String> {
    (|| -> Result<()> {
        let meta = state.vault.read_meta()?;
        let salt = B64.decode(meta.kdf.salt.as_bytes())?;
        let kek = crypto::derive_key(password.as_bytes(), &salt, &meta.kdf.params)?;
        let dek_vec = crypto::unseal(&kek, &meta.wrapped_dek)?;
        if dek_vec.len() != crypto::KEY_LEN {
            return Err(anyhow!("DEK 长度异常"));
        }
        let mut dek = [0u8; crypto::KEY_LEN];
        dek.copy_from_slice(&dek_vec);
        state.set_dek(dek);
        Ok(())
    })()
    .map_err(|_| "密码错误".to_string())
}

/// 锁定：清除内存中的 DEK。
#[tauri::command]
pub fn lock(state: State<AppState>) {
    state.clear();
}

/// 改密码：用旧密码验证并取出 DEK，再用新密码 + 新 salt 重新包裹。日记内容不动。
#[tauri::command]
pub fn change_password(
    state: State<AppState>,
    old_password: String,
    new_password: String,
    new_hint: Option<String>,
) -> Result<(), String> {
    if new_password.is_empty() {
        return Err("新密码不能为空".to_string());
    }
    (|| -> Result<()> {
        let mut meta = state.vault.read_meta()?;
        let salt = B64.decode(meta.kdf.salt.as_bytes())?;
        let old_kek = crypto::derive_key(old_password.as_bytes(), &salt, &meta.kdf.params)?;
        let dek_vec = crypto::unseal(&old_kek, &meta.wrapped_dek)
            .map_err(|_| anyhow!("原密码错误"))?;
        let mut dek = [0u8; crypto::KEY_LEN];
        dek.copy_from_slice(&dek_vec);

        let params = KdfParams::default();
        let new_salt = crypto::random_salt();
        let new_kek = crypto::derive_key(new_password.as_bytes(), &new_salt, &params)?;
        let wrapped = crypto::seal(&new_kek, &dek)?;

        meta.kdf = vault::KdfMeta {
            algo: "argon2id".to_string(),
            params,
            salt: B64.encode(new_salt),
        };
        meta.wrapped_dek = wrapped;
        if let Some(h) = new_hint {
            meta.hint = h;
        }
        state.vault.write_meta(&meta)?;
        Ok(())
    })()
    .map_err(|e| e.to_string())
}

// ───────────────────────── 日记条目 ─────────────────────────

/// 返回全部条目（前端据此渲染日历高亮 + 当天列表）。
#[tauri::command]
pub fn list_entries(state: State<AppState>) -> Result<Vec<IndexEntry>, String> {
    let dek = state.dek()?;
    state
        .vault
        .read_index(&dek)
        .map(|i| i.entries)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_entry(state: State<AppState>, id: String) -> Result<String, String> {
    let dek = state.dek()?;
    state.vault.read_entry(&dek, &id).map_err(|e| e.to_string())
}

/// 保存一条日记。id 为空则新建并返回新 id；否则更新。
#[tauri::command]
pub fn save_entry(
    state: State<AppState>,
    id: Option<String>,
    date: String,
    content: String,
) -> Result<String, String> {
    let dek = state.dek()?;
    (|| -> Result<String> {
        let mut index = state.vault.read_index(&dek)?;
        let now = now_ms();
        let title = derive_title(&content);
        let entry_id = match id {
            Some(existing) if !existing.is_empty() => {
                if let Some(e) = index.entries.iter_mut().find(|e| e.id == existing) {
                    e.title = title;
                    e.date = date.clone();
                    e.updated_at = now;
                } else {
                    index.entries.push(IndexEntry {
                        id: existing.clone(),
                        date: date.clone(),
                        title,
                        created_at: now,
                        updated_at: now,
                    });
                }
                existing
            }
            _ => {
                let nid = Uuid::new_v4().to_string();
                index.entries.push(IndexEntry {
                    id: nid.clone(),
                    date: date.clone(),
                    title,
                    created_at: now,
                    updated_at: now,
                });
                nid
            }
        };
        state.vault.write_entry(&dek, &entry_id, &content)?;
        state.vault.write_index(&dek, &index)?;
        Ok(entry_id)
    })()
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_entry(state: State<AppState>, id: String) -> Result<(), String> {
    let dek = state.dek()?;
    (|| -> Result<()> {
        let mut index = state.vault.read_index(&dek)?;
        index.entries.retain(|e| e.id != id);
        state.vault.write_index(&dek, &index)?;
        state.vault.delete_entry_file(&id)?;
        Ok(())
    })()
    .map_err(|e| e.to_string())
}

// ───────────────────────── 图片 ─────────────────────────

/// 保存一张图片（来自剪贴板粘贴等，base64 传入），返回 "<id>.<ext>" 供 Markdown 引用。
#[tauri::command]
pub fn add_image(
    state: State<AppState>,
    data_base64: String,
    ext: String,
) -> Result<String, String> {
    let dek = state.dek()?;
    (|| -> Result<String> {
        let bytes = B64.decode(data_base64.as_bytes())?;
        let id = Uuid::new_v4().to_string();
        state.vault.write_blob(&dek, &id, &bytes)?;
        Ok(format!("{id}.{}", sanitize_ext(&ext)))
    })()
    .map_err(|e| e.to_string())
}

/// 从本地文件导入图片（后端直接读取并加密），返回 "<id>.<ext>"。
#[tauri::command]
pub fn import_image_path(state: State<AppState>, path: String) -> Result<String, String> {
    let dek = state.dek()?;
    (|| -> Result<String> {
        let bytes = std::fs::read(&path)?;
        let ext = std::path::Path::new(&path)
            .extension()
            .and_then(|s| s.to_str())
            .map(sanitize_ext)
            .unwrap_or_else(|| "png".to_string());
        let id = Uuid::new_v4().to_string();
        state.vault.write_blob(&dek, &id, &bytes)?;
        Ok(format!("{id}.{ext}"))
    })()
    .map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct ImageData {
    pub mime: String,
    pub data_base64: String,
}

/// 读取并解密一张图片，返回 base64（前端拼成 data URL 显示）。
#[tauri::command]
pub fn get_image(state: State<AppState>, name: String) -> Result<ImageData, String> {
    let dek = state.dek()?;
    (|| -> Result<ImageData> {
        let (id, ext) = split_name(&name);
        let bytes = state.vault.read_blob(&dek, &id)?;
        Ok(ImageData {
            mime: mime_for(&ext),
            data_base64: B64.encode(bytes),
        })
    })()
    .map_err(|e| e.to_string())
}

// ───────────────────────── 导出明文备份 ─────────────────────────

/// 把全部日记解密导出到 target_dir/diary-backup（YYYY-MM-DD-<id前6>.md + images/）。
/// 返回导出根目录路径。
#[tauri::command]
pub fn export_plaintext_backup(state: State<AppState>, target_dir: String) -> Result<String, String> {
    let dek = state.dek()?;
    (|| -> Result<String> {
        let re = regex::Regex::new(r"blob:([0-9a-fA-F\-]+)\.([A-Za-z0-9]+)")?;
        let index = state.vault.read_index(&dek)?;
        let root = std::path::Path::new(&target_dir).join("diary-backup");
        let img_dir = root.join("images");
        std::fs::create_dir_all(&img_dir)?;

        for e in &index.entries {
            let md = state.vault.read_entry(&dek, &e.id)?;
            // 导出每条日记引用到的图片，并把 blob: 引用改写为相对 images/ 路径
            for caps in re.captures_iter(&md) {
                let id = &caps[1];
                let ext = &caps[2];
                if let Ok(bytes) = state.vault.read_blob(&dek, id) {
                    std::fs::write(img_dir.join(format!("{id}.{ext}")), bytes)?;
                }
            }
            let rewritten = re.replace_all(&md, "images/$1.$2").to_string();
            let short: String = e.id.chars().take(6).collect();
            let fname = format!("{}-{}.md", e.date, short);
            std::fs::write(root.join(fname), rewritten)?;
        }
        Ok(root.to_string_lossy().to_string())
    })()
    .map_err(|e| e.to_string())
}

// ───────────────────────── 云同步（坚果云 WebDAV） ─────────────────────────

#[derive(Serialize)]
pub struct WebDavConfigPublic {
    pub url: String,
    pub username: String,
    pub remote_dir: String,
    pub has_password: bool,
    pub configured: bool,
}

/// 读取云同步配置（不返回密码本身，只返回是否已设置）。
#[tauri::command]
pub fn webdav_get_config(state: State<AppState>) -> Result<WebDavConfigPublic, String> {
    let dek = state.dek()?;
    match state.vault.read_webdav_config(&dek).map_err(|e| e.to_string())? {
        Some(c) => Ok(WebDavConfigPublic {
            url: c.url,
            username: c.username,
            remote_dir: c.remote_dir,
            has_password: !c.password.is_empty(),
            configured: true,
        }),
        None => Ok(WebDavConfigPublic {
            url: "https://dav.jianguoyun.com/dav/".to_string(),
            username: String::new(),
            remote_dir: "private-diary".to_string(),
            has_password: false,
            configured: false,
        }),
    }
}

/// 保存云同步配置（加密）。password 为空且已有配置时，沿用旧密码。
#[tauri::command]
pub fn webdav_save_config(
    state: State<AppState>,
    url: String,
    username: String,
    password: Option<String>,
    remote_dir: String,
) -> Result<(), String> {
    let dek = state.dek()?;
    (|| -> Result<()> {
        let existing = state.vault.read_webdav_config(&dek)?;
        let pass = match password {
            Some(p) if !p.is_empty() => p,
            _ => existing.map(|c| c.password).unwrap_or_default(),
        };
        let cfg = WebDavConfig {
            url: url.trim().to_string(),
            username: username.trim().to_string(),
            password: pass,
            remote_dir: remote_dir.trim().to_string(),
        };
        state.vault.write_webdav_config(&dek, &cfg)?;
        Ok(())
    })()
    .map_err(|e| e.to_string())
}

/// 测试 WebDAV 连接。
#[tauri::command]
pub async fn webdav_test(state: State<'_, AppState>) -> Result<String, String> {
    let dek = state.dek()?;
    let cfg = state
        .vault
        .read_webdav_config(&dek)
        .map_err(|e| e.to_string())?
        .ok_or("尚未保存云同步配置")?;
    let dav = WebDav::new(&cfg.url, &cfg.username, &cfg.password, &cfg.remote_dir)
        .map_err(|e| e.to_string())?;
    dav.test().await.map_err(|e| e.to_string())?;
    Ok("连接成功".to_string())
}

/// 立即执行一次双向同步（带并发守卫，避免重叠）。
#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>) -> Result<SyncReport, String> {
    use std::sync::atomic::Ordering;
    if state
        .syncing
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("正在同步中，请稍候".to_string());
    }
    let result = async {
        let dek = state.dek()?;
        let cfg = state
            .vault
            .read_webdav_config(&dek)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "尚未配置云同步".to_string())?;
        let dav = WebDav::new(&cfg.url, &cfg.username, &cfg.password, &cfg.remote_dir)
            .map_err(|e| e.to_string())?;
        sync::run_sync(&state.vault, &dav)
            .await
            .map_err(|e| e.to_string())
    }
    .await;
    state.syncing.store(false, Ordering::SeqCst);
    result
}

// ───────────────────────── 应用偏好 ─────────────────────────

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<Settings, String> {
    state.vault.read_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_settings(state: State<AppState>, settings: Settings) -> Result<(), String> {
    state.vault.write_settings(&settings).map_err(|e| e.to_string())
}

/// 从坚果云恢复：下载全部密文到本地，并用日记密码解锁。用于换设备/灾难恢复。
/// 注意：会用云端数据覆盖本地同名文件。
#[tauri::command]
pub async fn webdav_restore(
    state: State<'_, AppState>,
    url: String,
    username: String,
    password: String,
    remote_dir: String,
    diary_password: String,
) -> Result<(), String> {
    async move {
        let dav = WebDav::new(&url, &username, &password, &remote_dir)?;

        // 列出远端全部文件
        let mut remote = std::collections::BTreeMap::new();
        for dir in ["", "entries", "blobs"] {
            for e in dav.propfind(dir).await? {
                if !e.is_dir {
                    remote.insert(e.rel, e.etag);
                }
            }
        }
        if !remote.contains_key("meta.json") {
            return Err(anyhow!("云端没有可恢复的日记（缺少 meta.json）"));
        }

        // 下载到本地 vault
        for rel in remote.keys() {
            let bytes = dav.get(rel).await?;
            let path = state.vault.abs(rel);
            if let Some(p) = path.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::write(path, bytes)?;
        }

        // 用日记密码解锁，得到 DEK
        let meta = state.vault.read_meta()?;
        let salt = B64.decode(meta.kdf.salt.as_bytes())?;
        let kek = crypto::derive_key(diary_password.as_bytes(), &salt, &meta.kdf.params)?;
        let dek_vec = crypto::unseal(&kek, &meta.wrapped_dek).map_err(|_| anyhow!("日记密码错误"))?;
        let mut dek = [0u8; crypto::KEY_LEN];
        dek.copy_from_slice(&dek_vec);

        // 持久化云配置（加密），方便后续同步
        let cfg = WebDavConfig {
            url: url.trim().to_string(),
            username: username.trim().to_string(),
            password,
            remote_dir: remote_dir.trim().to_string(),
        };
        state.vault.write_webdav_config(&dek, &cfg)?;
        state.set_dek(dek);
        Ok::<(), anyhow::Error>(())
    }
    .await
    .map_err(|e| e.to_string())
}
