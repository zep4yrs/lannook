use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use crate::server::{self, ServiceStatus, SharedState};

#[cfg(target_os = "windows")]
const WINDOWS_FIREWALL_RULE_NAME: &str = "LanNook LAN File Transfer";

#[cfg(target_os = "windows")]
const LEGACY_WINDOWS_FIREWALL_RULE_NAME: &str = "LYNQO LAN File Transfer";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    pub status: String,
    pub port: u16,
    pub local_ip: Option<String>,
    pub local_url: Option<String>,
    pub started_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub ip: String,
    pub port: u16,
    pub token: String,
    pub control_token: String,
    pub local_domain: String,
    pub qr_url: String,
    pub network_name: String,
    pub receive_folder: String,
    pub device_name: String,
    pub addresses: Vec<ConnectionAddress>,
    /// Active 6-digit pairing code (None when unset or expired).
    pub pin: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionAddress {
    pub ip: String,
    pub interface_name: String,
    pub kind: String,
    pub url: String,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDiagnostics {
    pub service_status: String,
    pub configured_port: u16,
    pub listening_port: u16,
    pub bind_address: String,
    pub local_ip: Option<String>,
    pub local_url: Option<String>,
    pub qr_url: Option<String>,
    pub ip_is_private: Option<bool>,
    pub loopback_reachable: Option<bool>,
    pub lan_address_reachable: Option<bool>,
    pub mdns_advertised: bool,
    pub firewall_status: String,
    pub connected_device_count: usize,
    pub interfaces: Vec<crate::server::LanInterface>,
    pub warnings: Vec<String>,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResult {
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QrCodeData {
    pub url: String,
    /// SVG string of the QR code
    pub svg: String,
}

/// Frontend settings are saved incrementally, so accepting a patch prevents a
/// UI change from accidentally resetting fields that are not shown in the UI.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsPatch {
    device_name: Option<String>,
    receive_folder: Option<String>,
    require_approval: Option<bool>,
    auto_approve_known: Option<bool>,
    port: Option<u16>,
    max_file_size: Option<i64>,
    theme_mode: Option<String>,
    authorization_expiry_hours: Option<i64>,
    download_speed_limit_mbps: Option<i64>,
}

fn timestamp_to_iso(value: &str) -> String {
    value
        .parse::<i64>()
        .ok()
        .and_then(|seconds| chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, 0))
        .map(|timestamp| timestamp.to_rfc3339())
        .unwrap_or_else(|| value.to_string())
}

fn connection_url(ip: &str, port: u16, token: &str) -> String {
    format!("http://{ip}:{port}/mobile?token={token}")
}

#[cfg(target_os = "windows")]
fn powershell_literal(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(target_os = "windows")]
fn inspect_windows_firewall(port: u16) -> String {
    use std::os::windows::process::CommandExt;

    // `powershell.exe` is only used to query a firewall rule. Explicitly
    // suppress its console window so opening a connection address never looks
    // like LanNook is launching a terminal.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let executable = match std::env::current_exe() {
        Ok(path) => powershell_literal(&path.to_string_lossy()),
        Err(_) => return "unknown".to_string(),
    };
    let rule_name = powershell_literal(WINDOWS_FIREWALL_RULE_NAME);
    let legacy_rule_name = powershell_literal(LEGACY_WINDOWS_FIREWALL_RULE_NAME);
    let script = format!(
        r#"$ErrorActionPreference = 'Stop'
$activeProfiles = @(Get-NetFirewallProfile | Where-Object {{ $_.Enabled }})
if ($activeProfiles.Count -eq 0) {{ Write-Output 'disabled'; exit 0 }}
$target = '{executable}'
$rules = @(Get-NetFirewallRule -ErrorAction SilentlyContinue | Where-Object {{ ($_.DisplayName -eq '{rule_name}' -or $_.DisplayName -eq '{legacy_rule_name}') -and $_.Enabled -eq 'True' -and $_.Direction -eq 'Inbound' -and $_.Action -eq 'Allow' }})
foreach ($rule in $rules) {{
  $application = Get-NetFirewallApplicationFilter -AssociatedNetFirewallRule $rule
  $portFilter = Get-NetFirewallPortFilter -AssociatedNetFirewallRule $rule
  $programMatches = $application.Program -eq 'Any' -or $application.Program -ieq $target
  $portMatches = $portFilter.LocalPort -eq 'Any' -or $portFilter.LocalPort -eq '{port}'
  $protocolMatches = $portFilter.Protocol -eq 'TCP' -or $portFilter.Protocol -eq 6
  if ($programMatches -and $portMatches -and $protocolMatches) {{ Write-Output 'allowed'; exit 0 }}
}}
Write-Output 'missing'
"#
    );

    match std::process::Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
    {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout
                .lines()
                .rev()
                .map(str::trim)
                .find(|line| matches!(*line, "allowed" | "disabled" | "missing"))
                .unwrap_or("unknown")
                .to_string()
        }
        Ok(output) => {
            tracing::warn!(
                "Windows firewall inspection failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
            "unknown".to_string()
        }
        Err(error) => {
            tracing::warn!(
                "Could not start PowerShell for firewall inspection: {}",
                error
            );
            "unknown".to_string()
        }
    }
}

