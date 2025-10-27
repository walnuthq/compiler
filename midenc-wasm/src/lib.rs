#![no_std]

extern crate alloc;

mod vfs;

use alloc::{
    boxed::Box,
    format,
    rc::Rc,
    string::{String, ToString},
    sync::Arc,
    vec,
    vec::Vec,
};
use wasm_bindgen::prelude::*;

pub use vfs::VirtualFileSystem;

use midenc_codegen_masm::{ToMasmComponent, register_dialect_hooks};
use midenc_frontend_wasm::{translate, WasmTranslationConfig};
use midenc_hir::{
    dialects::builtin,
    pass::{AnalysisManager, Nesting, PassManager},
    patterns::{GreedyRewriteConfig, RegionSimplificationLevel},
    Context,
};
use midenc_hir_transform::{Canonicalizer, ControlFlowSink, SinkOperandDefs};
use midenc_dialect_hir::transforms::TransformSpills;
use midenc_dialect_scf::transforms::LiftControlFlowToSCF;
use midenc_session::{
    diagnostics::{Buffer, DefaultSourceManager, Emitter, Report},
    ColorChoice, OptLevel, Options, OutputTypes, PathBuf, ProjectType, RollupTarget,
    Session, TargetEnv, Verbosity, Warnings,
};

// We use RefCell since WASM is single-threaded and we're in no_std mode
use core::cell::RefCell;

/// Initialize the WASM module with better panic messages
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Thread-safe wrapper for RefCell (safe in WASM since it's single-threaded)
struct SyncRefCell<T>(RefCell<T>);

// SAFETY: WASM is single-threaded, so RefCell is effectively thread-safe
unsafe impl<T> Sync for SyncRefCell<T> {}
unsafe impl<T> Send for SyncRefCell<T> {}

impl<T> SyncRefCell<T> {
    fn new(value: T) -> Self {
        Self(RefCell::new(value))
    }

    fn borrow(&self) -> core::cell::Ref<'_, T> {
        self.0.borrow()
    }

    fn borrow_mut(&self) -> core::cell::RefMut<'_, T> {
        self.0.borrow_mut()
    }
}

/// WASM-compatible diagnostic emitter that captures diagnostics to a string buffer
///
/// This emitter is designed for browser environments where we cannot write to
/// stderr. Instead, it captures all diagnostic output into an in-memory buffer
/// that can be retrieved and returned to JavaScript.
///
/// Note: We use SyncRefCell (RefCell with unsafe Sync) since WASM is single-threaded.
struct CaptureEmitter {
    buffer: SyncRefCell<Vec<u8>>,
    ansi: bool,
}

impl CaptureEmitter {
    fn new() -> Self {
        Self {
            buffer: SyncRefCell::new(Vec::new()),
            ansi: false, // No ANSI colors in browser console by default
        }
    }

    fn captured(&self) -> String {
        let buffer = self.buffer.borrow();
        String::from_utf8_lossy(&buffer).into_owned()
    }

    fn clear(&self) {
        self.buffer.borrow_mut().clear();
    }
}

impl Emitter for CaptureEmitter {
    fn buffer(&self) -> Buffer {
        if self.ansi {
            Buffer::ansi()
        } else {
            Buffer::no_color()
        }
    }

    fn print(&self, buffer: Buffer) -> Result<(), Report> {
        let mut bytes = buffer.into_inner();
        let mut buf = self.buffer.borrow_mut();
        buf.push(b'\n');
        buf.append(&mut bytes);
        Ok(())
    }
}

/// Optimization level for compilation
#[wasm_bindgen]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationLevel {
    /// No optimizations (fastest compile time)
    None = 0,
    /// Minimal optimizations (constant propagation only)
    Minimal = 1,
    /// Balanced optimizations (default, recommended)
    Balanced = 2,
    /// Aggressive optimizations (all available passes)
    Aggressive = 3,
}

impl Default for OptimizationLevel {
    fn default() -> Self {
        OptimizationLevel::None
    }
}

