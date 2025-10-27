# midenc-wasm

WebAssembly bindings for the Miden compiler, allowing you to compile WebAssembly to Miden Assembly directly in the browser or Node.js.

## Features

- Compile WebAssembly bytecode to Miden Assembly
- Configurable target environments (Base, Emu, Rollup)
- Multiple optimization levels (None, Minimal, Balanced, Aggressive)
- Program vs Library project types
- Virtual File System (VFS) for browser environments
- Debug info support (DWARF parsing) with VFS integration
- TypeScript type definitions (auto-generated)
- Works in browser and Node.js

## Prerequisites

Before building, ensure you have:

1. **Rust toolchain** with `wasm32-unknown-unknown` target:
   ```bash
   rustup target add wasm32-unknown-unknown
   ```

2. **wasm-pack** installed:
   ```bash
   cargo install wasm-pack
   ```

3. **Miden VM** checked out at `../miden-vm`:
   ```bash
   cd ..
   git clone https://github.com/walnuthq/miden-vm.git
   cd miden-vm
   git feature/wasm32-support
   cd ../compiler/midenc-wasm
   ```

## Building for WASM

### Development Build (faster, larger)

```bash
wasm-pack build --target web --dev
```

This creates unoptimized builds with debug symbols for faster compilation during development.

### Production Build (slower, optimized)

```bash
wasm-pack build --target web --release
```

This creates optimized builds suitable for production use.

### Build for Different Targets

```bash
# For direct use in web browsers
wasm-pack build --target web

# For Node.js
wasm-pack build --target nodejs

# For bundlers (webpack, rollup, etc.)
wasm-pack build --target bundler
```

The built package will be in the `pkg/` directory.

## Testing

### Interactive Compiler

Run the browser-based compiler:

```bash
# Start a local web server
python3 -m http.server 8000

# Open in your browser:
# http://localhost:8000/test.html
```

**Features:**
- 📁 **Drag & drop or select WASM files** - Upload any `.wasm` file
- 🎯 **Target selection** - Choose from Base, Emu, or Rollup environments
- ⚡ **Optimization toggle** - Enable/disable optimization passes
- 📊 **Real-time compilation** - See MASM output immediately
- ⏱️ **Performance metrics** - View compilation time and output size

**Try the included test files:**
- `test_add.wasm` - Simple addition function (41 bytes, ~60ms compile time)
  ```wasm
  (func $add (param i32 i32) (result i32)
    local.get 0
    local.get 1
    i32.add)
  ```
  Compiles to: `u32wrapping_add`

- `test_factorial.wasm` - Iterative factorial with loops and conditionals (82 bytes)
  ```wasm
  (func $factorial (param i32) (result i32) ...)
  ```
  Tests complex control flow transformation (CF → SCF)

## Usage

### In Browser

```html
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Miden Compiler WASM Example</title>
</head>
<body>
    <h1>Miden Compiler in Browser</h1>
    <script type="module">
        import init, { compileWasm, CompileOptions, getVersion } from './pkg/midenc_wasm.js';

        async function main() {
            // Initialize the WASM module
            await init();

            console.log('Miden Compiler Version:', getVersion());

            // Load WASM file
            const response = await fetch('example.wasm');
            const wasmBytes = new Uint8Array(await response.arrayBuffer());

            // Create compilation options
            const options = new CompileOptions();
            options.setTarget('base');
            options.setOptimize(true);
            options.setProjectType('program');

            // Compile WASM to MASM
            const result = compileWasm(wasmBytes, options);

            if (result.success) {
                console.log('Compiled MASM:', result.output);
            } else {
                console.error('Compilation error:', result.error);
            }
        }

        main().catch(console.error);
    </script>
</body>
</html>
```

### In Node.js

```javascript
const { compileWasm, CompileOptions, getVersion } = require('./pkg/midenc_wasm');
const fs = require('fs');

async function main() {
    console.log('Miden Compiler Version:', getVersion());

    // Load WASM file
    const wasmBytes = fs.readFileSync('example.wasm');

    // Create compilation options
    const options = new CompileOptions();
    options.setTarget('base');
    options.setOptimize(false);
    options.setProjectType('program');

    // Compile WASM to MASM
    const result = compileWasm(wasmBytes, options);

    if (result.success) {
        console.log('Compiled MASM:', result.output);
        fs.writeFileSync('output.masm', result.output);
    } else {
        console.error('Compilation error:', result.error);
    }
}

main().catch(console.error);
```

