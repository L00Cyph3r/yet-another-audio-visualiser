use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use crossbeam_channel::Sender;

use crate::api::types::{AppCommand, DeviceDto, SelectDeviceRequest, SettingsDto, SettingsPatch};

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
        Ok(Ok(Ok(settings))) => Ok(Json(settings)),
        Ok(Ok(Err(err_msg))) => Err((
            StatusCode::BAD_REQUEST,
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
                .worker_threads(1)
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
            gain: 2.5,
            smoothing: 0.5,
            visualiser_settings: Default::default(),
            active_device: Some("Test Device".to_string()),
        };

        let json = serde_json::to_string(&settings).expect("Serialization failed");
        let deserialized: SettingsDto =
            serde_json::from_str(&json).expect("Deserialization failed");

        assert_eq!(settings, deserialized);
    }

    #[test]
    fn test_patch_deserialization() {
        let json = r#"{"visualiser":"Equalizer"}"#;
        let patch: SettingsPatch = serde_json::from_str(json).expect("Deserialization failed");

        assert_eq!(patch.visualiser, Some(VisualisationType::Equalizer));
        assert_eq!(patch.gain, None);

        let json_seg = r#"{"visualiser":"SegmentedSpectrumBars"}"#;
        let patch_seg: SettingsPatch =
            serde_json::from_str(json_seg).expect("Deserialization failed");
        assert_eq!(
            patch_seg.visualiser,
            Some(VisualisationType::SegmentedSpectrumBars)
        );
    }
    #[test]
    fn test_patch_with_visualiser_settings_deserialization() {
        let json = r#"{"visualiser_settings":{"waveform":{"corner_radius":4}}}"#;
        let patch: SettingsPatch = serde_json::from_str(json).expect("Deserialization failed");
        let waveform = patch.visualiser_settings.unwrap().waveform.unwrap();
        assert_eq!(waveform["corner_radius"], 4);
    }

    /// Sends a raw HTTP request and returns the complete response.
    async fn http(addr: SocketAddr, method: &str, path: &str, body: &str) -> String {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    }

    /// Returns the JSON body of an HTTP response.
    fn json_body(response: &str) -> serde_json::Value {
        let body = response
            .split("\r\n\r\n")
            .nth(1)
            .expect("response has a body");
        // Be lenient about chunked transfer encoding: take everything from the first brace.
        let start = body.find('{').expect("body contains JSON");
        let end = body.rfind('}').expect("body contains JSON");
        serde_json::from_str(&body[start..=end]).expect("valid JSON body")
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
        let body = r#"{"visualiser":"Waveform","gain":1.5}"#;
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

        // 5. GET /api/settings exposes the individual settings of every visualisation
        let response = http(local_addr, "GET", "/api/settings", "").await;
        assert!(response.contains("200 OK"));
        let settings = json_body(&response);
        assert_eq!(
            settings["visualiser_settings"]["waveform"]["corner_radius"],
            2
        );
        assert_eq!(
            settings["visualiser_settings"]["segmented_spectrum_bars"]["segment_count"],
            8
        );

        // 6. PATCH individual settings: only the mentioned fields change
        let body = r#"{"visualiser_settings":{"segmented_spectrum_bars":{"segment_count":16,"color_mapping":"Rainbow"},"waveform":{"corner_radius":5}}}"#;
        let response = http(local_addr, "PATCH", "/api/settings", body).await;
        assert!(response.contains("200 OK"), "{response}");
        let settings = json_body(&response);
        let bars = &settings["visualiser_settings"]["segmented_spectrum_bars"];
        assert_eq!(bars["segment_count"], 16);
        assert_eq!(bars["color_mapping"], "Rainbow");
        assert_eq!(bars["vertical_gap"], 2.0);
        assert_eq!(
            settings["visualiser_settings"]["waveform"]["corner_radius"],
            5
        );
        // Earlier, unrelated settings are untouched.
        assert_eq!(settings["visualiser"], "Waveform");

        // 7. Unknown settings are rejected with 400 and nothing changes
        let body = r#"{"visualiser_settings":{"waveform":{"corner_radius":9,"bogus":1}}}"#;
        let response = http(local_addr, "PATCH", "/api/settings", body).await;
        assert!(response.contains("400 Bad Request"), "{response}");
        assert!(
            json_body(&response)["error"]
                .as_str()
                .unwrap()
                .contains("bogus")
        );

        let response = http(local_addr, "GET", "/api/settings", "").await;
        let settings = json_body(&response);
        assert_eq!(
            settings["visualiser_settings"]["waveform"]["corner_radius"],
            5
        );

        // 8. Out-of-range values are clamped instead of breaking the analyser
        let body = r#"{"gain":-3.0,"visualiser_settings":{"segmented_spectrum_bars":{"segment_count":0}}}"#;
        let response = http(local_addr, "PATCH", "/api/settings", body).await;
        assert!(response.contains("200 OK"), "{response}");
        let settings = json_body(&response);
        assert_eq!(settings["gain"], 0.0);
        assert_eq!(
            settings["visualiser_settings"]["segmented_spectrum_bars"]["segment_count"],
            1
        );

        let _ = stop_tx.send(());
        let _ = gui_handle.join();
    }
}
