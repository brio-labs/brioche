//! File explorer and editor filesystem operations.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use serde::Serialize;

/// File/directory entry for the file explorer.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Struct containing heap-allocated string representations of path entries. O(1).
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Debug, Serialize)]
pub struct DirEntry {
    /// File or directory name.
    pub name: String,
    /// Whether this is a directory.
    pub is_dir: bool,
    /// Full path to the entry.
    pub path: String,
}

/// Validates a user-supplied path string.
///
/// Rejects empty or whitespace-only paths and paths containing NUL bytes,
/// which cannot be represented on any supported platform. The returned
/// `PathBuf` is the normalized input ready for filesystem operations.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(L) where L is the length of the path string.
///
/// # Panic / Safety
/// Never panics.
fn validate_path(path: &str) -> Result<std::path::PathBuf, String> {
    if path.trim().is_empty() {
        return Err("Path cannot be empty".into());
    }
    if path.contains('\0') {
        return Err("Path cannot contain NUL bytes".into());
    }
    Ok(std::path::PathBuf::from(path))
}

/// Reads the contents of a directory.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(N log N) where N is the number of files in the directory. Performs async directory read and sorting.
///
/// # Panic / Safety
/// Never panics. Returns Err if path does not exist, is not a directory, or fails to read.
#[tauri::command]
pub async fn read_directory(path: String) -> Result<Vec<DirEntry>, String> {
    let path = validate_path(&path)?;
    if !path.exists() {
        return Err("Path does not exist".into());
    }
    if !path.is_dir() {
        return Err("Path is not a directory".into());
    }
    let mut entries = Vec::new();
    let mut dir = tokio::fs::read_dir(&path)
        .await
        .map_err(|e| format!("Failed to read directory: {e}"))?;
    while let Some(entry) = dir
        .next_entry()
        .await
        .map_err(|e| format!("Failed to read entry: {e}"))?
    {
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = match entry.file_type().await {
            Ok(ft) => ft.is_dir(),
            Err(_) => false,
        };
        let path = entry.path().to_string_lossy().to_string();
        entries.push(DirEntry { name, is_dir, path });
    }
    entries.sort_by(|a, b| {
        // Directories first, then by name
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        }
    });
    Ok(entries)
}

/// Reads the contents of a text file.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(F) where F is the file size on disk. Performs async file read.
///
/// # Panic / Safety
/// Never panics. Returns Err if file reading fails.
#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    let path = validate_path(&path)?;
    tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Failed to read file: {e}"))
}

/// Writes content to a file, creating it if necessary.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(C) where C is the content size. Performs async file write.
///
/// # Panic / Safety
/// Never panics. Returns Err if writing fails.
#[tauri::command]
pub async fn write_file(path: String, content: String) -> Result<(), String> {
    let path = validate_path(&path)?;
    tokio::fs::write(&path, content)
        .await
        .map_err(|e| format!("Failed to write file: {e}"))
}

/// Deletes a file or empty directory.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) file system metadata read and deletion.
///
/// # Panic / Safety
/// Never panics. Returns Err if metadata read or deletion fails.
#[tauri::command]
pub async fn delete_file(path: String) -> Result<(), String> {
    let path = validate_path(&path)?;
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|e| format!("Failed to read metadata: {e}"))?;
    if metadata.is_dir() {
        tokio::fs::remove_dir(&path)
            .await
            .map_err(|e| format!("Failed to remove directory: {e}"))?;
    } else {
        tokio::fs::remove_file(&path)
            .await
            .map_err(|e| format!("Failed to remove file: {e}"))?;
    }
    Ok(())
}

/// Creates a new empty file.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) file creation.
///
/// # Panic / Safety
/// Never panics. Returns Err if file creation fails.
#[tauri::command]
pub async fn create_file(path: String) -> Result<(), String> {
    let path = validate_path(&path)?;
    tokio::fs::File::create(&path)
        .await
        .map_err(|e| format!("Failed to create file: {e}"))?;
    Ok(())
}

