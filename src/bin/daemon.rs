use anyhow::{anyhow, Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use plumbob::{
    effects::{self, Rgb},
    Plumbob,
};
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, Mutex};
use tokio::task;

/// Target color the worker should fade to. The worker watches this channel and
/// interrupts any in-flight fade as soon as a new target arrives.
#[derive(Clone, Copy, Debug)]
struct Target {
    color: Rgb,
    fade_ms: u64,
}

#[derive(Clone)]
struct AppState {
    tx: watch::Sender<Target>,
}

#[derive(Debug, Deserialize)]
struct EmotionPayload {
    /// Free-form game id; helpful for logs / future per-game palettes.
    #[serde(default)]
    game: String,
    /// Paralives emotion name (case-insensitive).
    emotion: String,
    /// Optional fade duration in milliseconds (default 800).
    #[serde(default)]
    fade_ms: Option<u64>,
    /// Optional fallback color, supplied by the mod from the in-game BackgroundColor.
    /// Used only when the emotion name doesn't match our palette AND the color isn't black.
    #[serde(default)]
    rgb: Option<[u8; 3]>,
}

#[derive(Debug, Deserialize)]
struct RgbPayload {
    r: u8,
    g: u8,
    b: u8,
    #[serde(default)]
    fade_ms: Option<u64>,
}

/// GameSense-style envelope: enough of the schema for our needs.
#[derive(Debug, Deserialize)]
struct GameEvent {
    #[serde(default)]
    game: String,
    #[serde(default)]
    event: String,
    #[serde(default)]
    data: serde_json::Value,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Open the Plumbob once, share via a blocking-friendly Mutex.
    let bob = Arc::new(Mutex::new(
        Plumbob::open().context("failed to open Plumbob")?,
    ));

    // watch channel: latest-value-wins semantics for color targets.
    let (tx, rx) = watch::channel(Target {
        color: (0, 0, 0),
        fade_ms: 0,
    });

    // Worker task: drives the LED, interruptable by new watch updates.
    let bob_worker = bob.clone();
    task::spawn(async move { worker(bob_worker, rx).await });

    let state = AppState { tx };
    let app = Router::new()
        .route("/", get(|| async { "plumbob-daemon\n" }))
        .route("/healthz", get(|| async { "ok\n" }))
        .route("/emotion", post(post_emotion))
        .route("/rgb", post(post_rgb))
        .route("/off", post(post_off))
        // GameSense-compatible endpoint (Sims 4 talks to this).
        .route("/game_event", post(post_game_event))
        .with_state(state);

    let addr: SocketAddr = "127.0.0.1:27301".parse().unwrap();
    tracing::info!("listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tokio::select! {
        res = axum::serve(listener, app) => res?,
        _ = tokio::signal::ctrl_c() => tracing::info!("ctrl-c received, shutting down"),
    }

    // Best-effort: turn off on shutdown.
    if let Ok(mut bob) = bob.try_lock() {
        let _ = bob.off();
    }
    Ok(())
}

async fn worker(bob: Arc<Mutex<Plumbob>>, mut rx: watch::Receiver<Target>) {
    let mut current: Rgb = (0, 0, 0);
    loop {
        let target = *rx.borrow_and_update();
        let dur = Duration::from_millis(target.fade_ms);
        let bob = bob.clone();

        // Run the fade in a blocking thread so the hidraw write doesn't stall the runtime.
        let from = current;
        let to = target.color;
        let fade_handle = task::spawn_blocking(move || {
            let mut bob = bob.blocking_lock();
            effects::fade(&mut bob, from, to, dur)
        });

        tokio::select! {
            r = fade_handle => {
                if let Ok(Err(e)) = r { tracing::warn!("fade failed: {e:#}"); }
                current = to;
            }
            changed = rx.changed() => {
                // New target came in mid-fade. The current fade thread keeps running
                // briefly (we can't preempt the std::thread::sleep), but the next
                // iteration picks up the newer target and overwrites.
                if changed.is_err() { return; }
                // We don't know exactly where the interrupted fade landed; approximating
                // with the previous target is good enough — the next fade smooths it.
                current = to;
            }
        }
    }
}

fn err500(msg: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, msg.into())
}

async fn post_emotion(
    State(state): State<AppState>,
    Json(p): Json<EmotionPayload>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let color = match effects::paralives_emotion(&p.emotion) {
        Some(c) => c,
        None => match p.rgb {
            Some([0, 0, 0]) | None => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("unknown emotion: {} (no rgb fallback)", p.emotion),
                ));
            }
            Some([r, g, b]) => {
                tracing::info!(game = %p.game, emotion = %p.emotion, "emotion: using mod-supplied rgb fallback");
                (r, g, b)
            }
        },
    };
    let fade_ms = p.fade_ms.unwrap_or(800);
    tracing::info!(game = %p.game, emotion = %p.emotion, ?color, "emotion");
    state
        .tx
        .send(Target { color, fade_ms })
        .map_err(|e| err500(format!("worker gone: {e}")))?;
    Ok(Json(serde_json::json!({"ok": true, "color": color})))
}

async fn post_rgb(
    State(state): State<AppState>,
    Json(p): Json<RgbPayload>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let fade_ms = p.fade_ms.unwrap_or(0);
    state
        .tx
        .send(Target { color: (p.r, p.g, p.b), fade_ms })
        .map_err(|e| err500(format!("worker gone: {e}")))?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn post_off(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    state
        .tx
        .send(Target { color: (0, 0, 0), fade_ms: 400 })
        .map_err(|e| err500(format!("worker gone: {e}")))?;
    Ok(Json(serde_json::json!({"ok": true})))
}

/// Minimal GameSense compatibility: accepts {"game","event","data":{"value":...}}.
/// For now we treat any `event` matching MOOD / EMOTION as a color request via
/// the `data.value` field, which can be an emotion name or an [R,G,B] array.
async fn post_game_event(
    State(state): State<AppState>,
    Json(ev): Json<GameEvent>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let color = parse_gamesense_value(&ev.data)
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "could not derive color from data".into()))?;
    tracing::info!(game = %ev.game, event = %ev.event, ?color, "game_event");
    state
        .tx
        .send(Target { color, fade_ms: 800 })
        .map_err(|e| err500(format!("worker gone: {e}")))?;
    Ok(Json(serde_json::json!({"ok": true})))
}

fn parse_gamesense_value(data: &serde_json::Value) -> Option<Rgb> {
    // Accept {"value": "Happy"} or {"value": [r,g,b]} or {"value": {"r":..,"g":..,"b":..}}
    let v = data.get("value")?;
    if let Some(s) = v.as_str() {
        return effects::paralives_emotion(s);
    }
    if let Some(arr) = v.as_array() {
        if arr.len() == 3 {
            let r = arr[0].as_u64()? as u8;
            let g = arr[1].as_u64()? as u8;
            let b = arr[2].as_u64()? as u8;
            return Some((r, g, b));
        }
    }
    if let (Some(r), Some(g), Some(b)) =
        (v.get("r")?.as_u64(), v.get("g")?.as_u64(), v.get("b")?.as_u64())
    {
        return Some((r as u8, g as u8, b as u8));
    }
    None
}

// Suppress unused-helper warnings in this crate-root file when split builds.
#[allow(dead_code)]
fn _refer<T>(_: T) -> Result<()> { Err(anyhow!("never called")) }
