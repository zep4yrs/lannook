pub mod download;
pub mod handlers;
mod network;
pub mod ws;

use axum::extract::DefaultBodyLimit;
use axum::response::IntoResponse;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::storage::Database;
use handlers::SpeedTracker;

/// Default server port
pub const DEFAULT_PORT: u16 = 53317;

/// Maximum simultaneous WebSocket sessions served by the LAN endpoint.
pub const MAX_WS_CONNECTIONS: usize = 512;

/// Maximum simultaneous WebSocket sessions per authenticated device.
pub const MAX_WS_PER_DEVICE: usize = 4;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanInterface {
    pub name: String,
    pub ip: String,
    pub is_private: bool,
    pub is_virtual: bool,
    pub kind: String,
    pub has_default_gateway: bool,
    pub is_default_route: bool,
    pub selected: bool,
}

/// File metadata included in transfer request events
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub id: String,
    pub name: String,
    pub size: i64,
}

/// Events broadcast to all WebSocket clients
#[derive(Debug, Clone, serde::Serialize)]
#[serde(
    tag = "type",
    content = "payload",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WsEvent {
    #[serde(rename = "device.connected")]
    DeviceConnected {
        device_id: String,
        name: String,
        platform: String,
        device_type: String,
        ip: String,
        approved: bool,
    },
    #[serde(rename = "device.disconnected")]
    DeviceDisconnected { device_id: String },
    #[serde(rename = "device.approval_required")]
    DeviceApprovalRequired {
        device_id: String,
        name: String,
        ip: String,
        platform: String,
        device_type: String,
        user_agent: String,
    },
    #[serde(rename = "device.approved")]
    DeviceApproved { device_id: String },
    #[serde(rename = "device.rejected")]
    DeviceRejected { device_id: String },
    #[serde(rename = "transfer.created")]
    TransferCreated {
        transfer_id: String,
        file_name: String,
        total_bytes: i64,
    },
    #[serde(rename = "transfer.started")]
    TransferStarted { transfer_id: String },
    #[serde(rename = "transfer.progress")]
    TransferProgress {
        transfer_id: String,
        file_name: String,
        transferred_bytes: i64,
        total_bytes: i64,
        progress: f64,
        speed_bytes_per_second: i64,
        remaining_seconds: Option<i64>,
    },
    #[serde(rename = "transfer.checksum_ready")]
    TransferChecksumReady {
        transfer_id: String,
        file_id: String,
        checksum: String,
    },
    #[serde(rename = "transfer.checksum_progress")]
    TransferChecksumProgress {
        transfer_id: String,
        file_id: String,
        /// 0.0..=1.0 fraction of the file hashed so far.
        progress: f64,
    },
    #[serde(rename = "transfer.verifying")]
    TransferVerifying { transfer_id: String },
    #[serde(rename = "transfer.completed")]
    TransferCompleted {
        transfer_id: String,
        save_path: String,
    },
    #[serde(rename = "transfer.cancelled")]
    TransferCancelled { transfer_id: String },
    #[serde(rename = "transfer.failed")]
    TransferFailed { transfer_id: String, error: String },
    #[serde(rename = "transfer.deleted")]
    TransferDeleted { transfer_id: String },
    // --- Bidirectional transfer events ---
    #[serde(rename = "transfer.requested")]
    TransferRequested {
        transfer_id: String,
        source_device_name: String,
        files: Vec<FileInfo>,
        total_bytes: i64,
        /// Used only by the WebSocket fan-out layer. The target never needs
        /// to receive its own identifier in the payload.
        #[serde(skip_serializing)]
        target_device_id: String,
    },
    #[serde(rename = "transfer.accepted")]
    TransferAccepted { transfer_id: String },
    #[serde(rename = "transfer.rejected")]
    TransferRejected { transfer_id: String },
    #[serde(rename = "transfer.expired")]
    TransferExpired { transfer_id: String },
    #[serde(rename = "transfer.paused")]
    TransferPaused { transfer_id: String },
    #[serde(rename = "transfer.resumed")]
    TransferResumed { transfer_id: String },
    #[serde(rename = "transfer.download_started")]
    TransferDownloadStarted { transfer_id: String },
    #[serde(rename = "transfer.download_progress")]
    TransferDownloadProgress {
        transfer_id: String,
        file_id: String,
        transferred_bytes: i64,
        total_bytes: i64,
        progress: f64,
        speed_bytes_per_second: i64,
        remaining_seconds: Option<i64>,
    },
    #[serde(rename = "transfer.relay_stage_changed")]
    TransferRelayStageChanged { transfer_id: String, stage: String },
}

