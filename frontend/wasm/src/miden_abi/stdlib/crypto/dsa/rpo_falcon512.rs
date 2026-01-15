use midenc_hir::{
    CallConv, FunctionType, SymbolNameComponent, SymbolPath,
    Type::*,
    interner::{Symbol, symbols},
};

use crate::miden_abi::{FunctionTypeMap, ModuleFunctionTypeMap};

pub(crate) const MODULE_ID: &str = "miden::core::crypto::dsa::falcon512rpo";

pub(crate) const RPO_FALCON512_VERIFY: &str = "verify";

fn module_path() -> SymbolPath {
    // Build '::miden::core::crypto::dsa::falcon512rpo' using interned symbol components
    let parts = [
        SymbolNameComponent::Root,
        SymbolNameComponent::Component(symbols::Miden),
        SymbolNameComponent::Component(Symbol::intern("core")),
        SymbolNameComponent::Component(symbols::Crypto),
        SymbolNameComponent::Component(symbols::Dsa),
        SymbolNameComponent::Component(Symbol::intern("falcon512rpo")),
    ];
    SymbolPath::from_iter(parts)
}

pub(crate) fn signatures() -> ModuleFunctionTypeMap {
    let mut m: ModuleFunctionTypeMap = Default::default();
    let mut funcs: FunctionTypeMap = Default::default();
    funcs.insert(
        Symbol::from(RPO_FALCON512_VERIFY),
        FunctionType::new(CallConv::Wasm, [Felt, Felt, Felt, Felt, Felt, Felt, Felt, Felt], []),
    );
    m.insert(module_path(), funcs);
    m
}
