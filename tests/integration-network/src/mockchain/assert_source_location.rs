//! Test to verify Rust assert! macro source locations are preserved through miden-client execution.
//!
//! This test verifies that when a Rust program containing an assert! macro fails,
//! the error message includes the original Rust source file location (e.g., lib.rs:26:13).

use miden_client::{
    Felt, Word,
    account::component::BasicWallet,
    testing::{AccountState, Auth, MockChain},
};
use miden_core::FieldElement;
use miden_protocol::{
    account::{AccountBuilder, AccountStorageMode, AccountType},
    transaction::TransactionScript,
};

use super::helpers::{block_on, compile_rust_package};

/// Tests that assertion failures from Rust code show the original Rust source location
/// when executed through miden-client's MockChain.
///
/// This test:
/// 1. Compiles the assert-debug-test Rust app (which has an assert!(x > 100) at lib.rs:26)
/// 2. Creates a transaction script from the compiled program
/// 3. Executes it via MockChain with x=50 (which should fail the assertion)
/// 4. Verifies the error message contains the Rust source location "lib.rs" and line 26
#[test]
pub fn test_rust_assert_source_location_via_miden_client() {
    // Compile the assert-debug-test app in dev mode to preserve debug info
    let assert_package = compile_rust_package("../rust-apps-wasm/rust-sdk/assert-debug-test", false);

    // Get the compiled program
    let program = assert_package.unwrap_program();

    // Create a transaction script from the program
    // Note: MastForest from miden-mast-package (v0.19) may differ from miden-protocol (v0.20)
    // This might need adaptation if types are incompatible
    let tx_script = TransactionScript::from_parts(
        program.mast_forest().clone(),
        program.entrypoint(),
    );

    // Create a basic account to execute the transaction
    let seed = [42_u8; 32];
    let account_builder = AccountBuilder::new(seed)
        .account_type(AccountType::RegularAccountUpdatableCode)
        .storage_mode(AccountStorageMode::Public)
        .with_component(BasicWallet);

    let mut builder = MockChain::builder();
    let account = builder
        .add_account_from_builder(Auth::BasicAuth, account_builder, AccountState::Exists)
        .expect("failed to add account to mock chain builder");

    let mut chain = builder.build().expect("failed to build mock chain");
    chain.prove_next_block().unwrap();
    chain.prove_next_block().unwrap();

    // Build a transaction context with the failing script
    // The assert-debug-test expects one input: x. When x <= 100, it panics.
    // We pass x=50 via tx_script_args to trigger the assertion failure.
    let tx_context_builder = chain
        .build_tx_context(account.id(), &[], &[])
        .unwrap()
        .tx_script(tx_script)
        .tx_script_args(Word::from([
            Felt::new(50), // x = 50, will trigger assert!(x > 100)
            Felt::ZERO,
            Felt::ZERO,
            Felt::ZERO,
        ]));

    let tx_context = tx_context_builder.build().unwrap();

    // Execute the transaction - it should fail with an assertion error
    let result = block_on(tx_context.execute());

    // Verify execution failed
    let err = result.expect_err("Expected transaction to fail due to assertion (x=50 <= 100)");

    // Render the error using Display (TransactionExecutorError doesn't implement Diagnostic)
    let rendered = format!("{err}");

    // Print the rendered error for debugging
    eprintln!("\n=== RENDERED ASSERTION ERROR ===\n{rendered}\n=== END ===\n");

    // Verify the error contains Rust source location information
    // The assert! is at lib.rs:26 in the assert-debug-test app
    let has_file = rendered.contains("lib.rs");
    // Check for line 26 in various formats
    let has_line = rendered.contains(":26:") || rendered.contains("line 26");

    if !has_file || !has_line {
        panic!(
            "Expected error to contain Rust source location 'lib.rs' and ':26:' (or 'line 26').\n\
             Found 'lib.rs': {has_file}\n\
             Found line info: {has_line}\n\
             Full rendered error:\n{rendered}"
        );
    }

    eprintln!("SUCCESS: Assertion error includes Rust source location!");
}
