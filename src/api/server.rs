use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use crossbeam_channel::Sender;

use crate::api::types::{
    AppCommand, DeviceDto, SelectDeviceRequest, SettingsDto, SettingsPatch,
};

#[derive(Clone)]
pub struct ApiState {
    pub command_tx: Sender<AppCommand>,
    pub egui_ctx: egui::Context,
}

pub fn create_router(state: ApiState) -> Router {
    Router::new()
        .route("/api/settings", get(get_settings).patch(patch_settings))
        .route("/api/devices", get(get_devices))
        .route("/api/devices/select", post(select_device))
        .with_state(state)
}

async fn get_settings(
    State(state): State<ApiState>,
) -> Result<Json<SettingsDto>, (StatusCode, Json<serde_json::Value>)> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    state
        .command_tx
        .send(AppCommand::GetSettings(tx))
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to send command: {}", e) })),
            )
        })?;

    state.egui_ctx.request_repaint();

    match tokio::time::timeout(Duration::from_secs(2), rx).await {
        Ok(Ok(settings)) => Ok(Json(settings)),
        Ok(Err(_)) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to receive response from GUI thread" })),
        )),
        Err(_) => Err((
            StatusCode::GATEWAY_TIMEOUT,
            Json(serde_json::json!({ "error": "Timed out waiting for GUI thread response" })),
        )),
    }
}

async fn patch_settings(
    State(state): State<ApiState>,
    Json(patch): Json<SettingsPatch>,
) -> Result<Json<SettingsDto>, (StatusCode, Json<serde_json::Value>)> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    state
        .command_tx
        .send(AppCommand::UpdateSettings(patch, Some(tx)))
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to send command: {}", e) })),
            )
        })?;

    state.egui_ctx.request_repaint();

    match tokio::time::timeout(Duration::from_secs(2), rx).await {
        Ok(Ok(settings)) => Ok(Json(settings)),
        Ok(Err(_)) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to receive response from GUI thread" })),
        )),
        Err(_) => Err((
            StatusCode::GATEWAY_TIMEOUT,
            Json(serde_json::json!({ "error": "Timed out waiting for GUI thread response" })),
        )),
    }
}

async fn get_devices(
    State(state): State<ApiState>,
) -> Result<Json<Vec<DeviceDto>>, (StatusCode, Json<serde_json::Value>)> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    state
        .command_tx
        .send(AppCommand::GetDevices(tx))
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to send command: {}", e) })),
            )
        })?;

    state.egui_ctx.request_repaint();

    match tokio::time::timeout(Duration::from_secs(2), rx).await {
        Ok(Ok(devices)) => Ok(Json(devices)),
        Ok(Err(_)) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to receive response from GUI thread" })),
        )),
        Err(_) => Err((
            StatusCode::GATEWAY_TIMEOUT,
            Json(serde_json::json!({ "error": "Timed out waiting for GUI thread response" })),
        )),
    }
}

async fn select_device(
    State(state): State<ApiState>,
    Json(payload): Json<SelectDeviceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    state
        .command_tx
        .send(AppCommand::SelectDevice(payload.name, Some(tx)))
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to send command: {}", e) })),
            )
        })?;

    state.egui_ctx.request_repaint();

    match tokio::time::timeout(Duration::from_secs(2), rx).await {
        Ok(Ok(Ok(()))) => Ok(Json(serde_json::json!({
            "status": "ok",
            "message": "Device selected successfully"
        }))),
        Ok(Ok(Err(err_msg))) => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": err_msg })),
        )),
        Ok(Err(_)) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to receive response from GUI thread" })),
        )),
        Err(_) => Err((
            StatusCode::GATEWAY_TIMEOUT,
            Json(serde_json::json!({ "error": "Timed out waiting for GUI thread response" })),
        )),
    }
}

pub async fn run_server(
    listener: tokio::net::TcpListener,
    state: ApiState,
) -> Result<(), std::io::Error> {
    let app = create_router(state);
    axum::serve(listener, app).await
}

