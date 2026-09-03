//! F014 — compatibility shim so the shared service layer compiles BOTH with the
//! desktop Tauri runtime and in the headless `noviq-server` build (no Tauri, no
//! GTK/WebKit — required to build on the Ubuntu-20.04 GPU VM).
//!
//! * Desktop build (`feature = "desktop"`): re-exports the real Tauri types.
//! * Server build (`--no-default-features --features server`): provides a tiny
//!   stand-in `AppHandle` and a no-op `Emitter`. On the headless path every
//!   `AppHandle` is `None`, so the `if let Some(app) = … { app.emit(…) }`
//!   branches are never reached at runtime — the stub exists only so they
//!   type-check.
//!
//! Service code uses `crate::compat::{AppHandle, Emitter}` instead of
//! `tauri::{AppHandle, Emitter}`. Anything that needs the *real* Tauri surface
//! (window/path/state managers, plugins) lives in `#[cfg(feature = "desktop")]`
//! code and keeps importing `tauri::` directly.

#[cfg(feature = "desktop")]
pub use tauri::{AppHandle, Emitter};

#[cfg(not(feature = "desktop"))]
pub use headless::{AppHandle, Emitter};

#[cfg(not(feature = "desktop"))]
mod headless {
    /// Stand-in for `tauri::AppHandle` in the headless server build. The HTTP
    /// chat route constructs one **scoped to a conversation** so the shared
    /// services' `app.emit("chat://token", …)` / `assistant://progress` calls —
    /// threaded through the whole generation path for the desktop — flow into
    /// the process-wide [`crate::services::chat_stream`] broadcast, which the
    /// `GET /api/chat/events` SSE endpoint fans out to the browser.
    #[derive(Clone, Default)]
    pub struct AppHandle {
        conversation_id: Option<String>,
    }

    impl AppHandle {
        /// A handle whose emits are tagged with (and filtered by) this
        /// conversation. Anything emitted through it reaches the web client
        /// subscribed to the same conversation's SSE stream.
        pub fn for_conversation(conversation_id: impl Into<String>) -> Self {
            Self { conversation_id: Some(conversation_id.into()) }
        }
    }

    /// Mirrors the subset of `tauri::Emitter` the shared services call.
    pub trait Emitter {
        fn emit<S: serde::Serialize>(&self, event: &str, payload: S) -> Result<(), EmitError>;
    }

    impl Emitter for AppHandle {
        fn emit<S: serde::Serialize>(&self, event: &str, payload: S) -> Result<(), EmitError> {
            if let Some(id) = &self.conversation_id {
                if let Ok(value) = serde_json::to_value(payload) {
                    // No receivers (nobody watching) is fine — event is dropped.
                    let _ = crate::services::chat_stream::channel().send(
                        crate::services::chat_stream::ChatStreamEvent {
                            conversation_id: id.clone(),
                            event: event.to_string(),
                            payload: value,
                        },
                    );
                }
            }
            Ok(())
        }
    }

    /// Placeholder error type; the call sites all discard the result
    /// (`let _ = app.emit(...)`), so it only needs to exist.
    #[derive(Debug)]
    pub struct EmitError;

    impl std::fmt::Display for EmitError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "headless emit (no-op)")
        }
    }

    impl std::error::Error for EmitError {}
}