/// Compilation options for the Miden compiler
#[wasm_bindgen]
#[derive(Debug)]
pub struct CompileOptions {
    target: TargetEnv,
    optimize: bool,
    opt_level: OptimizationLevel,
    project_type: ProjectType,
    vfs: Option<VirtualFileSystem>,
    parse_debug_info: bool,
}

impl Clone for CompileOptions {
    fn clone(&self) -> Self {
        Self {
            target: self.target,
            optimize: self.optimize,
            opt_level: self.opt_level,
            project_type: self.project_type,
            vfs: self.vfs.as_ref().map(|vfs| {
                // Create a new VFS instance that shares the same underlying source manager
                VirtualFileSystem {
                    source_manager: vfs.source_manager(),
                }
            }),
            parse_debug_info: self.parse_debug_info,
        }
    }
}

#[wasm_bindgen]
impl CompileOptions {
    /// Create new compilation options with default settings
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            target: TargetEnv::Base,
            optimize: false,
            opt_level: OptimizationLevel::None,
            project_type: ProjectType::Program,
            vfs: None,
            parse_debug_info: false,
        }
    }

    /// Set the target environment (base, emu, rollup-account, rollup-note, etc.)
    #[wasm_bindgen(js_name = setTarget)]
    pub fn set_target(&mut self, target: &str) -> Result<(), JsValue> {
        self.target = match target {
            "base" => TargetEnv::Base,
            "emu" => TargetEnv::Emu,
            "rollup" | "rollup-account" => {
                TargetEnv::Rollup {
                    target: RollupTarget::Account,
                }
            }
            "rollup-note" => {
                TargetEnv::Rollup {
                    target: RollupTarget::NoteScript,
                }
            }
            "rollup-tx" => {
                TargetEnv::Rollup {
                    target: RollupTarget::TransactionScript,
                }
            }
            "rollup-auth" => {
                TargetEnv::Rollup {
                    target: RollupTarget::AuthComponent,
                }
            }
            _ => {
                return Err(JsValue::from_str(&format!(
                    "Invalid target: {}. Valid targets are: base, emu, rollup-account, rollup-note, rollup-tx, rollup-auth",
                    target
                )))
            }
        };
        Ok(())
    }

    /// Enable or disable optimizations (maps to Balanced/None optimization levels)
    #[wasm_bindgen(js_name = setOptimize)]
    pub fn set_optimize(&mut self, optimize: bool) {
        self.optimize = optimize;
        self.opt_level = if optimize {
            OptimizationLevel::Balanced
        } else {
            OptimizationLevel::None
        };
    }

    /// Set optimization level: 'none', 'minimal', 'balanced', or 'aggressive'
    #[wasm_bindgen(js_name = setOptimizationLevel)]
    pub fn set_optimization_level(&mut self, level: &str) -> Result<(), JsValue> {
        self.opt_level = match level {
            "none" => OptimizationLevel::None,
            "minimal" => OptimizationLevel::Minimal,
            "balanced" => OptimizationLevel::Balanced,
            "aggressive" => OptimizationLevel::Aggressive,
            _ => {
                return Err(JsValue::from_str(&format!(
                    "Invalid optimization level: {}. Valid levels are: none, minimal, balanced, aggressive",
                    level
                )))
            }
        };
        self.optimize = self.opt_level != OptimizationLevel::None;
        Ok(())
    }

    /// Set the project type (program or library)
    #[wasm_bindgen(js_name = setProjectType)]
    pub fn set_project_type(&mut self, project_type: &str) -> Result<(), JsValue> {
        self.project_type = match project_type {
            "program" => ProjectType::Program,
            "library" => ProjectType::Library,
            _ => {
                return Err(JsValue::from_str(&format!(
                    "Invalid project type: {}. Valid types are: program, library",
                    project_type
                )))
            }
        };
        Ok(())
    }

    /// Set the virtual file system for source file management
    ///
    /// This allows you to provide source files, debug information, and library
    /// dependencies in a browser environment where filesystem access is not available.
    ///
    /// # Example (JavaScript)
    ///
    /// ```javascript
    /// const vfs = new VirtualFileSystem();
    /// vfs.addFile('/lib/stdlib.masm', stdlibSource);
    ///
    /// const options = new CompileOptions();
    /// options.setVirtualFS(vfs);
    /// ```
    #[wasm_bindgen(js_name = setVirtualFS)]
    pub fn set_virtual_fs(&mut self, vfs: VirtualFileSystem) {
        self.vfs = Some(vfs);
    }

    /// Check if a virtual file system has been configured
    #[wasm_bindgen(js_name = hasVirtualFS)]
    pub fn has_virtual_fs(&self) -> bool {
        self.vfs.is_some()
    }

    /// Enable or disable parsing of WASM debug information
    ///
    /// When enabled, the compiler will extract debug information from WASM files
    /// (DWARF sections) and use it to provide better error messages and source
    /// location tracking. Source files referenced in debug info should be added
    /// to the VFS using `setVirtualFS()`.
    ///
    /// # Example (JavaScript)
    ///
    /// ```javascript
    /// const vfs = new VirtualFileSystem();
    /// vfs.addTextFile('/src/main.c', sourceCode);
    ///
    /// const options = new CompileOptions();
    /// options.setVirtualFS(vfs);
    /// options.setParseDebugInfo(true); // Enable debug info parsing
    /// ```
    #[wasm_bindgen(js_name = setParseDebugInfo)]
    pub fn set_parse_debug_info(&mut self, parse_debug_info: bool) {
        self.parse_debug_info = parse_debug_info;
    }

    /// Check if debug info parsing is enabled
    #[wasm_bindgen(js_name = getParseDebugInfo)]
    pub fn get_parse_debug_info(&self) -> bool {
        self.parse_debug_info
    }
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Source code location for error reporting
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct SourceLocation {
    file: String,
    line: u32,
    column: u32,
}

