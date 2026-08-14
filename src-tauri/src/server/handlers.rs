use axum::body::Bytes;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::io::SeekFrom;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use crate::error::{AppError, AppResult};
use crate::server::download;
use crate::server::{FileInfo, PairingAttempt, SharedState, TransferTelemetry, WsEvent};
use crate::storage::{
    ChunkRecord, DeviceRecord, RelayFile, TransferEvent, TransferFileRecord, TransferRecord,
};
use crate::transfer::{self, CHUNK_SIZE};

const MAX_FILES_PER_TRANSFER: usize = 1_000;
/// Maximum number of 512 KiB chunks a single transfer may declare. Prevents
/// a client from pre-materialising millions of chunk rows in the database.
const MAX_CHUNKS_PER_TRANSFER: i64 = 100_000;
/// Upper bound on transfers one device may keep in-flight at the same time.
const MAX_ACTIVE_TRANSFERS_PER_DEVICE: i64 = 50;

pub async fn api_not_found() -> (axum::http::StatusCode, Json<serde_json::Value>) {
    (
        axum::http::StatusCode::NOT_FOUND,
        Json(json!({
            "error": {
                "code": "API_NOT_FOUND",
                "message": "API endpoint not found"
            }
        })),
    )
}

// --- Request/Response types ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDeviceRequest {
    pub name: String,
    pub platform: String,
    #[serde(default)]
    pub device_type: String,
    #[serde(default)]
    pub user_agent: String,
    /// A randomly generated, browser-local identifier. It survives refreshes
    /// but is not an authorization credential.
    #[serde(default)]
    pub client_id: String,
    pub token: String,
    /// The session token previously issued to this browser, if any. It proves
    /// the caller already owns the device record for client_id; without it,
    /// an existing client_id is never reused as an authorization anchor.
    #[serde(default)]
    pub session_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairRequest {
    pub pin: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTransferRequest {
    pub files: Vec<FileEntry>,
    pub session_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub size: i64,
    #[serde(default)]
    pub mime_type: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferFileResponse {
    id: String,
    name: String,
    chunk_size: i64,
    total_chunks: i32,
}

// --- Helper functions ---

/// Extract Bearer token from Authorization header
fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Validate session token and return the device record
async fn validate_session(state: &SharedState, token: &str) -> AppResult<DeviceRecord> {
    let s = state.lock().await;
    let device =
        s.db.get_device_by_session_token(token)?
            .ok_or(AppError::InvalidToken)?;

    // Authorization expiry: once the deadline passes, the approval is revoked
    // here and now so a stale browser can never continue transferring.
    if s.db.revoke_device_if_expired(&device)? {
        let _ = s.event_tx.send(WsEvent::DeviceRejected {
            device_id: device.id.clone(),
        });
        tracing::info!("Device approval expired: {}", device.id);
        return Err(AppError::DeviceNotApproved);
    }

    // Refresh the activity timestamp, keeping the stored IP (ISSUE 8). This is
    // best-effort: a failure here must not reject an otherwise valid session.
    let _ = s.db.update_device_last_seen(&device.id, &device.ip);
    Ok(device)
}

/// Browser session tokens are credentials, but only an approved device may
/// read or mutate transfer state. Keeping this check in one helper prevents a
/// revoked session from continuing an old upload, pause, cancel, or download.
async fn validate_approved_session(state: &SharedState, token: &str) -> AppResult<DeviceRecord> {
    let device = validate_session(state, token).await?;
    if !device.approved {
        return Err(AppError::DeviceNotApproved);
    }
    Ok(device)
}

fn validate_transfer_files(files: &[FileEntry], max_file_size: i64) -> AppResult<i64> {
    if files.is_empty() {
        return Err(AppError::Internal("No files specified".to_string()));
    }
    if files.len() > MAX_FILES_PER_TRANSFER {
        return Err(AppError::Internal(format!(
            "Too many files in one transfer (maximum: {})",
            MAX_FILES_PER_TRANSFER
        )));
    }

    let mut total_bytes = 0_i64;
    for file in files {
        if file.size < 0 {
            return Err(AppError::Internal(format!(
                "File '{}' has an invalid size",
                file.name
            )));
        }
        if max_file_size > 0 && file.size > max_file_size {
            return Err(AppError::Internal(format!(
                "File '{}' exceeds the configured size limit",
                file.name
            )));
        }

        // Validate all client-controlled metadata before creating any rows.
        transfer::sanitize_filename(&file.name)?;
        total_bytes = total_bytes
            .checked_add(file.size)
            .ok_or_else(|| AppError::Internal("Transfer size is too large".to_string()))?;
    }

    Ok(total_bytes)
}

/// Bound the total number of chunk records a single transfer may declare so
/// a malicious client cannot inflate the database (anti-DoS). The declared
/// size limit is already enforced by `validate_transfer_files`.
fn validate_transfer_chunk_budget(files: &[FileEntry]) -> AppResult<i64> {
    let mut total_chunks: i64 = 0;
    for file in files {
        let chunks = if file.size == 0 {
            0
        } else {
            (file.size - 1) / CHUNK_SIZE + 1
        };
        total_chunks = total_chunks
            .checked_add(chunks)
            .ok_or_else(|| AppError::Internal("Transfer requires too many chunks".to_string()))?;
        if total_chunks > MAX_CHUNKS_PER_TRANSFER {
            return Err(AppError::Internal(format!(
                "Transfer exceeds the maximum number of chunks ({}). Reduce the file count or size.",
                MAX_CHUNKS_PER_TRANSFER
            )));
        }
    }
    Ok(total_chunks)
}

fn host_platform() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "macos",
        _ => "web",
    }
}

/// The host is always a valid target for an approved browser: this response
/// itself proves the host is reachable. It is kept virtual so the synthetic
/// database record used for foreign keys never leaks its stale name or loopback
/// address into the mobile UI.
fn mobile_target_list(
    caller_id: &str,
    host_name: &str,
    host_ip: Option<&str>,
    devices: &[DeviceRecord],
    connected_devices: &HashMap<String, usize>,
) -> Vec<serde_json::Value> {
    let mut targets = vec![json!({
        "id": "desktop",
        "name": host_name,
        "platform": host_platform(),
        "deviceType": "desktop",
        "approved": true,
        "ip": host_ip.unwrap_or_default(),
        "online": true,
        "lastSeenAt": now_ts(),
    })];

    targets.extend(
        devices
            .iter()
            .filter(|device| device.id != "desktop" && device.id != caller_id && device.approved)
            .map(|device| {
                json!({
                    "id": device.id,
                    "name": device.name,
                    "platform": device.platform,
                    "deviceType": device.device_type,
                    "approved": device.approved,
                    "ip": device.ip,
                    "online": connected_devices.contains_key(&device.id),
                    "lastSeenAt": device.last_seen,
                })
            }),
    );

    targets
}

/// Speed tracker using a moving window plus an exponential moving average.
///
/// The 3-second window bounds memory and reacts to stalls, while the EMA
/// removes the jitter between individual chunk uploads so the displayed
/// speed no longer "flashes" between wildly different values. The remaining
/// time is derived from the same smoothed value, keeping phone and desktop
/// telemetry consistent.
pub(crate) struct SpeedTracker {
    samples: Vec<(Instant, i64)>,
    smoothed_speed: f64,
}

impl SpeedTracker {
    /// EMA weight applied to each new instantaneous reading.
    const EMA_ALPHA: f64 = 0.35;

    fn new() -> Self {
        Self {
            samples: Vec::new(),
            smoothed_speed: 0.0,
        }
    }

    fn record(&mut self, bytes: i64) {
        let now = Instant::now();
        self.samples.push((now, bytes));
        // Keep only last 3 seconds of samples
        self.samples
            .retain(|(t, _)| now.duration_since(*t).as_secs() < 3);
        // Fold this reading into the EMA immediately so every call site
        // observes the same smoothed value.
        self.update_smoothed();
    }

    fn update_smoothed(&mut self) {
        let raw = self.raw_speed_bytes_per_second();
        if raw > 0.0 {
            self.smoothed_speed = if self.smoothed_speed <= 0.0 {
                raw
            } else {
                Self::EMA_ALPHA * raw + (1.0 - Self::EMA_ALPHA) * self.smoothed_speed
            };
        } else if self.samples.len() < 2 && self.smoothed_speed > 0.0 {
            // The window drained without a new chunk: the transfer stalled.
            self.smoothed_speed = 0.0;
        }
    }

    fn raw_speed_bytes_per_second(&self) -> f64 {
        if self.samples.len() < 2 {
            return 0.0;
        }
        let first = &self.samples[0];
        let last = &self.samples[self.samples.len() - 1];
        let elapsed = last.0.duration_since(first.0).as_secs_f64();
        if elapsed < 0.01 {
            return 0.0;
        }
        let bytes_transferred = last.1 - first.1;
        bytes_transferred as f64 / elapsed
    }

    fn speed_bytes_per_second(&self) -> i64 {
        self.smoothed_speed as i64
    }
}

fn estimate_remaining_seconds(
    total_bytes: i64,
    transferred_bytes: i64,
    speed_bytes_per_second: i64,
) -> Option<i64> {
    if speed_bytes_per_second <= 0 || total_bytes <= transferred_bytes {
        return None;
    }

    let remaining_bytes = total_bytes - transferred_bytes;
    Some((remaining_bytes + speed_bytes_per_second - 1) / speed_bytes_per_second)
}

// --- Shared helpers (relay flow + lifecycle events) ---

