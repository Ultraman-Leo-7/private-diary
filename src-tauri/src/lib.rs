mod commands;
mod crypto;
mod sync;
mod vault;
mod webdav;

use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use tauri::Manager;
use vault::Vault;
use zeroize::Zeroizing;

/// 应用运行期状态：vault 路径 + 内存中的数据密钥 DEK。
/// DEK 仅存在于此处，永不持久化、永不返回前端；锁定/退出即清零。
pub struct AppState {
    pub vault: Vault,
    dek: Mutex<Option<Zeroizing<[u8; crypto::KEY_LEN]>>>,
    /// 同步进行中标志，防止并发重叠。
    pub syncing: AtomicBool,
}

impl AppState {
    /// 取出 DEK 的副本（已解锁时）；未解锁返回错误字符串。
    pub fn dek(&self) -> Result<[u8; crypto::KEY_LEN], String> {
        let guard = self.dek.lock().unwrap();
        match guard.as_ref() {
            Some(z) => Ok(**z),
            None => Err("已锁定，请先解锁".to_string()),
        }
    }

    pub fn set_dek(&self, dek: [u8; crypto::KEY_LEN]) {
        *self.dek.lock().unwrap() = Some(Zeroizing::new(dek));
    }

    pub fn clear(&self) {
        *self.dek.lock().unwrap() = None;
    }

    pub fn is_unlocked(&self) -> bool {
        self.dek.lock().unwrap().is_some()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let dir = app
                .path()
                .app_local_data_dir()
                .expect("无法获取应用数据目录")
                .join("vault");
            app.manage(AppState {
                vault: Vault::new(dir),
                dek: Mutex::new(None),
                syncing: AtomicBool::new(false),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_exists,
            commands::is_unlocked,
            commands::get_hint,
            commands::setup_password,
            commands::unlock,
            commands::lock,
            commands::change_password,
            commands::list_entries,
            commands::get_entry,
            commands::save_entry,
            commands::delete_entry,
            commands::add_image,
            commands::import_image_path,
            commands::get_image,
            commands::export_plaintext_backup,
            commands::webdav_get_config,
            commands::webdav_save_config,
            commands::webdav_test,
            commands::sync_now,
            commands::webdav_restore,
            commands::get_settings,
            commands::set_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
