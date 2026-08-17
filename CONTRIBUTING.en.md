# Contribution Guide

Thank you for improving LanNook. Reproducible bug reports, documentation updates, tests, and focused feature patches are all valuable contributions.

## Before you start

1. Read the [README](README.en.md) to understand the product goal and security boundaries.
2. Search [Issues](https://github.com/by-fengqiao/lannook/issues) before opening a new one. If no matching report exists, include reproducible steps in a new issue.
3. For a substantial feature, first describe the goal, interaction, and scope in an issue. This avoids duplicate work and keeps the project focused.

## Local development

```bash
git clone https://github.com/by-fengqiao/lannook.git
cd lannook
npm ci
npm run tauri dev
```

Before opening a pull request, run:

```bash
npm run build
cd src-tauri
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Pull request expectations

- Keep each pull request focused on one clear problem.
- Use a descriptive title. Explain the context, change, verification, and known limitations in the body.
- Include a real screenshot or recording for interaction or visual changes.
- For connection, transfer, or cross-platform changes, refer to the [compatibility checklist](docs/testing/compatibility.md) and describe the verification scope and results.
- Do not commit `node_modules`, `dist`, `src-tauri/target`, local databases, logs, tokens, passwords, or `.env` files.
- For device authorization, LAN access, file paths, transfer permissions, or private data, explain the threat model and failure behavior.

## Code conventions

- Use Vue Composition API and TypeScript. Keep state, presentation, and side effects clearly separated.
- Rust changes must pass `cargo fmt` and Clippy. Do not suppress a real warning with `allow` unless the pull request explains why.
- Add tests for new behavior, especially authorization, filename cleanup, path handling, transfer state, and WebSocket events.
- Documentation and UI copy must describe real behavior. Do not promise unimplemented cloud, encryption, compatibility, or performance features.

## License

By submitting a contribution, you confirm that you have the right to submit it and agree to license it under [GPL-3.0-only](LICENSE).
