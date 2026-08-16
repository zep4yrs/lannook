# LanNook v26.3.0

PIN 码配对 · 跨会话断点续传 · 下载限速 · PWA · 全量中英本地化

PIN pairing · Cross-session resume · Download throttling · PWA · Full zh/en localization

---

## 🆕 新功能 / What’s New

**🔐 安全 / Security**
- 6 位 PIN 码配对：无法扫码时，在手机连接页输入电脑面板上的配对码即可连接；一次性使用、5 分钟有效、连续 5 次输错自动锁定（防暴力破解）。
  - PIN pairing: can’t scan the QR code? Enter the 6-digit code shown on the desktop. Single-use, 5-min expiry, brute-force lockout.
- 零成本自签名脚本：`scripts/sign-windows.ps1` 为手工分发的 Windows 安装包补充 Authenticode 签名（详见 docs/signing.md）。
  - Zero-cost self-signed Windows installer signing script + guide.

**⚡ 传输可靠性 / Reliability**
- 跨会话断点续传：重启电脑或本地服务后，进行中的任务自动转为可续传状态，从已完成分片继续，不再从头开始。
  - Cross-session resume: after a restart, in-flight transfers reset to paused and continue from completed chunks.
- 下载限速设置（MiB/s，0=不限速），避免大文件传输占满整个网络。
  - Download speed limit setting.
- 双端完成确认：手机端通过 WebSocket 实时收到自己传输的完成/失败/取消事件，不再依赖轮询。
  - Ownership-scoped events: phones see their own transfer completion/failure in real time.

**📱 移动端 / Mobile**
- PWA 支持：iOS “添加到主屏”即可获得类似 App 的图标与全屏体验；Android 手动添加主屏。
  - PWA manifest + icons; iOS add-to-home-screen support.
- 文件选择增加图片缩略图预览。
  - Image thumbnails in the file picker.

**🌐 本地化 / Localization**
- 全量中英双语：桌面端对话框、状态徽章、无障碍标签、法律文档页、全部提示补齐英文。
  - Full zh/en coverage across desktop dialogs, status badges, a11y labels and the legal pages.

**🛠 工程 / Engineering**
- 端到端集成测试（注册→审批→上传→完成→SHA-256 校验；PIN 配对；暴力破解锁定）。
  - E2E integration tests covering the full upload flow, PIN pairing and lockout.
- 性能基准（32 MiB 流式 SHA-256 等）。
  - Criterion benchmarks.
- CI 增加 `cargo audit` 安全审计。
  - cargo audit in CI.

---

## 📥 下载 / Downloads

| 平台 | 文件 |
| --- | --- |
| Windows | `LanNook_26.3.0_x64-setup.exe`（NSIS，推荐）/ `LanNook_26.3.0_x64_en-US.msi` |
| macOS | `LanNook_26.3.0_aarch64.dmg`（Apple 芯片）/ `LanNook_26.3.0_x64.dmg`（Intel） |
| Linux | `LanNook_26.3.0_amd64.AppImage` / `LanNook_26.3.0_amd64.deb` |

## 📝 说明 / Notes

- 手机端直接用浏览器打开二维码地址即可使用，无需安装 App。
- 移动端连接为局域网 HTTP/WebSocket，请仅在可信网络中使用。
- 自签名安装包可能被系统提示“未知发布者”，属正常现象；请只从本仓库 Release 页面下载。
- 使用中发现任何问题，欢迎提交 Issue。
