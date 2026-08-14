use serde::Serialize;
use tauri::State;

use crate::server::{FileInfo, SharedState, WsEvent};
use crate::storage::TransferRecord;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResult {
    pub success: bool,
    pub error: Option<String>,
}

/// The result returned after the desktop has created a transfer invitation.
/// Creating an invitation is intentionally distinct from completing the file
/// download on the receiving device.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTransferResult {
    pub success: bool,
    pub transfer_id: String,
    pub status: String,
    pub total_bytes: i64,
    pub file_count: usize,
}

/// Metadata used by the desktop UI after a native file picker or a native
/// drag-and-drop operation returns local paths.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickedFile {
    pub path: String,
    pub name: String,
    pub size: i64,
}

#[tauri::command]
pub async fn get_file_metadata(
    state: State<'_, SharedState>,
    file_paths: Vec<String>,
) -> Result<Vec<PickedFile>, String> {
    let max_file_size = {
        let s = state.lock().await;
        s.db.get_settings()
            .map(|settings| settings.max_file_size)
            .unwrap_or(0)
    };

    file_paths
        .into_iter()
        .map(|path_str| {
            let path = std::path::Path::new(&path_str);
            let metadata = std::fs::metadata(path)
                .map_err(|e| format!("Failed to read file '{}': {}", path_str, e))?;
            if !metadata.is_file() {
                return Err(format!("'{}' is not a file", path_str));
            }

            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .ok_or_else(|| format!("'{}' has no file name", path_str))?;

            let size = i64::try_from(metadata.len())
                .map_err(|_| "File is too large to transfer".to_string())?;
            if max_file_size > 0 && size > max_file_size {
                return Err(format!(
                    "'{}' exceeds the configured max file size",
                    path_str
                ));
            }

            Ok(PickedFile {
                path: path_str,
                name,
                size,
            })
        })
        .collect()
}

/// Send files from the desktop to a target device.
///
/// Reads file metadata from disk, creates a "download_from_host" transfer
/// record, and broadcasts a `transfer.requested` event so the target device
/// can prompt the user to accept.
#[tauri::command]
pub async fn send_files_to_device(
    state: State<'_, SharedState>,
    file_paths: Vec<String>,
    target_device_id: String,
) -> Result<SendTransferResult, String> {
    if file_paths.is_empty() {
        return Err("请至少选择一个要发送的文件。".to_string());
    }

    // Read metadata for every file up-front so we fail early on bad paths.
    let mut files: Vec<(String, String, i64)> = Vec::new(); // (path, name, size)
    let mut total_bytes: i64 = 0;

    for path_str in &file_paths {
        let path = std::path::Path::new(path_str);
        let metadata =
            std::fs::metadata(path).map_err(|e| format!("无法读取文件“{}”：{}", path_str, e))?;

        if !metadata.is_file() {
            return Err(format!("“{}”不是普通文件。", path_str));
        }

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let size = metadata.len() as i64;
        total_bytes += size;
        files.push((path_str.clone(), name, size));
    }

    let s = state.lock().await;

    // The desktop send path must honour the same per-file size limit as the
    // mobile upload path (validate_transfer_files) so the setting applies to
    // both directions.
    let max_file_size =
        s.db.get_settings()
            .map(|settings| settings.max_file_size)
            .unwrap_or(0);
    for (_, name, size) in &files {
        if max_file_size > 0 && *size > max_file_size {
            return Err(format!("文件“{}”超过设置的文件大小上限。", name));
        }
    }

    // Only approved devices can be selected as receive targets.
    let target =
        s.db.get_device_by_id(&target_device_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "找不到目标设备，请重新选择设备。".to_string())?;
    if !target.approved {
        return Err(format!(
            "{} 尚未授权，请先在电脑的“设备”页面批准它。",
            target.name
        ));
    }
    if !s.connected_devices.contains_key(&target_device_id) {
        return Err(format!(
            "{} 已离线，请保持手机上的 LanNook 页面打开后重试。",
            target.name
        ));
    }

    let transfer_id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default();

    let transfer = TransferRecord {
        id: transfer_id.clone(),
        device_id: "desktop".to_string(), // Synthetic desktop device (created in migration)
        direction: "download_from_host".to_string(),
        status: "pending".to_string(),
        total_bytes,
        transferred_bytes: 0,
        file_count: files.len() as i32,
        save_path: None,
        created_at: now,
        completed_at: None,
        target_device_id: Some(target_device_id.clone()),
        relay_stage: None,
        accepted_at: None,
        expires_at: None,
        paused_at: None,
    };

    s.db.insert_transfer(&transfer).map_err(|e| e.to_string())?;

    let mut files_info: Vec<FileInfo> = Vec::new();
    let mut checksum_jobs: Vec<(String, String)> = Vec::new();

    for (path, name, size) in &files {
        let file_id = uuid::Uuid::new_v4().to_string();
        let file_record = crate::storage::TransferFileRecord {
            id: file_id.clone(),
            transfer_id: transfer_id.clone(),
            name: name.clone(),
            size: *size,
            mime_type: String::new(),
            chunk_size: crate::transfer::CHUNK_SIZE,
            total_chunks: 0,
            completed_chunks: 0,
            sha256: None,
            save_path: Some(path.clone()),
            status: "pending".to_string(),
        };
        s.db.insert_transfer_file(&file_record)
            .map_err(|e| e.to_string())?;

        checksum_jobs.push((file_id.clone(), path.clone()));

        files_info.push(FileInfo {
            id: file_id.clone(),
            name: name.clone(),
            size: *size,
        });
    }

    // Broadcast so the target device's UI can show an incoming-transfer prompt.
    let device_name = s.device_name.clone();
    let _ = s.event_tx.send(WsEvent::TransferRequested {
        transfer_id: transfer_id.clone(),
        source_device_name: device_name,
        files: files_info,
        total_bytes,
        target_device_id: target_device_id.clone(),
    });

    tracing::info!(
        "Desktop send transfer created: {} ({} files, {} bytes) -> {}",
        transfer_id,
        files.len(),
        total_bytes,
        target_device_id
    );

    // Calculating a large source file's digest must not delay the recipient's
    // invitation. Persist it as soon as it is ready and notify the desktop UI
    // so "SHA-256" never remains a permanent placeholder.
    let checksum_db = s.db.clone();
    let checksum_events = s.event_tx.clone();
    let checksum_transfer_id = transfer_id.clone();
    drop(s);
    tokio::spawn(async move {
        for (file_id, path) in checksum_jobs {
            match crate::transfer::compute_sha256_with_progress(
                std::path::Path::new(&path),
                |progress| {
                    let _ =
                        checksum_events.send(crate::server::WsEvent::TransferChecksumProgress {
                            transfer_id: checksum_transfer_id.clone(),
                            file_id: file_id.clone(),
                            progress,
                        });
                },
            )
            .await
            {
                Ok(checksum) => {
                    if let Err(error) =
                        checksum_db.update_transfer_file_checksum(&file_id, &checksum)
                    {
                        tracing::warn!(
                            "Failed to store source checksum for {}: {}",
                            file_id,
                            error
                        );
                        continue;
                    }
                    let _ = checksum_events.send(WsEvent::TransferChecksumReady {
                        transfer_id: checksum_transfer_id.clone(),
                        file_id,
                        checksum,
                    });
                }
                Err(error) => tracing::warn!(
                    "Failed to calculate source checksum for {}: {}",
                    path,
                    error
                ),
            }
        }
    });

    Ok(SendTransferResult {
        success: true,
        transfer_id,
        status: "pending".to_string(),
        total_bytes,
        file_count: files.len(),
    })
}

