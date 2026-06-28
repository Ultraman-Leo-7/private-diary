# 加密日记 · 存储格式说明（FORMAT v1）

> 这份文档的存在，是为了保证**即使本 app 永久消失或损坏，只要你记得密码，就一定能拿回日记**。
> 格式完全公开，任何人都能照着它写程序解密（仓库里的 `tools/decrypt.py` 就是一个不依赖本 app 的参考实现）。

## 1. 总览

- **密钥派生（KDF）**：Argon2id
- **对称加密**：XChaCha20-Poly1305（AEAD 认证加密；与 libsodium 的 `crypto_aead_xchacha20poly1305_ietf` 兼容）
- **两层密钥**：
  - **DEK**（Data Encryption Key，32 字节随机）：真正用于加密所有日记与图片。
  - **KEK**（Key Encryption Key）：由密码经 Argon2id 派生，仅用于加密（包裹）DEK。
  - 好处：改密码时只需用新 KEK 重新包裹 DEK，无需重新加密全部日记。

## 2. 目录布局

数据位于 `%LOCALAPPDATA%\com.privatediary.desktop\vault\`：

```
vault/
  meta.json            明文(非机密)：版本/KDF参数/salt/密码提示；密文：包裹后的 DEK
  index.enc            加密索引：条目清单（id/date/title/时间）
  entries/<id>.enc     每条日记：一个加密块，明文是 UTF-8 的 Markdown
  blobs/<id>.enc       每张图片：一个加密块，明文是图片原始字节
```

## 3. 加密块（.enc 文件）布局

每个 `.enc` 文件（以及 `index.enc`）都是：

```
[ nonce: 24 字节 ] ++ [ ciphertext + Poly1305 tag(16 字节) ]
```

解密：用对应密钥（日记/图片/索引都用 **DEK**）做 XChaCha20-Poly1305 解密，nonce 取前 24 字节，其余为密文（含 16 字节认证标签）。AAD 为空。

## 4. meta.json 字段

```jsonc
{
  "version": 1,
  "cipher": "xchacha20poly1305",
  "kdf": {
    "algo": "argon2id",
    "m_cost": 19456,   // 内存开销，单位 KiB
    "t_cost": 2,       // 迭代次数
    "p_cost": 1,       // 并行度
    "salt": "<base64>" // Argon2id 的盐
  },
  "hint": "密码提示（明文，非机密）",
  "wrapped_dek": {     // 被 KEK 加密的 DEK
    "nonce": "<base64, 24 字节>",
    "ct": "<base64, 32 字节明文 + 16 字节 tag = 48 字节>"
  },
  "created_at": 1719000000000
}
```

## 5. 解密步骤（任何语言都能复刻）

1. 读 `meta.json`。
2. `KEK = Argon2id(密码, salt = base64decode(kdf.salt), m=kdf.m_cost(KiB), t=kdf.t_cost, p=kdf.p_cost, 输出 32 字节, 版本 0x13)`。
3. `DEK = XChaCha20Poly1305_decrypt(key=KEK, nonce=base64decode(wrapped_dek.nonce), ct=base64decode(wrapped_dek.ct), aad="")`。
   - 若此步认证失败 → **密码错误**。
4. 对每个 `entries/<id>.enc`：`明文Markdown = XChaCha20Poly1305_decrypt(key=DEK, nonce=前24字节, ct=其余字节)`。
5. 图片 `blobs/<id>.enc` 同理（明文是图片原始字节）。Markdown 里以 `![](blob:<id>.<ext>)` 引用图片，`<ext>` 为原始扩展名。
6. `index.enc` 也用 DEK 解密，明文是 JSON：`{ "entries": [ { "id", "date", "title", "created_at", "updated_at" }, ... ] }`，用于把条目映射回日期/标题。

## 6. 参考实现

见 `tools/decrypt.py`（依赖 `argon2-cffi` 与 `pynacl`）。它不依赖本 app，只要有 `vault/` 文件夹和密码即可还原全部日记与图片。
