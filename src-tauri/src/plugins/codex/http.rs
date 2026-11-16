use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Json, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;

use crate::http_api::{app_state, err_response, ok_json, HttpState};

pub(super) fn routes() -> Router<Arc<HttpState>> {
    Router::new()
        .route("/api/codex/sessions", get(list_sessions))
        .route("/api/codex/settings", get(get_settings))
        .route("/api/codex/root", post(set_root))
        .route("/api/codex/session", post(load_session))
        .route("/api/codex/efficiency/prepare", post(prepare_efficiency))
}

async fn list_sessions(State(state): State<Arc<HttpState>>) -> Response {
    let settings = match app_state(&state).settings.lock() {
        Ok(guard) => guard.clone(),
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    match tokio::task::spawn_blocking(move || super::visible_sessions(&settings)).await {
        Ok(sessions) => ok_json(&sessions),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn get_settings(State(state): State<Arc<HttpState>>) -> Response {
    let settings = match app_state(&state).settings.lock() {
        Ok(guard) => guard.clone(),
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    match tokio::task::spawn_blocking(move || super::settings_view(&settings)).await {
        Ok(settings) => ok_json(&settings),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Deserialize)]
struct SetRootBody {
    path: Option<String>,
}

async fn set_root(State(state): State<Arc<HttpState>>, Json(body): Json<SetRootBody>) -> Response {
    let settings = {
        let mut guard = match app_state(&state).settings.lock() {
            Ok(guard) => guard,
            Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
        if let Err(e) = super::set_root(&mut guard, body.path) {
            return err_response(StatusCode::BAD_REQUEST, e);
        }
        if let Err(e) = crate::settings::save_settings(&guard) {
            return err_response(StatusCode::INTERNAL_SERVER_ERROR, e);
        }
        guard.clone()
    };
    match tokio::task::spawn_blocking(move || super::settings_view(&settings)).await {
        Ok(settings) => ok_json(&settings),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Deserialize)]
struct LoadBody {
    path: String,
    start: Option<usize>,
    limit: Option<usize>,
}

async fn load_session(State(state): State<Arc<HttpState>>, Json(body): Json<LoadBody>) -> Response {
    let settings = match app_state(&state).settings.lock() {
        Ok(guard) => guard.clone(),
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    match tokio::task::spawn_blocking(move || {
        super::load_session(&settings, &body.path, body.start, body.limit)
    })
    .await
    {
        Ok(Ok(window)) => ok_json(&window),
        Ok(Err(e)) => err_response(StatusCode::BAD_REQUEST, e),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrepareBody {
    path: String,
    payload_mode: Option<crate::efficiency::settings::PayloadMode>,
}

async fn prepare_efficiency(
    State(state): State<Arc<HttpState>>,
    Json(body): Json<PrepareBody>,
) -> Response {
    let settings = match app_state(&state).settings.lock() {
        Ok(guard) => guard.clone(),
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    match tokio::task::spawn_blocking(move || {
        super::prepare_efficiency_payload(&settings, &body.path, body.payload_mode)
    })
    .await
    {
        Ok(Ok(payload)) => ok_json(&payload),
        Ok(Err(e)) => err_response(StatusCode::BAD_REQUEST, e),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    const USER_ROW: &str = r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Invent a small demo"}]}}"#;

    fn test_state() -> Arc<HttpState> {
        Arc::new(HttpState {
            app_state: Arc::new(crate::state::AppState::for_tests(
                crate::auth::AuthMode::Disabled,
            )),
            app: None,
        })
    }

    /// Point the test state at `home` and turn Codex on or off.
    fn configure(state: &HttpState, home: &std::path::Path, enabled: bool) {
        let mut settings = state.app_state.settings.lock().unwrap();
        crate::plugins::set_enabled(&mut settings, super::super::ID, enabled).unwrap();
        super::super::set_root(&mut settings, Some(home.to_string_lossy().into_owned())).unwrap();
    }

    fn write_rollout(path: &std::path::Path, body: &str) -> String {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
        path.to_string_lossy().into_owned()
    }

    async fn post(
        router: &Router,
        uri: &str,
        body: serde_json::Value,
    ) -> (StatusCode, serde_json::Value, Vec<u8>) {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json, bytes)
    }

    #[tokio::test]
    async fn routes_list_disabled_source_and_load_a_fabricated_rollout() {
        let state = test_state();
        let router = routes().with_state(state.clone());
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/codex/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            serde_json::json!([])
        );

        let dir = tempfile::tempdir().unwrap();
        let path = write_rollout(
            &dir.path()
                .join("sessions/2026/01/01/rollout-fabricated.jsonl"),
            USER_ROW,
        );
        configure(&state, dir.path(), true);
        let (status, body, _) = post(
            &router,
            "/api/codex/session",
            serde_json::json!({"path":path,"start":0,"limit":1}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["count"], 1);
        assert_eq!(body["messages"][0]["content"], "Invent a small demo");
    }

    #[tokio::test]
    async fn read_routes_refuse_a_file_outside_a_codex_home() {
        let state = test_state();
        let router = routes().with_state(state.clone());
        let home = tempfile::tempdir().unwrap();
        configure(&state, home.path(), true);
        let outside = tempfile::tempdir().unwrap();
        let stray = write_rollout(&outside.path().join("rollout-fabricated.jsonl"), USER_ROW);
        let index = write_rollout(&home.path().join("session_index.jsonl"), USER_ROW);
        for path in [stray, index] {
            for uri in ["/api/codex/session", "/api/codex/efficiency/prepare"] {
                let (status, body, _) = post(&router, uri, serde_json::json!({"path":path})).await;
                assert_eq!(status, StatusCode::BAD_REQUEST, "{uri} {path}");
                assert!(
                    body["error"]
                        .as_str()
                        .is_some_and(|e| e.contains("configured Codex home")),
                    "{uri}: {body}"
                );
            }
        }
    }

    #[tokio::test]
    async fn read_routes_refuse_every_chat_while_the_source_is_off() {
        let state = test_state();
        let router = routes().with_state(state.clone());
        let home = tempfile::tempdir().unwrap();
        let path = write_rollout(
            &home.path().join("sessions/rollout-fabricated.jsonl"),
            USER_ROW,
        );
        configure(&state, home.path(), false);
        for uri in ["/api/codex/session", "/api/codex/efficiency/prepare"] {
            let (status, body, _) = post(&router, uri, serde_json::json!({"path":path})).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
            assert_eq!(body["error"], "Codex source is disabled", "{uri}");
        }
        configure(&state, home.path(), true);
        let (status, _, _) = post(
            &router,
            "/api/codex/session",
            serde_json::json!({"path":path}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn efficiency_preview_uses_codex_messages_and_redacts_locally() {
        let state = test_state();
        let dir = tempfile::tempdir().unwrap();
        let path = write_rollout(&dir.path().join("sessions/rollout-invented.jsonl"), concat!(
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"Build a fictional calculator API_KEY=pretend-secret-value\"}]}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"exec_command\",\"call_id\":\"sample-call\",\"arguments\":\"{\\\"cmd\\\":\\\"echo fictional\\\"}\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"call_id\":\"sample-call\",\"output\":\"fictional\"}}\n"
        ));
        configure(&state, dir.path(), true);
        let (status, body, bytes) = post(
            &routes().with_state(state),
            "/api/codex/efficiency/prepare",
            serde_json::json!({"path":path,"payloadMode":"full-transcript"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["input"]["task"]["turns"], 2);
        assert_eq!(body["input"]["signals"]["toolCalls"], 1);
        assert_eq!(body["input"]["actions"][0]["tool"], "exec_command");
        assert!(body["input"]["task"]["firstUserMessage"]
            .as_str()
            .unwrap()
            .starts_with("Build a fictional calculator"));
        assert!(!String::from_utf8(bytes)
            .unwrap()
            .contains("pretend-secret-value"));
    }
}
