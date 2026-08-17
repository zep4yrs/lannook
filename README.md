<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="96" alt="LanNook 应用图标" />
</p>

<h1 align="center">LanNook</h1>

<p align="center">
  <strong>简体中文</strong> · <a href="README.en.md">English</a>
</p>

<p align="center">
  在同一局域网里，用手机浏览器和电脑互传文件。<br />
  手机不用安装 App，也不用注册账号。
</p>

<p align="center">
  <a href="https://github.com/by-fengqiao/lannook/releases/latest"><img src="https://img.shields.io/github/v/release/by-fengqiao/lannook?display_name=tag&sort=semver" alt="最新版本" /></a>
  <a href="https://github.com/by-fengqiao/lannook/actions/workflows/ci.yml"><img src="https://github.com/by-fengqiao/lannook/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI 状态" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0--only-4a32c5.svg" alt="GPL-3.0-only" /></a>
</p>

LanNook 处理的是一个很具体的场景：文件就在手机或身边的电脑上，你只想直接把它送到另一台设备。电脑运行 LanNook 后会提供二维码和局域网地址；手机打开页面，电脑默认确认接入，随后两边都能发送文件。官方发行版不会把文件上传到公共云盘。

当前版本面向可信局域网，不提供公共中继或跨公网传输模式。移动端连接使用局域网 HTTP/WebSocket，目前没有 TLS 或端到端加密，请先阅读下方的[安全边界](#安全边界)。

## 下载

从 [最新 Release](https://github.com/by-fengqiao/lannook/releases/latest) 下载电脑端安装包。手机和平板直接使用浏览器打开二维码地址，不需要另装客户端。`v26.1.7` 及更早的历史安装包仍保留原来的 LYNQO 文件名；自首个 LanNook 发行版起，安装包使用下表的新文件名。

| 电脑系统 | 应下载的文件 | 说明 |
| --- | --- | --- |
| Windows 10/11 x64 | `LanNook_*_x64-setup.exe` | 推荐，当前用户安装 |
| Windows 10/11 x64 | `LanNook_*_x64_en-US.msi` | 适合 MSI 或集中部署场景 |
| macOS 10.15+，Apple 芯片 | `LanNook_*_aarch64.dmg` | M1、M2、M3、M4 等 |
| macOS 10.15+，Intel | `LanNook_*_x64.dmg` | Intel Mac |
| Linux x64 | `LanNook_*_amd64.AppImage` 或 `LanNook_*_amd64.deb` | AppImage 通用；deb 适合 Debian/Ubuntu |

当前没有 Windows ARM、Windows 32 位或 Linux ARM 安装包。发行包默认不进行操作系统级签名，系统可能显示“未知发布者”一类提示；请只从本仓库的 Release 页面下载。Release 中的 `.sig` 是应用更新校验文件，不等同于操作系统代码签名。如需为手工分发的 Windows 安装包补充零成本的 Authenticode 签名，可运行 `scripts/sign-windows.ps1`（见 [docs/signing.md](docs/signing.md)）。

## 从 LYNQO 升级

LanNook 会自动迁移旧版的设备授权、传输记录、设置、日志和浏览器偏好，不需要重新连接设备。为保证已安装 LYNQO 能原地收到 LanNook 更新，桌面包标识会暂时保留为旧值；它不会出现在产品界面或安装包名称中。完整说明见[迁移文档](docs/migrations/lynqo-to-lannook.md)。

## 第一次使用

1. 在电脑启动 LanNook，确认顶部显示“运行中”。
2. 点击“连接设备”，用手机扫描二维码；也可以把面板中的完整地址输入手机浏览器，或在无法扫码时输入面板中显示的 6 位 PIN 码。
3. 新设备默认会在电脑主页弹出接入请求。选择“仅本次允许”，或勾选“信任此设备”后允许。信任记录可在“设备”页撤销。
4. 在手机或电脑选择文件和目标设备，然后到“传输中心”查看进度、速度、剩余时间与结果。

手机向另一台手机发送时，文件会先进入运行 LanNook 的电脑，再由目标手机接收；这不是手机之间的直接 P2P 连接。

## 目前可以做什么

- 手机端直接在现代浏览器中运行，无需单独安装应用；支持 PWA，可从浏览器“添加到主屏”，获得类似 App 的图标与全屏体验。界面为中英双语，可在设置中切换。
- 在手机和电脑之间双向发送文件；电脑端支持选择文件和拖放；手机端文件选择带图片缩略图预览。
- 用一个传输中心查看等待、进行中、已完成、暂停任务，以及当前会话发生的错误；支持按文件名或设备搜索，并可多选批量重试、批量删除记录（磁盘上的已收文件保留）。
- 文件按 512 KiB 分块上传，单块失败自动重试并指数退避；网络中断时自动从断点继续（最多自动重试 2 次）；失败的上传/下载任务可直接“继续”，从已完成分块处恢复；重启电脑或本地服务后，进行中的任务转为可续传状态，支持跨会话续传。
- 同步显示进度、平滑后的速度和预计剩余时间；传输完成后在后台计算 SHA-256（大文件实时显示校验进度），可在桌面传输中心核对首个文件的完整校验值与短指纹。
- 无法扫码时，可在手机连接页输入电脑面板上的 6 位 PIN 码配对：一次性使用、5 分钟有效，连续输错自动锁定。
- 可设置下载限速（MiB/s，0=不限速），避免大文件传输占满整个网络；新设备授权可设时长（本次服务/1 小时/24 小时/7 天），到期自动撤销。
- 保存设备、授权和传输记录到电脑本地 SQLite 数据库；接收目录可以修改。
- 提供连接地址选择、mDNS 状态、本机监听自检和 Windows 防火墙诊断；传输事件通过 WebSocket 实时推送到两端，无需手动刷新。
- 支持系统托盘、开机自启，以及“关闭窗口时退出、隐藏到托盘或询问”的行为设置。
- 可在关于页面检查并安装由项目更新密钥签名的新版。

## 手机打不开连接地址

“连着同一个 Wi-Fi”不一定代表设备之间可以互访。遇到超时或电脑没有收到接入请求时，依次检查：

1. 不要在手机输入 `localhost` 或 `127.0.0.1`，它们指向手机自身。请使用“连接设备”面板给出的完整地址。
2. 访客 Wi-Fi、校园网、公司网络和开启 AP 隔离的路由器可能禁止设备互访。换到普通家庭网络或电脑热点再试。
3. 关闭手机浏览器的云端加速、VPN、代理或流量节省功能；它们可能无法访问 `192.168.x.x` 一类私有地址。
4. Windows 网络建议设为“专用网络”。如果连接诊断显示防火墙规则缺失，可由用户确认后添加只允许当前程序、当前 TCP 端口和本地子网的规则。
5. 电脑同时连接路由器、热点、VPN 或虚拟网卡时，在连接面板中选择与手机处于同一网络的地址。默认端口是 `53317`，修改端口后以面板实际显示为准。

连接面板的“本机自检”只能确认服务已经监听，不能证明手机到电脑的整条网络路径一定可达，最终仍要在手机浏览器中验证。

## 安全边界

- 新设备默认需要电脑端审批，但可以在设置中关闭。关闭后，拿到有效连接地址的设备可直接接入。
- “仅本次允许”在当前局域网服务运行周期内有效；“信任此设备”会持久保存，直到在设备页撤销或移除。
- 二维码地址包含配对参数，请不要把二维码或完整连接地址发到不可信的群聊、网页或公共场所。
- 当前移动连接是局域网明文 HTTP/WebSocket，不是 TLS 或端到端加密。请只在你信任且没有陌生用户接入的网络中使用。
- 官方桌面发行版默认把设备记录、授权、传输历史和接收文件留在电脑本地，不使用公共云文件中转。自行部署前端或配置自定义 API 网关时，需要自行评估新的数据路径。
- SHA-256 目前只生成指纹供手工核对，应用不会自动证明发送端与接收端完全一致；它也不能判断文件是否安全。LanNook 不是云盘、远程备份、内容审核或杀毒软件。

## 从源码运行

需要 Node.js 20.x 或 22+、Rust stable，以及当前平台的 [Tauri 2 构建依赖](https://v2.tauri.app/start/prerequisites/)。CI 使用 Node.js 22；Windows 通常还需要 MSVC Build Tools 和 WebView2。

```bash
git clone https://github.com/by-fengqiao/lannook.git
cd lannook
npm ci
npm run tauri dev
```

正常扫码使用不需要 `.env`。只有把前端放到自定义 API 网关后面时，才需要设置可选的 `VITE_LANNOOK_API_BASE_URL`，格式见 [.env.example](.env.example)。它只改变 REST API 地址；网关还必须在前端同源代理 `/ws` WebSocket 路径。

构建当前平台安装包：

```bash
npm run tauri build
```

## 验证改动

```bash
npm run test
npm run build

cd src-tauri
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

## 代码位置

| 路径 | 内容 |
| --- | --- |
| `src/` | Vue 界面、路由、状态和浏览器端传输逻辑 |
| `src-tauri/src/` | Rust 命令、局域网服务、传输、发现与本地存储 |
| `src-tauri/icons/` | 应用图标与平台图标资源 |
| `.github/workflows/` | 检查、跨平台构建与更新清单发布 |

## 参与项目

发现问题时，请先在 [Issues](https://github.com/by-fengqiao/lannook/issues) 搜索，并尽量附上系统版本、网络环境、复现步骤和连接诊断结果。代码提交方式见 [贡献指南](CONTRIBUTING.md)；英文版见 [Contribution Guide](CONTRIBUTING.en.md)。

- [常见问题（FAQ）](docs/FAQ.md)
- [v26.3.0 版本说明](release-notes.md)
- [v26.2.2 修复说明](docs/releases/v26.2.2.md)
- [v26.2.1 修复说明](docs/releases/v26.2.1.md)
- [v26.2.0 更名说明](docs/releases/v26.2.0.md)
- [v26.1.7 版本说明](docs/releases/v26.1.7.md)
- [LYNQO → LanNook 迁移说明](docs/migrations/lynqo-to-lannook.md)
- 发起与维护：[by-fengqiao](https://github.com/by-fengqiao)

## 许可证

Copyright (C) 2026 LanNook contributors.

LanNook 采用 [GNU General Public License v3.0](LICENSE)（`GPL-3.0-only`）。部分主要依赖的许可证摘要见 [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)。