/// Latest measured transfer telemetry. This is intentionally ephemeral: the
/// database stores durable progress, while this cache lets a freshly opened
/// desktop or mobile page render live speed and ETA immediately.
#[derive(Debug, Clone, Default)]
pub struct TransferTelemetry {
    pub speed_bytes_per_second: i64,
    pub remaining_seconds: Option<i64>,
}

/// A short-lived 6-digit pairing code shown on the desktop for browsers that
/// cannot scan the QR code. Single use with a 5-minute expiry, plus a
/// per-IP brute-force lockout after repeated failures.
pub struct PairingPin {
    pub code: String,
    /// Unix seconds after which the code stops working.
    pub expires_at: i64,
}

/// Per-IP brute-force guard for PIN pairing.
pub struct PairingAttempt {
    pub failures: u32,
    /// Unix seconds until this IP is allowed to try again.
    pub locked_until: i64,
}

/// Generate a fresh 6-digit decimal pairing code (uniform-ish via SHA-256).
pub fn generate_pairing_code() -> String {
    use sha2::{Digest, Sha256};
    let material = format!(
        "{}:{}",
        uuid::Uuid::new_v4(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    let digest = Sha256::digest(material.as_bytes());
    let mut code = String::with_capacity(6);
    for byte in digest.iter().take(4) {
        code.push_str(&format!("{:02}", byte % 100));
        if code.len() >= 6 {
            break;
        }
    }
    code[..6].to_string()
}

/// Service lifecycle status
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
}

impl std::fmt::Display for ServiceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceStatus::Stopped => write!(f, "stopped"),
            ServiceStatus::Starting => write!(f, "starting"),
            ServiceStatus::Running => write!(f, "running"),
            ServiceStatus::Stopping => write!(f, "stopping"),
            ServiceStatus::Failed => write!(f, "failed"),
        }
    }
}

/// Shared application state for the HTTP server
pub struct AppState {
    pub status: ServiceStatus,
    pub port: u16,
    pub local_ip: Option<String>,
    /// Pairing capability embedded in the QR code. It can only validate a
    /// pairing link and create a normal device session.
    pub connection_token: String,
    /// Desktop-only control credential. It is returned through Tauri IPC and
    /// never appears in a browser URL or the QR code.
    pub desktop_control_token: String,
    pub device_name: String,
    pub network_name: String,
    pub started_at: Option<String>,
    pub error: Option<String>,
    pub db: Arc<Database>,
    pub event_tx: broadcast::Sender<WsEvent>,
    pub shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    pub receive_folder: String,
    /// Devices with an active authenticated WebSocket session. This is the
    /// source of truth for the desktop's online indicator.
    /// Number of active authenticated WebSocket sessions per device. A device
    /// stays online until its last browser tab/socket disconnects.
    pub connected_devices: HashMap<String, usize>,
    /// Total authenticated WebSocket sessions currently served. Bounded by
    /// MAX_WS_CONNECTIONS to protect against connection-flooding.
    pub ws_connection_count: usize,
    /// Live speed/remaining values keyed by transfer id.
    pub transfer_telemetry: HashMap<String, TransferTelemetry>,
    /// Per-transfer moving-window speed trackers. Lives in AppState (not a
    /// process-global) so stalled transfers are dropped with the service and
    /// state stays inspectable.
    pub(crate) speed_trackers: HashMap<String, SpeedTracker>,
    /// Keeps the mDNS service registered while present. Taking and dropping
    /// this guard unregisters the service from the LAN (see stop_local_service).
    pub mdns_guard: Option<crate::discovery::MdnsGuard>,
    /// Incremented whenever the selected network endpoint changes. Async mDNS
    /// work must match this generation before installing a new advertisement.
    pub network_generation: u64,
    /// Generation currently being advertised outside the state lock. This
    /// prevents concurrent refresh commands from racing duplicate mDNS work.
    pub mdns_refresh_generation: Option<u64>,
    /// Active pairing code (if any) shown in the desktop connect panel.
    pub pairing_pin: Option<PairingPin>,
    /// Per-IP brute-force guard for PIN pairing.
    pub pairing_attempts: HashMap<String, PairingAttempt>,
}