async fn system_firewall_status(port: u16) -> String {
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(move || inspect_windows_firewall(port))
            .await
            .unwrap_or_else(|_| "unknown".to_string())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = port;
        "unsupported".to_string()
    }
}

#[cfg(target_os = "windows")]
fn configure_windows_firewall_rule(port: u16) -> Result<(), String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE},
        UI::{
            Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW},
            WindowsAndMessaging::SW_HIDE,
        },
    };

    fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
        value.as_ref().encode_wide().chain(Some(0)).collect()
    }

    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let rule_name = powershell_literal(WINDOWS_FIREWALL_RULE_NAME);
    let executable = powershell_literal(&executable.to_string_lossy());
    let elevated_script = format!(
        r#"$ErrorActionPreference = 'Stop'
$netsh = Join-Path $env:SystemRoot 'System32\netsh.exe'
$target = '{executable}'
& $netsh advfirewall firewall delete rule "name={rule_name}" | Out-Null
& $netsh advfirewall firewall add rule "name={rule_name}" "dir=in" "action=allow" ('program=' + $target) "protocol=TCP" "localport={port}" "remoteip=LocalSubnet" "profile=any" "enable=yes" | Out-Null
if ($LASTEXITCODE -ne 0) {{ exit $LASTEXITCODE }}
"#
    );
    let encoded_bytes: Vec<u8> = elevated_script
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let parameters = format!(
        "-NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {}",
        STANDARD.encode(encoded_bytes)
    );
    let system_root = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| "SystemRoot is unavailable".to_string())?;
    let powershell = system_root
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    let verb = wide("runas");
    let executable = wide(powershell.as_os_str());
    let parameters = wide(parameters);

    let mut execute_info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: verb.as_ptr(),
        lpFile: executable.as_ptr(),
        lpParameters: parameters.as_ptr(),
        lpDirectory: ptr::null(),
        nShow: SW_HIDE,
        ..Default::default()
    };
    if unsafe { ShellExecuteExW(&mut execute_info) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    if execute_info.hProcess.is_null() {
        return Err("The elevated firewall process did not start".to_string());
    }

    let wait_result = unsafe { WaitForSingleObject(execute_info.hProcess, INFINITE) };
    if wait_result != WAIT_OBJECT_0 {
        unsafe { CloseHandle(execute_info.hProcess) };
        return Err("Waiting for the elevated firewall process failed".to_string());
    }
    let mut exit_code = 1u32;
    let exit_code_read = unsafe { GetExitCodeProcess(execute_info.hProcess, &mut exit_code) };
    unsafe { CloseHandle(execute_info.hProcess) };
    if exit_code_read == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    if exit_code == 0 {
        Ok(())
    } else {
        Err(format!(
            "The elevated firewall command exited with code {exit_code}"
        ))
    }
}

/// Refresh the selected adapter from the operating system. When a network
/// switch changes the advertised endpoint, update mDNS in the same operation
/// so the QR code and discovery record cannot drift apart.
pub(crate) async fn synchronize_network_state(state: &SharedState) -> bool {
    let selected = server::get_selected_interface();
    let (changed, should_register, ip, port, device_name, generation) = {
        let mut s = state.lock().await;
        let next_ip = selected.as_ref().map(|interface| interface.ip.clone());
        let next_name = selected
            .as_ref()
            .map(|interface| interface.name.clone())
            .unwrap_or_else(|| "Local Network".to_string());
        let changed = s.local_ip != next_ip || s.network_name != next_name;

        if changed {
            tracing::info!(
                previous_ip = ?s.local_ip,
                next_ip = ?next_ip,
                interface = %next_name,
                "Active LAN endpoint changed"
            );
            s.local_ip = next_ip.clone();
            s.network_name = next_name;
            s.mdns_guard = None;
            s.network_generation = s.network_generation.wrapping_add(1);
        }

        let should_register = s.status == ServiceStatus::Running
            && next_ip.is_some()
            && s.mdns_guard.is_none()
            && s.mdns_refresh_generation.is_none();
        if should_register {
            s.mdns_refresh_generation = Some(s.network_generation);
        }

        (
            changed,
            should_register,
            next_ip,
            s.port,
            s.device_name.clone(),
            s.network_generation,
        )
    };

    if should_register {
        if let Some(ip) = ip {
            match crate::discovery::MdnsGuard::register(&ip, port, &device_name) {
                Ok(guard) => {
                    let mut s = state.lock().await;
                    let request_is_current = s.mdns_refresh_generation == Some(generation);
                    if request_is_current {
                        s.mdns_refresh_generation = None;
                    }
                    if request_is_current
                        && s.status == ServiceStatus::Running
                        && s.local_ip.as_deref() == Some(&ip)
                        && s.device_name == device_name
                        && s.network_generation == generation
                    {
                        s.mdns_guard = Some(guard);
                    }
                }
                Err(error) => {
                    let mut s = state.lock().await;
                    if s.mdns_refresh_generation == Some(generation) {
                        s.mdns_refresh_generation = None;
                    }
                    tracing::warn!("mDNS refresh failed: {}", error);
                }
            }
        }
    }
    changed
}

