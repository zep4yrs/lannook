use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;

use crate::server::{SharedState, WsEvent, MAX_WS_CONNECTIONS, MAX_WS_PER_DEVICE};
use crate::storage::DeviceRecord;

fn add_device_connection(connections: &mut HashMap<String, usize>, device_id: &str) -> bool {
    let count = connections.entry(device_id.to_string()).or_insert(0);
    let became_online = *count == 0;
    *count += 1;
    became_online
}

fn remove_device_connection(connections: &mut HashMap<String, usize>, device_id: &str) -> bool {
    let Some(count) = connections.get_mut(device_id) else {
        return false;
    };
    if *count > 1 {
        *count -= 1;
        false
    } else {
        connections.remove(device_id);
        true
    }
}

/// Announce a device which has opened a WebSocket session. A device that is
/// not yet approved needs an authorization prompt, rather than a generic
/// online event that the desktop has no reason to surface to the user.
fn device_session_event(device: &DeviceRecord) -> WsEvent {
    if device.approved {
        WsEvent::DeviceConnected {
            device_id: device.id.clone(),
            name: device.name.clone(),
            platform: device.platform.clone(),
            device_type: device.device_type.clone(),
            ip: device.ip.clone(),
            approved: true,
        }
    } else {
        WsEvent::DeviceApprovalRequired {
            device_id: device.id.clone(),
            name: device.name.clone(),
            ip: device.ip.clone(),
            platform: device.platform.clone(),
            device_type: device.device_type.clone(),
            user_agent: device.user_agent.clone(),
        }
    }
}

/// GET /ws?token=xxx - upgrade to WebSocket
///
/// Validates the token (either the desktop-only control token, or a session
/// token for a mobile/remote device), then subscribes to the
/// broadcast channel and forwards WsEvents as JSON to the client.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    let token = params.get("token").cloned().unwrap_or_default();

    // Validate token and remember the device identity for targeted events and
    // the desktop online indicator. The desktop control token has no device
    // id, while mobile sessions do.
    let device = {
        let s = state.lock().await;
        if token == s.desktop_control_token {
            None
        } else {
            match s.db.get_device_by_session_token(&token) {
                Ok(Some(device)) => {
                    // An expired approval must not keep its WebSocket alive.
                    // Revoke it and reject the connection so the phone is
                    // forced through the approval flow again.
                    match s.db.revoke_device_if_expired(&device) {
                        Ok(true) => {
                            tracing::info!("WebSocket connection rejected: approval expired");
                            return (axum::http::StatusCode::UNAUTHORIZED, "Invalid token")
                                .into_response();
                        }
                        Ok(false) => Some(device),
                        Err(error) => {
                            tracing::error!("Failed to check device approval expiry: {}", error);
                            return (
                                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                                "Internal error",
                            )
                                .into_response();
                        }
                    }
                }
                _ => {
                    tracing::error!("WebSocket connection rejected: invalid token");
                    return (axum::http::StatusCode::UNAUTHORIZED, "Invalid token").into_response();
                }
            }
        }
    };

    // Reject before the upgrade when the server is saturated: a flood of
    // handshakes must never grow the broadcast fan-out without bound.
    {
        let mut s = state.lock().await;
        if s.ws_connection_count >= MAX_WS_CONNECTIONS {
            tracing::warn!("WebSocket rejected: connection limit reached");
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "Too many connections",
            )
                .into_response();
        }
        if let Some(device) = &device {
            let sessions = s.connected_devices.get(&device.id).copied().unwrap_or(0);
            if sessions >= MAX_WS_PER_DEVICE {
                tracing::warn!("WebSocket rejected: device session limit reached");
                return (
                    axum::http::StatusCode::TOO_MANY_REQUESTS,
                    "Too many sessions for this device",
                )
                    .into_response();
            }
        }
        s.ws_connection_count += 1;
    }

    // Subscribe before upgrading so the new socket cannot miss an event that
    // races with the handshake. Online accounting starts only after Axum has
    // actually completed the WebSocket upgrade.
    let rx = {
        let s = state.lock().await;
        s.event_tx.subscribe()
    };

    tracing::info!("WebSocket connection accepted");

    ws.on_upgrade(move |socket| handle_socket(socket, rx, state, device))
}

