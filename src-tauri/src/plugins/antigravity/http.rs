//! HTTP routes for the Antigravity source plugin. Each handler is the HTTP
//! twin of a command in `commands.rs`, sharing its logic in `mod.rs`.

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
        .route("/api/antigravity/folders", get(api_list_folders))
        .route("/api/antigravity/settings", get(api_get_settings))
        .route("/api/antigravity/root", post(api_set_root))
        .route("/api/antigravity/session", post(api_load_session))
}

async fn api_list_folders(State(state): State<Arc<HttpState>>) -> Response {
    let settings = match app_state(&state).settings.lock() {
        Ok(guard) => guard.clone(),
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    ok_json(&super::visible_folders(&settings))
}

async fn api_get_settings(State(state): State<Arc<HttpState>>) -> Response {
    match app_state(&state).settings.lock() {
        Ok(guard) => ok_json(&super::settings_view(&guard)),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Deserialize)]
struct SetRootBody {
    /// `null` or blank clears the override and restores flavor auto-detection.
    #[serde(default)]
    path: Option<String>,
}

async fn api_set_root(
    State(state): State<Arc<HttpState>>,
    Json(body): Json<SetRootBody>,
) -> Response {
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
    ok_json(&super::settings_view(&guard))
}

#[derive(Deserialize)]
struct LoadSessionBody {
    path: String,
    /// Index of the first step wanted. Omitting it reads from the start.
    #[serde(default)]
    start: Option<usize>,
    /// How many steps. Omitting it reads the whole conversation; `0` returns
    /// the index alone.
    #[serde(default)]
    limit: Option<usize>,
}

async fn api_load_session(Json(body): Json<LoadSessionBody>) -> Response {
    match super::load_session(&body.path, body.start, body.limit) {
        Ok(window) => ok_json(&window),
        Err(e) => err_response(StatusCode::BAD_REQUEST, e),
    }
}