pub fn start_api_server(
    addr: SocketAddr,
    command_tx: Sender<AppCommand>,
    egui_ctx: egui::Context,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("api-server".to_string())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to build Tokio runtime for API server");

            rt.block_on(async move {
                let state = ApiState {
                    command_tx,
                    egui_ctx,
                };
                match tokio::net::TcpListener::bind(addr).await {
                    Ok(listener) => {
                        log::info!("API server listening on http://{}", addr);
                        if let Err(e) = run_server(listener, state).await {
                            log::error!("API server error: {}", e);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to bind API server to {}: {}", addr, e);
                    }
                }
            });
        })
        .expect("Failed to spawn API server thread")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::app::GuiApp;
    use crate::visualisations::VisualisationType;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn test_dto_serialization() {
        let settings = SettingsDto {
            visualiser: VisualisationType::Waveform,
            bands: 32,
            gain: 2.5,
            smoothing: 0.5,
            band_gap: 4.0,
            active_device: Some("Test Device".to_string()),
        };

        let json = serde_json::to_string(&settings).expect("Serialization failed");
        let deserialized: SettingsDto =
            serde_json::from_str(&json).expect("Deserialization failed");

        assert_eq!(settings, deserialized);
    }

    #[test]
    fn test_patch_deserialization() {
        let json = r#"{"visualiser":"Equalizer","bands":16}"#;
        let patch: SettingsPatch = serde_json::from_str(json).expect("Deserialization failed");

        assert_eq!(patch.visualiser, Some(VisualisationType::Equalizer));
        assert_eq!(patch.bands, Some(16));
        assert_eq!(patch.gain, None);
    }

    #[tokio::test]
    async fn test_api_e2e_mock_gui() {
        let (command_tx, command_rx) = crossbeam_channel::unbounded();
        let egui_ctx = egui::Context::default();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_addr = listener.local_addr().unwrap();

        let state = ApiState {
            command_tx,
            egui_ctx,
        };

        tokio::spawn(async move {
            let _ = run_server(listener, state).await;
        });

        let mut gui_app = GuiApp::new(command_rx);
        let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();

        let gui_handle = std::thread::spawn(move || {
            while stop_rx.try_recv().is_err() {
                gui_app.handle_commands();
                std::thread::sleep(Duration::from_millis(10));
            }
        });

        // 1. Test GET /api/settings
        let mut stream = tokio::net::TcpStream::connect(local_addr).await.unwrap();
        let request = format!(
            "GET /api/settings HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            local_addr
        );
        stream.write_all(request.as_bytes()).await.unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        assert!(response.contains("200 OK"));
        assert!(response.contains(r#""visualiser":"Equalizer""#));

        // 2. Test PATCH /api/settings
        let mut stream = tokio::net::TcpStream::connect(local_addr).await.unwrap();
        let body = r#"{"visualiser":"Waveform","bands":64,"gain":1.5}"#;
        let request = format!(
            "PATCH /api/settings HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            local_addr,
            body.len(),
            body
        );
        stream.write_all(request.as_bytes()).await.unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        assert!(response.contains("200 OK"));
        assert!(response.contains(r#""visualiser":"Waveform""#));
        assert!(response.contains(r#""bands":64"#));

        // 3. Test GET /api/devices
        let mut stream = tokio::net::TcpStream::connect(local_addr).await.unwrap();
        let request = format!(
            "GET /api/devices HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            local_addr
        );
        stream.write_all(request.as_bytes()).await.unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        assert!(response.contains("200 OK"));

        // 4. Test POST /api/devices/select (nonexistent device returns 404)
        let mut stream = tokio::net::TcpStream::connect(local_addr).await.unwrap();
        let body = r#"{"name":"Nonexistent Audio Input Device"}"#;
        let request = format!(
            "POST /api/devices/select HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            local_addr,
            body.len(),
            body
        );
        stream.write_all(request.as_bytes()).await.unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        assert!(response.contains("404 Not Found"));

        let _ = stop_tx.send(());
        let _ = gui_handle.join();
    }
}
