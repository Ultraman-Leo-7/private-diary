import { useState } from "react";
import { api } from "../api";
import RestoreForm from "./RestoreForm";

export default function SetupScreen({ onDone }: { onDone: () => void }) {
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [hint, setHint] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [restoring, setRestoring] = useState(false);

  if (restoring) {
    return (
      <div className="center-screen">
        <RestoreForm onRestored={onDone} onCancel={() => setRestoring(false)} />
      </div>
    );
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    if (password.length < 4) {
      setError("密码至少 4 位");
      return;
    }
    if (password !== confirm) {
      setError("两次输入的密码不一致");
      return;
    }
    setBusy(true);
    try {
      await api.setupPassword(password, hint);
      onDone();
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  }

  return (
    <div className="center-screen">
      <form className="card auth-card" onSubmit={submit}>
        <h1>🔒 创建你的日记</h1>
        <p className="muted">
          设置一个密码来保护日记。<b>密码无法找回</b>，请牢记——只要知道它就一定能解开你的日记。
        </p>
        <label>密码</label>
        <input
          type="password"
          value={password}
          autoFocus
          onChange={(e) => setPassword(e.target.value)}
          placeholder="设置密码"
        />
        <label>确认密码</label>
        <input
          type="password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          placeholder="再次输入"
        />
        <label>密码提示（可选）</label>
        <input
          type="text"
          value={hint}
          onChange={(e) => setHint(e.target.value)}
          placeholder="忘记时帮你回忆，但别直接写出密码"
        />
        {error && <div className="error">{error}</div>}
        <button type="submit" disabled={busy}>
          {busy ? "正在创建…" : "创建并进入"}
        </button>
        <button type="button" className="link-btn" onClick={() => setRestoring(true)}>
          已有云端备份？从坚果云恢复
        </button>
      </form>
    </div>
  );
}