### With TypeScript

```typescript
import init, { compileWasm, CompileOptions, CompileResult, getVersion } from './pkg/midenc_wasm';

async function compile(wasmBytes: Uint8Array): Promise<string> {
    await init();

    const options = new CompileOptions();
    options.setTarget('base');
    options.setOptimize(true);

    const result: CompileResult = compileWasm(wasmBytes, options);

    if (!result.success) {
        throw new Error(result.error);
    }

    return result.output;
}
```

### With Virtual File System and Debug Info

```javascript
import init, { compileWasm, CompileOptions, VirtualFileSystem } from './pkg/midenc_wasm.js';

async function compileWithDebugInfo() {
    await init();

    // Create VFS and add source files
    const vfs = new VirtualFileSystem();

    // Add source files that are referenced in WASM debug info
    vfs.addTextFile('/src/main.c', `
        int add(int a, int b) {
            return a + b;
        }
    `);

    vfs.addTextFile('/lib/helper.c', `
        int multiply(int a, int b) {
            return a * b;
        }
    `);

    // Load WASM file (compiled with debug info: clang -g -target wasm32)
    const response = await fetch('example.wasm');
    const wasmBytes = new Uint8Array(await response.arrayBuffer());

    // Configure compilation options
    const options = new CompileOptions();
    options.setTarget('base');
    options.setOptimizationLevel('balanced');
    options.setProjectType('program');
    options.setVirtualFS(vfs);
    options.setParseDebugInfo(true);  // Enable debug info parsing

    // Compile with enhanced error messages
    const result = compileWasm(wasmBytes, options);

    if (result.success) {
        console.log('Compiled MASM:', result.output);
        // Errors will now include source locations from VFS files
    } else {
        console.error('Compilation error:', result.error);
    }
}
```

## API Reference

### `init()`

Initialize the WASM module. Must be called before using any other functions.

```javascript
await init();
```

### `getVersion(): string`

Get the version of the Miden compiler.

```javascript
const version = getVersion();
console.log('Version:', version);
```

### `class CompileOptions`

Configuration options for compilation.

#### Constructor

```javascript
const options = new CompileOptions();
```

#### Methods

##### `setTarget(target: string): void`

Set the target environment. Valid targets:
- `'base'` - Default Miden VM environment
- `'emu'` - Emulator environment (restricted instruction set)
- `'rollup-account'` - Rollup account component
- `'rollup-note'` - Rollup note script
- `'rollup-tx'` - Rollup transaction script
- `'rollup-auth'` - Rollup authentication component

```javascript
options.setTarget('base');
```

##### `setOptimize(optimize: boolean): void`

Enable or disable optimizations.

```javascript
options.setOptimize(true);
```

##### `setProjectType(projectType: string): void`

Set the project type. Valid types:
- `'program'` - Executable Miden program
- `'library'` - Miden library

```javascript
options.setProjectType('program');
```

##### `setOptimizationLevel(level: string): void`

Set the optimization level. Valid levels:
- `'none'` - No optimizations (fastest compilation)
- `'minimal'` - Minimal optimizations (constant propagation only)
- `'balanced'` - Balanced optimizations (recommended, default)
- `'aggressive'` - All available optimizations

```javascript
options.setOptimizationLevel('balanced');
```

##### `setVirtualFS(vfs: VirtualFileSystem): void`

Set a virtual file system for source file management. This is essential for browser environments where filesystem access is not available.

```javascript
const vfs = new VirtualFileSystem();
vfs.addTextFile('/src/main.c', sourceCode);
options.setVirtualFS(vfs);
```

##### `setParseDebugInfo(enable: boolean): void`

Enable or disable parsing of WASM debug information (DWARF sections). When enabled, the compiler will extract source location information and map it to files in the VFS.

```javascript
options.setParseDebugInfo(true);
```

### `class VirtualFileSystem`