impl AppState {
    pub fn new(db: Arc<Database>) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        let connection_token = uuid::Uuid::new_v4().to_string();
        let desktop_control_token = uuid::Uuid::new_v4().to_string();

        // A one-time authorization belongs to one desktop service lifetime.
        // Clear stale approvals left by a crash or forced shutdown while
        // preserving devices the user explicitly trusted.
        if let Err(error) = db.revoke_untrusted_device_access() {
            tracing::warn!("Failed to reset one-time device access: {}", error);
        }

        let settings = match db.get_settings() {
            Ok(settings) => settings,
            Err(e) => {
                tracing::warn!("Failed to restore application settings: {}", e);
                crate::storage::Settings::default()
            }
        };

        let selected_interface = get_network_interfaces().into_iter().next();

        Self {
            status: ServiceStatus::Stopped,
            port: settings.port,
            local_ip: selected_interface
                .as_ref()
                .map(|interface| interface.ip.clone()),
            connection_token,
            desktop_control_token,
            device_name: settings.device_name,
            network_name: selected_interface
                .map(|interface| interface.name)
                .unwrap_or_else(|| "Local Network".to_string()),
            started_at: None,
            error: None,
            db,
            event_tx,
            shutdown_tx: None,
            receive_folder: settings.receive_folder,
            connected_devices: HashMap::new(),
            ws_connection_count: 0,
            transfer_telemetry: HashMap::new(),
            speed_trackers: HashMap::new(),
            mdns_guard: None,
            network_generation: 0,
            mdns_refresh_generation: None,
            pairing_pin: None,
            pairing_attempts: HashMap::new(),
        }
    }

    /// Returns the local HTTP URL, e.g. http://192.168.1.100:53317
    pub fn local_url(&self) -> Option<String> {
        self.local_ip
            .as_ref()
            .map(|ip| format!("http://{}:{}", ip, self.port))
    }

    /// Returns the QR connection URL with embedded token
    pub fn qr_url(&self) -> Option<String> {
        self.local_ip.as_ref().map(|ip| {
            format!(
                "http://{}:{}/mobile?token={}",
                ip, self.port, self.connection_token
            )
        })
    }
}

pub type SharedState = Arc<Mutex<AppState>>;