#[wasm_bindgen]
impl SourceLocation {
    /// Get the file path
    #[wasm_bindgen(getter)]
    pub fn file(&self) -> String {
        self.file.clone()
    }

    /// Get the line number (1-indexed)
    #[wasm_bindgen(getter)]
    pub fn line(&self) -> u32 {
        self.line
    }

    /// Get the column number (1-indexed)
    #[wasm_bindgen(getter)]
    pub fn column(&self) -> u32 {
        self.column
    }
}

/// Detailed compilation error information
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct CompileError {
    kind: ErrorKind,
    message: String,
    source_location: Option<SourceLocation>,
    suggestions: Vec<String>,
    formatted: String,
}

#[wasm_bindgen]
impl CompileError {
    /// Get the error kind (parse, validation, transform, codegen)
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        format!("{:?}", self.kind).to_lowercase()
    }

    /// Get the error message
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.message.clone()
    }

    /// Get the source location if available
    #[wasm_bindgen(getter, js_name = sourceLocation)]
    pub fn source_location(&self) -> Option<SourceLocation> {
        self.source_location.clone()
    }

    /// Get the list of suggestions for fixing the error
    #[wasm_bindgen(getter)]
    pub fn suggestions(&self) -> Vec<String> {
        self.suggestions.clone()
    }

    /// Get the formatted error message with all details
    #[wasm_bindgen(getter)]
    pub fn formatted(&self) -> String {
        self.formatted.clone()
    }
}

/// Error categories for structured error handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorKind {
    /// Error parsing WASM bytecode
    Parse,
    /// Error validating WASM module
    Validation,
    /// Error during HIR transformation passes
    Transform,
    /// Error generating MASM code
    Codegen,
    /// Internal compiler error
    Internal,
}

/// Result of compilation
#[wasm_bindgen]
pub struct CompileResult {
    success: bool,
    output: Option<String>,
    error: Option<String>,
    structured_error: Option<CompileError>,
}

#[wasm_bindgen]
impl CompileResult {
    /// Check if compilation was successful
    #[wasm_bindgen(getter)]
    pub fn success(&self) -> bool {
        self.success
    }

    /// Get the compiled output (MASM code)
    #[wasm_bindgen(getter)]
    pub fn output(&self) -> Option<String> {
        self.output.clone()
    }

    /// Get the error message if compilation failed
    #[wasm_bindgen(getter)]
    pub fn error(&self) -> Option<String> {
        self.error.clone()
    }

