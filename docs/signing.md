# 代码签名与公证（Windows Authenticode / macOS notarization）

LanNook 当前发行包尚未进行操作系统级代码签名，Windows 会提示“未知发布者”，
macOS 会提示无法验证开发者。以下是启用签名所需的外部资源与 CI 接入点。

## 为什么需要外部资源

代码签名需要证书/开发者账号，这是无法从代码库内自行生成的：

| 平台 | 需要的资源 | 用途 |
| --- | --- | --- |
| Windows | OV/EV 代码签名证书（.pfx，或 Azure Trusted Signing） | 对 nsis/msi 与 exe 做 Authenticode 签名，消除“未知发布者” |
| macOS | Apple Developer 账号（99 USD/年）+ Developer ID Application 证书 | 对 .app/.dmg 签名并提交公证（notarization） |
| 两者 | 相应私钥托管在 GitHub Actions Secrets 中 | CI 构建时注入签名 |

## GitHub Actions Secrets 准备

在仓库 Settings → Secrets and variables → Actions 中添加：

- `WINDOWS_CERT_BASE64`：Windows 证书（.pfx）的 Base64 编码
- `WINDOWS_CERT_PASSWORD`：证书密码
- `APPLE_CERT_BASE64`：Developer ID Application 证书（.p12）Base64
- `APPLE_CERT_PASSWORD`：.p12 密码
- `APPLE_NOTARY_KEY_ID` / `APPLE_NOTARY_ISSUER` / `APPLE_NOTARY_PRIVATE_KEY`：App Store Connect API Key（用于 notarytool 公证）

（现有 TAURI_SIGNING_PRIVATE_KEY / TAURI_SIGNING_PRIVATE_KEY_PASSWORD 是应用内更新签名，
与操作系统代码签名是两套独立凭据，前者已启用。）

## Windows Authenticode 接入点

在 .github/workflows/release-updater.yml 的 Windows 步骤中，于 npx tauri build 之后添加：

```yaml
      - name: Sign Windows installers (Authenticode)
        if: runner.os == "Windows" && env.WINDOWS_CERT_BASE64 != ""
        shell: bash
        env:
          WINDOWS_CERT_BASE64: ${{ secrets.WINDOWS_CERT_BASE64 }}
          WINDOWS_CERT_PASSWORD: ${{ secrets.WINDOWS_CERT_PASSWORD }}
        run: |
          echo "$WINDOWS_CERT_BASE64" | base64 -d > cert.pfx
          powershell -Command "Get-ChildItem -Recurse -Include *.exe,*.msi src-tauri/target/release/bundle | ForEach-Object { Set-AuthenticodeSignature -FilePath $_.FullName -Certificate (New-Object System.Security.Cryptography.X509Certificates.X509Certificate2 -ArgumentList cert.pfx, \"$WINDOWS_CERT_PASSWORD\") -HashAlgorithm SHA256 }"
          rm cert.pfx
```

## macOS 公证接入点

在 macOS 步骤的 npx tauri build 之后：

```yaml
      - name: Sign and notarize macOS app
        if: runner.os == "macOS" && env.APPLE_CERT_BASE64 != ""
        shell: bash
        env:
          APPLE_CERT_BASE64: ${{ secrets.APPLE_CERT_BASE64 }}
          APPLE_CERT_PASSWORD: ${{ secrets.APPLE_CERT_PASSWORD }}
          APPLE_NOTARY_KEY_ID: ${{ secrets.APPLE_NOTARY_KEY_ID }}
          APPLE_NOTARY_ISSUER: ${{ secrets.APPLE_NOTARY_ISSUER }}
          APPLE_NOTARY_PRIVATE_KEY: ${{ secrets.APPLE_NOTARY_PRIVATE_KEY }}
        run: |
          echo "$APPLE_CERT_BASE64" | base64 -d > cert.p12
          APP_BUNDLE=$(find src-tauri/target -path "*release/bundle/macos/*.app" | head -1)
          codesign --force --options runtime --sign "Developer ID Application" "$APP_BUNDLE"
          xcrun notarytool submit "$APP_BUNDLE" --key-id "$APPLE_NOTARY_KEY_ID" --issuer "$APPLE_NOTARY_ISSUER" --key "$APPLE_NOTARY_PRIVATE_KEY" --wait
          rm cert.p12
```

## 验证

- Windows：Get-AuthenticodeSignature <exe> 应显示 Valid。
- macOS：spctl --assess --type execute -v <app> 应通过；Gatekeeper 不再拦截。

## 注意

- 签名与公证应在每个发布 tag 上运行（release-updater.yml 已按 tag 触发）。
- macOS 公证后还可使用 stapler 将票据钉进 dmg（可选）。
- 证书私钥务必只存在于 Secrets 中，切勿提交到仓库。

## 零成本自签名方案（开源项目默认）

不需要购买任何证书。仓库内置了 `scripts/sign-windows.ps1`：

```powershell
# 在 Windows 上、`npx tauri build` 之后运行：
pwsh ./scripts/sign-windows.ps1
```

脚本行为：

- 若配置了 `WINDOWS_CERT_BASE64` / `WINDOWS_CERT_PASSWORD`（商业证书），用它签名；
- 否则**自动生成一次性的自签名 CodeSigning 证书**并签名全部 `.exe` / `.msi`。
- 签名后可用 `Get-AuthenticodeSignature <exe>` 查看：状态为 `Valid`，但发布者为
  “LanNook (self-signed)”——系统仍会提示“未知发布者”，因为该根证书未被系统信任。
  自签名的价值在于**防止安装包在传输/分发中被篡改**（可校验完整性），
  而不是消除系统信任警告。

## 自签名与自动更新的约束（重要）

LanNook 的自动更新使用 `TAURI_SIGNING_PRIVATE_KEY` 对安装包做 Ed25519 签名（`.sig`），
校验文件是 `latest.json` 中的 `signature`。**Authenticode 签名会改变安装包字节，
从而使 `.sig` 校验失效**。因此：

- 走自动更新链路（`release-updater.yml` 的 tauri-action）的产物**不要**再做 Authenticode 签名；
- 手工分发/网站下载的安装包，可以在 `npx tauri build` 之后运行
  `scripts/sign-windows.ps1` 补上 Authenticode 签名（两种签名互不冲突，
  只需注意：签名后再改动文件会让已有 `.sig` 失效，需用 `tauri signer sign` 重新生成）。

## macOS / Linux 的自签名说明

- macOS：`codesign --force --deep -s -` 可做 ad-hoc 签名（零成本），但公证（notarization）
  必须依赖 Apple 开发者账号，无法绕过；Gatekeeper 仍会拦截。
- Linux：`.deb` / `.AppImage` 自带校验（AppImage 有内嵌摘要），一般无需额外签名。
