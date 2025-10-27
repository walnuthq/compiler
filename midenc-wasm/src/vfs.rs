//! Virtual File System for browser-based compilation
//!
//! This module provides a JavaScript-accessible API for managing source files
//! in a browser environment where filesystem access is not available.

use crate::alloc::{string::{String, ToString}, sync::Arc, vec::Vec};
use wasm_bindgen::prelude::*;
use midenc_session::diagnostics::{DefaultSourceManager, SourceManager, SourceLanguage, Uri};

/// Virtual File System for browser-based compilation
///
/// This provides an in-memory file system that can be populated from JavaScript
/// and used by the compiler to resolve source files, debug information, and
/// library dependencies.
///
/// # Example (JavaScript)
///
/// ```javascript
/// const vfs = new VirtualFileSystem();
///
/// // Add source files
/// vfs.addFile('/src/main.wasm', wasmBytes);
/// vfs.addFile('/lib/stdlib.masm', stdlibSource);
///
/// // Use with compilation
/// const options = new CompileOptions();
/// options.setVirtualFS(vfs);
/// const result = compileWasm(wasmBytes, options);
/// ```
#[derive(Debug)]
#[wasm_bindgen]
pub struct VirtualFileSystem {
    pub(crate) source_manager: Arc<DefaultSourceManager>,
}

#[wasm_bindgen]
impl VirtualFileSystem {
    /// Create a new empty virtual file system
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            source_manager: Arc::new(DefaultSourceManager::default()),
        }
    }

    /// Add a file to the virtual filesystem
    ///
    /// # Arguments
    /// * `path` - Virtual path like "/src/main.wasm" or "/lib/utils.masm"
    /// * `contents` - File contents as bytes (will be converted to UTF-8 string)
    ///
    /// # Returns
    /// * `Ok(())` on success
    /// * `Err(JsValue)` if the contents are not valid UTF-8
    ///
    /// # Example (JavaScript)
    ///
    /// ```javascript
    /// vfs.addFile('/input.wasm', wasmBytes);
    /// vfs.addTextFile('/lib/math.masm', masmSource);
    /// ```
    #[wasm_bindgen(js_name = addFile)]
    pub fn add_file(&self, path: &str, contents: &[u8]) -> Result<(), JsValue> {
        // Convert bytes to UTF-8 string
        use crate::alloc::format;
        let content = String::from_utf8(contents.to_vec())
            .map_err(|e| JsValue::from_str(&format!(
                "Failed to parse file '{}' as UTF-8: {}",
                path, e
            )))?;

        // Determine language from file extension
        let lang = determine_language(path);

        // Create URI from path
        let uri = Uri::from(path);

        // Load into source manager
        self.source_manager.load(lang, uri, content);

        Ok(())
    }

    /// Add a text file to the virtual filesystem
    ///
    /// This is a convenience method for adding text files (like .masm source files)
    ///
    /// # Arguments
    /// * `path` - Virtual path like "/lib/utils.masm"
    /// * `contents` - File contents as a string
    ///
    /// # Example (JavaScript)
    ///
    /// ```javascript
    /// vfs.addTextFile('/lib/math.masm', 'proc.add\n  add\nend');
    /// ```
    #[wasm_bindgen(js_name = addTextFile)]
    pub fn add_text_file(&self, path: &str, contents: &str) -> Result<(), JsValue> {
        let lang = determine_language(path);
        let uri = Uri::from(path);
        self.source_manager.load(lang, uri, contents.to_string());
        Ok(())
    }

    /// Check if a file exists in the VFS
    ///
    /// # Arguments
    /// * `path` - Virtual path to check
    ///
    /// # Returns
    /// * `true` if the file exists, `false` otherwise
    #[wasm_bindgen(js_name = hasFile)]
    pub fn has_file(&self, path: &str) -> bool {
        let uri = Uri::from(path);
        self.source_manager.find(&uri).is_some()
    }

    /// Get a file's contents from the VFS
    ///
    /// # Arguments
    /// * `path` - Virtual path to read
    ///
    /// # Returns
    /// * File contents as a string, or `null` if the file doesn't exist
    #[wasm_bindgen(js_name = getFile)]
    pub fn get_file(&self, path: &str) -> Option<String> {
        let uri = Uri::from(path);
        self.source_manager.get_by_uri(&uri)
            .map(|file| file.as_str().to_string())
    }

    /// List all files in the VFS
    ///
    /// # Returns
    /// * Array of file paths currently stored in the VFS
    ///
    /// Note: This is currently limited because DefaultSourceManager doesn't
    /// expose a way to iterate over all URIs. For now, returns an empty array.
    /// Files can still be accessed via `getFile` if you know their paths.
    #[wasm_bindgen(js_name = listFiles)]
    pub fn list_files(&self) -> Vec<String> {
        // Note: DefaultSourceManager doesn't expose an iterator over URIs
        // This would require extending the SourceManager trait or wrapping it
        // For now, return empty array - files are still accessible via getFile
        Vec::new()
    }

    /// Clear all files from the VFS
    ///
    /// Creates a new empty source manager, effectively removing all registered files.
    #[wasm_bindgen(js_name = clear)]
    pub fn clear(&mut self) {
        self.source_manager = Arc::new(DefaultSourceManager::default());
    }
}