/// Persist a transfer lifecycle event (ISSUE 9). Payloads are kept minimal and
/// must never contain tokens or file contents. Failures are logged but never
/// propagate – event recording is best-effort and must not break a transfer.
async fn record_transfer_event(
    state: &SharedState,
    transfer_id: &str,
    event_type: &str,
    payload: serde_json::Value,
) {
    let event = TransferEvent {
        id: uuid::Uuid::new_v4().to_string(),
        transfer_id: transfer_id.to_string(),
        event_type: event_type.to_string(),
        event_id: uuid::Uuid::new_v4().to_string(),
        timestamp: now_ts(),
        payload_json: payload.to_string(),
    };
    let s = state.lock().await;
    if let Err(e) = s.db.insert_transfer_event(&event) {
        tracing::warn!("Failed to record transfer event: {}", e);
    }
}

/// Update a relay transfer's stage in the database and broadcast the
/// `transfer.relay_stage_changed` event (ISSUE 1).
async fn set_relay_stage(state: &SharedState, transfer_id: &str, stage: &str) {
    let s = state.lock().await;
    if let Err(e) = s.db.update_transfer_relay_stage(transfer_id, stage) {
        tracing::error!("Failed to update relay stage: {}", e);
        return;
    }
    let _ = s.event_tx.send(WsEvent::TransferRelayStageChanged {
        transfer_id: transfer_id.to_string(),
        stage: stage.to_string(),
    });
}

/// Build the per-transfer relay temp directory: `{receive_folder}/.relay/{transfer_id}`.
fn relay_temp_dir(receive_folder: &std::path::Path, transfer_id: &str) -> PathBuf {
    receive_folder.join(".relay").join(transfer_id)
}

/// Remove a transfer's relay temp files from disk, mark them cleaned in the
/// database, and best-effort remove the now-empty relay temp directory. Called
/// when a relay completes or is cancelled (ISSUE 1).
async fn cleanup_relay_files(state: &SharedState, transfer_id: &str) {
    let relay_files = {
        let s = state.lock().await;
        match s.db.get_relay_files_by_transfer(transfer_id) {
            Ok(files) => files,
            Err(e) => {
                tracing::error!("Failed to load relay files for cleanup: {}", e);
                return;
            }
        }
    };

    let mut relay_dir: Option<PathBuf> = None;
    for rf in &relay_files {
        if rf.temp_path.is_empty() {
            continue;
        }
        let path = PathBuf::from(&rf.temp_path);
        if relay_dir.is_none() {
            relay_dir = path.parent().map(|p| p.to_path_buf());
        }
        if !rf.cleaned && path.exists() {
            if let Err(e) = tokio::fs::remove_file(&path).await {
                tracing::error!("Failed to remove relay temp file {}: {}", path.display(), e);
            }
        }
    }

    {
        let s = state.lock().await;
        for rf in &relay_files {
            let _ = s.db.mark_relay_cleaned(&rf.id);
        }
    }

    // Best-effort removal of the (now empty) relay temp directory.
    if let Some(dir) = relay_dir {
        let _ = tokio::fs::remove_dir(&dir).await;
    }
}