#[tauri::command]
pub async fn start_local_service(
    state: State<'_, SharedState>,
    app: tauri::AppHandle,
) -> Result<CommandResult, String> {
    let shared = state.inner().clone();

    {
        let mut s = shared.lock().await;
        if s.status == ServiceStatus::Running {
            return Ok(CommandResult {
                success: true,
                error: None,
            });
        }
        if s.status == ServiceStatus::Starting {
            return Ok(CommandResult {
                success: false,
                error: Some("Service is already starting".to_string()),
            });
        }
        if s.status == ServiceStatus::Stopping {
            return Ok(CommandResult {
                success: false,
                error: Some("Service is still stopping".to_string()),
            });
        }

        let settings = s.db.get_settings().map_err(|error| error.to_string())?;
        s.port = settings.port;
        s.device_name = settings.device_name;
        s.receive_folder = settings.receive_folder;
        let selected_interface = server::get_selected_interface();
        s.local_ip = selected_interface
            .as_ref()
            .map(|interface| interface.ip.clone());
        s.network_name = selected_interface
            .map(|interface| interface.name)
            .unwrap_or_else(|| "Local Network".to_string());
        s.status = ServiceStatus::Starting;
        s.error = None;
        s.mdns_guard = None;
    }

    // Determine frontend directory with multiple fallback paths
    let serve_dir = {
        let candidates = [
            app.path().resource_dir().ok().map(|p| p.join("dist")),
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|pp| pp.join("../dist"))),
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .map(|p| p.join("dist")),
        ];
        candidates
            .into_iter()
            .flatten()
            .find(|p| p.exists())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "../dist".to_string())
    };

    // Start server in a background task
    let server_state = shared.clone();
    tokio::spawn(async move {
        if let Err(e) = server::start_server(server_state, serve_dir).await {
            tracing::error!("Server failed: {}", e);
        }
    });

    // Wait for the authoritative state transition instead of assuming that a
    // fixed delay means the socket has bound successfully.
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
    loop {
        let status = {
            let s = shared.lock().await;
            (s.status.clone(), s.error.clone())
        };
        match status.0 {
            ServiceStatus::Running => break,
            ServiceStatus::Failed => {
                return Ok(CommandResult {
                    success: false,
                    error: status.1.or(Some("Failed to start server".to_string())),
                });
            }
            _ if tokio::time::Instant::now() >= deadline => {
                let mut s = shared.lock().await;
                let message = "Timed out while waiting for the local service to start".to_string();
                s.status = ServiceStatus::Failed;
                s.error = Some(message.clone());
                return Ok(CommandResult {
                    success: false,
                    error: Some(message),
                });
            }
            _ => tokio::time::sleep(tokio::time::Duration::from_millis(50)).await,
        }
    }

    // Advertise only after Axum has successfully bound the real endpoint.
    // The centralized synchronizer also retries a failed advertisement on a
    // later monitor tick without creating duplicate concurrent registrations.
    synchronize_network_state(&shared).await;

    Ok(CommandResult {
        success: true,
        error: None,
    })
}

