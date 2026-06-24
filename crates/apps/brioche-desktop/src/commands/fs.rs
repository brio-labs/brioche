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
    let mut entries = Vec::new();
    let path = std::path::PathBuf::from(path);
    if !path.exists() {
        return Err("Path does not exist".into());
    }
    if !path.is_dir() {
        return Err("Path is not a directory".into());
    }
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
    tokio::fs::create_dir_all(&path)
        .await
        .map_err(|e| format!("Failed to create directory: {e}"))?;
    Ok(())
}
