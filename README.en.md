<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="96" alt="LanNook app icon" />
</p>

<h1 align="center">LanNook</h1>

<p align="center">
  <a href="README.md">简体中文</a> · <strong>English</strong>
</p>

<p align="center">
  Move files between a phone browser and a computer on the same LAN.<br />
  No mobile app and no account required.
</p>

<p align="center">
  <a href="https://github.com/by-fengqiao/lannook/releases/latest"><img src="https://img.shields.io/github/v/release/by-fengqiao/lannook?display_name=tag&sort=semver" alt="Latest release" /></a>
  <a href="https://github.com/by-fengqiao/lannook/actions/workflows/ci.yml"><img src="https://github.com/by-fengqiao/lannook/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI status" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0--only-4a32c5.svg" alt="GPL-3.0-only" /></a>
</p>

LanNook is for a specific, ordinary task: a file is on your phone or a nearby computer, and you want it on the other device without sending it through a cloud drive. The desktop app provides a QR code and LAN address. Open that page on a phone, approve the device on the desktop by default, and either side can start a transfer. Official builds do not upload file contents to a public cloud.

The current release is intended for trusted local networks and has no public relay or internet-transfer mode. Its mobile connection uses LAN HTTP and WebSocket without TLS or end-to-end encryption. Read the [security boundaries](#security-boundaries) before using it.

## Download

Get the desktop installer from the [latest Release](https://github.com/by-fengqiao/lannook/releases/latest). Phones and tablets open the QR-code address in a browser and do not need a separate client. Historical packages through `v26.1.7` retain their former LYNQO filenames; starting with the first LanNook release, installers use the filenames below.

| Desktop system | Download | Notes |
| --- | --- | --- |
| Windows 10/11 x64 | `LanNook_*_x64-setup.exe` | Recommended per-user installer |
| Windows 10/11 x64 | `LanNook_*_x64_en-US.msi` | For MSI or managed deployment |
| macOS 10.15+, Apple silicon | `LanNook_*_aarch64.dmg` | M1, M2, M3, M4, and later Apple chips |
| macOS 10.15+, Intel | `LanNook_*_x64.dmg` | Intel Macs |
| Linux x64 | `LanNook_*_amd64.AppImage` or `LanNook_*_amd64.deb` | AppImage is portable; deb targets Debian/Ubuntu |

There are currently no Windows ARM, Windows 32-bit, or Linux ARM packages. Releases are also not yet protected by Windows Authenticode signing or Apple notarization, so the operating system may show an unknown-publisher warning. Download only from this repository's Release page. The `.sig` files attached to a Release authenticate application updates; they are not operating-system code signatures.

## Upgrade from LYNQO

LanNook automatically migrates LYNQO device approvals, transfer records, settings, logs, and browser preferences, so devices do not need to be connected again. The legacy desktop bundle identifier remains temporarily so existing LYNQO installations can update in place; it does not appear in the product UI or installer name. See the [migration note](docs/migrations/lynqo-to-lannook.md) for details.

## First use

1. Start LanNook on the computer and confirm that the top bar says the service is running.
2. Select **Connect device**, then scan the QR code with a phone. You can also enter the full address shown in the panel, or the 6-digit PIN shown there when scanning is not possible.
3. By default, a request from a new device appears on the desktop home screen. Approve it for the current service session, or select **Trust this device** before approving it. Trust can be revoked from the Devices page.
4. Choose files and a target on either device. The Transfer Center shows progress, speed, remaining time, and the final result.

When one phone sends to another, the file is uploaded to the computer running LanNook and then downloaded by the target phone. This is not a direct phone-to-phone P2P path.

## What works today

- The mobile interface runs in a modern browser and needs no separate installation. PWA support lets iOS and Android users add the page to the home screen for an app-like icon and fullscreen experience. The UI is fully bilingual (zh/en) and the language can be switched in Settings.
- File transfer works in both directions between a phone and the desktop; the desktop supports file selection and drag-and-drop, and the mobile file picker shows image thumbnails.
- One Transfer Center shows pending, active, completed, and paused work, plus errors from the current session. It supports searching by file name or device and multi-select batch retry and batch delete (files already received on disk are kept).
- Uploads use 512 KiB chunks with automatic per-chunk retry and exponential backoff. If the network drops, the upload resumes from its chunk checkpoint (up to 2 automatic retries). A failed upload or download can be continued directly from its completed chunks; after a computer or service restart, in-flight transfers switch to a resumable state for cross-session continuation.
- Both interfaces show live progress, smoothed speed, and estimated time remaining. After a transfer completes, the SHA-256 digest is computed in the background with visible progress on large files; the desktop Transfer Center lets you compare the full checksum and a short fingerprint for the first file.
- Can't scan the QR code? Enter the 6-digit PIN shown on the desktop panel on the phone's connection page. The code is single-use, expires after 5 minutes, and locks out after repeated wrong entries.
- A download speed limit (MiB/s, 0 = unlimited) keeps large transfers from saturating the network; new-device approvals can carry an expiry (until the service closes / 1 hour / 24 hours / 7 days) and are revoked automatically.
- Device, approval, and transfer records are stored in a local SQLite database. The desktop receive folder is configurable.
- Connection tools expose address selection, mDNS state, a listener self-check, and Windows Firewall diagnostics. Transfer events are pushed to both ends over WebSocket in real time, with no manual refresh.
- Desktop integration includes a tray icon, launch at login, and configurable close behavior: quit, hide to tray, or ask.
- The About page can check for and install releases signed with the project's updater key.

## If the phone cannot open the address

Being connected to Wi-Fi with the same name does not always mean that two devices may talk to each other. If the page times out or no approval request appears on the desktop, check these in order:

1. Do not enter `localhost` or `127.0.0.1` on the phone. They point back to the phone. Use the full address from the **Connect device** panel.
2. Guest Wi-Fi, campus and corporate networks, and routers with AP isolation can block traffic between clients. Try a normal home network or a computer hotspot.
3. Disable cloud acceleration, VPN, proxy, or data-saving features in the phone browser. They may not route private addresses such as `192.168.x.x`.
4. On Windows, use a Private network profile. If diagnostics report a missing firewall rule, LanNook can ask for confirmation before adding a rule scoped to the current executable, TCP port, and local subnet.
5. If the computer has a router connection, hotspot, VPN, or virtual adapter at the same time, select the address that shares a network with the phone. The default port is `53317`; after changing it, trust the value currently shown by the panel.

The panel's local self-check proves that the service is listening on the selected desktop address. It cannot prove that the complete phone-to-computer network path is open; the final test must come from the phone.

## Security boundaries

- New devices require desktop approval by default, but this setting can be disabled. With approval disabled, a device that has a valid connection address may connect immediately.
- One-time approval lasts for the current LAN service run. A trusted-device record persists until it is revoked or removed on the Devices page.
- The QR-code URL contains pairing data. Do not post the QR code or complete URL to an untrusted chat, website, or public display.
- The mobile path currently uses plain LAN HTTP and WebSocket. It is not protected by TLS or end-to-end encryption, so use it only on a network you trust and control.
- Official desktop builds keep device records, approvals, transfer history, and received files on the computer and do not use a public-cloud file relay by default. A custom frontend or API gateway can change that data path and must be assessed separately.
- SHA-256 currently provides a fingerprint for manual comparison; LanNook does not automatically prove that both endpoints hold identical files. It also says nothing about whether a file is safe. LanNook is not cloud storage, remote backup, content moderation, or anti-virus software.

## Run from source

You need Node.js 20.x or 22+, Rust stable, and the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform. CI uses Node.js 22. Windows normally also needs MSVC Build Tools and WebView2.

```bash
git clone https://github.com/by-fengqiao/lannook.git
cd lannook
npm ci
npm run tauri dev
```

The ordinary QR-code workflow does not need an `.env` file. `VITE_LANNOOK_API_BASE_URL` is optional and only applies when the frontend is placed behind a custom API gateway; see [.env.example](.env.example). It changes only the REST API base. The gateway must also proxy the `/ws` WebSocket path on the frontend origin.

Build installers for the current platform with:

```bash
npm run tauri build
```

## Verify a change

```bash
npm run test
npm run build

cd src-tauri
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Repository map

| Path | Contents |
| --- | --- |
| `src/` | Vue UI, routes, stores, and browser-side transfer logic |
| `src-tauri/src/` | Rust commands, LAN service, transfers, discovery, and local storage |
| `src-tauri/icons/` | Application and platform icon assets |
| `.github/workflows/` | Checks, cross-platform builds, and updater-manifest publishing |

## Contributing

Before opening an issue, search the existing [Issues](https://github.com/by-fengqiao/lannook/issues). A useful report includes the operating-system version, network setup, reproduction steps, and connection diagnostics. See the [Contribution Guide](CONTRIBUTING.en.md) for code submissions or the [中文贡献指南](CONTRIBUTING.md).

- [FAQ](docs/FAQ.en.md)
- [v26.3.0 release notes](release-notes.md)
- [v26.2.2 fix notes](docs/releases/v26.2.2.md)
- [v26.2.1 fix notes](docs/releases/v26.2.1.md)
- [v26.2.0 rename notes](docs/releases/v26.2.0.md)
- [v26.1.7 release notes](docs/releases/v26.1.7.md)
- [LYNQO to LanNook migration](docs/migrations/lynqo-to-lannook.md)
- Created and maintained by [by-fengqiao](https://github.com/by-fengqiao)

## License

Copyright (C) 2026 LanNook contributors.

LanNook is licensed under the [GNU General Public License v3.0](LICENSE) (`GPL-3.0-only`). [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md) summarizes the licenses of several major dependencies; it is not yet a complete notices bundle.