#[tauri::command]
pub async fn stop_local_service(state: State<'_, SharedState>) -> Result<CommandResult, String> {
    let shared = state.inner().clone();

    // Drop the mDNS guard to unregister the service from the LAN.
    {
        let mut s = shared.lock().await;
        s.network_generation = s.network_generation.wrapping_add(1);
        s.mdns_refresh_generation = None;
        if let Some(guard) = s.mdns_guard.take() {
            drop(guard);
        }
    }

    match server::stop_server(shared.clone()).await {
        Ok(()) => {
            let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
            loop {
                let status = { shared.lock().await.status.clone() };
                if matches!(status, ServiceStatus::Stopped | ServiceStatus::Failed) {
                    break;
                }
                if tokio::time::Instant::now() >= deadline {
                    return Ok(CommandResult {
                        success: false,
                        error: Some(
                            "Timed out while waiting for the local service to stop".to_string(),
                        ),
                    });
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }

            let revoked = {
                let s = shared.lock().await;
                s.db.revoke_untrusted_device_access()
                    .map_err(|error| error.to_string())?
            };
            let s = shared.lock().await;
            for device_id in revoked {
                let _ = s
                    .event_tx
                    .send(crate::server::WsEvent::DeviceRejected { device_id });
            }
            Ok(CommandResult {
                success: true,
                error: None,
            })
        }
        Err(e) => Ok(CommandResult {
            success: false,
            error: Some(e),
        }),
    }
}

#[tauri::command]
pub async fn get_local_service_status(
    state: State<'_, SharedState>,
) -> Result<ServiceInfo, String> {
    let s = state.lock().await;
    Ok(ServiceInfo {
        status: s.status.to_string(),
        port: s.port,
        local_ip: s.local_ip.clone(),
        local_url: s.local_url(),
        started_at: s.started_at.clone(),
        error: s.error.clone(),
    })
}

#[tauri::command]
pub async fn refresh_local_ip(state: State<'_, SharedState>) -> Result<String, String> {
    synchronize_network_state(state.inner()).await;
    state
        .lock()
        .await
        .local_ip
        .clone()
        .ok_or_else(|| "No local IP available".to_string())
}

#[tauri::command]
pub async fn regenerate_connection_token(state: State<'_, SharedState>) -> Result<String, String> {
    // Pairing credentials are intentionally not advertised through mDNS, so
    // rotating the token must not tear down or recreate the network record.
    let (new_token, revoked_devices) = {
        let mut s = state.lock().await;
        s.connection_token = uuid::Uuid::new_v4().to_string();
        let revoked_devices =
            s.db.revoke_untrusted_device_access()
                .map_err(|error| error.to_string())?;
        (s.connection_token.clone(), revoked_devices)
    };

    {
        let s = state.lock().await;
        for device_id in revoked_devices {
            let _ = s
                .event_tx
                .send(crate::server::WsEvent::DeviceRejected { device_id });
        }
    }

    Ok(new_token)
}

/// Generate a fresh 6-digit pairing code for browsers that cannot scan the
/// QR code. Replaces any previous code and expires after 5 minutes.
#[tauri::command]
pub async fn refresh_pairing_pin(state: State<'_, SharedState>) -> Result<String, String> {
    let mut s = state.lock().await;
    let code = crate::server::generate_pairing_code();
    let expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64 + 5 * 60)
        .unwrap_or(0);
    s.pairing_pin = Some(crate::server::PairingPin {
        code: code.clone(),
        expires_at,
    });
    s.pairing_attempts.clear();
    tracing::info!("Pairing PIN refreshed (valid for 5 minutes)");
    Ok(code)
}

#[tauri::command]
pub async fn get_connection_info(state: State<'_, SharedState>) -> Result<ConnectionInfo, String> {
    synchronize_network_state(state.inner()).await;
    let interfaces = server::get_network_interfaces();
    let s = state.lock().await;
    let ip = s
        .local_ip
        .clone()
        .ok_or_else(|| "No local IP available".to_string())?;

    let local_domain = format!(
        "{}.local",
        crate::discovery::safe_host_label(&s.device_name)
    );
    let addresses = interfaces
        .into_iter()
        .filter(|interface| interface.is_private && !interface.is_virtual)
        .map(|interface| ConnectionAddress {
            url: connection_url(&interface.ip, s.port, &s.connection_token),
            ip: interface.ip,
            interface_name: interface.name,
            kind: interface.kind,
            selected: interface.selected,
        })
        .collect();

    let pin = s
        .pairing_pin
        .as_ref()
        .filter(|pairing| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(i64::MAX);
            now <= pairing.expires_at
        })
        .map(|pairing| pairing.code.clone());

    Ok(ConnectionInfo {
        ip: ip.clone(),
        port: s.port,
        token: s.connection_token.clone(),
        control_token: s.desktop_control_token.clone(),
        local_domain,
        qr_url: s.qr_url().unwrap_or_default(),
        network_name: s.network_name.clone(),
        receive_folder: s.receive_folder.clone(),
        device_name: s.device_name.clone(),
        addresses,
        pin,
    })
}

async fn can_connect(ip: &str, port: u16) -> bool {
    let address = format!("{ip}:{port}");
    matches!(
        tokio::time::timeout(
            tokio::time::Duration::from_millis(750),
            tokio::net::TcpStream::connect(address),
        )
        .await,
        Ok(Ok(_))
    )
}

