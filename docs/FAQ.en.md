# LanNook FAQ

> Applies to v26.3.0. Answers may not apply to older releases.
> Before filing an issue, read this page and search the [existing issues](https://github.com/by-fengqiao/lannook/issues).

## Connection

### The phone and the computer are on the same Wi-Fi, why can't the phone open the address?

Being on the same Wi-Fi does not always mean the devices may talk to each other. Check in order:

1. Do not enter `localhost` or `127.0.0.1` on the phone — they point back to the phone. Use the full address shown in the **Connect device** panel.
2. Guest Wi-Fi, campus and corporate networks, and routers with AP isolation can block client-to-client traffic. Try a normal home network or a computer hotspot.
3. Disable cloud acceleration, VPN, proxy, or data-saving features in the phone browser.
4. On Windows, use a Private network profile; if diagnostics report a missing firewall rule, LanNook can add one scoped to the current executable, TCP port, and local subnet after confirmation.
5. If the computer is also connected to a router, hotspot, VPN, or virtual adapter, pick the address that shares a network with the phone. The default port is `53317`.

The panel's listener self-check proves the service is listening on the selected desktop address. It cannot prove the full phone-to-computer network path is open — always verify from the phone.

### Why does the connection panel sometimes show more than one address?

The computer may be connected to a router, a hotspot, a VPN, or a virtual adapter at the same time. Pick the address that shares a network with the phone; if unsure, try each one.

### Can I share the QR code or connection address with other people?

No. The QR-code URL contains pairing parameters. Do not post the QR code or the full address in untrusted chats, web pages, or public places.

## Transfers

### A transfer was interrupted. Is my file lost?

No, and it does not restart from zero. LanNook has several layers of recovery:

- Uploads use 512 KiB chunks; a failed chunk is retried automatically with exponential backoff.
- If the network drops, the upload resumes from its chunk checkpoint automatically (up to 2 retries).
- A failed upload or download can be continued directly from its completed chunks.
- After a computer or service restart, in-flight transfers switch to a resumable state for cross-session continuation.

### How do I verify that a received file is unchanged?

After a transfer completes, LanNook computes the file's SHA-256 digest in the background (large files show live progress). The desktop Transfer Center lets you compare the full checksum and a short fingerprint for the first file.

Note: the fingerprint exists for manual comparison. LanNook does not automatically prove that sender and receiver hold identical bytes, and a checksum cannot tell you whether a file is safe.

### Can two phones transfer directly to each other?

Not peer-to-peer. When one phone sends to another, the file is uploaded to the computer running LanNook and then downloaded by the target phone.

### Transfers are slow. What can I do?

- Check whether a download speed limit is set (Settings → Download speed limit; `0` means unlimited).
- 5 GHz Wi-Fi is usually faster and more stable than 2.4 GHz.
- Weak signal or being far from the router slows transfers significantly.
- If the computer has several network connections, confirm the phone is on the same network as the computer.

## Security and privacy

### Do files pass through the cloud?

No. Files travel on the LAN directly. Device records, approvals, transfer history, and received files stay on your computer (a local SQLite database). Official builds use no public cloud relay.

### Is the transfer encrypted?

The current mobile connection uses plaintext LAN HTTP/WebSocket — there is no TLS or end-to-end encryption. Use it only on networks you trust and where no unknown devices are connected.

### Why does the installer show an "unknown publisher" warning?

Official packages are not yet protected by operating-system code signing. The warning is expected; download only from this repository's Release page. The `.sig` files attached to a Release authenticate application updates — they are not OS code signatures. To add a zero-cost Authenticode signature to manually distributed Windows installers, see [docs/signing.md](docs/signing.md).

### If someone gets the QR-code address, can they connect?

New devices require desktop approval by default. Even with the address, a device shows an access request on the desktop, and you decide. You can disable approval in Settings, but then any device with a valid address can connect immediately — not recommended on untrusted networks.

## Usage

### Does the phone need an app or an account?

No. Phones and tablets use a modern browser to open the QR-code address. PWA support lets iOS and Android users add the page to the home screen for an app-like experience. No account is needed.

### What is PIN pairing?

If you cannot scan the QR code, enter the 6-digit PIN shown on the desktop panel on the phone's connection page. The PIN is single-use, expires after 5 minutes, and locks out after repeated wrong entries (brute-force protection).

### How do approval durations work?

- On the Devices page: when a new device requests access, choose a duration (until the service closes / 1 hour / 24 hours / 7 days). Approval is revoked automatically at expiry.
- In Settings: configure a default approval duration.

### How do I update LanNook?

Check for updates on the About page; the app installs releases signed with the project's updater key. You can also download installers from the [Releases page](https://github.com/by-fengqiao/lannook/releases/latest).

### Which platforms are supported?

- Desktop: Windows 10/11 x64 (`exe`/`msi`), macOS 10.15+ (`dmg` for Apple silicon and Intel), Linux x64 (`AppImage`/`deb`).
- Phones: any modern browser; iOS Safari and Android Chrome work best.

### How do I export diagnostics?

Use the export-diagnostics entry under Settings/About (includes version and logs) and attach it when filing an issue.

## Feedback

### What should a bug report include?

Use the [issue templates](https://github.com/by-fengqiao/lannook/issues/new/choose): app version, desktop OS, phone model and browser, network setup, reproduction steps, and the actual behavior. For connection problems, attach the output of the connection diagnostics panel.