// 对 Tauri 命令的类型化封装。所有加解密都在后端完成，前端只发请求。
import { invoke } from "@tauri-apps/api/core";
import type {
  IndexEntry,
  ImageData,
  WebDavConfigPublic,
  SyncReport,
  Settings,
} from "./types";

export const api = {
  vaultExists: () => invoke<boolean>("vault_exists"),
  isUnlocked: () => invoke<boolean>("is_unlocked"),
  getHint: () => invoke<string>("get_hint"),

  setupPassword: (password: string, hint: string) =>
    invoke<void>("setup_password", { password, hint }),
  unlock: (password: string) => invoke<void>("unlock", { password }),
  lock: () => invoke<void>("lock"),
  changePassword: (oldPassword: string, newPassword: string, newHint: string | null) =>
    invoke<void>("change_password", { oldPassword, newPassword, newHint }),

  listEntries: () => invoke<IndexEntry[]>("list_entries"),
  getEntry: (id: string) => invoke<string>("get_entry", { id }),
  saveEntry: (id: string | null, date: string, content: string) =>
    invoke<string>("save_entry", { id, date, content }),
  deleteEntry: (id: string) => invoke<void>("delete_entry", { id }),

  addImage: (dataBase64: string, ext: string) =>
    invoke<string>("add_image", { dataBase64, ext }),
  importImagePath: (path: string) => invoke<string>("import_image_path", { path }),
  getImage: (name: string) => invoke<ImageData>("get_image", { name }),

  exportBackup: (targetDir: string) =>
    invoke<string>("export_plaintext_backup", { targetDir }),

  webdavGetConfig: () => invoke<WebDavConfigPublic>("webdav_get_config"),
  webdavSaveConfig: (
    url: string,
    username: string,
    password: string | null,
    remoteDir: string,
  ) => invoke<void>("webdav_save_config", { url, username, password, remoteDir }),
  webdavTest: () => invoke<string>("webdav_test"),
  syncNow: () => invoke<SyncReport>("sync_now"),
  webdavRestore: (
    url: string,
    username: string,
    password: string,
    remoteDir: string,
    diaryPassword: string,
  ) => invoke<void>("webdav_restore", { url, username, password, remoteDir, diaryPassword }),

  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<void>("set_settings", { settings }),
};
