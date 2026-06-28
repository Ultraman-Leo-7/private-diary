import { useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "../api";
import type { IndexEntry, Settings } from "../types";
import Calendar, { ymd } from "./Calendar";
import Editor from "./Editor";
import SettingsModal from "./SettingsModal";

type SyncKind = "idle" | "syncing" | "ok" | "error";

export default function MainScreen({ onLock }: { onLock: () => void }) {
  const [entries, setEntries] = useState<IndexEntry[]>([]);
  const [selectedDate, setSelectedDate] = useState(() => ymd(new Date()));
  const [activeId, setActiveId] = useState<string | null>(null);
  const [isNew, setIsNew] = useState(false);
  const [showSettings, setShowSettings] = useState(false);

  const [settings, setSettings] = useState<Settings | null>(null);
  const [configured, setConfigured] = useState(false);
  const [sync, setSync] = useState<{ kind: SyncKind; text: string }>({ kind: "idle", text: "" });

  // 用 ref 保存最新值，供一次性注册的窗口关闭回调读取
  const settingsRef = useRef<Settings | null>(null);
  settingsRef.current = settings;
  const configuredRef = useRef(false);
  configuredRef.current = configured;
  const syncingRef = useRef(false);
  const runSyncRef = useRef<((reason?: string) => Promise<void>) | undefined>(undefined);
  const saveTimer = useRef<number | undefined>(undefined);
  const didInitialPull = useRef(false);

  async function refresh() {
    try {
      setEntries(await api.listEntries());
    } catch {
      /* 可能已锁定 */
    }
  }

  async function loadPrefs() {
    try {
      const [s, c] = await Promise.all([api.getSettings(), api.webdavGetConfig()]);
      setSettings(s);
      setConfigured(c.configured && c.has_password);
    } catch {
      /* ignore */
    }
  }

  async function runSync(_reason?: string) {
    if (!configuredRef.current || syncingRef.current) return;
    syncingRef.current = true;
    setSync({ kind: "syncing", text: "同步中…" });
    try {
      const r = await api.syncNow();
      const now = new Date();
      const hh = String(now.getHours()).padStart(2, "0");
      const mm = String(now.getMinutes()).padStart(2, "0");
      setSync({ kind: "ok", text: `已同步 ${hh}:${mm}` });
      if (r.downloaded > 0 || r.deleted_remote > 0 || r.conflicts > 0) {
        await refresh();
      }
    } catch {
      setSync({ kind: "error", text: "同步失败" });
    } finally {
      syncingRef.current = false;
    }
  }
  runSyncRef.current = runSync;

  // 初始：加载日记 + 偏好/云配置
  useEffect(() => {
    refresh();
    loadPrefs();
  }, []);

  // 解锁后自动拉取（仅一次）
  useEffect(() => {
    if (!settings || didInitialPull.current) return;
    didInitialPull.current = true;
    if (settings.sync_pull_on_unlock && configured) {
      runSyncRef.current?.("pull");
    }
  }, [settings, configured]);

  // 定时同步（可选）
  useEffect(() => {
    if (!settings || !configured) return;
    const min = settings.sync_interval_min;
    if (!min || min <= 0) return;
    const id = window.setInterval(() => runSyncRef.current?.("interval"), min * 60000);
    return () => window.clearInterval(id);
  }, [settings, configured]);

  // 关窗前自动推送（可选）
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    const w = getCurrentWindow();
    w.onCloseRequested(async (event) => {
      const s = settingsRef.current;
      if (s?.sync_push_on_exit && configuredRef.current && !syncingRef.current) {
        event.preventDefault();
        try {
          await runSyncRef.current?.("push");
        } catch {
          /* 即使同步失败也要让窗口关闭 */
        }
        await w.destroy();
      }
    }).then((un) => {
      unlisten = un;
    });
    return () => unlisten?.();
  }, []);

  const datesWithEntries = useMemo(() => new Set(entries.map((e) => e.date)), [entries]);
  const dayEntries = useMemo(
    () =>
      entries
        .filter((e) => e.date === selectedDate)
        .sort((a, b) => b.updated_at - a.updated_at),
    [entries, selectedDate],
  );

  function selectDate(d: string) {
    setSelectedDate(d);
    setActiveId(null);
    setIsNew(false);
  }
  function newEntry() {
    setActiveId(null);
    setIsNew(true);
  }
  function openEntry(id: string) {
    setActiveId(id);
    setIsNew(false);
  }

  async function onSaved(id: string) {
    await refresh();
    setActiveId(id);
    setIsNew(false);
    if (settings?.sync_on_save && configured) {
      if (saveTimer.current) window.clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(() => runSyncRef.current?.("save"), 5000);
    }
  }
  async function onDeleted() {
    await refresh();
    setActiveId(null);
    setIsNew(false);
    if (settings?.sync_on_save && configured) {
      runSyncRef.current?.("save");
    }
  }

  const showEditor = activeId !== null || isNew;

  return (
    <div className="main">
      <aside className="sidebar">
        <div className="sidebar-header">
          <span className="app-title">📔 私密日记</span>
          <div className="spacer" />
          <button className="icon-btn" title="设置" onClick={() => setShowSettings(true)}>
            ⚙
          </button>
          <button className="icon-btn" title="锁定退出" onClick={onLock}>
            🔒
          </button>
        </div>

        {configured && (
          <div
            className={"sync-bar sync-" + sync.kind}
            title="点击立即同步"
            onClick={() => runSyncRef.current?.("manual")}
          >
            <span className="sync-dot" />
            {sync.text || "点此同步"}
          </div>
        )}

        <Calendar
          datesWithEntries={datesWithEntries}
          selectedDate={selectedDate}
          onSelect={selectDate}
        />

        <div className="day-list">
          <div className="day-list-header">
            <span>{selectedDate}</span>
            <button className="small primary" onClick={newEntry}>
              ＋ 新建
            </button>
          </div>
          {dayEntries.length === 0 && <div className="muted empty-hint">这一天还没有日记</div>}
          {dayEntries.map((e) => (
            <button
              key={e.id}
              className={"entry-item" + (e.id === activeId ? " active" : "")}
              onClick={() => openEntry(e.id)}
            >
              {e.title || "(无标题)"}
            </button>
          ))}
        </div>
      </aside>

      <main className="content">
        {showEditor ? (
          <Editor
            key={activeId ?? "new"}
            entryId={activeId}
            date={selectedDate}
            onSaved={onSaved}
            onDeleted={onDeleted}
          />
        ) : (
          <div className="placeholder">
            <p>
              选择左侧某篇日记查看，或点「＋ 新建」开始写 <b>{selectedDate}</b> 的日记。
            </p>
          </div>
        )}
      </main>

      {showSettings && (
        <SettingsModal
          onClose={() => setShowSettings(false)}
          onSynced={refresh}
          onSettingsChanged={loadPrefs}
        />
      )}
    </div>
  );
}
