// 与 Rust 后端约定的数据结构（字段为 snake_case，与 serde 序列化一致）。

export interface IndexEntry {
  id: string;
  date: string; // YYYY-MM-DD
  title: string;
  created_at: number;
  updated_at: number;
}

export interface ImageData {
  mime: string;
  data_base64: string;
}

export type Phase = "loading" | "setup" | "locked" | "unlocked";

export interface WebDavConfigPublic {
  url: string;
  username: string;
  remote_dir: string;
  has_password: boolean;
  configured: boolean;
}

export interface SyncReport {
  uploaded: number;
  downloaded: number;
  deleted_remote: number;
  conflicts: number;
  messages: string[];
}

export interface Settings {
  sync_pull_on_unlock: boolean;
  sync_push_on_exit: boolean;
  sync_interval_min: number;
  sync_on_save: boolean;
  auto_update_check: boolean;
}
