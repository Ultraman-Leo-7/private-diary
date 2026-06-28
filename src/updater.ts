import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/**
 * 检查更新。notifyIfNone=true 时（手动检查）即使没有更新也返回提示文字。
 * 默认（自动检查）静默：没更新就返回空字符串。
 */
export async function checkForUpdate(notifyIfNone: boolean): Promise<string> {
  const update = await check();
  if (!update) {
    return notifyIfNone ? "已是最新版本 ✓" : "";
  }
  const ok = window.confirm(
    `发现新版本 ${update.version}，是否现在下载并重启更新？\n\n${update.body ?? ""}`,
  );
  if (!ok) return "已取消更新";
  await update.downloadAndInstall();
  await relaunch();
  return "更新完成，正在重启…";
}