    /// Get structured error information if available
    #[wasm_bindgen(getter, js_name = structuredError)]
    pub fn structured_error(&self) -> Option<CompileError> {
        self.structured_error.clone()
    }
}

/// Compile WebAssembly bytecode to Miden Assembly
///
/// # Arguments
/// * `wasm_bytes` - The WebAssembly bytecode as a Uint8Array
/// * `options` - Compilation options (pass by reference to avoid ownership transfer)
///
/// # Returns
/// A CompileResult containing the compiled MASM code or an error message
#[wasm_bindgen(js_name = compileWasm)]
pub fn compile_wasm(wasm_bytes: &[u8], options: &CompileOptions) -> CompileResult {
    let options = options.clone();

    // Compile the WASM to MASM
    match compile_to_masm(wasm_bytes, options) {
        Ok(masm) => CompileResult {
            success: true,
            output: Some(masm),
            error: None,
            structured_error: None,
        },
        Err((error_msg, structured_error)) => CompileResult {
            success: false,
            output: None,
            error: Some(error_msg),
            structured_error: Some(structured_error),
        },
    }
}

/// Internal compilation result with structured error information
type CompilationResult = Result<String, (String, CompileError)>;

/// Create a structured error from a message and error kind
fn create_error(kind: ErrorKind, message: String, formatted: Option<String>) -> (String, CompileError) {
    let suggestions = match kind {
        ErrorKind::Parse => vec![
            "Verify that the input is valid WebAssembly bytecode".to_string(),
            "Check that the WASM file is not corrupted".to_string(),
        ],
        ErrorKind::Validation => vec![
            "Ensure the WASM module passes validation".to_string(),
            "Check that all required imports are provided".to_string(),
        ],
        ErrorKind::Transform => vec![
            "Try disabling optimizations with setOptimize(false)".to_string(),
            "Report this issue if it persists".to_string(),
        ],
        ErrorKind::Codegen => vec![
            "Check that the WASM module uses supported features".to_string(),
            "Some WASM instructions may not be supported yet".to_string(),
        ],
        ErrorKind::Internal => vec![
            "This is a compiler bug - please report it".to_string(),
        ],
    };

    let structured_error = CompileError {
        kind,
        message: message.clone(),
        source_location: None, // TODO: Extract from diagnostics
        suggestions,
        formatted: formatted.unwrap_or_else(|| message.clone()),
    };

    (message, structured_error)
}

