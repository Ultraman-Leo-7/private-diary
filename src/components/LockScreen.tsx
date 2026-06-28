import { useState } from "react";
import { api } from "../api";
import RestoreForm from "./RestoreForm";

export default function LockScreen({ onUnlock }: { onUnlock: () => void }) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [hint, setHint] = useState("");
  const [showHint, setShowHint] = useState(false);
  const [busy, setBusy] = useState(false);
  const [restoring, setRestoring] = useState(false);

  if (restoring) {
    return (
      <div className="center-screen">
        <RestoreForm onRestored={onUnlock} onCancel={() => setRestoring(false)} />
      </div>
    );
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    setBusy(true);
    try {
      await api.unlock(password);
      onUnlock();
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  }

  async function revealHint() {
    try {
      const h = await api.getHint();
      setHint(h || "（没有设置提示）");
    } catch {
      setHint("（没有设置提示）");
    }
    setShowHint(true);
  }

  return (
    <div className="center-screen">
      <form className="card auth-card" onSubmit={submit}>
        <h1>🔓 输入密码</h1>
        <p className="muted">解锁你的日记。</p>
        <input
          type="password"
          value={password}
          autoFocus
          onChange={(e) => setPassword(e.target.value)}
          placeholder="密码"
        />
        {error && <div className="error">{error}</div>}
        <button type="submit" disabled={busy}>
          {busy ? "验证中…" : "解锁"}
        </button>
        <button type="button" className="link-btn" onClick={revealHint}>
          忘记密码？看提示
        </button>
        {showHint && <div className="hint-box">提示：{hint}</div>}
        <button type="button" className="link-btn" onClick={() => setRestoring(true)}>
          换了设备？从坚果云恢复
        </button>
      </form>
    </div>
  );
}
