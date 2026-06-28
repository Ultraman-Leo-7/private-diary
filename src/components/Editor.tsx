import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import MarkdownPreview from "./MarkdownPreview";

function arrayBufferToBase64(buf: ArrayBuffer): string {
  const bytes = new Uint8Array(buf);
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

export default function Editor({
  entryId,
  date,
  onSaved,
  onDeleted,
}: {
  entryId: string | null;
  date: string;
  onSaved: (id: string) => void;
  onDeleted: () => void;
}) {
  const [content, setContent] = useState("");
  const [preview, setPreview] = useState(false);
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState("");
  const taRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    let cancelled = false;
    if (entryId) {
      api
        .getEntry(entryId)
        .then((c) => {
          if (!cancelled) setContent(c);
        })
        .catch((e) => setStatus("读取失败：" + e));
    } else {
      setContent("");
    }
    setPreview(false);
    setStatus("");
    return () => {
      cancelled = true;
    };
  }, [entryId]);

  function insertAtCursor(text: string) {
    const ta = taRef.current;
    if (!ta) {
      setContent((c) => c + text);
      return;
    }
    const start = ta.selectionStart;
    const end = ta.selectionEnd;
    setContent((c) => c.slice(0, start) + text + c.slice(end));
    requestAnimationFrame(() => {
      ta.focus();
      const pos = start + text.length;
      ta.setSelectionRange(pos, pos);
    });
  }

  async function pickImage() {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp"] }],
      });
      if (typeof selected === "string") {
        const name = await api.importImagePath(selected);
        insertAtCursor(`\n![](blob:${name})\n`);
      }
    } catch (e) {
      setStatus("插入图片失败：" + e);
    }
  }

  async function onPaste(e: React.ClipboardEvent<HTMLTextAreaElement>) {
    const items = e.clipboardData?.items;
    if (!items) return;
    for (let i = 0; i < items.length; i++) {
      const it = items[i];
      if (it.type.startsWith("image/")) {
        e.preventDefault();
        const file = it.getAsFile();
        if (!file) continue;
        const ext = it.type.split("/")[1] || "png";
        const buf = await file.arrayBuffer();
        const b64 = arrayBufferToBase64(buf);
        try {
          const name = await api.addImage(b64, ext);
          insertAtCursor(`\n![](blob:${name})\n`);
        } catch (err) {
          setStatus("粘贴图片失败：" + err);
        }
      }
    }
  }

  async function save() {
    setSaving(true);
    setStatus("");
    try {
      const id = await api.saveEntry(entryId, date, content);
      onSaved(id);
      setStatus("已保存 ✓");
    } catch (e) {
      setStatus("保存失败：" + e);
    }
    setSaving(false);
  }

  async function remove() {
    if (!entryId) {
      onDeleted();
      return;
    }
    if (!confirm("确定删除这篇日记吗？此操作不可撤销。")) return;
    try {
      await api.deleteEntry(entryId);
      onDeleted();
    } catch (e) {
      setStatus("删除失败：" + e);
    }
  }

  function onKeyDown(e: React.KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      save();
    }
  }

  return (
    <div className="editor" onKeyDown={onKeyDown}>
      <div className="editor-toolbar">
        <span className="editor-date">
          {date}
          {entryId ? "" : " · 新日记"}
        </span>
        <div className="spacer" />
        <button onClick={pickImage}>🖼 插入图片</button>
        <button onClick={() => setPreview((p) => !p)}>{preview ? "✏️ 编辑" : "👁 预览"}</button>
        <button onClick={save} disabled={saving} className="primary">
          {saving ? "保存中…" : "💾 保存"}
        </button>
        <button onClick={remove} className="danger">
          🗑 删除
        </button>
      </div>
      {preview ? (
        <MarkdownPreview content={content} />
      ) : (
        <textarea
          ref={taRef}
          className="editor-textarea"
          value={content}
          onChange={(e) => setContent(e.target.value)}
          onPaste={onPaste}
          placeholder={"# 今天…\n\n在这里写下你的日记。支持 Markdown，可粘贴或点「插入图片」。\n按 Ctrl+S 保存。"}
        />
      )}
      {status && <div className="editor-status">{status}</div>}
    </div>
  );
}
