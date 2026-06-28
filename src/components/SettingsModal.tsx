import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import type { Settings } from "../types";
import { checkForUpdate } from "../updater";

export default function SettingsModal({
  onClose,
  onSynced,
  onSettingsChanged,
}: {
  onClose: () => void;
  onSynced: () => void;
  onSettingsChanged: () => void;
}) {
  // 改密码
  const [oldPw, setOldPw] = useState("");
  const [newPw, setNewPw] = useState("");
  const [newPw2, setNewPw2] = useState("");
  const [newHint, setNewHint] = useState("");
  const [msg, setMsg] = useState("");

  // 云同步配置
  const [url, setUrl] = useState("https://dav.jianguoyun.com/dav/");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [remoteDir, setRemoteDir] = useState("private-diary");
  const [hasPassword, setHasPassword] = useState(false);
  const [cloudMsg, setCloudMsg] = useState("");
  const [busy, setBusy] = useState(false);

  // 应用偏好（自动同步开关等）
  const [settings, setSettings] = useState<Settings | null>(null);
  const [updateMsg, setUpdateMsg] = useState("");

  async function checkUpdate() {
    setUpdateMsg("检查中…");
    try {
      setUpdateMsg((await checkForUpdate(true)) || "已是最新版本 ✓");
    } catch (e) {
      setUpdateMsg("检查失败：" + e);
    }
  }

  useEffect(() => {
    api
      .webdavGetConfig()
      .then((c) => {
        setUrl(c.url);
        setUsername(c.username);
        setRemoteDir(c.remote_dir);
        setHasPassword(c.has_password);
      })
      .catch(() => {});
    api
      .getSettings()
      .then(setSettings)
      .catch(() => {});
  }, []);

  async function updateSetting(patch: Partial<Settings>) {
    if (!settings) return;
    const next = { ...settings, ...patch };
    setSettings(next);
    try {
      await api.setSettings(next);
      onSettingsChanged();
    } catch (e) {
      setCloudMsg("保存设置失败：" + e);
    }
  }

  async function changePw() {
    setMsg("");
    if (newPw.length < 4) {
      setMsg("新密码至少 4 位");
      return;
    }
    if (newPw !== newPw2) {
      setMsg("两次新密码不一致");
      return;
    }
    try {
      await api.changePassword(oldPw, newPw, newHint.trim() ? newHint : null);
      setMsg("密码已修改 ✓");
      setOldPw("");
      setNewPw("");
      setNewPw2("");
    } catch (e) {
      setMsg(String(e));
    }
  }

  async function exportBackup() {
    setMsg("");
    try {
      const dir = await open({ directory: true, multiple: false, title: "选择导出位置" });
      if (typeof dir === "string") {
        const out = await api.exportBackup(dir);
        setMsg("已导出明文备份到：" + out);
      }
    } catch (e) {
      setMsg("导出失败：" + e);
    }
  }

  async function saveConfig() {
    setCloudMsg("");
    setBusy(true);
    try {
      await api.webdavSaveConfig(url, username, password ? password : null, remoteDir);
      setPassword("");
      const c = await api.webdavGetConfig();
      setHasPassword(c.has_password);
      setCloudMsg("配置已保存 ✓");
    } catch (e) {
      setCloudMsg(String(e));
    }
    setBusy(false);
  }

  async function testConn() {
    setCloudMsg("");
    setBusy(true);
    try {
      const r = await api.webdavTest();
      setCloudMsg("✓ " + r);
    } catch (e) {
      setCloudMsg("连接失败：" + e);
    }
    setBusy(false);
  }

  async function doSync() {
    setCloudMsg("");
    setBusy(true);
    try {
      const r = await api.syncNow();
      const parts = [
        `上传 ${r.uploaded}`,
        `下载 ${r.downloaded}`,
        `删除云端 ${r.deleted_remote}`,
        `冲突 ${r.conflicts}`,
      ];
      setCloudMsg("同步完成：" + parts.join("，") + (r.messages.length ? "\n" + r.messages.join("\n") : ""));
      onSynced();
    } catch (e) {
      setCloudMsg("同步失败：" + e);
    }
    setBusy(false);
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>设置</h2>
          <button className="icon-btn" onClick={onClose}>
            ✕
          </button>
        </div>

        <section className="settings-section">
          <h3>☁ 云同步（坚果云 WebDAV）</h3>
          <p className="muted">
            上传的就是本地的<b>密文</b>，坚果云上看到的是乱码。请在坚果云「账户信息 → 安全选项 →
            第三方应用管理」里生成<b>应用密码</b>填到下方。
          </p>
          <input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="WebDAV 地址" />
          <input
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            placeholder="坚果云账号（邮箱）"
          />
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder={hasPassword ? "应用密码（已保存，留空则不改）" : "坚果云应用密码"}
          />
          <input
            value={remoteDir}
            onChange={(e) => setRemoteDir(e.target.value)}
            placeholder="远程文件夹"
          />
          <div className="btn-row">
            <button onClick={saveConfig} disabled={busy}>
              保存配置
            </button>
            <button onClick={testConn} disabled={busy}>
              测试连接
            </button>
            <button className="primary" onClick={doSync} disabled={busy}>
              {busy ? "处理中…" : "立即同步"}
            </button>
          </div>
          {cloudMsg && <div className="modal-msg cloud-msg">{cloudMsg}</div>}
        </section>

        <section className="settings-section">
          <h3>自动同步</h3>
          {!settings ? (
            <p className="muted">加载中…</p>
          ) : (
            <>
              <label className="check-row">
                <input
                  type="checkbox"
                  checked={settings.sync_pull_on_unlock}
                  onChange={(e) => updateSetting({ sync_pull_on_unlock: e.target.checked })}
                />
                解锁后自动从云端拉取
              </label>
              <label className="check-row">
                <input
                  type="checkbox"
                  checked={settings.sync_push_on_exit}
                  onChange={(e) => updateSetting({ sync_push_on_exit: e.target.checked })}
                />
                退出应用时自动推送到云端
              </label>
              <label className="check-row">
                <input
                  type="checkbox"
                  checked={settings.sync_on_save}
                  onChange={(e) => updateSetting({ sync_on_save: e.target.checked })}
                />
                每次保存后自动同步（更费坚果云额度）
              </label>
              <label className="check-row">
                定时同步：
                <select
                  value={settings.sync_interval_min}
                  onChange={(e) => updateSetting({ sync_interval_min: Number(e.target.value) })}
                >
                  <option value={0}>关闭</option>
                  <option value={5}>每 5 分钟</option>
                  <option value={10}>每 10 分钟</option>
                  <option value={30}>每 30 分钟</option>
                </select>
              </label>
              <p className="muted">默认只「解锁时拉、退出时推」，最省坚果云请求。</p>
            </>
          )}
        </section>

        <section className="settings-section">
          <h3>修改密码</h3>
          <input
            type="password"
            placeholder="当前密码"
            value={oldPw}
            onChange={(e) => setOldPw(e.target.value)}
          />
          <input
            type="password"
            placeholder="新密码"
            value={newPw}
            onChange={(e) => setNewPw(e.target.value)}
          />
          <input
            type="password"
            placeholder="确认新密码"
            value={newPw2}
            onChange={(e) => setNewPw2(e.target.value)}
          />
          <input
            type="text"
            placeholder="新密码提示（可选，留空则不修改提示）"
            value={newHint}
            onChange={(e) => setNewHint(e.target.value)}
          />
          <button className="primary" onClick={changePw}>
            修改密码
          </button>
        </section>

        <section className="settings-section">
          <h3>导出明文备份</h3>
          <p className="muted">
            把全部日记解密导出为 Markdown 文件 + 图片文件夹，用于长期保存或迁移。
            导出的是<b>明文</b>，请妥善保管。
          </p>
          <button onClick={exportBackup}>选择文件夹并导出</button>
        </section>

        <section className="settings-section">
          <h3>更新</h3>
          {settings && (
            <label className="check-row">
              <input
                type="checkbox"
                checked={settings.auto_update_check}
                onChange={(e) => updateSetting({ auto_update_check: e.target.checked })}
              />
              启动时自动检查更新（默认关闭）
            </label>
          )}
          <div className="btn-row">
            <button onClick={checkUpdate}>立即检查更新</button>
          </div>
          {updateMsg && <div className="modal-msg cloud-msg">{updateMsg}</div>}
        </section>

        {msg && <div className="modal-msg">{msg}</div>}
      </div>
    </div>
  );
}