/// Compile WASM bytes to Miden Assembly
fn compile_to_masm(wasm_bytes: &[u8], options: CompileOptions) -> CompilationResult {
    log(&format!("[DEBUG] compile_to_masm: Starting compilation with {} bytes", wasm_bytes.len()));

    // Validate input
    if wasm_bytes.is_empty() {
        return Err(create_error(
            ErrorKind::Parse,
            "Input WASM bytes are empty".to_string(),
            None,
        ));
    }

    log("[DEBUG] compile_to_masm: Input validation passed");

    // Validate WASM magic number
    if wasm_bytes.len() < 8 {
        return Err(create_error(
            ErrorKind::Parse,
            format!("WASM input too short: {} bytes (need at least 8)", wasm_bytes.len()),
            None,
        ));
    }

    if &wasm_bytes[0..4] != b"\0asm" {
        return Err(create_error(
            ErrorKind::Parse,
            format!("Invalid WASM magic number: {:?}", &wasm_bytes[0..4]),
            None,
        ));
    }

    log(&format!("[DEBUG] compile_to_masm: WASM magic valid, version: {:?}", &wasm_bytes[4..8]));

    // Create session for compilation with diagnostic capture
    log("[DEBUG] compile_to_masm: Creating session");
    let (session, emitter) = create_session_rc(options.clone())
        .map_err(|e| create_error(ErrorKind::Internal, format!("Session creation error: {}", e), None))?;
    log("[DEBUG] compile_to_masm: Session created");

    // Create HIR context
    log("[DEBUG] compile_to_masm: Creating HIR context");
    let context = Rc::new(Context::new(session));
    log("[DEBUG] compile_to_masm: HIR context created");

    // Register codegen dialect hooks
    log("[DEBUG] compile_to_masm: Registering dialect hooks");
    register_dialect_hooks(&context);
    log("[DEBUG] compile_to_masm: Dialect hooks registered");

    // Create WASM translation config
    use alloc::borrow::Cow;
    let config = WasmTranslationConfig {
        source_name: Cow::Borrowed("input.wasm"),
        override_name: None,
        world: None,
        generate_native_debuginfo: false,
        parse_wasm_debuginfo: options.parse_debug_info,
    };
    log(&format!("[DEBUG] compile_to_masm: WASM config created (parse_debug_info: {})", options.parse_debug_info));

    // Step 1: Translate WASM to HIR
    log("[DEBUG] compile_to_masm: Starting WASM → HIR translation");
    let frontend_output = match translate(wasm_bytes, &config, context.clone()) {
        Ok(output) => {
            log("[DEBUG] compile_to_masm: WASM translation successful");
            output
        },
        Err(e) => {
            let error_msg = format!("WASM translation error: {}", e);
            log(&format!("[ERROR] {}", error_msg));
            let formatted = emitter.captured();
            return Err(create_error(
                ErrorKind::Validation,
                error_msg,
                if formatted.is_empty() { None } else { Some(formatted) },
            ));
        }
    };

    let component = frontend_output.component;
    log(&format!("[DEBUG] compile_to_masm: Got component from frontend"));

    // Step 2: Run required transformation passes
    // These are REQUIRED (not optional) - they transform CF to SCF for code generation
    log("[DEBUG] compile_to_masm: Running transformation passes (CF→SCF)");

    // Set up pass manager to run on the Component
    let mut pm = PassManager::on::<builtin::Component>(context.clone(), Nesting::Implicit);

    let mut rewrite_config = GreedyRewriteConfig::default();
    rewrite_config.with_region_simplification_level(RegionSimplificationLevel::Normal);

    // Apply function-level transformation passes
    // CRITICAL: Functions are nested inside Module inside Component, so we need to nest twice!
    {
        // First nest to Module level
        let mut module_pm = pm.nest::<builtin::Module>();

        // Then nest to Function level
        let mut func_pm = module_pm.nest::<builtin::Function>();

        // Canonicalize HIR
        func_pm.add_pass(Canonicalizer::create_with_config(&rewrite_config));

        // CRITICAL: Transform CF (control flow graph) to SCF (structured control flow)
        // This converts cf.br operations to scf.if/scf.while that can be lowered to MASM
        func_pm.add_pass(Box::new(LiftControlFlowToSCF));

        // Re-canonicalize to clean up
        func_pm.add_pass(Canonicalizer::create_with_config(&rewrite_config));

        // Optimization passes based on optimization level
        match options.opt_level {
            OptimizationLevel::None => {
                // No optimizations - fastest compile time
            }
            OptimizationLevel::Minimal => {
                // FIXME: SCCP causes AliasingViolationError in WASM environment
                // See: hir/src/ir/visit/walkable.rs:364 and hir/src/pass/pass.rs:126
                // The pass manager borrows IR immutably while SCCP's analyses need mutable access
                // For now, minimal just uses extra canonicalization
                func_pm.add_pass(Canonicalizer::create_with_config(&rewrite_config));
            }
            OptimizationLevel::Balanced => {
                // Recommended: sinking optimizations (SCCP disabled due to aliasing bug)
                func_pm.add_pass(Canonicalizer::create_with_config(&rewrite_config));
                func_pm.add_pass(Box::new(SinkOperandDefs));
                func_pm.add_pass(Box::new(ControlFlowSink));
            }
            OptimizationLevel::Aggressive => {
                // All available optimizations (same as Balanced until SCCP is fixed)
                func_pm.add_pass(Canonicalizer::create_with_config(&rewrite_config));
                func_pm.add_pass(Box::new(SinkOperandDefs));
                func_pm.add_pass(Box::new(ControlFlowSink));
                // TODO: Re-enable SCCP when aliasing bug is fixed
                // TODO: Add CSE, DCE, Inliner when implemented
            }
        }

        // Transform spills
        func_pm.add_pass(Box::new(TransformSpills));
    }

    // Run the pass pipeline on the component
    log("[DEBUG] compile_to_masm: Executing pass pipeline on component");
    match pm.run(component.as_operation_ref()) {
        Ok(_) => log("[DEBUG] compile_to_masm: Pass pipeline completed successfully"),
        Err(e) => {
            let error_msg = format!("Pass pipeline error: {}", e);
            log(&format!("[ERROR] {}", error_msg));
            let formatted = emitter.captured();
            return Err(create_error(
                ErrorKind::Transform,
                error_msg,
                if formatted.is_empty() { None } else { Some(formatted) },
            ));
        }
    }

    // Step 3: Generate MASM from HIR
    log("[DEBUG] compile_to_masm: Creating analysis manager");
    let analysis_manager = AnalysisManager::new(component.as_operation_ref(), None);
    log("[DEBUG] compile_to_masm: Starting HIR → MASM code generation");

    let masm_component = match component.borrow().to_masm_component(analysis_manager) {
        Ok(component) => {
            log("[DEBUG] compile_to_masm: Code generation successful");
            component
        },
        Err(e) => {
            let error_msg = format!("Code generation error: {}", e);
            log(&format!("[ERROR] {}", error_msg));
            let formatted = emitter.captured();
            return Err(create_error(
                ErrorKind::Codegen,
                error_msg,
                if formatted.is_empty() { None } else { Some(formatted) },
            ));
        }
    };

    // Step 4: Convert to string
    log("[DEBUG] compile_to_masm: Formatting MASM output");
    let mut output = String::new();
    use core::fmt::Write;
    match write!(&mut output, "{}", masm_component) {
        Ok(_) => {
            log(&format!("[DEBUG] compile_to_masm: Successfully generated {} bytes of MASM", output.len()));
            Ok(output)
        },
        Err(e) => {
            let error_msg = format!("Failed to format MASM: {}", e);
            log(&format!("[ERROR] {}", error_msg));
            Err(create_error(ErrorKind::Internal, error_msg, None))
        }
    }
}