/// Creates a new directory, including parent directories if they do not exist.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) directory creation.
///
/// # Panic / Safety
/// Never panics. Returns Err if directory creation fails.
#[tauri::command]
pub async fn create_directory(path: String) -> Result<(), String> {
    let path = validate_path(&path)?;
    tokio::fs::create_dir_all(&path)
        .await
        .map_err(|e| format!("Failed to create directory: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_path_rejects_empty() -> Result<(), String> {
        match validate_path("") {
            Err(e) => assert_eq!(e, "Path cannot be empty"),
            Ok(_) => return Err("expected empty path to be rejected".into()),
        }
        Ok(())
    }

    #[test]
    fn validate_path_rejects_whitespace() -> Result<(), String> {
        match validate_path("   ") {
            Err(e) => assert_eq!(e, "Path cannot be empty"),
            Ok(_) => return Err("expected whitespace-only path to be rejected".into()),
        }
        Ok(())
    }

    #[test]
    fn validate_path_rejects_nul_bytes() -> Result<(), String> {
        match validate_path("/tmp/foo\0bar") {
            Err(e) => assert_eq!(e, "Path cannot contain NUL bytes"),
            Ok(_) => return Err("expected NUL byte path to be rejected".into()),
        }
        Ok(())
    }

    #[test]
    fn validate_path_accepts_normal_paths() -> Result<(), String> {
        let path = validate_path("/tmp/foo")?;
        assert_eq!(path, std::path::PathBuf::from("/tmp/foo"));
        Ok(())
    }

    #[tokio::test]
    async fn read_directory_sorts_dirs_first() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let dir_path = tmp.path().to_path_buf();
        tokio::fs::create_dir(dir_path.join("z_dir"))
            .await
            .map_err(|e| e.to_string())?;
        tokio::fs::write(dir_path.join("a_file.txt"), "hello")
            .await
            .map_err(|e| e.to_string())?;
        tokio::fs::write(dir_path.join("b_file.txt"), "world")
            .await
            .map_err(|e| e.to_string())?;

        let entries = read_directory(dir_path.to_string_lossy().to_string()).await?;
        assert_eq!(entries.len(), 3);
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].name, "z_dir");
        assert!(!entries[1].is_dir);
        assert_eq!(entries[1].name, "a_file.txt");
        assert_eq!(entries[2].name, "b_file.txt");
        Ok(())
    }

    #[tokio::test]
    async fn read_directory_rejects_nonexistent() -> Result<(), String> {
        match read_directory("/nonexistent/path/that/should/not/exist".into()).await {
            Err(e) => assert_eq!(e, "Path does not exist"),
            Ok(_) => return Err("expected nonexistent directory to be rejected".into()),
        }
        Ok(())
    }

    #[tokio::test]
    async fn read_directory_rejects_file() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let file_path = tmp.path().join("not_a_dir.txt");
        tokio::fs::write(&file_path, "content")
            .await
            .map_err(|e| e.to_string())?;
        match read_directory(file_path.to_string_lossy().to_string()).await {
            Err(e) => assert_eq!(e, "Path is not a directory"),
            Ok(_) => return Err("expected file path to be rejected".into()),
        }
        Ok(())
    }

    #[tokio::test]
    async fn write_and_read_file_roundtrip() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let file_path = tmp.path().join("roundtrip.txt");
        let path_str = file_path.to_string_lossy().to_string();

        write_file(path_str.clone(), "brioche".into()).await?;
        let content = read_file(path_str).await?;
        assert_eq!(content, "brioche");
        Ok(())
    }

    #[tokio::test]
    async fn read_file_rejects_empty_path() -> Result<(), String> {
        match read_file("".into()).await {
            Err(e) => assert_eq!(e, "Path cannot be empty"),
            Ok(_) => return Err("expected empty path to be rejected".into()),
        }
        Ok(())
    }

    #[tokio::test]
    async fn delete_file_removes_file() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let file_path = tmp.path().join("to_delete.txt");
        tokio::fs::write(&file_path, "x")
            .await
            .map_err(|e| e.to_string())?;
        delete_file(file_path.to_string_lossy().to_string()).await?;
        assert!(!file_path.exists());
        Ok(())
    }

    #[tokio::test]
    async fn create_directory_creates_nested_path() -> Result<(), String> {
        let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let nested = tmp.path().join("a").join("b").join("c");
        create_directory(nested.to_string_lossy().to_string()).await?;
        assert!(nested.is_dir());
        Ok(())
    }

    #[tokio::test]
    async fn create_file_rejects_empty_path() -> Result<(), String> {
        match create_file("".into()).await {
            Err(e) => assert_eq!(e, "Path cannot be empty"),
            Ok(_) => return Err("expected empty path to be rejected".into()),
        }
        Ok(())
    }
}