/// Get transfers that are waiting for the desktop user to act on (e.g.
/// incoming transfers from phones that need approval).
#[tauri::command]
pub async fn get_pending_transfers(state: State<'_, SharedState>) -> Result<String, String> {
    let s = state.lock().await;

    // Pending transfers are those with status "pending" regardless of direction.
    let all = s.db.list_transfers().map_err(|e| e.to_string())?;
    let pending: Vec<&TransferRecord> = all.iter().filter(|t| t.status == "pending").collect();

    let mut result: Vec<serde_json::Value> = Vec::new();
    for t in &pending {
        let files = s.db.get_transfer_files(&t.id).map_err(|e| e.to_string())?;
        let file_names: Vec<String> = files.iter().map(|f| f.name.clone()).collect();

        let device_name =
            s.db.get_device_by_id(&t.device_id)
                .ok()
                .flatten()
                .map(|d| d.name)
                .unwrap_or_default();

        result.push(serde_json::json!({
            "id": t.id,
            "deviceId": t.device_id,
            "deviceName": device_name,
            "direction": t.direction,
            "status": t.status,
            "totalBytes": t.total_bytes,
            "fileCount": t.file_count,
            "fileNames": file_names,
            "targetDeviceId": t.target_device_id,
            "createdAt": t.created_at,
        }));
    }

    serde_json::to_string(&serde_json::json!({ "transfers": result })).map_err(|e| e.to_string())
}

/// Pause an active transfer from the desktop UI.
#[tauri::command]
pub async fn pause_transfer(
    state: State<'_, SharedState>,
    transfer_id: String,
) -> Result<CommandResult, String> {
    let s = state.lock().await;

    let transfer =
        s.db.get_transfer(&transfer_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Transfer '{}' not found", transfer_id))?;

    if transfer.status == "completed" {
        return Ok(CommandResult {
            success: false,
            error: Some("Transfer already completed".to_string()),
        });
    }
    if transfer.status == "cancelled" {
        return Ok(CommandResult {
            success: false,
            error: Some("Transfer already cancelled".to_string()),
        });
    }

    match s.db.pause_transfer(&transfer_id) {
        Ok(()) => {
            let _ = s.event_tx.send(WsEvent::TransferPaused {
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

/// Resume a paused transfer from the desktop UI.
#[tauri::command]
pub async fn resume_transfer(
    state: State<'_, SharedState>,
    transfer_id: String,
) -> Result<CommandResult, String> {
    let s = state.lock().await;

    let transfer =
        s.db.get_transfer(&transfer_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Transfer '{}' not found", transfer_id))?;

    // Retrying a failed transfer is allowed: the phone side resumes from its
    // completed chunks, so a verification failure or a dropped connection
    // never forces the user to restart from zero.
    if transfer.status != "paused" && transfer.status != "failed" {
        return Ok(CommandResult {
            success: false,
            error: Some(format!(
                "Transfer is not paused or failed (status: {})",
                transfer.status
            )),
        });
    }

    match s.db.resume_transfer(&transfer_id) {
        Ok(_) => {
            let _ = s.event_tx.send(WsEvent::TransferResumed {
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
