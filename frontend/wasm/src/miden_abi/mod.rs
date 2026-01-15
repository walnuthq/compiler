pub(crate) mod stdlib;
pub(crate) mod transform;
pub(crate) mod tx_kernel;

use midenc_hir::{FunctionType, FxHashMap, SymbolNameComponent, SymbolPath, interner::Symbol};
use midenc_hir_symbol::symbols;

pub(crate) type FunctionTypeMap = FxHashMap<Symbol, FunctionType>;
pub(crate) type ModuleFunctionTypeMap = FxHashMap<SymbolPath, FunctionTypeMap>;

/// Check if a path is a stdlib path (either old `::std::*` or new `::miden::core::*`)
fn is_stdlib_path(path: &SymbolPath) -> bool {
    // Old v0.19 stdlib prefix: ::std::*
    const STD_PREFIX: &[SymbolNameComponent] =
        &[SymbolNameComponent::Root, SymbolNameComponent::Component(symbols::Std)];

    if path.is_prefixed_by(STD_PREFIX) {
        return true;
    }

    // New v0.20 stdlib prefix: ::miden::core::*
    // We need to check manually since "core" is not a pre-interned symbol
    let mut components = path.components();
    if components.next() != Some(SymbolNameComponent::Root) {
        return false;
    }
    if components.next().map(|c| c.as_symbol_name()) != Some(symbols::Miden) {
        return false;
    }
    if let Some(comp) = components.next() {
        return comp.as_symbol_name().as_str() == "core";
    }
    false
}

pub fn is_miden_abi_module(path: &SymbolPath) -> bool {
    let module_path = path.without_leaf();
    is_miden_stdlib_module(&module_path) || is_miden_sdk_module(&module_path)
}

fn is_miden_sdk_module(module_path: &SymbolPath) -> bool {
    tx_kernel::signatures().contains_key(module_path)
}

fn is_miden_stdlib_module(module_path: &SymbolPath) -> bool {
    stdlib::signatures().contains_key(module_path)
}

pub fn miden_abi_function_type(path: &SymbolPath) -> FunctionType {
    if is_stdlib_path(path) {
        miden_stdlib_function_type(path)
    } else {
        miden_sdk_function_type(path)
    }
}

/// Get the target Miden ABI tx kernel function type for the given module and function id
pub fn miden_sdk_function_type(path: &SymbolPath) -> FunctionType {
    let module_path = path.without_leaf();
    let funcs = tx_kernel::signatures()
        .get(module_path.as_ref())
        .unwrap_or_else(|| panic!("No Miden ABI function types found for module {module_path}"));
    funcs
        .get(&path.name())
        .cloned()
        .unwrap_or_else(|| panic!("No Miden ABI function type found for function {path}"))
}

/// Get the target Miden ABI stdlib function type for the given module and function id
fn miden_stdlib_function_type(path: &SymbolPath) -> FunctionType {
    let module_path = path.without_leaf();
    let funcs = stdlib::signatures()
        .get(module_path.as_ref())
        .unwrap_or_else(|| panic!("No Miden ABI function types found for module {module_path}"));
    funcs
        .get(&path.name())
        .cloned()
        .unwrap_or_else(|| panic!("No Miden ABI function type found for function {path}"))
}
