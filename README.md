# 私密日记 · PrivateDiary

一款注重隐私的本地加密日记桌面应用（Windows）。日记必须凭密码进入，内容在磁盘上和云端都是密文，
别人拿到文件夹也读不出来；但**只要你记得密码，就一定能还原**——格式公开、附独立解密脚本，绝不被 app 自身绑架。

> 技术栈：Tauri 2（Rust 后端 + React/TypeScript 前端），产物是独立离线运行的 `.exe`。

## ✨ 功能

- 🔒 **密码门禁**：启动必须输密码；支持密码提示。
- 📝 **Markdown 日记 + 图片**：写法简洁、可粘贴/插入图片。
- 📅 **日历**：高亮有日记的日期，点选某天查看当天日记。
- 🛡️ **端到端本地加密**：Argon2id 派生密钥 + XChaCha20-Poly1305 认证加密，内容/图片落盘皆密文。
- ☁️ **坚果云 WebDAV 同步**：上传的就是密文（云端看到的是乱码）；支持自动同步与换设备恢复。
- 💾 **永不丢失**：格式公开（见 [FORMAT.md](FORMAT.md)）+ 独立解密脚本 [tools/decrypt.py](tools/decrypt.py) + 一键导出明文备份。

## 🔐 安全模型

- 密码经 **Argon2id**（+ 随机 salt）派生出 KEK；真正加密日记的是随机 **DEK**，DEK 被 KEK 包裹后存于 `meta.json`（两层密钥，改密码无需重新加密全部日记）。
- 所有日记/图片/索引用 **XChaCha20-Poly1305** 加密（带认证标签，密码错或数据被篡改都会解密失败）。
- 数据密钥只存在于后端内存，**永不持久化、永不返回前端**，锁定/退出即清零。
- 完整格式与算法参数见 [FORMAT.md](FORMAT.md)。即使本 app 消失，用 `tools/decrypt.py`（`pip install argon2-cffi pynacl`）+ 密码即可还原全部日记。

> 边界说明：加密保护的是**静止数据**（磁盘、云端文件）。它不防御解锁状态下的木马/键盘记录器或已被攻陷的系统。

## 📦 安装使用

- **普通用户**：到 [Releases](../../releases) 下载最新的 `.exe`/`.msi` 安装包，安装即用。
  - 安装包未做代码签名，首次运行 Windows SmartScreen 可能提示「未知发布者」，点「更多信息 → 仍要运行」即可。
- **数据位置**：`%LOCALAPPDATA%\com.privatediary.desktop\vault\`。更新或重装 app **不会动你的日记**。

## ☁️ 坚果云同步设置

1. 登录坚果云网页端 → 账户信息 → 安全选项 → 第三方应用管理 → **生成「应用密码」**（不是登录密码）。
2. 应用内 ⚙ 设置 →「云同步」填：账号(邮箱) + 应用密码 → 保存配置 → 测试连接。
3. 点「立即同步」，或在「自动同步」里开启解锁拉取 / 退出推送等。
4. 换设备：锁屏页「从坚果云恢复」，输入凭据 + 日记密码即可拉回全部日记。

## 🛠️ 从源码开发

前置：[Node.js](https://nodejs.org/) 18+、[Rust](https://www.rust-lang.org/tools/install)、Windows 上需 VS C++ 生成工具与 WebView2（Win11 自带）。

```bash
npm install
npm run tauri dev      # 开发运行（开窗口）
npm run tauri build    # 打包安装包（生成 .exe/.msi）
```

后端测试：

```bash
cd src-tauri && cargo test
```

## 📄 许可证

[MIT](LICENSE)
