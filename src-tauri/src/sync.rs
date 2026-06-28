//! 双向同步引擎。
//!
//! 思路：为每个文件记录上次同步时的「本地内容哈希 + 远端 etag」作为基线。
//! 本次同步时分别判断本地/远端是否相对基线发生变化，据此上传/下载/传播删除。
//! 双方都改了的冲突文件：先把本地另存为 `.conflict-<时间>` 副本，再采用云端版本——**绝不丢数据**。
//!
//! 同步集合 = meta.json + index.enc + entries/*.enc + blobs/*.enc，
//! 刻意排除 webdav.enc（含云端凭据，不该上云）与 sync_state.json（本地基线）。

use crate::vault::Vault;
use crate::webdav::WebDav;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone)]
struct FileState {
    local_hash: String,
    remote_etag: String,
}

#[derive(Serialize, Deserialize, Default)]
struct SyncState {
    files: BTreeMap<String, FileState>,
}

#[derive(Serialize, Default, Clone)]
pub struct SyncReport {
    pub uploaded: u32,
    pub downloaded: u32,
    pub deleted_remote: u32,
    pub conflicts: u32,
    pub messages: Vec<String>,
}

fn state_path(vault: &Vault) -> PathBuf {
    vault.dir.join("sync_state.json")
}

fn read_state(vault: &Vault) -> SyncState {
    match fs::read(state_path(vault)) {
        Ok(b) => serde_json::from_slice(&b).unwrap_or_default(),
        Err(_) => SyncState::default(),
    }
}

fn write_state(vault: &Vault, st: &SyncState) -> Result<()> {
    fs::create_dir_all(&vault.dir)?;
    fs::write(state_path(vault), serde_json::to_vec_pretty(st)?)?;
    Ok(())
}

fn hash_bytes(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    let digest = h.finalize();
    let mut s = String::with_capacity(digest.len() * 2);
    for x in digest {
        s.push_str(&format!("{x:02x}"));
    }
    s
}

/// 列出远端所有文件（rel -> etag）。
async fn list_remote(dav: &WebDav) -> Result<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    for dir in ["", "entries", "blobs"] {
        for e in dav.propfind(dir).await? {
            if !e.is_dir {
                map.insert(e.rel, e.etag);
            }
        }
    }
    Ok(map)
}

fn write_local(vault: &Vault, rel: &str, bytes: &[u8]) -> Result<()> {
    let path = vault.abs(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}

fn save_conflict_copy(vault: &Vault, rel: &str) -> Result<()> {
    let path = vault.abs(rel);
    if path.exists() {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut conflict = path.clone().into_os_string();
        conflict.push(format!(".conflict-{ts}"));
        fs::copy(&path, conflict)?;
    }
    Ok(())
}

pub async fn run_sync(vault: &Vault, dav: &WebDav) -> Result<SyncReport> {
    let mut report = SyncReport::default();

    // 确保远端目录存在
    dav.mkcol("").await.ok();
    dav.mkcol("entries").await.ok();
    dav.mkcol("blobs").await.ok();

    let baseline = read_state(vault);
    let remote_before = list_remote(dav).await?;
    let local_files: BTreeSet<String> = vault.local_sync_files().into_iter().collect();

    // 预先算好本地哈希
    let mut local_hashes: BTreeMap<String, String> = BTreeMap::new();
    for rel in &local_files {
        if let Ok(b) = fs::read(vault.abs(rel)) {
            local_hashes.insert(rel.clone(), hash_bytes(&b));
        }
    }

    let mut all: BTreeSet<String> = BTreeSet::new();
    all.extend(local_files.iter().cloned());
    all.extend(remote_before.keys().cloned());
    all.extend(baseline.files.keys().cloned());

    for rel in &all {
        let l = local_files.contains(rel);
        let r = remote_before.contains_key(rel);
        let base = baseline.files.get(rel);
        let lhash = local_hashes.get(rel);
        let retag = remote_before.get(rel);

        let local_changed = if !l {
            base.is_some()
        } else {
            base.map_or(true, |b| Some(&b.local_hash) != lhash)
        };
        let remote_changed = if !r {
            base.is_some()
        } else {
            base.map_or(true, |b| Some(&b.remote_etag) != retag)
        };

        match (l, r) {
            (true, true) => {
                if !local_changed && !remote_changed {
                    // 一致，跳过
                } else if local_changed && !remote_changed {
                    let bytes = fs::read(vault.abs(rel))?;
                    dav.put(rel, bytes).await?;
                    report.uploaded += 1;
                } else if !local_changed && remote_changed {
                    let bytes = dav.get(rel).await?;
                    write_local(vault, rel, &bytes)?;
                    report.downloaded += 1;
                } else {
                    // 双方都改：下载远端比对，内容相同则视为收敛
                    let remote_bytes = dav.get(rel).await?;
                    let same = hash_bytes(&remote_bytes) == lhash.cloned().unwrap_or_default();
                    if !same {
                        save_conflict_copy(vault, rel)?;
                        write_local(vault, rel, &remote_bytes)?;
                        report.conflicts += 1;
                        report
                            .messages
                            .push(format!("冲突：{rel} 已保留本地副本(.conflict)，采用云端版本"));
                    }
                }
            }
            (true, false) => {
                // 本地有、远端无：上传（新文件，或云端被删但本地仍在——优先保数据）
                let bytes = fs::read(vault.abs(rel))?;
                dav.put(rel, bytes).await?;
                report.uploaded += 1;
            }
            (false, true) => {
                if base.is_some() && !remote_changed {
                    // 本地删除、远端未变 → 把删除传播到云端
                    dav.delete(rel).await?;
                    report.deleted_remote += 1;
                } else {
                    // 远端新增，或远端有改动 → 下载到本地
                    let bytes = dav.get(rel).await?;
                    write_local(vault, rel, &bytes)?;
                    report.downloaded += 1;
                }
            }
            (false, false) => {}
        }
    }

    // 优化：本次没有任何上传/下载/删除时，基线仍然有效，直接返回，省去再次列目录的请求。
    let ops = report.uploaded + report.downloaded + report.deleted_remote + report.conflicts;
    if ops == 0 {
        return Ok(report);
    }

    // 用同步后的真实状态重建基线，避免下次产生虚假“变化”
    let remote_after = list_remote(dav).await?;
    let local_after: BTreeSet<String> = vault.local_sync_files().into_iter().collect();
    let mut new_state = SyncState::default();
    for rel in &local_after {
        let lh = match fs::read(vault.abs(rel)) {
            Ok(b) => hash_bytes(&b),
            Err(_) => continue,
        };
        let et = remote_after.get(rel).cloned().unwrap_or_default();
        new_state.files.insert(
            rel.clone(),
            FileState {
                local_hash: lh,
                remote_etag: et,
            },
        );
    }
    write_state(vault, &new_state)?;

    Ok(report)
}