/// Opportunistically remove relay temp files whose retention has expired, using
/// `get_expired_relay_files` / `mark_relay_cleaned`. Invoked when new relays are
/// created so stale temp files do not accumulate (ISSUE 1).
async fn cleanup_expired_relays(state: &SharedState) {
    let expired = {
        let s = state.lock().await;
        match s.db.get_expired_relay_files() {
            Ok(files) => files,
            Err(e) => {
                tracing::warn!("Failed to query expired relay files: {}", e);
                return;
            }
        }
    };

    for rf in &expired {
        if !rf.temp_path.is_empty() {
            let path = PathBuf::from(&rf.temp_path);
            if path.exists() {
                if let Err(e) = tokio::fs::remove_file(&path).await {
                    tracing::warn!(
                        "Failed to remove expired relay file {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }
        let s = state.lock().await;
        let _ = s.db.mark_relay_cleaned(&rf.id);
    }
}

// --- Handlers ---

/// GET /api/status - public, returns service info (no token needed)
pub async fn get_status(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let s = state.lock().await;
    Json(json!({
        "name": s.device_name,
        "port": s.port,
        "status": s.status.to_string(),
        "localIp": s.local_ip,
        "networkName": s.network_name,
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// GET /api/connect?token=xxx - validate connection token, returns session info
pub async fn connect(
    State(state): State<SharedState>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Json<serde_json::Value>> {
    let token = params.get("token").cloned().unwrap_or_default();

    let s = state.lock().await;
    if token != s.connection_token {
        return Err(AppError::InvalidToken);
    }

    Ok(Json(json!({
        "valid": true,
        "deviceName": s.device_name,
        "networkName": s.network_name,
    })))
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// POST /api/pair - exchange a desktop-shown 6-digit PIN for the pairing
/// capability, so a browser that cannot scan the QR code can still connect.
/// The code is single-use, expires after 5 minutes, and is guarded against
/// brute force with a per-IP lockout after five failures.
pub async fn pair_with_pin(
    State(state): State<SharedState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<PairRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let pin = body.pin.trim().to_string();
    let peer_ip = addr.ip().to_string();
    let now = now_secs();

    let mut s = state.lock().await;

    // Brute-force lockout: too many failures blocks this IP for 10 minutes.
    if let Some(attempt) = s.pairing_attempts.get(&peer_ip) {
        if now < attempt.locked_until {
            tracing::warn!("Pairing PIN rejected: IP {} is locked out", peer_ip);
            return Err(AppError::InvalidToken);
        }
    }

    let Some(pairing) = s.pairing_pin.as_ref() else {
        return Err(AppError::InvalidToken);
    };
    if now > pairing.expires_at {
        // Expired: drop it so the desktop must refresh before another try.
        s.pairing_pin = None;
        return Err(AppError::InvalidToken);
    }

    if pairing.code != pin {
        let attempt = s
            .pairing_attempts
            .entry(peer_ip.clone())
            .or_insert(PairingAttempt {
                failures: 0,
                locked_until: 0,
            });
        attempt.failures += 1;
        if attempt.failures >= 5 {
            attempt.locked_until = now + 600;
            attempt.failures = 0;
            tracing::warn!("Pairing PIN locked out for IP {} after 5 failures", peer_ip);
        }
        return Err(AppError::InvalidToken);
    }

    // Success: consume the code and hand out the pairing capability so the
    // phone continues with the exact same flow as a scanned QR code.
    s.pairing_pin = None;
    s.pairing_attempts.remove(&peer_ip);
    let token = s.connection_token.clone();
    let device_name = s.device_name.clone();
    let network_name = s.network_name.clone();

    tracing::info!("Device paired via PIN from {}", peer_ip);

    Ok(Json(json!({
        "valid": true,
        "token": token,
        "deviceName": device_name,
        "networkName": network_name,
    })))
}

/// POST /api/devices/register - register a new device
pub async fn register_device(
    State(state): State<SharedState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<RegisterDeviceRequest>,
) -> AppResult<Json<serde_json::Value>> {
    // Validate connection token
    {
        let s = state.lock().await;
        if body.token != s.connection_token {
            return Err(AppError::InvalidToken);
        }
    }

    // Capture the remote peer's IP address from the connection (ISSUE 8).
    let peer_ip = addr.ip().to_string();
    let client_id = body.client_id.trim().to_string();
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err(AppError::InvalidRequest(
            "Device name must contain 1 to 120 characters".to_string(),
        ));
    }
    if !(16..=128).contains(&client_id.len())
        || !client_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err(AppError::InvalidRequest(
            "A valid stable client ID is required".to_string(),
        ));
    }
    if body.platform.len() > 32 || body.device_type.len() > 32 || body.user_agent.len() > 1_024 {
        return Err(AppError::InvalidRequest(
            "Device metadata exceeds the allowed length".to_string(),
        ));
    }

    let device_id = uuid::Uuid::new_v4().to_string();
    let session_token = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default();

    // Determine if auto-approved based on the persisted settings
    let require_approval = {
        let s = state.lock().await;
        match s.db.get_settings() {
            Ok(settings) => settings.require_approval,
            Err(e) => {
                tracing::warn!("Failed to read require_approval setting: {}", e);
                true
            }
        }
    };

    let approved = !require_approval;

    let candidate = DeviceRecord {
        id: device_id.clone(),
        name: name.to_string(),
        platform: body.platform.clone(),
        device_type: body.device_type.clone(),
        user_agent: body.user_agent.clone(),
        client_id,
        session_token: session_token.clone(),
        approved,
        trusted: false,
        ip: peer_ip.clone(),
        created_at: now.clone(),
        last_seen: now,
        approved_until: None,
    };

    let (device, created) = {
        let s = state.lock().await;
        s.db.register_or_refresh_device(&candidate, body.session_token.as_deref())?
    };

    // Re-emit a pending request on every registration. This makes the desktop
    // recover from a missed broadcast while the stable device record and
    // session token remain unchanged.
    if !device.approved {
        let _ = {
            let s = state.lock().await;
            s.event_tx.send(WsEvent::DeviceApprovalRequired {
                device_id: device.id.clone(),
                name: device.name.clone(),
                ip: device.ip.clone(),
                platform: device.platform.clone(),
                device_type: device.device_type.clone(),
                user_agent: device.user_agent.clone(),
            })
        };
    }

    tracing::info!(
        "Device registration {}: {} ({}) approved={}",
        if created { "created" } else { "refreshed" },
        device.name,
        device.id,
        device.approved
    );

    Ok(Json(json!({
        "deviceId": device.id,
        "sessionToken": device.session_token,
        "approved": device.approved,
        "trusted": device.trusted,
    })))
}

/// GET /api/devices/me - return only the calling device's own authorization
/// state. Pending mobile browsers poll this as a fallback if a WebSocket event
/// is missed while the page reloads or the browser suspends the socket.
pub async fn get_current_device(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let token = extract_bearer_token(&headers).ok_or(AppError::InvalidToken)?;
    let device = validate_session(&state, &token).await?;

    Ok(Json(json!({
        "deviceId": device.id,
        "approved": device.approved,
        "trusted": device.trusted,
    })))
}

/// GET /api/devices - list devices (requires session token)
pub async fn list_devices(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let token = extract_bearer_token(&headers).ok_or(AppError::InvalidToken)?;
    let caller = validate_approved_session(&state, &token).await?;

    let s = state.lock().await;
    let devices = s.db.list_visible_devices()?;
    let device_list = mobile_target_list(
        &caller.id,
        &s.device_name,
        s.local_ip.as_deref(),
        &devices,
        &s.connected_devices,
    );

    Ok(Json(json!({ "devices": device_list })))
}

/// POST /api/transfers - create a new transfer task
pub async fn create_transfer(
    State(state): State<SharedState>,
    Json(body): Json<CreateTransferRequest>,
) -> AppResult<Json<serde_json::Value>> {
    // Validate session
    let device = validate_approved_session(&state, &body.session_token).await?;

    // The database owns its own lock, so DB-heavy work never holds the global
    // state lock that WebSocket fan-out and heartbeat also contend on.
    let db = {
        let s = state.lock().await;
        s.db.clone()
    };

    let max_file_size = db.get_settings()?.max_file_size;
    let total_bytes = validate_transfer_files(&body.files, max_file_size)?;
    validate_transfer_chunk_budget(&body.files)?;

    // Bound how many transfers one device may keep in flight (anti-DoS).
    let active = db.count_active_transfers_for_device(&device.id)?;
    if active >= MAX_ACTIVE_TRANSFERS_PER_DEVICE {
        return Err(AppError::Internal(format!(
            "Too many active transfers for this device (maximum: {})",
            MAX_ACTIVE_TRANSFERS_PER_DEVICE
        )));
    }

    let transfer_id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default();

    let transfer = TransferRecord {
        id: transfer_id.clone(),
        device_id: device.id.clone(),
        direction: "receive".to_string(),
        status: "pending".to_string(),
        total_bytes,
        transferred_bytes: 0,
        file_count: body.files.len() as i32,
        save_path: None,
        created_at: now,
        completed_at: None,
        target_device_id: None,
        relay_stage: None,
        accepted_at: None,
        expires_at: None,
        paused_at: None,
    };

    let mut file_responses: Vec<TransferFileResponse> = Vec::new();

    db.insert_transfer(&transfer)?;

    for file_entry in &body.files {
        let file_name = transfer::sanitize_filename(&file_entry.name)?;
        let file_id = uuid::Uuid::new_v4().to_string();
        let total_chunks_i64 = if file_entry.size == 0 {
            0
        } else {
            (file_entry.size - 1) / CHUNK_SIZE + 1
        };
        let total_chunks = i32::try_from(total_chunks_i64).map_err(|_| {
            AppError::Internal("File requires too many transfer chunks".to_string())
        })?;

        let file_record = TransferFileRecord {
            id: file_id.clone(),
            transfer_id: transfer_id.clone(),
            name: file_name.clone(),
            size: file_entry.size,
            mime_type: file_entry.mime_type.clone(),
            chunk_size: CHUNK_SIZE,
            total_chunks,
            completed_chunks: 0,
            sha256: None,
            save_path: None,
            status: "pending".to_string(),
        };

        db.insert_transfer_file(&file_record)?;

        // Create chunk records
        let mut chunks = Vec::new();
        for i in 0..total_chunks {
            let offset = i as i64 * CHUNK_SIZE;
            let chunk_size = std::cmp::min(CHUNK_SIZE, file_entry.size - offset);
            chunks.push(ChunkRecord {
                id: uuid::Uuid::new_v4().to_string(),
                file_id: file_id.clone(),
                chunk_index: i,
                offset,
                size: chunk_size,
                completed: false,
            });
        }
        db.insert_chunks(&chunks)?;

        file_responses.push(TransferFileResponse {
            id: file_id,
            name: file_name,
            chunk_size: CHUNK_SIZE,
            total_chunks,
        });
    }

    // Broadcast transfer created
    let first_file_name = file_responses
        .first()
        .map(|f| f.name.clone())
        .unwrap_or_default();
    let _ = {
        let s = state.lock().await;
        s.event_tx.send(WsEvent::TransferCreated {
            transfer_id: transfer_id.clone(),
            file_name: first_file_name,
            total_bytes,
        })
    };

    record_transfer_event(
        &state,
        &transfer_id,
        "created",
        json!({ "fileCount": body.files.len(), "totalBytes": total_bytes }),
    )
    .await;

    tracing::info!(
        "Transfer created: {} ({} files, {} bytes)",
        transfer_id,
        body.files.len(),
        total_bytes
    );

    Ok(Json(json!({
        "transferId": transfer_id,
        "files": file_responses,
    })))
}

/// POST /api/transfers/:id/chunks/:index?fileId=xxx - upload a chunk (raw bytes body)
///
/// The optional `fileId` query parameter identifies which file of a
/// multi-file transfer the chunk belongs to. When omitted (legacy clients),
/// the first file of the transfer is used, which is only correct for
/// single-file transfers.
pub async fn upload_chunk(
    State(state): State<SharedState>,
    Path((transfer_id, chunk_index)): Path<(String, i32)>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Json<serde_json::Value>> {
    // Validate session
    let token = extract_bearer_token(&headers).ok_or(AppError::InvalidToken)?;
    let device = validate_approved_session(&state, &token).await?;

    let file_id = params.get("fileId").cloned();

    // Get transfer and validate
    let (receive_folder, files, direction) = {
        let s = state.lock().await;
        let transfer =
            s.db.get_transfer(&transfer_id)?
                .ok_or(AppError::TransferNotFound)?;

        if transfer.status == "cancelled" {
            return Err(AppError::TransferCancelled);
        }
        if transfer.status == "completed" {
            return Err(AppError::TransferCompleted);
        }
        if transfer.status == "paused" {
            // Reject chunks while paused so the client stops uploading.
            return Err(AppError::TransferPaused);
        }

        // Verify device owns this transfer
        if transfer.device_id != device.id {
            return Err(AppError::InvalidToken);
        }

        let files = s.db.get_transfer_files(&transfer_id)?;
        (s.receive_folder.clone(), files, transfer.direction.clone())
    };

    // Identify the file this chunk belongs to. The file list is already
    // scoped to this transfer, so a match by id cannot cross transfers.
    let file_record = match file_id {
        Some(ref fid) => files
            .iter()
            .find(|f| &f.id == fid)
            .cloned()
            .ok_or(AppError::TransferNotFound)?,
        // Legacy fallback: no fileId provided – assume the first file
        // (correct for single-file transfers).
        None => files.first().cloned().ok_or(AppError::TransferNotFound)?,
    };

    // Validate chunk index
    if chunk_index < 0 || chunk_index >= file_record.total_chunks {
        return Err(AppError::ChunkOutOfRange);
    }

    let chunk_offset = chunk_index as i64 * file_record.chunk_size;
    let expected_chunk_size = std::cmp::min(
        file_record.chunk_size,
        file_record.size.saturating_sub(chunk_offset),
    );
    if body.len() as i64 != expected_chunk_size {
        return Err(AppError::Internal(format!(
            "Invalid chunk size for {}: expected {} bytes, received {} bytes",
            file_record.name,
            expected_chunk_size,
            body.len()
        )));
    }
    let receive_path = PathBuf::from(&receive_folder);

    // Resolve the temp file path. Relay transfers write chunks into a
    // per-transfer relay temp directory (recorded in relay_files); normal
    // receives also use an isolated directory so matching filenames from
    // concurrent transfers cannot overwrite each other.
    let temp_path = if direction == "relay" {
        let relay_file = {
            let s = state.lock().await;
            s.db.get_relay_file_by_file_id(&file_record.id)?
        }
        .ok_or_else(|| AppError::Internal("Relay file record missing".to_string()))?;

        let path = PathBuf::from(&relay_file.temp_path);
        if let Some(parent) = path.parent() {
            transfer::ensure_receive_folder(parent)?;
        }
        path
    } else {
        // Ensure the transfer's isolated temporary directory exists.
        transfer::ensure_receive_folder(&receive_path)?;
        let temp_dir = transfer::temp_transfer_dir(&receive_path, &transfer_id);
        transfer::ensure_receive_folder(&temp_dir)?;
        transfer::temp_file_path(&receive_path, &transfer_id, &file_record.id)
    };

    {
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            // Chunks can arrive out of order, so preserve any bytes already
            // written by earlier chunks.
            .truncate(false)
            .write(true)
            .open(&temp_path)
            .await
            .map_err(AppError::Io)?;

        file.seek(SeekFrom::Start(chunk_offset as u64))
            .await
            .map_err(AppError::Io)?;
        file.write_all(&body).await.map_err(AppError::Io)?;
        file.flush().await.map_err(AppError::Io)?;
    }

    // Update DB
    let (completed_chunks, total_bytes, transferred_bytes) = {
        let s = state.lock().await;

        // Mark chunk as completed. This is idempotent: it returns false when
        // the chunk was already completed (e.g. client retry), so progress
        // counters are not incremented twice for the same chunk.
        let newly_completed = s.db.mark_chunk_completed(&file_record.id, chunk_index)?;

        let completed = if newly_completed {
            s.db.increment_file_completed_chunks(&file_record.id)?
        } else {
            // Re-upload of an already-completed chunk: keep the existing count.
            s.db.get_transfer_file(&file_record.id)?
                .map(|f| f.completed_chunks)
                .unwrap_or(0)
        };

        // Update transfer progress (only count bytes for new chunks)
        let transfer =
            s.db.get_transfer(&transfer_id)?
                .ok_or(AppError::TransferNotFound)?;
        let total_transferred = if newly_completed {
            let total_transferred = transfer.transferred_bytes + body.len() as i64;
            s.db.update_transfer_progress(&transfer_id, total_transferred)?;
            total_transferred
        } else {
            transfer.transferred_bytes
        };

        // Update transfer status to transferring if still pending
        if transfer.status == "pending" {
            s.db.update_transfer_status(&transfer_id, "transferring")?;
        }

        (completed, transfer.total_bytes, total_transferred)
    };

    // Track speed and broadcast progress under one state lock so the speed
    // tracker, telemetry cache, and event bus stay consistent.
    {
        let mut s = state.lock().await;
        let tracker = s
            .speed_trackers
            .entry(transfer_id.clone())
            .or_insert_with(SpeedTracker::new);
        tracker.record(transferred_bytes);
        let speed = tracker.speed_bytes_per_second();
        let progress = if total_bytes > 0 {
            (transferred_bytes as f64 / total_bytes as f64) * 100.0
        } else {
            0.0
        };
        let remaining_seconds = estimate_remaining_seconds(total_bytes, transferred_bytes, speed);
        s.transfer_telemetry.insert(
            transfer_id.clone(),
            TransferTelemetry {
                speed_bytes_per_second: speed,
                remaining_seconds,
            },
        );
        let _ = s.event_tx.send(WsEvent::TransferProgress {
            transfer_id: transfer_id.clone(),
            file_name: file_record.name.clone(),
            transferred_bytes,
            total_bytes,
            progress,
            speed_bytes_per_second: speed,
            remaining_seconds,
        });
    };

    // Broadcast transfer started on first chunk
    if completed_chunks == 1 {
        let _ = {
            let s = state.lock().await;
            s.event_tx.send(WsEvent::TransferStarted {
                transfer_id: transfer_id.clone(),
            })
        };
        record_transfer_event(&state, &transfer_id, "started", json!({})).await;
    }

    Ok(Json(json!({
        "success": true,
        "chunkIndex": chunk_index,
        "completedChunks": completed_chunks,
        "totalChunks": file_record.total_chunks,
    })))
}

/// GET /api/transfers/:id/chunks?fileId=xxx - get completed chunk indices (for resume)
///
/// When `fileId` is provided, returns the completed chunk indices for that
/// specific file (chunk indices are per-file). Without it, returns all
/// completed chunk indices across the whole transfer (legacy behavior).
pub async fn get_chunks(
    State(state): State<SharedState>,
    Path(transfer_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let token = extract_bearer_token(&headers).ok_or(AppError::InvalidToken)?;
    let device = validate_approved_session(&state, &token).await?;

    let s = state.lock().await;
    let transfer =
        s.db.get_transfer(&transfer_id)?
            .ok_or(AppError::TransferNotFound)?;
    if transfer.device_id != device.id && transfer.target_device_id.as_deref() != Some(&device.id) {
        return Err(AppError::InvalidToken);
    }

    let indices = match params.get("fileId") {
        Some(file_id) => {
            // Ensure the file belongs to this transfer before exposing chunks.
            let file =
                s.db.get_transfer_file(file_id)?
                    .ok_or(AppError::TransferNotFound)?;
            if file.transfer_id != transfer_id {
                return Err(AppError::TransferNotFound);
            }
            s.db.get_completed_chunk_indices(file_id)?
        }
        None => s.db.get_all_completed_chunk_indices(&transfer_id)?,
    };

    Ok(Json(json!({
        "transferId": transfer_id,
        "completedChunks": indices,
    })))
}

/// POST /api/transfers/:id/complete - finalize transfer
pub async fn complete_transfer(
    State(state): State<SharedState>,
    Path(transfer_id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let token = extract_bearer_token(&headers).ok_or(AppError::InvalidToken)?;
    let device = validate_approved_session(&state, &token).await?;

    let (transfer, receive_folder, files) = {
        let s = state.lock().await;
        let transfer =
            s.db.get_transfer(&transfer_id)?
                .ok_or(AppError::TransferNotFound)?;

        if transfer.device_id != device.id {
            return Err(AppError::InvalidToken);
        }
        if transfer.status == "cancelled" {
            return Err(AppError::TransferCancelled);
        }
        if transfer.status == "completed" {
            // Completion requests are idempotent. A mobile browser can retry
            // after receiving the final response without creating a false 409.
            return Ok(Json(json!({
                "success": true,
                "transferId": transfer_id,
                "alreadyCompleted": true,
            })));
        }

        let files = s.db.get_transfer_files(&transfer_id)?;
        (transfer, s.receive_folder.clone(), files)
    };

    let is_relay = transfer.direction == "relay";

    // For relay transfers this endpoint is used by the source device to signal
    // that the upload to the host finished. That logic only runs during the
    // "uploading_to_host" stage; calling it again later is an idempotent no-op
    // (ISSUE 1).
    if is_relay && transfer.relay_stage.as_deref() != Some("uploading_to_host") {
        return Ok(Json(json!({
            "success": true,
            "transferId": transfer_id,
            "relayStage": transfer.relay_stage,
        })));
    }

    // Broadcast verifying
    let _ = {
        let s = state.lock().await;
        s.event_tx.send(WsEvent::TransferVerifying {
            transfer_id: transfer_id.clone(),
        })
    };

    let receive_path = PathBuf::from(&receive_folder);
    let mut save_paths: Vec<String> = Vec::new();

    // For relay transfers, map file_id -> relay temp path so verification reads
    // the files the source device uploaded into the relay temp directory.
    let relay_temp_paths: HashMap<String, String> = if is_relay {
        let s = state.lock().await;
        s.db.get_relay_files_by_transfer(&transfer_id)?
            .into_iter()
            .map(|rf| (rf.file_id, rf.temp_path))
            .collect()
    } else {
        HashMap::new()
    };

    for file_record in &files {
        let temp_path = if is_relay {
            let tp = relay_temp_paths.get(&file_record.id).ok_or_else(|| {
                AppError::Internal(format!("Relay temp path missing for {}", file_record.name))
            })?;
            PathBuf::from(tp)
        } else {
            transfer::temp_file_path(&receive_path, &transfer_id, &file_record.id)
        };
        let expected_size = file_record.size.max(0) as u64;

        // Zero-byte files never receive chunk uploads, so the client does not
        // create their temp file. Create an empty placeholder so the rest of
        // the completion flow (hash + rename) works uniformly.
        if !temp_path.exists() && expected_size == 0 {
            if let Some(parent) = temp_path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(AppError::Io)?;
            }
            tokio::fs::write(&temp_path, [])
                .await
                .map_err(AppError::Io)?;
        }

        // Verify file exists
        if !temp_path.exists() {
            let _ = {
                let s = state.lock().await;
                s.event_tx.send(WsEvent::TransferFailed {
                    transfer_id: transfer_id.clone(),
                    error: format!("Temp file missing for {}", file_record.name),
                })
            };
            record_transfer_event(
                &state,
                &transfer_id,
                "failed",
                json!({ "error": format!("Temp file missing for {}", file_record.name) }),
            )
            .await;
            return Err(AppError::Internal(format!(
                "Temp file missing for {}",
                file_record.name
            )));
        }

        // Integrity check 1: every chunk of this file must have arrived.
        // (Zero-byte files legitimately have no chunk uploads from clients.)
        if expected_size > 0 {
            let completed_count = {
                let s = state.lock().await;
                s.db.get_completed_chunk_indices(&file_record.id)?.len()
            };
            if completed_count as i32 != file_record.total_chunks {
                let error = format!(
                    "Incomplete upload for {}: {}/{} chunks received",
                    file_record.name, completed_count, file_record.total_chunks
                );
                let _ = {
                    let s = state.lock().await;
                    s.event_tx.send(WsEvent::TransferFailed {
                        transfer_id: transfer_id.clone(),
                        error: error.clone(),
                    })
                };
                record_transfer_event(
                    &state,
                    &transfer_id,
                    "failed",
                    json!({ "error": error.clone() }),
                )
                .await;
                return Err(AppError::Internal(error));
            }
        }

        // Integrity check 2: the received bytes must match the declared size
        // exactly before the file is finalized.
        let actual_size = tokio::fs::metadata(&temp_path)
            .await
            .map_err(AppError::Io)?
            .len();
        if actual_size != expected_size {
            let error = format!(
                "Size mismatch for {}: expected {} bytes, received {}",
                file_record.name, expected_size, actual_size
            );
            let _ = {
                let s = state.lock().await;
                s.event_tx.send(WsEvent::TransferFailed {
                    transfer_id: transfer_id.clone(),
                    error: error.clone(),
                })
            };
            record_transfer_event(
                &state,
                &transfer_id,
                "failed",
                json!({ "error": error.clone() }),
            )
            .await;
            return Err(AppError::Internal(error));
        }

        // Compute SHA-256 with live progress so the transfer center shows a
        // real verification bar instead of an indefinite "verifying" state.
        let progress_tx = {
            let s = state.lock().await;
            s.event_tx.clone()
        };
        let progress_transfer_id = transfer_id.clone();
        let progress_file_id = file_record.id.clone();
        let sha256 = transfer::compute_sha256_with_progress(&temp_path, |progress| {
            let _ = progress_tx.send(WsEvent::TransferChecksumProgress {
                transfer_id: progress_transfer_id.clone(),
                file_id: progress_file_id.clone(),
                progress,
            });
        })
        .await
        .map_err(AppError::Io)?;

        if is_relay {
            // Relay: keep the file in the relay temp directory so the target
            // device can download it. Record the sha256 and the temp path
            // (used by the download endpoint). The transfer itself is NOT
            // finalized here (ISSUE 1).
            let temp_path_str = temp_path.to_string_lossy().to_string();
            {
                let s = state.lock().await;
                s.db.complete_transfer_file(&file_record.id, &sha256, &temp_path_str)?;
            }
            save_paths.push(temp_path_str);

            tracing::info!(
                "Relay file verified: {} (sha256: {})",
                file_record.name,
                sha256
            );
        } else {
            // Determine final path with unique name
            let final_path = transfer::unique_filename(&receive_path, &file_record.name);

            // Rename temp file to final destination
            tokio::fs::rename(&temp_path, &final_path)
                .await
                .map_err(AppError::Io)?;

            let final_path_str = final_path.to_string_lossy().to_string();
            save_paths.push(final_path_str.clone());

            // Update file record
            {
                let s = state.lock().await;
                s.db.complete_transfer_file(&file_record.id, &sha256, &final_path_str)?;
            }

            tracing::info!(
                "File completed: {} -> {} (sha256: {})",
                file_record.name,
                final_path_str,
                sha256
            );
        }
    }

    if is_relay {
        // Upload to host finished. Advance the relay to the waiting stage and
        // ask the target device to accept the incoming files. The transfer is
        // NOT completed yet – it completes only after the target downloads and
        // finalizes (ISSUE 1).
        set_relay_stage(&state, &transfer_id, "waiting_for_target").await;
        {
            let s = state.lock().await;
            // Return to pending so the target device can accept the relay.
            s.db.update_transfer_status(&transfer_id, "pending")?;
        }

        // Broadcast transfer.requested so the target device's UI prompts the user.
        let files_info: Vec<FileInfo> = files
            .iter()
            .map(|f| FileInfo {
                id: f.id.clone(),
                name: f.name.clone(),
                size: f.size,
            })
            .collect();
        let total_bytes = transfer.total_bytes;
        let source_name = device.name.clone();
        let _ = {
            let s = state.lock().await;
            s.event_tx.send(WsEvent::TransferRequested {
                transfer_id: transfer_id.clone(),
                source_device_name: source_name,
                files: files_info,
                total_bytes,
                target_device_id: transfer.target_device_id.clone().unwrap_or_default(),
            })
        };

        record_transfer_event(
            &state,
            &transfer_id,
            "relay_stage_changed",
            json!({ "stage": "waiting_for_target" }),
        )
        .await;

        // Clean up the speed tracker and telemetry together
        {
            let mut s = state.lock().await;
            s.speed_trackers.remove(&transfer_id);
            s.transfer_telemetry.remove(&transfer_id);
        }

        tracing::info!("Relay upload complete, waiting for target: {}", transfer_id);

        return Ok(Json(json!({
            "success": true,
            "transferId": transfer_id,
            "relayStage": "waiting_for_target",
            "files": save_paths,
        })));
    }

    // All files have moved to their final locations. This directory is unique
    // to the transfer, so removing it cannot affect another active upload.
    let temp_dir = transfer::temp_transfer_dir(&receive_path, &transfer_id);
    let _ = tokio::fs::remove_dir(&temp_dir).await;

    // Complete the transfer (non-relay)
    let save_path = save_paths.join(", ");
    {
        let s = state.lock().await;
        s.db.complete_transfer(&transfer_id, &save_path)?;
        let _ = s.event_tx.send(WsEvent::TransferCompleted {
            transfer_id: transfer_id.clone(),
            save_path: save_path.clone(),
        });
    }

    record_transfer_event(
        &state,
        &transfer_id,
        "completed",
        json!({ "savePath": save_path }),
    )
    .await;

    // Clean up the speed tracker and telemetry together
    {
        let mut s = state.lock().await;
        s.speed_trackers.remove(&transfer_id);
        s.transfer_telemetry.remove(&transfer_id);
    }

    tracing::info!("Transfer completed: {}", transfer_id);

    Ok(Json(json!({
        "success": true,
        "transferId": transfer_id,
        "savePath": save_path,
        "files": save_paths,
    })))
}

/// POST /api/transfers/:id/cancel - cancel transfer and cleanup
pub async fn cancel_transfer(
    State(state): State<SharedState>,
    Path(transfer_id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let token = extract_bearer_token(&headers).ok_or(AppError::InvalidToken)?;
    let device = validate_approved_session(&state, &token).await?;

    let (receive_folder, files, direction) = {
        let s = state.lock().await;
        let transfer =
            s.db.get_transfer(&transfer_id)?
                .ok_or(AppError::TransferNotFound)?;

        if transfer.device_id != device.id
            && transfer.target_device_id.as_deref() != Some(&device.id)
        {
            return Err(AppError::InvalidToken);
        }

        if transfer.status == "completed" || transfer.status == "cancelled" {
            // The user may press cancel while the final event is in flight.
            // Report the authoritative terminal state instead of an HTTP 409.
            return Ok(Json(json!({
                "success": true,
                "transferId": transfer_id,
                "status": transfer.status,
                "alreadyFinal": true,
            })));
        }

        let files = s.db.get_transfer_files(&transfer_id)?;
        (s.receive_folder.clone(), files, transfer.direction.clone())
    };

    // Remove temp files. Relay transfers store their temp files in a
    // per-transfer relay directory; normal receives use the receive folder.
    if direction == "relay" {
        cleanup_relay_files(&state, &transfer_id).await;
    } else {
        let receive_path = PathBuf::from(&receive_folder);
        for file_record in &files {
            let temp_path = transfer::temp_file_path(&receive_path, &transfer_id, &file_record.id);
            if temp_path.exists() {
                if let Err(e) = tokio::fs::remove_file(&temp_path).await {
                    tracing::error!("Failed to remove temp file {}: {}", temp_path.display(), e);
                }
            }
        }
        let temp_dir = transfer::temp_transfer_dir(&receive_path, &transfer_id);
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }

    // Update DB
    {
        let s = state.lock().await;
        s.db.update_transfer_status(&transfer_id, "cancelled")?;
        let _ = s.event_tx.send(WsEvent::TransferCancelled {
            transfer_id: transfer_id.clone(),
        });
    }

    record_transfer_event(&state, &transfer_id, "cancelled", json!({})).await;

    // Clean up the speed tracker and telemetry together
    {
        let mut s = state.lock().await;
        s.speed_trackers.remove(&transfer_id);
        s.transfer_telemetry.remove(&transfer_id);
    }

    tracing::info!("Transfer cancelled: {}", transfer_id);

    Ok(Json(json!({
        "success": true,
        "transferId": transfer_id,
    })))
}

/// GET /api/transfers - list transfers visible to the calling device.
pub async fn list_transfers(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let token = extract_bearer_token(&headers).ok_or(AppError::InvalidToken)?;

    let caller = validate_approved_session(&state, &token).await?;

    let s = state.lock().await;
    let transfers = s.db.list_transfers()?;

    let mut transfer_list = Vec::new();
    for t in &transfers {
        if t.device_id != caller.id && t.target_device_id.as_deref() != Some(&caller.id) {
            continue;
        }
        let files = s.db.get_transfer_files(&t.id)?;
        let file_infos: Vec<serde_json::Value> = files
            .iter()
            .map(|file| {
                json!({
                    "id": file.id,
                    "name": file.name,
                    "size": file.size,
                    "mimeType": file.mime_type,
                    "checksum": file.sha256.clone(),
                })
            })
            .collect();
        let (direction, source_device_id, target_device_id) = match t.direction.as_str() {
            "receive" => ("upload_to_host", t.device_id.clone(), "local".to_string()),
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

        transfer_list.push(json!({
            "id": t.id,
            "deviceId": source_device_id,
            "sourceDeviceId": source_device_id,
            "targetDeviceId": target_device_id,
            "direction": direction,
            "status": t.status,
            "totalBytes": t.total_bytes,
            "transferredBytes": t.transferred_bytes,
            "fileCount": t.file_count,
            "files": file_infos,
            "speedBytesPerSecond": telemetry.speed_bytes_per_second,
            "remainingSeconds": telemetry.remaining_seconds,
            "progress": progress,
            "savePath": t.save_path,
            "createdAt": t.created_at,
            "completedAt": t.completed_at,
            "relayStage": t.relay_stage,
            "acceptedAt": t.accepted_at,
            "expiresAt": t.expires_at,
            "pausedAt": t.paused_at,
        }));
    }

    Ok(Json(json!({ "transfers": transfer_list })))
}

// --- New request types for bidirectional transfers ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRelayRequest {
    pub files: Vec<FileEntry>,
    pub source_device_id: String,
    pub target_device_id: String,
    pub session_token: String,
}

/// Helper: current unix timestamp as string
fn now_ts() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

// --- Bidirectional transfer handlers ---

/// POST /api/transfers/:id/accept - Target device accepts a pending transfer.
/// Creates per-file download sessions (UUID tokens, 30-minute expiry).
pub async fn accept_transfer(
    State(state): State<SharedState>,
    Path(transfer_id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let token = extract_bearer_token(&headers).ok_or(AppError::InvalidToken)?;
    let device = validate_approved_session(&state, &token).await?;

    let files = {
        let s = state.lock().await;
        let transfer =
            s.db.get_transfer(&transfer_id)?
                .ok_or(AppError::TransferNotFound)?;

        // Only the intended target device may accept.
        if transfer.target_device_id.as_deref() != Some(&device.id) {
            return Err(AppError::InvalidToken);
        }
        if transfer.status != "pending" {
            return Err(AppError::Internal(format!(
                "Transfer is not pending (status: {})",
                transfer.status
            )));
        }
        // For relay transfers, the source must have finished uploading before
        // the target is allowed to accept.
        if let Some(ref stage) = transfer.relay_stage {
            if stage != "waiting_for_target" {
                return Err(AppError::Internal(format!(
                    "Relay is not ready for download (stage: {})",
                    stage
                )));
            }
        }

        s.db.set_transfer_accepted(&transfer_id)?;
        s.db.get_transfer_files(&transfer_id)?
    };

    // Create download sessions – tokens expire after 30 minutes.
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let expires_at = (now_secs + 30 * 60).to_string();

    let mut download_tokens: Vec<serde_json::Value> = Vec::new();
    {
        let s = state.lock().await;
        for file in &files {
            let dl_token = uuid::Uuid::new_v4().to_string();
            s.db.create_download_session(
                &transfer_id,
                &file.id,
                &device.id,
                &dl_token,
                &expires_at,
            )?;
            download_tokens.push(json!({
                "fileId": file.id,
                "fileName": file.name,
                "size": file.size,
                "downloadToken": dl_token,
            }));
        }
    }

    let _ = {
        let s = state.lock().await;
        s.event_tx.send(WsEvent::TransferAccepted {
            transfer_id: transfer_id.clone(),
        })
    };

    tracing::info!("Transfer accepted: {}", transfer_id);

    Ok(Json(json!({
        "transferId": transfer_id,
        "status": "accepted",
        "downloadTokens": download_tokens,
        "expiresAt": expires_at,
    })))
}

/// POST /api/transfers/:id/reject - Target device rejects a pending transfer.
pub async fn reject_transfer(
    State(state): State<SharedState>,
    Path(transfer_id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let token = extract_bearer_token(&headers).ok_or(AppError::InvalidToken)?;
    let device = validate_approved_session(&state, &token).await?;

    {
        let s = state.lock().await;
        let transfer =
            s.db.get_transfer(&transfer_id)?
                .ok_or(AppError::TransferNotFound)?;

        if transfer.target_device_id.as_deref() != Some(&device.id) {
            return Err(AppError::InvalidToken);
        }
        if transfer.status != "pending" {
            return Err(AppError::Internal(format!(
                "Transfer is not pending (status: {})",
                transfer.status
            )));
        }

        s.db.update_transfer_status(&transfer_id, "cancelled")?;
    }

    let _ = {
        let s = state.lock().await;
        s.event_tx.send(WsEvent::TransferRejected {
            transfer_id: transfer_id.clone(),
        })
    };

    record_transfer_event(&state, &transfer_id, "rejected", json!({})).await;

    tracing::info!("Transfer rejected: {}", transfer_id);

    Ok(Json(json!({
        "transferId": transfer_id,
        "status": "rejected",
    })))
}

/// Finish a desktop-originated download after every file has fully streamed.
/// This is driven by the server-side stream counter, not the browser's click
/// event, so relay files are never deleted before their HTTP response ends.
async fn finish_download_session(state: &SharedState, session_id: &str, transfer_id: &str) {
    let transfer = {
        let s = state.lock().await;
        match s.db.complete_download_session(session_id, transfer_id) {
            Ok(false) => return,
            Ok(true) => match s.db.get_transfer(transfer_id) {
                Ok(Some(transfer)) => transfer,
                Ok(None) => return,
                Err(error) => {
                    tracing::error!("Failed to load completed download transfer: {}", error);
                    return;
                }
            },
            Err(error) => {
                tracing::error!("Failed to complete download session: {}", error);
                return;
            }
        }
    };

    if transfer.status == "cancelled" || transfer.status == "completed" {
        return;
    }

    let save_path = transfer.save_path.clone().unwrap_or_default();
    {
        let s = state.lock().await;
        if let Err(error) = s.db.complete_transfer(transfer_id, &save_path) {
            tracing::error!("Failed to complete downloaded transfer: {}", error);
            return;
        }
        let _ = s.event_tx.send(WsEvent::TransferCompleted {
            transfer_id: transfer_id.to_string(),
            save_path,
        });
    }

    if transfer.direction == "relay" {
        set_relay_stage(state, transfer_id, "completed").await;
        cleanup_relay_files(state, transfer_id).await;
    }

    record_transfer_event(state, transfer_id, "download_completed", json!({})).await;
}

/// GET /api/transfers/:id/files/:fileId/download?token=xxx&deviceId=yyy
///
/// Streams the requested file with HTTP Range support (206 Partial Content).
/// Authentication is via a one-time download token issued during accept.
/// The real filesystem path is never exposed to the client.
pub async fn download_file(
    State(state): State<SharedState>,
    Path((transfer_id, file_id)): Path<(String, String)>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Response> {
    // The download token can come from a query parameter or the Authorization
    // header. The deviceId query parameter binds the token to a device.
    let download_token = params
        .get("token")
        .cloned()
        .or_else(|| extract_bearer_token(&headers))
        .ok_or(AppError::InvalidToken)?;

    let device_id = params
        .get("deviceId")
        .cloned()
        .ok_or(AppError::InvalidToken)?;

    // Validate the download token and check expiry.
    let session = {
        let s = state.lock().await;
        s.db.validate_download_token(&download_token, &device_id)?
            .ok_or(AppError::InvalidToken)?
    };

    if session.transfer_id != transfer_id || session.file_id != file_id {
        return Err(AppError::InvalidToken);
    }

    // Look up the file record (contains the real path) and the transfer.
    let (file_record, transfer) = {
        let s = state.lock().await;
        let file =
            s.db.get_transfer_file(&file_id)?
                .ok_or(AppError::TransferNotFound)?;
        let transfer =
            s.db.get_transfer(&transfer_id)?
                .ok_or(AppError::TransferNotFound)?;
        (file, transfer)
    };

    if transfer.status == "paused" {
        return Err(AppError::Internal("Transfer is paused".to_string()));
    }
    if transfer.status == "cancelled" {
        return Err(AppError::TransferCancelled);
    }

    let file_path_str = if let Some(path) = file_record.save_path.as_ref() {
        path.clone()
    } else if transfer.direction == "relay" {
        let s = state.lock().await;
        s.db.get_relay_file_by_file_id(&file_record.id)?
            .ok_or_else(|| AppError::Internal("Relay file record missing".to_string()))?
            .temp_path
    } else {
        return Err(AppError::Internal("No file path available".to_string()));
    };

    let path = std::path::Path::new(&file_path_str);

    // Boundary check: relay files must live inside the per-transfer relay
    // directory. Desktop-originated downloads point at user-picked source
    // paths and are trusted. This keeps a corrupted or forged database record
    // from streaming arbitrary files off the host disk.
    if transfer.direction == "relay" {
        let receive_folder = {
            let s = state.lock().await;
            s.receive_folder.clone()
        };
        let expected_dir = relay_temp_dir(std::path::Path::new(&receive_folder), &transfer_id);
        if !path.starts_with(&expected_dir) {
            return Err(AppError::Internal("Invalid relay file path".to_string()));
        }
    }

    if !path.exists() {
        return Err(AppError::Internal(
            "Source file not found on disk".to_string(),
        ));
    }

    let file_size = file_record.size.max(0) as u64;

    // Parse the Range header (if present).
    let range = headers
        .get("range")
        .and_then(|v| v.to_str().ok())
        .and_then(|r| download::parse_range_header(r, file_size));

    let mime_type = if file_record.mime_type.is_empty() {
        "application/octet-stream".to_string()
    } else {
        file_record.mime_type.clone()
    };

    // Optional bandwidth cap (host -> device) from settings, in bytes/sec.
    let speed_limit_bytes_per_sec = {
        let s = state.lock().await;
        let mbps =
            s.db.get_settings()
                .map(|settings| settings.download_speed_limit_mbps)
                .unwrap_or(0);
        if mbps > 0 {
            mbps as u64 * 1024 * 1024
        } else {
            0
        }
    };

    // Set up a shared byte counter for progress tracking.
    let bytes_counter = Arc::new(AtomicU64::new(0));

    // Build the streaming response (64KB chunks, never loads whole file).
    let response = download::stream_file_with_progress(
        path,
        range,
        file_size,
        &mime_type,
        bytes_counter.clone(),
        speed_limit_bytes_per_sec,
    )
    .await
    .map_err(AppError::Io)?;

    // Broadcast download started.
    let _ = {
        let s = state.lock().await;
        s.event_tx.send(WsEvent::TransferDownloadStarted {
            transfer_id: transfer_id.clone(),
        })
    };

    // Spawn a background task that periodically broadcasts download progress
    // while the stream is being consumed by the client.
    let event_tx = {
        let s = state.lock().await;
        s.event_tx.clone()
    };
    let progress_transfer_id = transfer_id.clone();
    let progress_file_id = file_id.clone();
    let completed_session_id = session.id.clone();
    let completion_state = state.clone();
    let total = file_size;

    if total == 0 {
        // Zero-byte file: there is nothing to stream, so a progress loop would
        // never reach its completion condition (sent >= total && total > 0)
        // and would broadcast for the entire 10-minute safety timeout. Emit a
        // single 100% event instead of spawning a task.
        let _ = event_tx.send(WsEvent::TransferDownloadProgress {
            transfer_id: progress_transfer_id,
            file_id: progress_file_id,
            transferred_bytes: 0,
            total_bytes: 0,
            progress: 100.0,
            speed_bytes_per_second: 0,
            remaining_seconds: Some(0),
        });
        tokio::spawn(async move {
            finish_download_session(&completion_state, &completed_session_id, &transfer_id).await;
        });
    } else {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
            let start = Instant::now();
            // EMA smoothing mirrors SpeedTracker so download telemetry does
            // not jitter between 500ms readings.
            let mut smoothed_speed: f64 = 0.0;

            loop {
                interval.tick().await;
                let sent = bytes_counter.load(Ordering::Relaxed);

                if sent >= total && total > 0 {
                    // Final progress event at 100%.
                    let _ = event_tx.send(WsEvent::TransferDownloadProgress {
                        transfer_id: progress_transfer_id.clone(),
                        file_id: progress_file_id.clone(),
                        transferred_bytes: sent as i64,
                        total_bytes: total as i64,
                        progress: 100.0,
                        speed_bytes_per_second: 0,
                        remaining_seconds: Some(0),
                    });
                    finish_download_session(
                        &completion_state,
                        &completed_session_id,
                        &progress_transfer_id,
                    )
                    .await;
                    break;
                }

                let elapsed = start.elapsed().as_secs_f64();
                let raw_speed = if elapsed > 0.1 {
                    sent as f64 / elapsed
                } else {
                    0.0
                };
                let speed_f64 = if raw_speed > 0.0 {
                    if smoothed_speed <= 0.0 {
                        raw_speed
                    } else {
                        0.35 * raw_speed + 0.65 * smoothed_speed
                    }
                } else {
                    smoothed_speed
                };
                smoothed_speed = speed_f64;
                let speed = speed_f64 as i64;
                let progress = if total > 0 {
                    (sent as f64 / total as f64) * 100.0
                } else {
                    0.0
                };
                let remaining = if speed > 0 && total > sent {
                    Some(((total - sent) / speed as u64) as i64)
                } else {
                    None
                };

                let _ = event_tx.send(WsEvent::TransferDownloadProgress {
                    transfer_id: progress_transfer_id.clone(),
                    file_id: progress_file_id.clone(),
                    transferred_bytes: sent as i64,
                    total_bytes: total as i64,
                    progress,
                    speed_bytes_per_second: speed,
                    remaining_seconds: remaining,
                });

                // Safety: stop broadcasting after 10 minutes even if the stream
                // has not finished (avoids leaked tasks).
                if start.elapsed().as_secs() > 600 {
                    break;
                }
            }
        });
    }

    Ok(response)
}

/// POST /api/transfers/:id/relay-complete - Target device signals that it has
/// finished downloading all relay files. Completes the transfer and cleans up.
pub async fn complete_relay_download(
    State(state): State<SharedState>,
    Path(transfer_id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let token = extract_bearer_token(&headers).ok_or(AppError::InvalidToken)?;
    let device = validate_approved_session(&state, &token).await?;

    {
        let s = state.lock().await;
        let transfer =
            s.db.get_transfer(&transfer_id)?
                .ok_or(AppError::TransferNotFound)?;

        // Only the target device may finalize the relay.
        if transfer.target_device_id.as_deref() != Some(&device.id) {
            return Err(AppError::InvalidToken);
        }
        // Must be a relay in the waiting_for_target stage (accepted is also OK
        // since accept sets status=accepted but relay_stage stays).
        if transfer.relay_stage.is_none() {
            return Err(AppError::Internal("Not a relay transfer".to_string()));
        }
        if transfer.status != "accepted" && transfer.status != "pending" {
            return Err(AppError::Internal(format!(
                "Relay cannot be completed from status: {}",
                transfer.status
            )));
        }

        // Mark completed in DB.
        let save_path = transfer.save_path.clone().unwrap_or_default();
        s.db.complete_transfer(&transfer_id, &save_path)?;
    }

    // Advance relay stage and clean up temp files.
    set_relay_stage(&state, &transfer_id, "completed").await;
    cleanup_relay_files(&state, &transfer_id).await;

    // Broadcast completion.
    let save_path = {
        let s = state.lock().await;
        let transfer =
            s.db.get_transfer(&transfer_id)?
                .ok_or(AppError::TransferNotFound)?;
        transfer.save_path.unwrap_or_default()
    };
    let _ = {
        let s = state.lock().await;
        s.event_tx.send(WsEvent::TransferCompleted {
            transfer_id: transfer_id.clone(),
            save_path,
        })
    };

    record_transfer_event(
        &state,
        &transfer_id,
        "relay_completed",
        json!({ "target_device": device.id }),
    )
    .await;

    tracing::info!("Relay transfer completed by target: {}", transfer_id);

    Ok(Json(json!({
        "success": true,
        "transferId": transfer_id,
        "status": "completed",
    })))
}

/// POST /api/transfers/:id/pause - Pause an active transfer.
pub async fn pause_transfer(
    State(state): State<SharedState>,
    Path(transfer_id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let token = extract_bearer_token(&headers).ok_or(AppError::InvalidToken)?;

    let device = validate_approved_session(&state, &token).await?;
    {
        let s = state.lock().await;
        let transfer =
            s.db.get_transfer(&transfer_id)?
                .ok_or(AppError::TransferNotFound)?;
        if transfer.device_id != device.id
            && transfer.target_device_id.as_deref() != Some(&device.id)
        {
            return Err(AppError::InvalidToken);
        }
    }

    {
        let s = state.lock().await;
        let transfer =
            s.db.get_transfer(&transfer_id)?
                .ok_or(AppError::TransferNotFound)?;
        if transfer.status == "completed" {
            return Err(AppError::TransferCompleted);
        }
        if transfer.status == "cancelled" {
            return Err(AppError::TransferCancelled);
        }
        s.db.pause_transfer(&transfer_id)?;
    }

    let _ = {
        let s = state.lock().await;
        s.event_tx.send(WsEvent::TransferPaused {
            transfer_id: transfer_id.clone(),
        })
    };

    record_transfer_event(&state, &transfer_id, "paused", json!({})).await;

    tracing::info!("Transfer paused: {}", transfer_id);

    Ok(Json(json!({
        "transferId": transfer_id,
        "status": "paused",
    })))
}

/// POST /api/transfers/:id/resume - Resume a paused transfer.
pub async fn resume_transfer(
    State(state): State<SharedState>,
    Path(transfer_id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let token = extract_bearer_token(&headers).ok_or(AppError::InvalidToken)?;
    let device = validate_approved_session(&state, &token).await?;

    {
        let s = state.lock().await;
        let transfer =
            s.db.get_transfer(&transfer_id)?
                .ok_or(AppError::TransferNotFound)?;
        if transfer.device_id != device.id
            && transfer.target_device_id.as_deref() != Some(&device.id)
        {
            return Err(AppError::InvalidToken);
        }
    }

    {
        let s = state.lock().await;
        let transfer =
            s.db.get_transfer(&transfer_id)?
                .ok_or(AppError::TransferNotFound)?;
        // Retrying a failed transfer is allowed: the uploader resumes from its
        // completed chunks instead of restarting from zero.
        if transfer.status != "paused" && transfer.status != "failed" {
            return Err(AppError::Internal(format!(
                "Transfer is not paused or failed (status: {})",
                transfer.status
            )));
        }
        s.db.resume_transfer(&transfer_id)?;
    }

    let _ = {
        let s = state.lock().await;
        s.event_tx.send(WsEvent::TransferResumed {
            transfer_id: transfer_id.clone(),
        })
    };

    record_transfer_event(&state, &transfer_id, "resumed", json!({})).await;

    tracing::info!("Transfer resumed: {}", transfer_id);

    Ok(Json(json!({
        "transferId": transfer_id,
        "status": "transferring",
    })))
}

/// GET /api/transfers/:id/resume-info - Return completed chunks / byte ranges
/// so a client can resume an interrupted download or upload.
pub async fn get_resume_info(
    State(state): State<SharedState>,
    Path(transfer_id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let token = extract_bearer_token(&headers).ok_or(AppError::InvalidToken)?;
    let device = validate_approved_session(&state, &token).await?;

    let s = state.lock().await;
    let transfer =
        s.db.get_transfer(&transfer_id)?
            .ok_or(AppError::TransferNotFound)?;
    if transfer.device_id != device.id && transfer.target_device_id.as_deref() != Some(&device.id) {
        return Err(AppError::InvalidToken);
    }

    let files = s.db.get_transfer_files(&transfer_id)?;

    let mut file_infos: Vec<serde_json::Value> = Vec::new();
    for file in &files {
        let completed_chunks = s.db.get_completed_chunk_indices(&file.id)?;
        file_infos.push(json!({
            "fileId": file.id,
            "fileName": file.name,
            "size": file.size,
            "status": file.status,
            "completedChunks": completed_chunks,
            "totalChunks": file.total_chunks,
        }));
    }

    Ok(Json(json!({
        "transferId": transfer_id,
        "status": transfer.status,
        "direction": transfer.direction,
        "transferredBytes": transfer.transferred_bytes,
        "totalBytes": transfer.total_bytes,
        "files": file_infos,
    })))
}

/// GET /api/devices/me/transfers/pending - List transfers that are waiting
/// for the current device to accept / reject.
pub async fn get_pending_transfers(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let token = extract_bearer_token(&headers).ok_or(AppError::InvalidToken)?;
    let device = validate_approved_session(&state, &token).await?;

    let s = state.lock().await;
    let transfers = s.db.get_pending_transfers_for_device(&device.id)?;

    let mut result: Vec<serde_json::Value> = Vec::new();
    for t in &transfers {
        let files = s.db.get_transfer_files(&t.id)?;
        let file_names: Vec<String> = files.iter().map(|f| f.name.clone()).collect();
        let file_infos: Vec<serde_json::Value> = files
            .iter()
            .map(|file| {
                json!({
                    "id": file.id,
                    "name": file.name,
                    "size": file.size,
                })
            })
            .collect();

        // Look up the source device name for display.
        let source_name =
            s.db.get_device_by_id(&t.device_id)?
                .map(|d| d.name)
                .unwrap_or_default();

        result.push(json!({
            "id": t.id,
            "direction": t.direction,
            "status": t.status,
            "totalBytes": t.total_bytes,
            "fileCount": t.file_count,
            "fileNames": file_names,
            "files": file_infos,
            "sourceDeviceName": source_name,
            "createdAt": t.created_at,
        }));
    }

    Ok(Json(json!({ "transfers": result })))
}

/// POST /api/transfers/relay - Create a relay task (phone A → PC → phone B).
/// Files are first uploaded to the host, then the target device downloads them.
pub async fn create_relay(
    State(state): State<SharedState>,
    Json(body): Json<CreateRelayRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let device = validate_approved_session(&state, &body.session_token).await?;
    if body.source_device_id != device.id {
        return Err(AppError::InvalidToken);
    }

    // The database owns its own lock, so DB-heavy work never holds the global
    // state lock that WebSocket fan-out and heartbeat also contend on.
    let db = {
        let s = state.lock().await;
        s.db.clone()
    };

    let (source_approved, target_approved, target_online, max_file_size, receive_folder) = {
        let source = db
            .get_device_by_id(&body.source_device_id)?
            .ok_or(AppError::DeviceNotFound)?;
        let target = db
            .get_device_by_id(&body.target_device_id)?
            .ok_or(AppError::DeviceNotFound)?;
        let max_file_size = db.get_settings()?.max_file_size;
        // Only the tiny online/receive-folder reads need the state lock.
        let s = state.lock().await;
        (
            source.approved,
            target.approved,
            s.connected_devices.contains_key(&body.target_device_id),
            max_file_size,
            s.receive_folder.clone(),
        )
    };
    if !source_approved || !target_approved {
        return Err(AppError::DeviceNotApproved);
    }
    if !target_online {
        return Err(AppError::Internal(
            "Target device is offline. Keep its LanNook page open and try again.".to_string(),
        ));
    }

    let total_bytes = validate_transfer_files(&body.files, max_file_size)?;
    validate_transfer_chunk_budget(&body.files)?;

    // Bound how many transfers one device may keep in flight (anti-DoS).
    let active = db.count_active_transfers_for_device(&device.id)?;
    if active >= MAX_ACTIVE_TRANSFERS_PER_DEVICE {
        return Err(AppError::Internal(format!(
            "Too many active transfers for this device (maximum: {})",
            MAX_ACTIVE_TRANSFERS_PER_DEVICE
        )));
    }

    let transfer_id = uuid::Uuid::new_v4().to_string();
    let now = now_ts();

    // Resolve and create the per-transfer relay temp directory. The source
    // device's chunks are written here (upload_chunk) and later streamed back
    // to the target device (download_file) before being cleaned up (ISSUE 1).
    let relay_dir = relay_temp_dir(std::path::Path::new(&receive_folder), &transfer_id);
    tokio::fs::create_dir_all(&relay_dir)
        .await
        .map_err(AppError::Io)?;

    // Opportunistically remove relay temp files whose retention has expired so
    // stale files from abandoned relays do not accumulate (ISSUE 1).
    cleanup_expired_relays(&state).await;

    let transfer = TransferRecord {
        id: transfer_id.clone(),
        device_id: body.source_device_id.clone(),
        direction: "relay".to_string(),
        status: "pending".to_string(),
        total_bytes,
        transferred_bytes: 0,
        file_count: body.files.len() as i32,
        save_path: None,
        created_at: now,
        completed_at: None,
        target_device_id: Some(body.target_device_id.clone()),
        relay_stage: Some("uploading_to_host".to_string()),
        accepted_at: None,
        expires_at: None,
        paused_at: None,
    };

    let mut file_responses: Vec<TransferFileResponse> = Vec::new();

    db.insert_transfer(&transfer)?;

    // Relay cleanup: temp files are cleaned up after 24 hours.
    let cleanup_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() + 24 * 3600)
        .unwrap_or(0);
    let cleanup_at = cleanup_secs.to_string();

    for file_entry in &body.files {
        let file_name = transfer::sanitize_filename(&file_entry.name)?;
        let file_id = uuid::Uuid::new_v4().to_string();
        let total_chunks_i64 = if file_entry.size == 0 {
            0
        } else {
            (file_entry.size - 1) / CHUNK_SIZE + 1
        };
        let total_chunks = i32::try_from(total_chunks_i64).map_err(|_| {
            AppError::Internal("File requires too many transfer chunks".to_string())
        })?;

        let file_record = TransferFileRecord {
            id: file_id.clone(),
            transfer_id: transfer_id.clone(),
            name: file_name.clone(),
            size: file_entry.size,
            mime_type: file_entry.mime_type.clone(),
            chunk_size: CHUNK_SIZE,
            total_chunks,
            completed_chunks: 0,
            sha256: None,
            save_path: None,
            status: "pending".to_string(),
        };
        db.insert_transfer_file(&file_record)?;

        // Create chunk records so the source device can upload in chunks.
        let mut chunks = Vec::new();
        for i in 0..total_chunks {
            let offset = i as i64 * CHUNK_SIZE;
            let chunk_size = std::cmp::min(CHUNK_SIZE, file_entry.size - offset);
            chunks.push(ChunkRecord {
                id: uuid::Uuid::new_v4().to_string(),
                file_id: file_id.clone(),
                chunk_index: i,
                offset,
                size: chunk_size,
                completed: false,
            });
        }
        db.insert_chunks(&chunks)?;

        // Relay temp path: {relay_dir}/.{filename}.uploading. upload_chunk
        // writes chunks here and complete_transfer verifies/hashes it.
        let temp_path = relay_dir.join(format!(".{}.uploading", file_id));

        let relay_file = RelayFile {
            id: uuid::Uuid::new_v4().to_string(),
            transfer_id: transfer_id.clone(),
            file_id: file_id.clone(),
            temp_path: temp_path.to_string_lossy().to_string(),
            cleanup_at: cleanup_at.clone(),
            cleaned: false,
        };
        db.insert_relay_file(&relay_file)?;

        file_responses.push(TransferFileResponse {
            id: file_id,
            name: file_name,
            chunk_size: CHUNK_SIZE,
            total_chunks,
        });
    }

    let event_tx = {
        let s = state.lock().await;
        s.event_tx.clone()
    };
    let _ = event_tx.send(WsEvent::TransferRelayStageChanged {
        transfer_id: transfer_id.clone(),
        stage: "uploading_to_host".to_string(),
    });

    tracing::info!(
        "Relay transfer created: {} ({} files, {} bytes) {} -> {}",
        transfer_id,
        body.files.len(),
        total_bytes,
        body.source_device_id,
        body.target_device_id
    );

    Ok(Json(json!({
        "transferId": transfer_id,
        "status": "pending",
        "relayStage": "uploading_to_host",
        "files": file_responses,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, approved: bool) -> DeviceRecord {
        DeviceRecord {
            id: id.to_string(),
            name: format!("Device {id}"),
            platform: "android".to_string(),
            device_type: "phone".to_string(),
            user_agent: "test-agent".to_string(),
            client_id: format!("client-{id}"),
            session_token: format!("session-{id}"),
            approved,
            trusted: false,
            ip: "192.168.1.5".to_string(),
            created_at: "0".to_string(),
            last_seen: "1".to_string(),
            approved_until: None,
        }
    }

    fn file(name: &str, size: i64) -> FileEntry {
        FileEntry {
            name: name.to_string(),
            size,
            mime_type: "application/octet-stream".to_string(),
        }
    }

    #[test]
    fn transfer_metadata_accepts_valid_files() {
        let files = vec![file("report.pdf", 2), file("photo.png", 3)];

        assert_eq!(validate_transfer_files(&files, 10).unwrap(), 5);
    }

    #[test]
    fn transfer_metadata_accepts_empty_and_rejects_oversized_files() {
        assert_eq!(
            validate_transfer_files(&[file("empty.txt", 0)], 10).unwrap(),
            0
        );
        assert!(validate_transfer_files(&[file("large.bin", 11)], 10).is_err());
    }

    #[test]
    fn transfer_metadata_rejects_unsafe_filenames() {
        assert!(validate_transfer_files(&[file("CON", 1)], 10).is_err());
    }

    #[test]
    fn remaining_time_stays_visible_until_the_last_byte_arrives() {
        assert_eq!(estimate_remaining_seconds(1_001, 1_000, 100), Some(1));
        assert_eq!(estimate_remaining_seconds(2_000, 1_000, 100), Some(10));
        assert_eq!(estimate_remaining_seconds(1_000, 1_000, 100), None);
        assert_eq!(estimate_remaining_seconds(2_000, 1_000, 0), None);
    }

    #[test]
    fn mobile_targets_include_the_reachable_host_and_approved_peers() {
        let caller = device("caller", true);
        let caller_id = caller.id.clone();
        let peer = device("peer", true);
        let unapproved = device("pending", false);
        let mut connected = HashMap::new();
        connected.insert(peer.id.clone(), 1);

        let targets = mobile_target_list(
            &caller_id,
            "FENGQIAO",
            Some("192.168.1.4"),
            &[caller, peer, unapproved],
            &connected,
        );

        assert_eq!(targets[0]["id"], "desktop");
        assert_eq!(targets[0]["name"], "FENGQIAO");
        assert_eq!(targets[0]["deviceType"], "desktop");
        assert_eq!(targets[0]["online"], true);
        assert_eq!(targets[1]["id"], "peer");
        assert_eq!(targets.len(), 2);
    }
}
