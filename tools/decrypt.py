#!/usr/bin/env python3
"""
私密日记 · 独立解密脚本（应急恢复工具）

这个脚本【不依赖日记 app】。只要你有 vault 文件夹和密码，就能把全部日记和图片
还原成明文。它存在的意义：即使 app 永久损坏/消失，你的日记也不会丢。

依赖：
    pip install argon2-cffi pynacl

用法：
    python decrypt.py <vault目录> [输出目录]

例：
    python decrypt.py "%LOCALAPPDATA%\\com.privatediary.desktop\\vault" out

格式细节见仓库根目录 FORMAT.md。
"""

import base64
import getpass
import json
import os
import sys

try:
    from argon2.low_level import hash_secret_raw, Type
    import nacl.bindings as sodium
except ImportError:
    print("缺少依赖，请先运行：pip install argon2-cffi pynacl")
    sys.exit(1)


def b64d(s: str) -> bytes:
    return base64.b64decode(s)


def derive_kek(password: str, kdf: dict) -> bytes:
    """用 Argon2id 从密码派生 KEK。参数与 app(meta.json) 完全一致。"""
    return hash_secret_raw(
        secret=password.encode("utf-8"),
        salt=b64d(kdf["salt"]),
        time_cost=kdf["t_cost"],
        memory_cost=kdf["m_cost"],  # 单位 KiB
        parallelism=kdf["p_cost"],
        hash_len=32,
        type=Type.ID,
        version=19,  # 0x13
    )


def xdecrypt(key: bytes, blob: bytes) -> bytes:
    """解密一个 .enc 块：前 24 字节是 nonce，其余是密文(含 tag)。"""
    nonce, ct = blob[:24], blob[24:]
    return sodium.crypto_aead_xchacha20poly1305_ietf_decrypt(ct, b"", nonce, key)


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)
    vault = os.path.expandvars(sys.argv[1])
    out = sys.argv[2] if len(sys.argv) > 2 else "decrypted"

    with open(os.path.join(vault, "meta.json"), encoding="utf-8") as f:
        meta = json.load(f)

    password = getpass.getpass("请输入日记密码: ")
    kek = derive_kek(password, meta["kdf"])

    wd = meta["wrapped_dek"]
    try:
        dek = sodium.crypto_aead_xchacha20poly1305_ietf_decrypt(
            b64d(wd["ct"]), b"", b64d(wd["nonce"]), kek
        )
    except Exception:
        print("❌ 密码错误（无法解开数据密钥）。")
        sys.exit(1)
    assert len(dek) == 32, "DEK 长度异常"

    # 读取索引，用于给输出文件取更友好的名字（日期 + id 前 6 位）
    id_to_meta = {}
    index_path = os.path.join(vault, "index.enc")
    if os.path.exists(index_path):
        with open(index_path, "rb") as f:
            index = json.loads(xdecrypt(dek, f.read()).decode("utf-8"))
        for e in index.get("entries", []):
            id_to_meta[e["id"]] = e

    os.makedirs(os.path.join(out, "images"), exist_ok=True)

    # 解密所有日记
    count = 0
    entries_dir = os.path.join(vault, "entries")
    if os.path.isdir(entries_dir):
        for fn in sorted(os.listdir(entries_dir)):
            if not fn.endswith(".enc"):
                continue
            eid = fn[:-4]
            with open(os.path.join(entries_dir, fn), "rb") as f:
                md = xdecrypt(dek, f.read()).decode("utf-8")
            em = id_to_meta.get(eid)
            name = f"{em['date']}-{eid[:6]}" if em else eid
            with open(os.path.join(out, name + ".md"), "w", encoding="utf-8") as f:
                f.write(md)
            count += 1

    # 解密所有图片（文件名为 <id>，扩展名见 Markdown 里的 blob:<id>.<ext> 引用）
    img_count = 0
    blobs_dir = os.path.join(vault, "blobs")
    if os.path.isdir(blobs_dir):
        for fn in sorted(os.listdir(blobs_dir)):
            if not fn.endswith(".enc"):
                continue
            with open(os.path.join(blobs_dir, fn), "rb") as f:
                data = xdecrypt(dek, f.read())
            with open(os.path.join(out, "images", fn[:-4]), "wb") as f:
                f.write(data)
            img_count += 1

    print(f"✅ 完成：解密 {count} 篇日记、{img_count} 张图片 → {os.path.abspath(out)}")


if __name__ == "__main__":
    main()
