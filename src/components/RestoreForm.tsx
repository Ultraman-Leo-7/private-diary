import { useState } from "react";
import { api } from "../api";

export default function RestoreForm({
  onRestored,
  onCancel,
}: {
  onRestored: () => void;
  onCancel: () => void;
}) {
  const [url, setUrl] = useState("https://dav.jianguoyun.com/dav/");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [remoteDir, setRemoteDir] = useState("private-diary");
  const [diaryPassword, setDiaryPassword] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    setBusy(true);
    try {
      await api.webdavRestore(url, username, password, remoteDir, diaryPassword);
      onRestored();
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  }

  return (
    <form className="card auth-card" onSubmit={submit}>
      <h1>☁ 从坚果云恢复</h1>
      <p className="muted">
        在新设备或重装后，用坚果云凭据 + 日记密码拉回全部日记。
        <b>会用云端数据覆盖本地同名文件。</b>
      </p>
      <label>WebDAV 地址</label>
      <input value={url} onChange={(e) => setUrl(e.target.value)} />
      <label>坚果云账号（邮箱）</label>
      <input value={username} onChange={(e) => setUsername(e.target.value)} autoFocus />
      <label>坚果云「应用密码」</label>
      <input
        type="password"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        placeholder="在坚果云「安全选项」里生成"
      />
      <label>远程文件夹</label>
      <input value={remoteDir} onChange={(e) => setRemoteDir(e.target.value)} />
      <label>日记密码</label>
      <input
        type="password"
        value={diaryPassword}
        onChange={(e) => setDiaryPassword(e.target.value)}
        placeholder="解密用，与当初设置的一致"
      />
      {error && <div className="error">{error}</div>}
      <button type="submit" className="primary" disabled={busy}>
        {busy ? "正在恢复…" : "拉取并解锁"}
      </button>
      <button type="button" className="link-btn" onClick={onCancel}>
        返回
      </button>
    </form>
  );
}
