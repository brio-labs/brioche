//! File and image attachment commands.
//!
//! Filesystem reads are isolated here so chat and session CRUD stay free of
//! attachment I/O concerns.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use brioche_core::ChatMessage;
use tauri::{AppHandle, State};

use super::chat::emit_system;
use crate::state::DesktopState;

/// Attaches a file or folder reference to the current conversation.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) plus the cost of reading filesystem metadata. Sends one user message.
///
/// # Panic / Safety
/// Never panics. Returns Err if the path cannot be read or no session is active.
pub async fn attach_reference(
    app: AppHandle,
    state: State<'_, DesktopState>,
    path: String,
) -> Result<(), String> {
    let content = attach_reference_impl(state.inner(), path).await?;
    emit_system(&app, content);
    Ok(())
}

/// Implementation of [`attach_reference`] that does not need a Tauri
/// [`AppHandle`], so it can be exercised from library tests.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) plus the cost of reading filesystem metadata. Sends one user message.
///
/// # Panic / Safety
/// Never panics. Returns Err if the path cannot be read or no session is active.
pub(super) async fn attach_reference_impl(
    state: &DesktopState,
    path: String,
) -> Result<String, String> {
    state.ensure_manager().await?;
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|e| format!("Failed to read reference: {e}"))?;
    let kind = if metadata.is_dir() { "folder" } else { "file" };
    let content = format!("User attached {kind}: {path}");
    {
        let mgr = state.manager.read().await;
        let manager = mgr.as_ref().ok_or("No active session")?;
        let entry = manager
            .get(manager.current_id())
            .ok_or("No active session")?;
        entry
            .llm
            .push_message(ChatMessage::User {
                content: content.clone(),
            })
            .await;
    }
    Ok(content)
}

/// Sends an image attachment for multimodal models.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(B) where B is the image file size. Encodes the image as base64.
///
/// # Panic / Safety
/// Never panics. Returns Err if the image cannot be read or no session is active.
pub async fn send_image(
    app: AppHandle,
    state: State<'_, DesktopState>,
    path: String,
) -> Result<String, String> {
    let (content, data_url) = send_image_impl(state.inner(), path).await?;
    emit_system(&app, content);
    Ok(data_url)
}

/// Implementation of [`send_image`] that does not need a Tauri
/// [`AppHandle`], so it can be exercised from library tests.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(B) where B is the image file size. Encodes the image as base64.
///
/// # Panic / Safety
/// Never panics. Returns Err if the image cannot be read or no session is active.
pub(super) async fn send_image_impl(
    state: &DesktopState,
    path: String,
) -> Result<(String, String), String> {
    state.ensure_manager().await?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| format!("Failed to read image: {e}"))?;
    let mime = match std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "image/png",
    };
    let b64 = base64_simd::STANDARD.encode_to_string(&bytes);
    let data_url = format!("data:{mime};base64,{b64}");
    let content = format!("User sent an image: {path}\n\n![image]({data_url})");
    {
        let mgr = state.manager.read().await;
        let manager = mgr.as_ref().ok_or("No active session")?;
        let entry = manager
            .get(manager.current_id())
            .ok_or("No active session")?;
        entry
            .llm
            .push_message(ChatMessage::User {
                content: content.clone(),
            })
            .await;
    }
    Ok((content, data_url))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::session::test_support::test_state;

    #[tokio::test]
    async fn attach_reference_impl_attaches_existing_file() -> Result<(), String> {
        let (state, temp) = test_state()?;
        state.ensure_manager().await?;
        let file_path = temp.path().join("reference.txt");
        tokio::fs::write(&file_path, "hello")
            .await
            .map_err(|e| format!("Failed to write test file: {e}"))?;
        let content =
            attach_reference_impl(&state, file_path.to_string_lossy().to_string()).await?;
        assert!(
            content.contains("User attached file"),
            "expected file attachment"
        );
        assert!(
            content.contains(file_path.to_string_lossy().as_ref()),
            "expected path in attachment"
        );
        Ok(())
    }

    #[tokio::test]
    async fn attach_reference_impl_errors_for_missing_file() -> Result<(), String> {
        let (state, temp) = test_state()?;
        state.ensure_manager().await?;
        let file_path = temp.path().join("missing.txt");
        let result = attach_reference_impl(&state, file_path.to_string_lossy().to_string()).await;
        let err = match result {
            Err(e) => e,
            Ok(_) => return Err("expected error for missing file".into()),
        };
        assert!(
            err.contains("Failed to read reference"),
            "expected read reference error"
        );
        Ok(())
    }

    #[tokio::test]
    async fn send_image_impl_encodes_existing_image() -> Result<(), String> {
        let (state, temp) = test_state()?;
        state.ensure_manager().await?;
        let image_path = temp.path().join("image.png");
        tokio::fs::write(&image_path, b"\x89PNG\r\n\x1a\n")
            .await
            .map_err(|e| format!("Failed to write test image: {e}"))?;
        let (content, data_url) =
            send_image_impl(&state, image_path.to_string_lossy().to_string()).await?;
        assert!(
            content.contains("User sent an image"),
            "expected image attachment"
        );
        assert!(
            data_url.starts_with("data:image/png;base64,"),
            "expected png data url"
        );
        Ok(())
    }

    #[tokio::test]
    async fn send_image_impl_errors_for_missing_image() -> Result<(), String> {
        let (state, temp) = test_state()?;
        state.ensure_manager().await?;
        let image_path = temp.path().join("missing.png");
        let result = send_image_impl(&state, image_path.to_string_lossy().to_string()).await;
        let err = match result {
            Err(e) => e,
            Ok(_) => return Err("expected error for missing image".into()),
        };
        assert!(
            err.contains("Failed to read image"),
            "expected read image error"
        );
        Ok(())
    }
}