impl VirtualFileSystem {
    /// Get the underlying source manager (internal API for Rust code)
    pub(crate) fn source_manager(&self) -> Arc<DefaultSourceManager> {
        Arc::clone(&self.source_manager)
    }
}

/// Determine the source language from a file path
fn determine_language(path: &str) -> SourceLanguage {
    // Extract extension
    if let Some(ext_start) = path.rfind('.') {
        let ext = &path[ext_start + 1..];
        match ext {
            "masm" => SourceLanguage::Masm,
            "wasm" | "wat" => SourceLanguage::Other("wasm"),
            "rs" => SourceLanguage::Rust,
            "c" => SourceLanguage::Other("c"),
            "cpp" | "cc" | "cxx" => SourceLanguage::Other("cpp"),
            _ => SourceLanguage::Other("unknown"),
        }
    } else {
        SourceLanguage::Other("unknown")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_language() {
        assert_eq!(determine_language("/src/main.masm"), SourceLanguage::Masm);
        assert_eq!(determine_language("/input.wasm"), SourceLanguage::Other("wasm"));
        assert_eq!(determine_language("/lib/math.wat"), SourceLanguage::Other("wasm"));
        assert_eq!(determine_language("/code.rs"), SourceLanguage::Rust);
        assert_eq!(determine_language("/no_extension"), SourceLanguage::Other("unknown"));
    }

    #[test]
    fn test_vfs_basic_operations() {
        let vfs = VirtualFileSystem::new();

        // File shouldn't exist initially
        assert!(!vfs.has_file("/test.masm"));

        // Add a text file
        vfs.add_text_file("/test.masm", "proc.test\nend").unwrap();

        // Now it should exist
        assert!(vfs.has_file("/test.masm"));

        // We should be able to retrieve it
        let content = vfs.get_file("/test.masm").unwrap();
        assert_eq!(content, "proc.test\nend");

        // Non-existent file should return None
        assert!(vfs.get_file("/nonexistent.masm").is_none());
    }

    #[test]
    fn test_vfs_add_file_utf8() {
        let vfs = VirtualFileSystem::new();

        // Valid UTF-8
        let valid_utf8 = b"proc.test\nend";
        assert!(vfs.add_file("/valid.masm", valid_utf8).is_ok());

        // Invalid UTF-8 should fail
        let invalid_utf8 = &[0xFF, 0xFE, 0xFD];
        assert!(vfs.add_file("/invalid.bin", invalid_utf8).is_err());
    }

    #[test]
    fn test_vfs_clear() {
        let mut vfs = VirtualFileSystem::new();

        vfs.add_text_file("/test.masm", "content").unwrap();
        assert!(vfs.has_file("/test.masm"));

        vfs.clear();
        assert!(!vfs.has_file("/test.masm"));
    }
}
