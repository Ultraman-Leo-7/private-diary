//! 极简 WebDAV 客户端（异步，基于 reqwest），适配坚果云。
//!
//! 坚果云需要使用「应用密码」（不是登录密码）做 HTTP Basic 认证。
//! 我们只在这里搬运字节，所有内容本就是密文。

use anyhow::{anyhow, Result};
use percent_encoding::percent_decode_str;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use reqwest::{Client, Method, RequestBuilder};

/// 远端一个条目（PROPFIND 解析结果）。
pub struct RemoteEntry {
    pub rel: String, // 相对 root 的路径，如 "meta.json" / "entries/x.enc"
    pub etag: String,
    pub is_dir: bool,
}

pub struct WebDav {
    root: String, // 末尾不带斜杠，如 https://dav.jianguoyun.com/dav/private-diary
    user: String,
    pass: String,
    http: Client,
}

/// 取出 URL 的 path 部分（避免引入 url crate）。
fn url_path(u: &str) -> String {
    if let Some(idx) = u.find("://") {
        let after = &u[idx + 3..];
        if let Some(slash) = after.find('/') {
            return after[slash..].to_string();
        }
        return "/".to_string();
    }
    if u.starts_with('/') {
        u.to_string()
    } else {
        format!("/{u}")
    }
}

impl WebDav {
    pub fn new(url: &str, user: &str, pass: &str, remote_dir: &str) -> Result<Self> {
        let mut root = url.trim().trim_end_matches('/').to_string();
        let dir = remote_dir.trim().trim_matches('/');
        if !dir.is_empty() {
            root = format!("{root}/{dir}");
        }
        let http = Client::builder()
            .build()
            .map_err(|e| anyhow!("创建 HTTP 客户端失败: {e}"))?;
        Ok(WebDav {
            root,
            user: user.to_string(),
            pass: pass.to_string(),
            http,
        })
    }

    fn url(&self, rel: &str) -> String {
        if rel.is_empty() {
            self.root.clone()
        } else {
            format!("{}/{}", self.root, rel)
        }
    }

    fn req(&self, method: Method, rel: &str) -> RequestBuilder {
        self.http
            .request(method, self.url(rel))
            .basic_auth(&self.user, Some(&self.pass))
    }

    /// 创建目录（已存在则忽略）。
    pub async fn mkcol(&self, rel: &str) -> Result<()> {
        let resp = self
            .req(Method::from_bytes(b"MKCOL").unwrap(), rel)
            .send()
            .await?;
        let s = resp.status().as_u16();
        // 201 已创建；405/301 已存在
        if s == 201 || s == 405 || s == 301 || resp.status().is_success() {
            Ok(())
        } else {
            Err(anyhow!("MKCOL `{rel}` 失败: HTTP {s}"))
        }
    }

    pub async fn put(&self, rel: &str, body: Vec<u8>) -> Result<()> {
        let resp = self.req(Method::PUT, rel).body(body).send().await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(anyhow!("上传 `{rel}` 失败: HTTP {}", resp.status().as_u16()))
        }
    }

    pub async fn get(&self, rel: &str) -> Result<Vec<u8>> {
        let resp = self.req(Method::GET, rel).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("下载 `{rel}` 失败: HTTP {}", resp.status().as_u16()));
        }
        Ok(resp.bytes().await?.to_vec())
    }

    pub async fn delete(&self, rel: &str) -> Result<()> {
        let resp = self.req(Method::DELETE, rel).send().await?;
        let s = resp.status().as_u16();
        if resp.status().is_success() || s == 404 {
            Ok(())
        } else {
            Err(anyhow!("删除 `{rel}` 失败: HTTP {s}"))
        }
    }

    /// PROPFIND Depth:1，列出某目录的直接子项。目录不存在(404)时返回空。
    pub async fn propfind(&self, rel_dir: &str) -> Result<Vec<RemoteEntry>> {
        let body = r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/><d:getetag/></d:prop></d:propfind>"#;
        let resp = self
            .req(Method::from_bytes(b"PROPFIND").unwrap(), rel_dir)
            .header("Depth", "1")
            .header("Content-Type", "application/xml")
            .body(body)
            .send()
            .await?;
        let s = resp.status().as_u16();
        if s == 404 {
            return Ok(vec![]);
        }
        if !(resp.status().is_success() || s == 207) {
            return Err(anyhow!("PROPFIND `{rel_dir}` 失败: HTTP {s}"));
        }
        let text = resp.text().await?;
        parse_propfind(&text, &self.root)
    }

    /// 测试连接：尝试列出 root（不存在也算连接成功）。
    pub async fn test(&self) -> Result<()> {
        self.propfind("").await.map(|_| ())
    }
}

fn local_name(qname: &[u8]) -> String {
    let s = String::from_utf8_lossy(qname);
    match s.rsplit_once(':') {
        Some((_, l)) => l.to_string(),
        None => s.to_string(),
    }
}

fn parse_propfind(xml: &str, root: &str) -> Result<Vec<RemoteEntry>> {
    let base_path = url_path(root); // 如 "/dav/private-diary"
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut entries = Vec::new();
    let (mut href, mut etag, mut is_dir) = (String::new(), String::new(), false);
    let (mut in_href, mut in_etag) = (false, false);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) => match local_name(e.name().as_ref()).as_str() {
                "response" => {
                    href.clear();
                    etag.clear();
                    is_dir = false;
                }
                "href" => in_href = true,
                "getetag" => in_etag = true,
                "collection" => is_dir = true,
                _ => {}
            },
            Event::Empty(e) => {
                if local_name(e.name().as_ref()) == "collection" {
                    is_dir = true;
                }
            }
            Event::Text(t) => {
                let s = t.xml_content().unwrap_or_default().into_owned();
                if in_href {
                    href.push_str(&s);
                } else if in_etag {
                    etag.push_str(&s);
                }
            }
            Event::End(e) => match local_name(e.name().as_ref()).as_str() {
                "href" => in_href = false,
                "getetag" => in_etag = false,
                "response" => {
                    let path = if href.contains("://") {
                        url_path(&href)
                    } else {
                        href.clone()
                    };
                    let decoded = percent_decode_str(&path).decode_utf8_lossy().into_owned();
                    if let Some(rel) = decoded.strip_prefix(&base_path) {
                        let rel = rel.trim_matches('/');
                        if !rel.is_empty() {
                            entries.push(RemoteEntry {
                                rel: rel.to_string(),
                                etag: etag.trim().to_string(),
                                is_dir,
                            });
                        }
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(entries)
}