/// Handle an individual WebSocket connection.
/// Forwards broadcast events to the client and handles incoming messages (ping/pong).
async fn handle_socket(
    socket: WebSocket,
    mut rx: tokio::sync::broadcast::Receiver<crate::server::WsEvent>,
    state: SharedState,
    device: Option<DeviceRecord>,
) {
    let (mut sender, mut receiver) = socket.split();
    let device_id = device.as_ref().map(|entry| entry.id.clone());

    if let Some(device) = &device {
        let mut s = state.lock().await;
        if add_device_connection(&mut s.connected_devices, &device.id) {
            let _ = s.event_tx.send(device_session_event(device));
        }
    }

    let mut heartbeat = tokio::time::interval(tokio::time::Duration::from_secs(20));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_activity = tokio::time::Instant::now();

    loop {
        tokio::select! {
            event = rx.recv() => match event {
                Ok(event) => {
                    if !should_deliver_event(&event, device_id.as_deref(), &state).await {
                        continue;
                    }
                    let json = match serde_json::to_string(&event) {
                        Ok(j) => j,
                        Err(e) => {
                            tracing::error!("Failed to serialize WsEvent: {}", e);
                            continue;
                        }
                    };

                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::error!("WebSocket client lagged, skipped {} events", n);
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            },
            message = receiver.next() => match message {
                Some(Ok(Message::Ping(data))) => {
                    last_activity = tokio::time::Instant::now();
                    if sender.send(Message::Pong(data)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Pong(_)))
                | Some(Ok(Message::Text(_)))
                | Some(Ok(Message::Binary(_))) => {
                    last_activity = tokio::time::Instant::now();
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Err(e)) => {
                    tracing::error!("WebSocket receive error: {}", e);
                    break;
                }
            },
            _ = heartbeat.tick() => {
                // 120 s instead of 60 s: a phone browser may throttle timers
                // or suspend its socket during a screen lock / Wi-Fi handoff.
                // The client reconnects with backoff when a socket does die.
                if last_activity.elapsed() > tokio::time::Duration::from_secs(120) {
                    tracing::warn!("WebSocket heartbeat timed out");
                    break;
                }
                if sender.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
        }
    }

    {
        let mut s = state.lock().await;
        s.ws_connection_count = s.ws_connection_count.saturating_sub(1);
        if let Some(device_id) = device_id {
            if remove_device_connection(&mut s.connected_devices, &device_id) {
                // A phone browser may suspend its socket during a screen lock or
                // Wi-Fi handoff. One-time approval remains valid for this desktop
                // service lifetime and is reset on stop, restart, or token reset.
                let _ = s.event_tx.send(WsEvent::DeviceDisconnected { device_id });
            }
        }
    }

    tracing::info!("WebSocket connection closed");
}

/// Extract the transfer id carried by a transfer-lifecycle event, if any.
fn transfer_id_of(event: &WsEvent) -> Option<&str> {
    match event {
        WsEvent::TransferCreated { transfer_id, .. }
        | WsEvent::TransferStarted { transfer_id }
        | WsEvent::TransferProgress { transfer_id, .. }
        | WsEvent::TransferChecksumReady { transfer_id, .. }
        | WsEvent::TransferChecksumProgress { transfer_id, .. }
        | WsEvent::TransferVerifying { transfer_id }
        | WsEvent::TransferCompleted { transfer_id, .. }
        | WsEvent::TransferCancelled { transfer_id }
        | WsEvent::TransferFailed { transfer_id, .. }
        | WsEvent::TransferDeleted { transfer_id }
        | WsEvent::TransferAccepted { transfer_id }
        | WsEvent::TransferRejected { transfer_id }
        | WsEvent::TransferExpired { transfer_id }
        | WsEvent::TransferPaused { transfer_id }
        | WsEvent::TransferResumed { transfer_id }
        | WsEvent::TransferDownloadStarted { transfer_id }
        | WsEvent::TransferDownloadProgress { transfer_id, .. }
        | WsEvent::TransferRelayStageChanged { transfer_id, .. } => Some(transfer_id),
        _ => None,
    }
}

/// Decide whether a client may receive a broadcast event. The desktop control
/// channel (no device id) observes everything. A mobile browser only receives
/// invitations/approvals explicitly addressed to it, plus transfer-lifecycle
/// and progress events whose transfer belongs to it - so another device's
/// names, sizes, or progress never leak, while the owning phone sees live
/// completion (the "received by the other side" confirmation).
async fn should_deliver_event(
    event: &WsEvent,
    device_id: Option<&str>,
    state: &SharedState,
) -> bool {
    let Some(id) = device_id else {
        return true;
    };
    match event {
        WsEvent::TransferRequested {
            target_device_id, ..
        } => id == target_device_id,
        WsEvent::DeviceApproved {
            device_id: device, ..
        } => id == device,
        WsEvent::DeviceRejected {
            device_id: device, ..
        } => id == device,
        // Remaining transfer.* events are only useful to the owning device.
        _ => {
            let Some(transfer_id) = transfer_id_of(event) else {
                return false;
            };
            let s = state.lock().await;
            match s.db.get_transfer(transfer_id) {
                Ok(Some(transfer)) => {
                    transfer.device_id == id || transfer.target_device_id.as_deref() == Some(id)
                }
                _ => false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{add_device_connection, device_session_event, remove_device_connection};
    use crate::storage::DeviceRecord;
    use std::collections::HashMap;

    fn device(approved: bool, trusted: bool) -> DeviceRecord {
        DeviceRecord {
            id: "device-1".to_string(),
            name: "Android Device".to_string(),
            platform: "android".to_string(),
            device_type: "phone".to_string(),
            user_agent: "test-agent".to_string(),
            client_id: "client-1".to_string(),
            session_token: "session-1".to_string(),
            approved,
            trusted,
            ip: "192.168.1.5".to_string(),
            created_at: "0".to_string(),
            last_seen: "0".to_string(),
            approved_until: None,
        }
    }

    #[test]
    fn device_stays_online_until_its_last_socket_closes() {
        let mut connections = HashMap::new();
        assert!(add_device_connection(&mut connections, "device-1"));
        assert!(!add_device_connection(&mut connections, "device-1"));
        assert!(!remove_device_connection(&mut connections, "device-1"));
        assert!(connections.contains_key("device-1"));
        assert!(remove_device_connection(&mut connections, "device-1"));
        assert!(!connections.contains_key("device-1"));
    }

    #[test]
    fn unapproved_reconnecting_device_reopens_the_authorization_request() {
        let payload = serde_json::to_value(device_session_event(&device(false, false))).unwrap();

        assert_eq!(payload["type"], "device.approval_required");
        assert_eq!(payload["payload"]["deviceId"], "device-1");
        assert_eq!(payload["payload"]["name"], "Android Device");
    }
}
