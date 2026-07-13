//! Sanitized storage errors — raw SQLite messages must never reach HTTP clients.

/// Logs the full error server-side and returns a safe user-facing message.
pub fn sanitize_storage_error(context: &str, err: impl std::fmt::Display) -> String {
    log::warn!("storage.{context}: {err}");
    format!("Storage operation failed ({context})")
}
