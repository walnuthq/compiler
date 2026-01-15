use midenc_hir::{
    CallConv, FunctionType, SymbolNameComponent, SymbolPath,
    Type::*,
    interner::{Symbol, symbols},
};

use crate::miden_abi::{FunctionTypeMap, ModuleFunctionTypeMap};

pub const MODULE_ID: &str = "miden::core::crypto::hashes::blake3";

pub(crate) fn module_prefix() -> [SymbolNameComponent; 6] {
    [
        SymbolNameComponent::Root,
        SymbolNameComponent::Component(symbols::Miden),
        SymbolNameComponent::Component(Symbol::intern("core")),
        SymbolNameComponent::Component(symbols::Crypto),
        SymbolNameComponent::Component(symbols::Hashes),
        SymbolNameComponent::Component(symbols::Blake3),
    ]
}

pub(crate) const HASH_1TO1: &str = "hash_1to1";
pub(crate) const HASH_2TO1: &str = "hash_2to1";

pub(crate) fn signatures() -> ModuleFunctionTypeMap {
    let mut m: ModuleFunctionTypeMap = Default::default();
    let mut blake3: FunctionTypeMap = Default::default();
    blake3.insert(
        Symbol::from(HASH_1TO1),
        FunctionType::new(
            CallConv::Wasm,
            [I32, I32, I32, I32, I32, I32, I32, I32],
            [I32, I32, I32, I32, I32, I32, I32, I32],
        ),
    );
    blake3.insert(
        Symbol::from(HASH_2TO1),
        FunctionType::new(
            CallConv::Wasm,
            [I32, I32, I32, I32, I32, I32, I32, I32, I32, I32, I32, I32, I32, I32, I32, I32],
            [I32, I32, I32, I32, I32, I32, I32, I32],
        ),
    );
    m.insert(SymbolPath::from_iter(module_prefix()), blake3);
    m
}