fn is_virtual_adapter(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    [
        "virtual",
        "vethernet",
        "hyper-v",
        "vmware",
        "virtualbox",
        "vbox",
        "docker",
        "wsl",
        "tailscale",
        "zerotier",
        "hamachi",
        "tunnel",
        "loopback",
        "vpn",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn is_hotspot_adapter(name: &str, description: &str, ip: std::net::Ipv4Addr) -> bool {
    let normalized = format!("{name} {description}").to_ascii_lowercase();
    ip == std::net::Ipv4Addr::new(192, 168, 137, 1)
        || normalized.contains("wi-fi direct")
        || normalized.contains("mobile hotspot")
}

fn interface_score(
    name: &str,
    description: &str,
    ip: std::net::Ipv4Addr,
    has_default_gateway: bool,
    is_default_route: bool,
) -> i32 {
    let octets = ip.octets();
    let mut score = 0;
    let classifier = format!("{name} {description}");
    let hotspot = is_hotspot_adapter(name, description, ip);
    let is_virtual = is_virtual_adapter(&classifier) && !hotspot;
    // A VPN can own the OS default route, but that does not make it reachable
    // from a phone on the physical LAN. Keep usable physical/hotspot adapters
    // in a higher score class before considering route preference.
    if !is_virtual {
        score += 100_000;
    }
    if ip.is_private() {
        score += 50_000;
    }
    if is_default_route {
        score += 20_000;
    }
    if has_default_gateway {
        score += 5_000;
    }
    if hotspot {
        score += 3_000;
    } else if !is_virtual {
        score += 1_000;
    }
    if octets[0] == 192 && octets[1] == 168 {
        score += 100;
    } else if octets[0] == 10 {
        score += 80;
    } else if octets[0] == 172 && (16..=31).contains(&octets[1]) {
        score += 60;
    }
    score
}

/// Enumerate usable IPv4 interfaces and deterministically choose the address
/// most likely to be reachable by another device on the same physical LAN.
pub fn get_network_interfaces() -> Vec<LanInterface> {
    let mut candidates = network::active_ipv4_interfaces();
    candidates.sort_by(|left, right| {
        interface_score(
            &right.name,
            &right.description,
            right.ip,
            right.has_default_gateway,
            right.is_default_route,
        )
        .cmp(&interface_score(
            &left.name,
            &left.description,
            left.ip,
            left.has_default_gateway,
            left.is_default_route,
        ))
        .then_with(|| left.name.cmp(&right.name))
        .then_with(|| left.ip.octets().cmp(&right.ip.octets()))
    });
    candidates.dedup_by(|left, right| left.name == right.name && left.ip == right.ip);

    let mut interfaces: Vec<LanInterface> = candidates
        .into_iter()
        .map(|candidate| {
            let classifier = format!("{} {}", candidate.name, candidate.description);
            let hotspot = is_hotspot_adapter(&candidate.name, &candidate.description, candidate.ip);
            let is_virtual = is_virtual_adapter(&classifier) && !hotspot;
            LanInterface {
                is_private: candidate.ip.is_private(),
                is_virtual,
                kind: if hotspot {
                    "hotspot".to_string()
                } else if is_virtual {
                    "virtual".to_string()
                } else {
                    "lan".to_string()
                },
                has_default_gateway: candidate.has_default_gateway,
                is_default_route: candidate.is_default_route,
                selected: false,
                name: candidate.name,
                ip: candidate.ip.to_string(),
            }
        })
        .collect();
    if let Some(selected) = interfaces
        .iter_mut()
        .find(|interface| interface.is_private && !interface.is_virtual)
    {
        selected.selected = true;
    }
    interfaces
}

pub fn get_local_ip() -> Option<String> {
    get_selected_interface().map(|interface| interface.ip)
}

pub fn get_selected_interface() -> Option<LanInterface> {
    get_network_interfaces()
        .into_iter()
        .find(|interface| interface.selected)
}

/// Start the Axum server on 0.0.0.0:53317
pub async fn start_server(state: SharedState, frontend_dir: String) -> Result<(), String> {
    {
        let mut s = state.lock().await;
        if s.status == ServiceStatus::Running {
            return Err("Server is already running".to_string());
        }
        s.status = ServiceStatus::Starting;
        s.error = None;
        // A restart interrupts every in-flight transfer; reset them to
        // paused so the owner can resume from completed chunks
        // (cross-session resume).
        match s.db.reset_inflight_transfers_to_paused() {
            Ok(count) if count > 0 => {
                tracing::info!(
                    "Reset {} in-flight transfers to paused after restart",
                    count
                );
            }
            Ok(_) => {}
            Err(error) => tracing::warn!("Failed to reset in-flight transfers: {}", error),
        }
    }

    let port = {
        let s = state.lock().await;
        s.port
    };

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));

    // The desktop UI uses history-mode Vue routes (for example `/mobile`).
    // Serve the entry document for an unknown path so a phone opening the QR
    // URL can boot the SPA and let the client router render that route.
    let frontend_root = std::path::Path::new(&frontend_dir);
    let index_html = frontend_root.join("index.html");
    let assets_dir = frontend_root.join("assets");
    let manifest_path = frontend_root.join("manifest.webmanifest");
    let icon_path = frontend_root.join("pwa-512.png");
    /// Serve a whitelisted static file (PWA manifest / icon) with a fixed
    /// content type. Paths are closed over constants, never from the URL.
    async fn serve_embedded_file(
        path: std::path::PathBuf,
        mime_type: &'static str,
    ) -> axum::response::Response {
        match tokio::fs::read(&path).await {
            Ok(bytes) => (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, mime_type)],
                bytes,
            )
                .into_response(),
            Err(error) => {
                tracing::warn!("Failed to serve PWA asset {}: {}", path.display(), error);
                axum::http::StatusCode::NOT_FOUND.into_response()
            }
        }
    }

    let spa_fallback = move |uri: axum::http::Uri| {
        let index_html = index_html.clone();
        async move {
            // Client-side routes have no file extension. Missing asset-like
            // paths must remain a real 404 instead of returning HTML with a
            // misleading 200 response.
            if std::path::Path::new(uri.path()).extension().is_some() {
                return axum::http::StatusCode::NOT_FOUND.into_response();
            }
            match tokio::fs::read(index_html).await {
                Ok(contents) => (
                    axum::http::StatusCode::OK,
                    [
                        (
                            axum::http::header::CONTENT_TYPE,
                            "text/html; charset=utf-8",
                        ),
                        (
                            axum::http::header::CONTENT_SECURITY_POLICY,
                            "default-src 'self'; base-uri 'self'; object-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self' ws: wss:; frame-ancestors 'none'",
                        ),
                    ],
                    contents,
                )
                    .into_response(),
                Err(error) => {
                    tracing::error!("Failed to serve SPA entry document: {}", error);
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            }
        }
    };

    // Build the router
    let app = axum::Router::new()
        .route("/api/status", axum::routing::get(handlers::get_status))
        .route("/api/connect", axum::routing::get(handlers::connect))
        .route("/api/pair", axum::routing::post(handlers::pair_with_pin))
        .route(
            "/api/devices/register",
            axum::routing::post(handlers::register_device),
        )
        .route("/api/devices", axum::routing::get(handlers::list_devices))
        .route(
            "/api/devices/me",
            axum::routing::get(handlers::get_current_device),
        )
        .route(
            "/api/devices/me/transfers/pending",
            axum::routing::get(handlers::get_pending_transfers),
        )
        .route(
            "/api/transfers",
            axum::routing::get(handlers::list_transfers).post(handlers::create_transfer),
        )
        .route(
            "/api/transfers/relay",
            axum::routing::post(handlers::create_relay),
        )
        .route(
            "/api/transfers/{id}/accept",
            axum::routing::post(handlers::accept_transfer),
        )
        .route(
            "/api/transfers/{id}/reject",
            axum::routing::post(handlers::reject_transfer),
        )
        .route(
            "/api/transfers/{id}/pause",
            axum::routing::post(handlers::pause_transfer),
        )
        .route(
            "/api/transfers/{id}/resume",
            axum::routing::post(handlers::resume_transfer),
        )
        .route(
            "/api/transfers/{id}/resume-info",
            axum::routing::get(handlers::get_resume_info),
        )
        .route(
            "/api/transfers/{id}/files/{file_id}/download",
            axum::routing::get(handlers::download_file),
        )
        .route(
            "/api/transfers/{id}/chunks/{index}",
            axum::routing::post(handlers::upload_chunk),
        )
        .route(
            "/api/transfers/{id}/chunks",
            axum::routing::get(handlers::get_chunks),
        )
        .route(
            "/api/transfers/{id}/complete",
            axum::routing::post(handlers::complete_transfer),
        )
        .route(
            "/api/transfers/{id}/relay-complete",
            axum::routing::post(handlers::complete_relay_download),
        )
        .route(
            "/api/transfers/{id}/cancel",
            axum::routing::post(handlers::cancel_transfer),
        )
        .route("/api/{*path}", axum::routing::any(handlers::api_not_found))
        .route("/ws", axum::routing::get(ws::ws_handler))
        // PWA static assets (whitelisted paths only - no user input here).
        .route(
            "/manifest.webmanifest",
            axum::routing::get({
                let p = manifest_path.clone();
                move || serve_embedded_file(p.clone(), "application/manifest+json")
            }),
        )
        .route(
            "/pwa-512.png",
            axum::routing::get({
                let p = icon_path.clone();
                move || serve_embedded_file(p.clone(), "image/png")
            }),
        )
        .nest_service("/assets", ServeDir::new(assets_dir))
        .fallback(spa_fallback)
        // File chunks are 512 KiB, but Axum otherwise rejects bodies above
        // 2 MiB before `upload_chunk` can process them.
        .layer(DefaultBodyLimit::max(
            crate::transfer::CHUNK_SIZE as usize + 1024,
        ))
        // The desktop serves the current embedded build. Prevent mobile
        // browsers from reviving an old index/assets mapping after an update.
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-store"),
        ))
        .with_state(state.clone());

    // Bind the listener
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(error) => {
            let error_message = format!("Failed to bind to {}: {}", addr, error);
            let mut s = state.lock().await;
            s.status = ServiceStatus::Failed;
            s.error = Some(error_message.clone());
            s.shutdown_tx = None;
            tracing::error!("{}", error_message);
            return Err(error_message);
        }
    };

    // Only expose a shutdown handle after the port is actually bound.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Update state to running
    {
        let mut s = state.lock().await;
        s.status = ServiceStatus::Running;
        s.shutdown_tx = Some(shutdown_tx);
        s.started_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs().to_string())
                .unwrap_or_default(),
        );
        let selected_interface = get_selected_interface();
        s.local_ip = selected_interface
            .as_ref()
            .map(|interface| interface.ip.clone());
        s.network_name = selected_interface
            .map(|interface| interface.name)
            .unwrap_or_else(|| "Local Network".to_string());
    }

    tracing::info!("LanNook server started on {}", addr);

    // Run the server
    let server_result = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = shutdown_rx.await;
    })
    .await;

    // Update state after server stops
    {
        let mut s = state.lock().await;
        s.status = ServiceStatus::Stopped;
        s.shutdown_tx = None;
        s.connected_devices.clear();
        match s.db.revoke_untrusted_device_access() {
            Ok(device_ids) => {
                for device_id in device_ids {
                    let _ = s.event_tx.send(WsEvent::DeviceRejected { device_id });
                }
            }
            Err(error) => tracing::warn!("Failed to expire one-time device access: {}", error),
        }
        if let Err(e) = server_result {
            s.error = Some(e.to_string());
            s.status = ServiceStatus::Failed;
            tracing::error!("Server error: {}", e);
        } else {
            tracing::info!("Server stopped gracefully");
        }
    }

    Ok(())
}

