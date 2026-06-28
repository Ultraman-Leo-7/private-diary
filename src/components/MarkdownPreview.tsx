import { useEffect, useState } from "react";
import MarkdownIt from "markdown-it";
import { api } from "../api";

const md = new MarkdownIt({ html: false, linkify: true, breaks: true });
// 允许内嵌图片 data URL 显示
const defaultValidate = md.validateLink.bind(md);
md.validateLink = (url: string) => url.startsWith("data:image/") || defaultValidate(url);

// blob 引用 -> data URL 的会话级缓存
const imageCache = new Map<string, string>();
const BLOB_RE = /blob:([0-9a-fA-F-]+\.[A-Za-z0-9]+)/g;

export default function MarkdownPreview({ content }: { content: string }) {
  const [html, setHtml] = useState("");

  useEffect(() => {
    let cancelled = false;
    (async () => {
      // 收集所有图片引用并解析为 data URL
      const names = new Set<string>();
      let m: RegExpExecArray | null;
      BLOB_RE.lastIndex = 0;
      while ((m = BLOB_RE.exec(content)) !== null) names.add(m[1]);
      for (const name of names) {
        if (!imageCache.has(name)) {
          try {
            const img = await api.getImage(name);
            imageCache.set(name, `data:${img.mime};base64,${img.data_base64}`);
          } catch {
            imageCache.set(name, "");
          }
        }
      }
      const replaced = content.replace(BLOB_RE, (_full, name) => imageCache.get(name) || "");
      if (!cancelled) setHtml(md.render(replaced));
    })();
    return () => {
      cancelled = true;
    };
  }, [content]);

  return <div className="md-preview" dangerouslySetInnerHTML={{ __html: html }} />;
}
