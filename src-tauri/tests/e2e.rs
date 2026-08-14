use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::Mutex;

use lannook_lib::server::{
    generate_pairing_code, start_server, AppState, PairingPin, ServiceStatus, SharedState,
};
use lannook_lib::storage::Database;

fn unique_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("lannook-e2e-{label}-{unique}"))
}

async fn start_test_server() -> (SharedState, String, PathBuf) {
    let db_path = unique_dir("db").join("test.db");
    let receive = unique_dir("receive");
    std::fs::create_dir_all(&receive).expect("create receive dir");

    let db = Arc::new(Database::open(&db_path).expect("open db"));
    db.save_settings(&lannook_lib::storage::Settings {
        device_name: "Test Host".to_string(),
        receive_folder: receive.to_string_lossy().to_string(),
        require_approval: true,
        auto_approve_known: true,
        port: 0,
        max_file_size: 10 * 1024 * 1024,
        theme_mode: "system".to_string(),
        authorization_expiry_hours: 0,
        download_speed_limit_mbps: 0,
    })
    .expect("save settings");

    // Find a free port to hand to the server.
    let probe = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);

    let state: SharedState = Arc::new(Mutex::new(AppState::new(db)));
    state.lock().await.port = port;
    state.lock().await.receive_folder = receive.to_string_lossy().to_string();

    let serve_state = state.clone();
    let _server = tokio::spawn(async move {
        let _ = start_server(serve_state, "missing-frontend".to_string()).await;
    });

    // Wait until the listener is actually bound.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if state.lock().await.status == ServiceStatus::Running {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("server did not start in time");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let base = format!("http://127.0.0.1:{port}");
    (state, base, receive)
}

#[tokio::test]
async fn mobile_upload_reaches_receive_folder_with_sha256() {
    let (state, base, receive) = start_test_server().await;
    let client = reqwest::Client::new();

    // 1. Register a device (pairing token from state).
    let pairing_token = state.lock().await.connection_token.clone();
    let register: serde_json::Value = client
        .post(format!("{base}/api/devices/register"))
        .json(&serde_json::json!({
            "name": "e2e-phone",
            "platform": "android",
            "deviceType": "phone",
            "userAgent": "test-agent",
            "clientId": "e2e-client-id-1234567890",
            "token": pairing_token,
        }))
        .send()
        .await
        .expect("register request")
        .json::<serde_json::Value>()
        .await
        .expect("register json");

    let device_id = register["deviceId"].as_str().expect("deviceId").to_string();
    let session_token = register["sessionToken"]
        .as_str()
        .expect("sessionToken")
        .to_string();
    assert_eq!(register["approved"].as_bool(), Some(false));

    // 2. Approve the device directly (as the desktop user would).
    state
        .lock()
        .await
        .db
        .set_device_access(&device_id, true, true)
        .expect("approve device");

    // 3. Create a transfer with two files (one empty, one small).
    let created: serde_json::Value = client
        .post(format!("{base}/api/transfers"))
        .json(&serde_json::json!({
            "files": [
                { "name": "hello.txt", "size": 6, "mimeType": "text/plain" },
                { "name": "empty.bin", "size": 0, "mimeType": "application/octet-stream" },
            ],
            "sessionToken": session_token,
        }))
        .send()
        .await
        .expect("create transfer")
        .json::<serde_json::Value>()
        .await
        .expect("create json");

    let transfer_id = created["transferId"]
        .as_str()
        .expect("transferId")
        .to_string();
    let files = created["files"].as_array().expect("files").clone();
    let hello_id = files[0]["id"].as_str().expect("file id").to_string();
    let _empty_id = files[1]["id"].as_str().expect("file id").to_string();

    // 4. Upload the single chunk for hello.txt.
    let chunk = b"hello\n";
    let resp = client
        .post(format!(
            "{base}/api/transfers/{transfer_id}/chunks/0?fileId={hello_id}"
        ))
        .header("Authorization", format!("Bearer {session_token}"))
        .header("Content-Type", "application/octet-stream")
        .body(chunk.to_vec())
        .send()
        .await
        .expect("chunk upload");
    assert!(
        resp.status().is_success(),
        "chunk upload failed: {}",
        resp.status()
    );

    // 5. Complete the transfer (host verifies size + sha256, moves files).
    let resp = client
        .post(format!("{base}/api/transfers/{transfer_id}/complete"))
        .header("Authorization", format!("Bearer {session_token}"))
        .send()
        .await
        .expect("complete request");
    assert!(
        resp.status().is_success(),
        "complete failed: {}",
        resp.status()
    );

    // 6. Verify the file landed in the receive folder with correct content.
    let saved = receive.join("hello.txt");
    assert_eq!(std::fs::read(&saved).expect("saved file"), chunk);
    let empty = receive.join("empty.bin");
    assert_eq!(std::fs::read(&empty).expect("empty file"), b"");

    // 7. Transfer is recorded as completed with a checksum.
    let transfer = state
        .lock()
        .await
        .db
        .get_transfer(&transfer_id)
        .unwrap()
        .unwrap();
    assert_eq!(transfer.status, "completed");
    let saved_file = state
        .lock()
        .await
        .db
        .get_transfer_file(&hello_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        saved_file.sha256.as_deref(),
        Some("5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03"),
    );
}

#[tokio::test]
async fn device_pairing_pin_grants_the_pairing_capability() {
    let (state, base, _receive) = start_test_server().await;
    let client = reqwest::Client::new();

    // Issue a PIN (as the desktop would) and pair with it.
    let code = generate_pairing_code();
    let expires_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64 + 300)
        .unwrap_or(0);
    state.lock().await.pairing_pin = Some(PairingPin {
        code: code.clone(),
        expires_at,
    });

    let paired: serde_json::Value = client
        .post(format!("{base}/api/pair"))
        .json(&serde_json::json!({ "pin": code }))
        .send()
        .await
        .expect("pair request")
        .json::<serde_json::Value>()
        .await
        .expect("pair json");

    assert_eq!(paired["valid"], serde_json::Value::Bool(true));
    let granted = paired["token"].as_str().expect("token");
    assert_eq!(granted, state.lock().await.connection_token);

    // The PIN is consumed: a second attempt must fail.
    let resp = client
        .post(format!("{base}/api/pair"))
        .json(&serde_json::json!({ "pin": code }))
        .send()
        .await
        .expect("second pair");
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_pin_triggers_brute_force_lockout() {
    let (state, base, _receive) = start_test_server().await;
    let client = reqwest::Client::new();

    let code = generate_pairing_code();
    let expires_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64 + 300)
        .unwrap_or(0);
    state.lock().await.pairing_pin = Some(PairingPin {
        code: code.clone(),
        expires_at,
    });

    // 5 wrong attempts lock this IP out.
    for _ in 0..5 {
        let resp = client
            .post(format!("{base}/api/pair"))
            .json(&serde_json::json!({ "pin": "000000" }))
            .send()
            .await
            .expect("wrong pin");
        assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    // Even the correct PIN is now rejected while locked out.
    let resp = client
        .post(format!("{base}/api/pair"))
        .json(&serde_json::json!({ "pin": code }))
        .send()
        .await
        .expect("locked out");
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}