/// Stop the server gracefully
pub async fn stop_server(state: SharedState) -> Result<(), String> {
    let shutdown_tx = {
        let mut s = state.lock().await;
        if s.status != ServiceStatus::Running {
            return Err("Server is not running".to_string());
        }
        s.status = ServiceStatus::Stopping;
        s.shutdown_tx.take()
    };

    match shutdown_tx {
        Some(tx) => {
            let _ = tx.send(());
            tracing::info!("Server shutdown signal sent");
            Ok(())
        }
        None => Err("No shutdown channel available".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websocket_event_payload_uses_camel_case_fields() {
        let event = WsEvent::TransferProgress {
            transfer_id: "transfer-1".to_string(),
            file_name: "photo.jpg".to_string(),
            transferred_bytes: 42,
            total_bytes: 100,
            progress: 42.0,
            speed_bytes_per_second: 7,
            remaining_seconds: Some(8),
        };

        let payload = serde_json::to_value(event).unwrap();
        assert_eq!(payload["type"], "transfer.progress");
        assert_eq!(payload["payload"]["transferId"], "transfer-1");
        assert_eq!(payload["payload"]["speedBytesPerSecond"], 7);
        assert!(payload["payload"].get("transfer_id").is_none());
    }

    #[test]
    fn physical_private_adapter_beats_virtual_and_public_candidates() {
        let wifi = interface_score(
            "Wi-Fi",
            "Wireless adapter",
            "192.168.1.5".parse().unwrap(),
            true,
            true,
        );
        let vpn = interface_score(
            "Tailscale Tunnel",
            "Virtual",
            "100.64.0.2".parse().unwrap(),
            false,
            false,
        );
        let docker = interface_score(
            "DockerNAT",
            "Virtual",
            "10.0.75.1".parse().unwrap(),
            false,
            false,
        );
        let public = interface_score(
            "Ethernet",
            "Physical",
            "8.8.8.8".parse().unwrap(),
            false,
            false,
        );

        assert!(wifi > vpn);
        assert!(wifi > docker);
        assert!(wifi > public);
    }

    #[test]
    fn default_route_beats_hotspot_but_hotspot_beats_unrouted_virtual_adapter() {
        let ethernet = interface_score(
            "Ethernet",
            "Physical adapter",
            "192.168.1.2".parse().unwrap(),
            true,
            true,
        );
        let hotspot = interface_score(
            "Local Area Connection* 10",
            "Microsoft Wi-Fi Direct Virtual Adapter",
            "192.168.137.1".parse().unwrap(),
            false,
            false,
        );
        let wsl = interface_score(
            "vEthernet (WSL)",
            "Hyper-V Virtual Ethernet Adapter",
            "172.19.176.1".parse().unwrap(),
            false,
            false,
        );

        assert!(ethernet > hotspot);
        assert!(hotspot > wsl);
    }

    #[test]
    fn private_lan_beats_public_route_and_true_default_beats_secondary_gateway() {
        let public_route = interface_score(
            "Ethernet 2",
            "Physical adapter",
            "8.8.8.8".parse().unwrap(),
            true,
            true,
        );
        let private_hotspot = interface_score(
            "Local Area Connection* 10",
            "Microsoft Wi-Fi Direct Virtual Adapter",
            "192.168.137.1".parse().unwrap(),
            false,
            false,
        );
        let default_lan = interface_score(
            "Ethernet",
            "Physical adapter",
            "192.168.1.2".parse().unwrap(),
            true,
            true,
        );
        let secondary_lan = interface_score(
            "Wi-Fi",
            "Physical adapter",
            "192.168.2.2".parse().unwrap(),
            true,
            false,
        );

        assert!(private_hotspot > public_route);
        assert!(default_lan > secondary_lan);
    }

    #[test]
    fn virtual_adapter_detection_covers_common_desktop_networks() {
        assert!(is_virtual_adapter("vEthernet (WSL)"));
        assert!(is_virtual_adapter("Tailscale Tunnel"));
        assert!(!is_virtual_adapter("Wi-Fi"));
        assert!(!is_virtual_adapter("Ethernet"));
    }

    #[tokio::test]
    async fn bind_failure_sets_failed_status_and_preserves_the_error() {
        let occupied = tokio::net::TcpListener::bind(("0.0.0.0", 0)).await.unwrap();
        let port = occupied.local_addr().unwrap().port();
        let database_path = std::env::temp_dir().join(format!(
            "lannook-server-bind-failure-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = std::sync::Arc::new(Database::open(&database_path).unwrap());
        let state = std::sync::Arc::new(Mutex::new(AppState::new(database)));
        state.lock().await.port = port;

        let result = start_server(state.clone(), "missing-frontend".to_string()).await;
        assert!(result.is_err());
        let snapshot = state.lock().await;
        assert_eq!(snapshot.status, ServiceStatus::Failed);
        assert!(snapshot
            .error
            .as_deref()
            .is_some_and(|message| message.contains(&port.to_string())));
        assert!(snapshot.shutdown_tx.is_none());
        assert!(snapshot.mdns_guard.is_none());
    }
}