#[tauri::command]
pub async fn get_connection_diagnostics(
    state: State<'_, SharedState>,
    ip: Option<String>,
) -> Result<ConnectionDiagnostics, String> {
    synchronize_network_state(state.inner()).await;
    let (
        service_status,
        listening_port,
        primary_ip,
        connection_token,
        mdns_advertised,
        connected_device_count,
        configured_port,
    ) = {
        let s = state.lock().await;
        let configured_port =
            s.db.get_settings()
                .map(|settings| settings.port)
                .unwrap_or(s.port);
        (
            s.status.to_string(),
            s.port,
            s.local_ip.clone(),
            s.connection_token.clone(),
            s.mdns_guard.is_some(),
            s.connected_devices.len(),
            configured_port,
        )
    };

    let mut interfaces = server::get_network_interfaces();
    let requested_address_is_active = ip.as_ref().is_none_or(|requested_ip| {
        interfaces.iter().any(|interface| {
            interface.is_private && !interface.is_virtual && &interface.ip == requested_ip
        })
    });
    let local_ip = ip.filter(|_| requested_address_is_active).or(primary_ip);
    for interface in &mut interfaces {
        interface.selected = local_ip.as_deref() == Some(interface.ip.as_str());
    }
    let local_url = local_ip
        .as_ref()
        .map(|ip| format!("http://{ip}:{listening_port}"));
    let qr_url = local_ip
        .as_ref()
        .map(|ip| connection_url(ip, listening_port, &connection_token));
    let running = service_status == "running";
    let loopback_reachable = if running {
        Some(can_connect("127.0.0.1", listening_port).await)
    } else {
        None
    };
    let lan_address_reachable = if running {
        match local_ip.as_deref() {
            Some(ip) => Some(can_connect(ip, listening_port).await),
            None => None,
        }
    } else {
        None
    };
    let ip_is_private = local_ip
        .as_deref()
        .and_then(|ip| ip.parse::<std::net::Ipv4Addr>().ok())
        .map(|ip| ip.is_private());
    let firewall_status = system_firewall_status(listening_port).await;

    let mut warnings = Vec::new();
    if !running {
        warnings.push("service_not_running".to_string());
    }
    if local_ip.is_none() {
        warnings.push("no_lan_address".to_string());
    }
    if !requested_address_is_active {
        warnings.push("selected_address_inactive".to_string());
    }
    if ip_is_private == Some(false) {
        warnings.push("selected_address_not_private".to_string());
    }
    if loopback_reachable == Some(false) {
        warnings.push("loopback_unreachable".to_string());
    }
    if lan_address_reachable == Some(false) {
        warnings.push("lan_address_unreachable".to_string());
    }
    if configured_port != listening_port {
        warnings.push("restart_required_for_port".to_string());
    }
    if interfaces
        .iter()
        .filter(|interface| interface.is_private && !interface.is_virtual)
        .count()
        > 1
    {
        warnings.push("multiple_private_interfaces".to_string());
    }
    if interfaces
        .iter()
        .any(|interface| interface.selected && interface.is_virtual)
    {
        warnings.push("virtual_adapter_selected".to_string());
    }
    if firewall_status == "missing" {
        warnings.push("firewall_rule_missing".to_string());
    }

    Ok(ConnectionDiagnostics {
        service_status,
        configured_port,
        listening_port,
        bind_address: format!("0.0.0.0:{listening_port}"),
        local_ip,
        local_url,
        qr_url,
        ip_is_private,
        loopback_reachable,
        lan_address_reachable,
        mdns_advertised,
        firewall_status,
        connected_device_count,
        interfaces,
        warnings,
        checked_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[tauri::command]
pub async fn configure_windows_firewall(
    state: State<'_, SharedState>,
) -> Result<CommandResult, String> {
    let port = state.lock().await.port;

    #[cfg(target_os = "windows")]
    {
        match tokio::task::spawn_blocking(move || configure_windows_firewall_rule(port)).await {
            Ok(Ok(())) => Ok(CommandResult {
                success: true,
                error: None,
            }),
            Ok(Err(error)) => Ok(CommandResult {
                success: false,
                error: Some(error),
            }),
            Err(error) => Ok(CommandResult {
                success: false,
                error: Some(error.to_string()),
            }),
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = port;
        Ok(CommandResult {
            success: false,
            error: Some(
                "Automatic firewall configuration is only available on Windows".to_string(),
            ),
        })
    }
}

#[tauri::command]
pub async fn get_connection_qr_code(
    state: State<'_, SharedState>,
    ip: Option<String>,
) -> Result<QrCodeData, String> {
    synchronize_network_state(state.inner()).await;
    let interfaces = server::get_network_interfaces();
    let s = state.lock().await;
    let selected_ip = match ip {
        Some(requested_ip)
            if interfaces.iter().any(|interface| {
                interface.is_private && !interface.is_virtual && interface.ip == requested_ip
            }) =>
        {
            requested_ip
        }
        Some(_) => return Err("The selected network address is no longer active".to_string()),
        None => s
            .local_ip
            .clone()
            .ok_or_else(|| "No local IP available for QR code".to_string())?,
    };
    let url = connection_url(&selected_ip, s.port, &s.connection_token);

    // Generate QR code as SVG manually
    use qrcode::QrCode;
    let code = QrCode::new(url.as_bytes()).map_err(|e| format!("QR generation failed: {}", e))?;

    let colors = code.to_colors();
    let width = code.width();
    let scale = 4;
    let size = width * scale;

    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {size} {size}" width="{size}" height="{size}">"#,
    );
    svg.push_str(&format!(
        r#"<rect width="{size}" height="{size}" fill="white"/>"#
    ));

    for y in 0..width {
        for x in 0..width {
            if colors[y * width + x] == qrcode::Color::Dark {
                let px = x * scale;
                let py = y * scale;
                svg.push_str(&format!(
                    r#"<rect x="{px}" y="{py}" width="{scale}" height="{scale}" fill="black"/>"#
                ));
            }
        }
    }
    svg.push_str("</svg>");

    Ok(QrCodeData { url, svg })
}

#[tauri::command]
pub async fn get_devices(state: State<'_, SharedState>) -> Result<String, String> {
    let s = state.lock().await;
    let devices = s.db.list_visible_devices().map_err(|e| e.to_string())?;
    let connected = &s.connected_devices;
    let payload: Vec<serde_json::Value> = devices
        .into_iter()
        .filter(|device| device.id != "desktop")
        .map(|device| {
            let online = connected.contains_key(&device.id);
            serde_json::json!({
                "id": device.id,
                "name": device.name,
                "platform": device.platform,
                "deviceType": device.device_type,
                "ip": device.ip,
                "approved": device.approved,
                "trusted": device.trusted,
                "approvedUntil": device.approved_until.as_deref().map(timestamp_to_iso),
                "online": online,
                "lastSeenAt": timestamp_to_iso(&device.last_seen),
            })
        })
        .collect();
    serde_json::to_string(&payload).map_err(|e| e.to_string())
}

#[tauri::command]
/// Positive hours bound the approval; `0` means one-time access until the
/// service stops; a negative value means permanent. `None` falls back to
/// the configured default (`authorizationExpiryHours` setting).
pub async fn approve_device(
    state: State<'_, SharedState>,
    device_id: String,
    trusted: bool,
    expiry_hours: Option<i64>,
) -> Result<CommandResult, String> {
    let s = state.lock().await;
    let default_hours =
        s.db.get_settings()
            .map(|settings| settings.authorization_expiry_hours)
            .unwrap_or(0);

    // Trusted devices are permanent by definition. Everything else follows
    // the requested (or default) expiry policy.
    let (effective_trusted, approved_until) = if trusted {
        (true, None)
    } else {
        let hours = expiry_hours.unwrap_or(default_hours);
        if hours < 0 {
            // Permanent authorization without the "trusted device" label.
            (true, None)
        } else if hours == 0 {
            // One-time access: valid for this service lifetime only.
            (false, None)
        } else {
            let now = chrono::Utc::now().timestamp();
            (false, Some((now + hours * 3600).to_string()))
        }
    };

    match s
        .db
        .set_device_access_with_expiry(&device_id, true, effective_trusted, approved_until)
    {
        Ok(()) => {
            let _ = s.event_tx.send(crate::server::WsEvent::DeviceApproved {
                device_id: device_id.clone(),
            });
            Ok(CommandResult {
                success: true,
                error: None,
            })
        }
        Err(e) => Ok(CommandResult {
            success: false,
            error: Some(e.to_string()),
        }),
    }
}

#[tauri::command]
pub async fn reject_device(
    state: State<'_, SharedState>,
    device_id: String,
) -> Result<CommandResult, String> {
    let s = state.lock().await;
    match s.db.set_device_access(&device_id, false, false) {
        Ok(()) => {
            let _ = s.event_tx.send(crate::server::WsEvent::DeviceRejected {
                device_id: device_id.clone(),
            });
            Ok(CommandResult {
                success: true,
                error: None,
            })
        }
        Err(e) => Ok(CommandResult {
            success: false,
            error: Some(e.to_string()),
        }),
    }
}

#[tauri::command]
pub async fn forget_device(
    state: State<'_, SharedState>,
    device_id: String,
) -> Result<CommandResult, String> {
    let mut s = state.lock().await;
    match s.db.forget_device(&device_id) {
        Ok(()) => {
            s.connected_devices.remove(&device_id);
            let _ = s.event_tx.send(crate::server::WsEvent::DeviceRejected {
                device_id: device_id.clone(),
            });
            let _ = s
                .event_tx
                .send(crate::server::WsEvent::DeviceDisconnected { device_id });
            Ok(CommandResult {
                success: true,
                error: None,
            })
        }
        Err(e) => Ok(CommandResult {
            success: false,
            error: Some(e.to_string()),
        }),
    }
}

#[tauri::command]
pub async fn get_transfers(state: State<'_, SharedState>) -> Result<String, String> {
    let s = state.lock().await;
    let transfers = s.db.list_transfers().map_err(|e| e.to_string())?;

    let mut transfer_list = Vec::new();
    for t in &transfers {
        let files = s.db.get_transfer_files(&t.id).map_err(|e| e.to_string())?;
        let file_infos: Vec<serde_json::Value> = files
            .iter()
            .map(|f| {
                serde_json::json!({
                    "id": f.id.clone(),
                    "name": f.name.clone(),
                    "size": f.size,
                    "mimeType": f.mime_type.clone(),
                    "checksum": f.sha256.clone(),
                })
            })
            .collect();

        let (direction, source_device_id, target_device_id) = match t.direction.as_str() {
            // The desktop receives a mobile upload in this flow.
            "receive" => ("upload_to_host", t.device_id.clone(), "local".to_string()),
            // Desktop-originated files use a synthetic "desktop" record in
            // storage; never expose that implementation detail to the UI.
            "download_from_host" => (
                "download_from_host",
                "local".to_string(),
                t.target_device_id.clone().unwrap_or_default(),
            ),
            "relay" => (
                "relay",
                t.device_id.clone(),
                t.target_device_id.clone().unwrap_or_default(),
            ),
            other => (
                other,
                t.device_id.clone(),
                t.target_device_id.clone().unwrap_or_default(),
            ),
        };
        let progress = if t.total_bytes > 0 {
            (t.transferred_bytes as f64 / t.total_bytes as f64).clamp(0.0, 1.0)
        } else if t.status == "completed" {
            1.0
        } else {
            0.0
        };
        let telemetry = s.transfer_telemetry.get(&t.id).cloned().unwrap_or_default();

        transfer_list.push(serde_json::json!({
            "id": t.id.clone(),
            "deviceId": source_device_id.clone(),
            "sourceDeviceId": source_device_id,
            "targetDeviceId": target_device_id,
            "direction": direction,
            "status": t.status.clone(),
            "totalBytes": t.total_bytes,
            "transferredBytes": t.transferred_bytes,
            "fileCount": t.file_count,
            "files": file_infos,
            "speedBytesPerSecond": telemetry.speed_bytes_per_second,
            "remainingSeconds": telemetry.remaining_seconds,
            "progress": progress,
            "savePath": t.save_path.clone(),
            "createdAt": timestamp_to_iso(&t.created_at),
            "completedAt": t.completed_at.as_deref().map(timestamp_to_iso),
            "relayStage": t.relay_stage.clone(),
            "acceptedAt": t.accepted_at.as_deref().map(timestamp_to_iso),
            "expiresAt": t.expires_at.as_deref().map(timestamp_to_iso),
            "pausedAt": t.paused_at.as_deref().map(timestamp_to_iso),
        }));
    }

    serde_json::to_string(&transfer_list).map_err(|e| e.to_string())
}

/// Delete transfer history records (and their child metadata) for the given
/// ids. Received files on disk are intentionally kept; only the transfer
/// history entry is removed.
#[tauri::command]
pub async fn delete_transfers(
    state: State<'_, SharedState>,
    transfer_ids: Vec<String>,
) -> Result<CommandResult, String> {
    let s = state.lock().await;
    match s.db.delete_transfers(&transfer_ids) {
        Ok(()) => {
            for transfer_id in &transfer_ids {
                let _ = s.event_tx.send(crate::server::WsEvent::TransferDeleted {
                    transfer_id: transfer_id.clone(),
                });
            }
            tracing::info!("Deleted {} transfer history records", transfer_ids.len());
            Ok(CommandResult {
                success: true,
                error: None,
            })
        }
        Err(e) => Ok(CommandResult {
            success: false,
            error: Some(e.to_string()),
        }),
    }
}

#[tauri::command]
pub async fn cancel_transfer(
    state: State<'_, SharedState>,
    transfer_id: String,
) -> Result<CommandResult, String> {
    // Gather info needed for cleanup before updating status.
    let (receive_folder, files, direction) = {
        let s = state.lock().await;
        let transfer = s.db.get_transfer(&transfer_id).map_err(|e| e.to_string())?;
        match transfer {
            Some(t) => {
                let files =
                    s.db.get_transfer_files(&transfer_id)
                        .map_err(|e| e.to_string())?;
                (s.receive_folder.clone(), files, t.direction.clone())
            }
            None => return Err("Transfer not found".to_string()),
        }
    };

    // Clean up temp files on disk.
    if direction == "relay" {
        let relay_dir = std::path::PathBuf::from(&receive_folder)
            .join(".relay")
            .join(&transfer_id);
        if relay_dir.exists() {
            let _ = tokio::fs::remove_dir_all(&relay_dir).await;
        }
    } else {
        let receive_path = std::path::PathBuf::from(&receive_folder);
        for file_record in &files {
            let temp_path =
                crate::transfer::temp_file_path(&receive_path, &transfer_id, &file_record.id);
            if temp_path.exists() {
                if let Err(e) = tokio::fs::remove_file(&temp_path).await {
                    tracing::error!("Failed to remove temp file {}: {}", temp_path.display(), e);
                }
            }
        }
        let temp_dir = crate::transfer::temp_transfer_dir(&receive_path, &transfer_id);
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }

    let s = state.lock().await;
    match s.db.update_transfer_status(&transfer_id, "cancelled") {
        Ok(()) => {
            let _ = s.event_tx.send(crate::server::WsEvent::TransferCancelled {
                transfer_id: transfer_id.clone(),
            });
            Ok(CommandResult {
                success: true,
                error: None,
            })
        }
        Err(e) => Ok(CommandResult {
            success: false,
            error: Some(e.to_string()),
        }),
    }
}

#[tauri::command]
pub async fn get_settings(state: State<'_, SharedState>) -> Result<String, String> {
    let s = state.lock().await;
    let settings = s.db.get_settings().map_err(|e| e.to_string())?;
    serde_json::to_string(&settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_settings(
    state: State<'_, SharedState>,
    settings_json: String,
) -> Result<CommandResult, String> {
    let patch: SettingsPatch =
        serde_json::from_str(&settings_json).map_err(|e| format!("Invalid settings: {}", e))?;

    let mut s = state.lock().await;
    let service_is_running = s.status == ServiceStatus::Running;
    let active_port = s.port;
    let mut settings = s.db.get_settings().map_err(|e| e.to_string())?;
    if let Some(value) = patch.device_name {
        settings.device_name = value;
    }
    if let Some(value) = patch.receive_folder {
        settings.receive_folder = value;
    }
    if let Some(value) = patch.require_approval {
        settings.require_approval = value;
    }
    if let Some(value) = patch.auto_approve_known {
        settings.auto_approve_known = value;
    }
    if let Some(value) = patch.port {
        settings.port = value;
    }
    if let Some(value) = patch.max_file_size {
        if value < 0 {
            return Err("maxFileSize must not be negative".to_string());
        }
        settings.max_file_size = value;
    }
    if let Some(value) = patch.theme_mode {
        if !matches!(value.as_str(), "light" | "dark" | "system") {
            return Err("themeMode must be light, dark, or system".to_string());
        }
        settings.theme_mode = value;
    }
    if let Some(value) = patch.authorization_expiry_hours {
        settings.authorization_expiry_hours = value;
    }
    if let Some(value) = patch.download_speed_limit_mbps {
        if value < 0 {
            return Err("downloadSpeedLimitMbps must not be negative".to_string());
        }
        settings.download_speed_limit_mbps = value;
    }
    std::fs::create_dir_all(&settings.receive_folder)
        .map_err(|e| format!("Unable to create receive folder: {}", e))?;
    match s.db.save_settings(&settings) {
        Ok(()) => {
            s.receive_folder = settings.receive_folder.clone();
            let device_name_changed = s.device_name != settings.device_name;
            s.device_name = settings.device_name.clone();
            if device_name_changed {
                s.network_generation = s.network_generation.wrapping_add(1);
            }
            // A running listener cannot change ports in place. Keep connection
            // info and the QR code on the real bound port; start_local_service
            // loads the configured port after the service is stopped.
            if !service_is_running {
                s.port = settings.port;
            } else {
                s.port = active_port;
            }

            // The host label shown in connection information must match the
            // active mDNS advertisement after a device-name change. Other
            // settings do not need to churn the discovery daemon.
            let should_refresh_mdns = service_is_running && device_name_changed;
            if should_refresh_mdns {
                s.mdns_refresh_generation = None;
                s.mdns_guard = None;
            }
            drop(s);
            if should_refresh_mdns {
                synchronize_network_state(state.inner()).await;
            }
            Ok(CommandResult {
                success: true,
                error: None,
            })
        }
        Err(e) => Ok(CommandResult {
            success: false,
            error: Some(e.to_string()),
        }),
    }
}

#[tauri::command]
pub async fn open_receive_folder(state: State<'_, SharedState>) -> Result<CommandResult, String> {
    let folder = {
        let s = state.lock().await;
        s.receive_folder.clone()
    };

    // Use tauri-plugin-shell to open the folder
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&folder)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&folder)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&folder)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    Ok(CommandResult {
        success: true,
        error: None,
    })
}

/// Reveal a concrete file in the platform file manager (audit-22: the
/// received-files list previously offered no per-file navigation).
#[tauri::command]
pub async fn reveal_in_folder(path: String) -> Result<CommandResult, String> {
    let target = path.trim().to_string();
    if target.is_empty() {
        return Ok(CommandResult {
            success: false,
            error: Some("empty_path".to_string()),
        });
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer")
            .raw_arg(format!("/select,\"{}\"", target))
            .spawn()
            .map_err(|e| format!("Failed to reveal file: {}", e))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("osascript")
            .arg("-e")
            .arg(format!(
                "tell application \"Finder\" to reveal POSIX file \"{}\"",
                target
            ))
            .arg("-e")
            .arg("activate")
            .spawn()
            .map_err(|e| format!("Failed to reveal file: {}", e))?;
    }
    #[cfg(target_os = "linux")]
    {
        let parent = std::path::Path::new(&target)
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| target.clone());
        std::process::Command::new("xdg-open")
            .arg(&parent)
            .spawn()
            .map_err(|e| format!("Failed to reveal file: {}", e))?;
    }

    Ok(CommandResult {
        success: true,
        error: None,
    })
}
