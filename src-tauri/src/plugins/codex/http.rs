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

async fn load_session(Json(body): Json<LoadBody>) -> Response {
    match tokio::task::spawn_blocking(move || {
        super::load_session(&body.path, body.start, body.limit)
    })
    .await
    {
        Ok(Ok(window)) => ok_json(&window),
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

    #[tokio::test]
    async fn routes_list_disabled_source_and_load_a_fabricated_rollout() {
        let state = Arc::new(HttpState {
            app_state: Arc::new(crate::state::AppState::for_tests(
                crate::auth::AuthMode::Disabled,
            )),
            app: None,
        });
        let router = routes().with_state(state);
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
        let path = dir.path().join("rollout-fabricated.jsonl");
        std::fs::write(&path, r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Invent a small demo"}]}}"#).unwrap();
        let request = Request::builder()
            .method("POST")
            .uri("/api/codex/session")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({"path":path,"start":0,"limit":1}).to_string(),
            ))
            .unwrap();
        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["count"], 1);
        assert_eq!(body["messages"][0]["content"], "Invent a small demo");
    }
}
