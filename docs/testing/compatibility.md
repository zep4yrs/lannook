# LanNook 兼容性测试表（Compatibility Matrix）

> 本文是给维护者与测试者用的测试记录表。每次发布前，尽量按此表验证一轮；已确认的组合打 ✅，未测试留空，发现问题的组合记录为 ❌ 并链接 Issue。
> This is a working checklist for maintainers and testers. Before each release, verify as many rows as possible: ✅ = verified, ❌ = broken (link an issue), blank = not tested yet.

## 桌面端（协议：Windows / macOS / Linux x64）

| 平台 | 说明 | 状态 |
| --- | --- | --- |
| Windows 10 x64 | `setup.exe` / `msi`；含防火墙规则、托盘、开机自启 | |
| Windows 11 x64 | 同上；Win11 专用网络/防火墙行为差异 | |
| macOS Apple silicon | `dmg`；网络权限（本地网络授权）提示 | |
| macOS Intel | `dmg`（x64 构建） | |
| Linux Debian/Ubuntu x64 | `deb` / `AppImage`；WebKitGTK 依赖 | |
| Linux 其他发行版 | AppImage 兼容性 | |

CI 自动覆盖：Windows / macOS 安装包构建 + Ubuntu 上的 TypeScript、Rust、单元/集成测试、cargo audit（见 `.github/workflows/ci.yml`）。

## 手机端浏览器（连接 + 发送 + 接收 + PHN/扫码）

| 浏览器 | 版本 | 扫码连接 | PIN 配对 | 上传（含大文件分块） | 下载 | PWA 添加到主屏 | 备注 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| iOS Safari | 最新 2 个大版本 | | | | | | 主屏图标、全屏 |
| iOS Chrome | 最新 | | | | | | WKWebView 内核 |
| Android Chrome | 最新 2 个大版本 | | | | | | 缩略图预览 |
| Android 微信/内置浏览器 | 常见版本 | | | | | | 内核差异，可能不支持部分特性 |
| Windows Edge / Chrome（电脑浏览器访问） | 最新 | | | | | | 桌面浏览器侧调试用 |
| Firefox（Android） | 最新 | | | | | | |
| 桌面 Firefox / Safari | 最新 | | | | | | 调试用 |

## 网络环境

| 场景 | 结果 | 备注 |
| --- | --- | --- |
| 家庭路由器（2.4 GHz） | | 同一 SSID |
| 家庭路由器（5 GHz） | | |
| 电脑热点 | | 手机连电脑热点 |
| 访客 Wi-Fi / AP 隔离 | | 预期连不上，验证诊断提示 |
| 校园/公司网络 | | 预期受限，验证诊断提示 |
| VPN / 代理开启 | | 手机端关闭代理要求 |
| 双网卡 / 多地址 | | 连接面板地址选择 |

## 传输可靠性场景

| 场景 | 结果 | 备注 |
| --- | --- | --- |
| 512 KiB 以上多分块上传 | | |
| 1 GB 以上大文件 | | 记录耗时、速度、校验值 |
| 上传中断（飞行模式/断网） | | 自动重试、从断点继续 |
| 失败任务手动“继续” | | 从已完成分块恢复 |
| 重启电脑/服务后跨会话续传 | | 任务转为可续传状态 |
| 下载限速生效 | | 设置 MiB/s 后测速 |
| SHA-256 校验值核对 | | 与源文件一致 |
| 手机 ↔ 手机（经电脑中转） | | |

## 反馈记录

发现兼容性问题时：

1. 在 [Issues](https://github.com/by-fengqiao/lannook/issues) 搜索是否已有报告；
2. 按 [Bug 模板](https://github.com/by-fengqiao/lannook/issues/new/choose) 提交，注明系统/浏览器版本、网络环境与连接诊断输出；
3. 在下方表格补充问题描述。

| 日期 | 版本 | 环境 | 问题 | Issue 链接 |
| --- | --- | --- | --- | --- |