In-memory file system for browser-based compilation. Allows you to provide source files for debug information and library dependencies.

#### Constructor

```javascript
const vfs = new VirtualFileSystem();
```

#### Methods

##### `addFile(path: string, contents: Uint8Array): void`

Add a file to the virtual filesystem from bytes.

```javascript
const bytes = new TextEncoder().encode('source code');
vfs.addFile('/src/main.c', bytes);
```

##### `addTextFile(path: string, contents: string): void`

Add a text file to the virtual filesystem.

```javascript
vfs.addTextFile('/lib/stdlib.masm', 'proc.add\n  add\nend');
```

##### `hasFile(path: string): boolean`

Check if a file exists in the VFS.

```javascript
if (vfs.hasFile('/src/main.c')) {
    console.log('File exists');
}
```

##### `getFile(path: string): string | null`

Get a file's contents from the VFS.

```javascript
const content = vfs.getFile('/src/main.c');
```

##### `clear(): void`

Clear all files from the VFS.

```javascript
vfs.clear();
```

### `compileWasm(wasmBytes: Uint8Array, options: CompileOptions): CompileResult`

Compile WebAssembly bytecode to Miden Assembly.

**Note:** Options are required to avoid ownership issues in the WASM boundary.

```javascript
const options = new CompileOptions();
const result = compileWasm(wasmBytes, options);
```

### `class CompileResult`

Result of compilation.

#### Properties

##### `success: boolean`

Whether compilation was successful.

##### `output: string | undefined`

The compiled Miden Assembly code (only present if `success` is `true`).

##### `error: string | undefined`

The error message (only present if `success` is `false`).

## Development

### Building for Native Target (Development)

```bash
# Build for native development/testing
cargo build --target wasm32-unknown-unknown --no-default-features

# Check compilation without building
cargo check --target wasm32-unknown-unknown --no-default-features
```

### Running Tests

```bash
# Run Rust unit tests
cargo test

# Run WASM tests in Node.js
wasm-pack test --node

# Run WASM tests in browser
wasm-pack test --headless --firefox  # or --chrome
```

## Package Structure

```
midenc-wasm/
├── src/
│   └── lib.rs              # Main WASM bindings and compilation pipeline
├── pkg/                    # Generated WASM package (after build)
│   ├── midenc_wasm.js      # JavaScript bindings
│   ├── midenc_wasm.d.ts    # TypeScript definitions
│   ├── midenc_wasm_bg.wasm # WASM binary
│   └── package.json        # NPM metadata
├── test.html               # Automated test suite
├── example.html            # Interactive demo UI
├── Cargo.toml              # Rust package manifest
└── README.md               # This file
```

## Implementation Details

The compilation pipeline consists of several stages:

### 1. WASM Parsing & HIR Translation
- **Frontend** (`midenc-frontend-wasm`): Parses WASM bytecode
- Creates HIR structure: `World → Component → Module → Function`
- Translates WASM instructions to HIR operations

### 2. Transformation Passes
The pass manager applies transformations with proper hierarchy nesting:

```rust
PassManager::on::<Component>
  └─ nest::<Module>
      └─ nest::<Function>
          ├─ Canonicalizer          // Simplify HIR
          ├─ LiftControlFlowToSCF   // CF → SCF transformation ⚡
          ├─ Canonicalizer          // Clean up
          ├─ SinkOperandDefs        // Optimize operands
          ├─ ControlFlowSink        // Sink control flow
          └─ TransformSpills        // Handle register spills
```

**Key Fix:** The pass manager must nest through `Module` to reach `Function` operations. The WASM frontend creates functions inside modules, not directly in components.

### 3. Code Generation
- **HIR** (`midenc-hir`): High-level intermediate representation
- **Codegen** (`midenc-codegen-masm`): Generates Miden Assembly from HIR
- Uses `ToMasmComponent` trait to convert HIR to MASM
- **Session** (`midenc-session`): Manages compilation state and diagnostics

### 4. Output
- Formatted Miden Assembly ready for execution
- Includes module declarations and exported functions

All dependencies are configured with `default-features = false` for no-std compatibility, enabling compilation to `wasm32-unknown-unknown` target.
