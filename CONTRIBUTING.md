# 贡献指南

感谢你愿意改进 LanNook。无论是复现 Bug、补充文档、编写测试还是提交功能补丁，都是有价值的贡献。

## 开始前

1. 阅读 [README.md](README.md)，确认项目目标和安全边界。
2. 在 [Issues](https://github.com/by-fengqiao/lannook/issues) 搜索已有问题；若没有，请先提交一个包含复现步骤的新 Issue。
3. 对较大的功能改动，先在 Issue 中说明目标、交互和边界，避免重复实现或偏离产品方向。

## 本地开发

```bash
git clone https://github.com/by-fengqiao/lannook.git
cd lannook
npm ci
npm run tauri dev
```

提交 Pull Request 前请运行：

```bash
npm run build
cd src-tauri
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Pull Request 要求

- 每个 Pull Request 只解决一个清晰的问题。
- 使用描述性的标题，并在正文写明：问题背景、改动内容、验证方式和已知限制。
- 涉及交互或视觉变化时，请附上真实截图或录屏。
- 涉及连接、传输或跨平台改动时，参考[兼容性测试表](docs/testing/compatibility.md)说明验证范围与结果。
- 不要提交 `node_modules`、`dist`、`src-tauri/target`、本地数据库、日志、Token、密码或 `.env` 文件。
- 涉及设备授权、局域网访问、文件路径、传输权限或隐私数据时，请优先说明威胁模型与失败处理。

## 代码约定

- Vue 使用 Composition API 与 TypeScript，状态、界面和副作用保持清晰分层。
- Rust 代码必须通过 `cargo fmt` 和 Clippy；不要用 `allow` 掩盖真实警告，除非 Pull Request 中明确说明原因。
- 新增行为应有相应测试，尤其是授权、文件名清理、路径处理、传输状态和 WebSocket 事件。
- 文案应说明真实产品行为；不要加入尚未实现的云端、加密、兼容性或性能承诺。

## 许可证

提交贡献即表示你有权提交该内容，并同意你的贡献在 [GPL-3.0-only](LICENSE) 下发布。