/// Create a compilation session with the given options (returns Rc and the emitter)
fn create_session_rc(options: CompileOptions) -> Result<(Rc<Session>, Arc<CaptureEmitter>), String> {
    // Create a capture emitter for diagnostics
    let emitter = Arc::new(CaptureEmitter::new());

    // Use VFS source manager if provided, otherwise create a new one
    let source_manager = if let Some(vfs) = options.vfs.as_ref() {
        vfs.source_manager()
    } else {
        Arc::new(DefaultSourceManager::default())
    };

    // Build session options
    let session_options = Options::new(
        Some("wasm_module".to_string()),
        options.target,
        options.project_type,
        PathBuf::from("."),
        None,
    )
    .with_verbosity(Verbosity::Silent)
    .with_warnings(Warnings::All)
    .with_color(ColorChoice::Never)
    .with_optimization(if options.optimize {
        OptLevel::Balanced
    } else {
        OptLevel::None
    })
    .with_output_types(OutputTypes::default());

    // Create session
    let session = Session::new(
        Vec::new(), // no input files for in-memory compilation
        None,       // no output_dir
        None,       // no output_file
        PathBuf::from("target/midenc"), // target_dir
        session_options,
        Some(emitter.clone() as Arc<dyn Emitter>),
        source_manager,
    );

    Ok((Rc::new(session), emitter))
}

/// Log a message to the browser console
#[wasm_bindgen]
pub fn log(message: &str) {
    web_sys::console::log_1(&JsValue::from_str(message));
}

/// Get version information
#[wasm_bindgen(js_name = getVersion)]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn test_compile_options() {
        let mut opts = CompileOptions::new();
        assert!(opts.set_target("base").is_ok());
        assert!(opts.set_target("rollup").is_ok());
        assert!(opts.set_target("invalid").is_err());
    }

    #[wasm_bindgen_test]
    fn test_version() {
        let version = get_version();
        assert!(!version.is_empty());
    }
}
