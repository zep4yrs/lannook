param(
    [string]$BundleRoot = "src-tauri\target\release\bundle"
)

# Sign Windows installers (.exe/.msi) with Authenticode.
#
# Zero-cost policy: if WINDOWS_CERT_BASE64 / WINDOWS_CERT_PASSWORD secrets are
# configured, they are used (commercial cert). Otherwise a fresh self-signed
# CodeSigning certificate is generated on the fly so every release still
# carries an Authenticode signature (helps integrity checks; the OS will still
# show "unknown publisher" because the root is not trusted).

$ErrorActionPreference = "Stop"

$files = Get-ChildItem -Path $BundleRoot -Recurse -Include *.exe, *.msi -File
if (-not $files) {
    Write-Host "No installers found under $BundleRoot - skipping signing."
    exit 0
}

$cert = $null
if ($env:WINDOWS_CERT_BASE64) {
    $bytes = [Convert]::FromBase64String($env:WINDOWS_CERT_BASE64)
    $cert = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new(
        $bytes,
        $env:WINDOWS_CERT_PASSWORD,
        [System.Security.Cryptography.X509Certificates.X509KeyStorageFlags]::Exportable
    )
    Write-Host "Signing with configured certificate: $($cert.Subject)"
} else {
    $tempCert = New-SelfSignedCertificate `
        -Type CodeSigningCert `
        -Subject "CN=LanNook (self-signed), O=LanNook Open Source" `
        -CertStoreLocation "Cert:\CurrentUser\My" `
        -KeyExportPolicy Exportable `
        -KeyAlgorithm RSA `
        -KeyLength 2048 `
        -NotAfter (Get-Date).AddYears(3)
    $cert = Get-Item $tempCert.PSPath
    Write-Host "Using self-signed certificate (zero-cost): $($cert.Subject)"
}

foreach ($file in $files) {
    $result = Set-AuthenticodeSignature -FilePath $file.FullName -Certificate $cert -HashAlgorithm SHA256
    if ($result.Status -ne "Valid") {
        Write-Warning "Signature status for $($file.Name): $($result.Status)"
    } else {
        Write-Host "Signed: $($file.Name)"
    }
}

Write-Host "Signing complete."
