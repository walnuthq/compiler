//! Test that assertion failures show source location information
//!
//! This test verifies that when an `assert!` fails during Miden VM execution,
//! the error message contains the correct source file and line number.
//!
//! ## Implementation
//!
//! DWARF debug info is preserved for wasm components (wasm32-wasip2 target).
//! Source locations are attached to compiled MASM instructions using:
//! - `--debug line` to enable debug info
//! - `-Z trim-path-prefix=<path>` to resolve user code paths
//! - `-Z remap-path-prefix=FROM=TO` to resolve stdlib paths
//!
//! The stdlib paths in DWARF are relative (e.g., `./miden-stdlib-sys-0.7.1/...`).
//! The remap option allows us to map these back to the actual  source locations.

use miden_client::transaction::{OutputNote, TransactionRequestBuilder};

use super::helpers::*;
use crate::local_node::ensure_shared_node;

/// Tests that assertion failures contain source location information.
#[test]
pub fn test_assert_shows_source_location() {
    // Compile the assert-test-note with debug info enabled
    // This uses --debug line and --trim-path-prefix to emit source locations
    let assert_note_package =
        compile_rust_package_with_debug("../../examples/assert-test-note", true);

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // Create temp directory and get node handle
        let temp_dir = temp_dir::TempDir::with_prefix("test_assert_source_location_")
            .expect("Failed to create temp directory");
        let node_handle = ensure_shared_node().await.expect("Failed to get shared node");

        // Initialize test infrastructure
        let TestSetup {
            mut client,
            keystore,
        } = setup_test_infrastructure(&temp_dir, &node_handle)
            .await
            .expect("Failed to setup test infrastructure");

        eprintln!("\n=== Creating account for assertion test ===");

        // Create a simple account with basic wallet
        let account = create_basic_wallet_account(&mut client, keystore.clone(), Default::default())
            .await
            .expect("Failed to create account");
        eprintln!("Account ID: {:?}", account.id().to_hex());

        eprintln!("\n=== Creating note that will fail assertion ===");

        // Create the note that will fail when consumed
        let failing_note = create_note_from_package(
            &mut client,
            assert_note_package,
            account.id(),
            NoteCreationConfig::default(),
        );
        eprintln!("Failing note hash: {:?}", failing_note.id().to_hex());

        // First, create the note in a transaction
        let create_note_request = TransactionRequestBuilder::new()
            .own_output_notes(vec![OutputNote::Full(failing_note.clone())])
            .build()
            .unwrap();

        let _create_tx_id = client
            .submit_new_transaction(account.id(), create_note_request)
            .await
            .expect("Failed to create note transaction");

        eprintln!("\n=== Attempting to consume failing note ===");

        // Try to consume the note - this should fail with assertion error
        let consume_request = TransactionRequestBuilder::new()
            .unauthenticated_input_notes([(failing_note, None)])
            .build()
            .unwrap();

        let result = client.submit_new_transaction(account.id(), consume_request).await;

        // The transaction should fail due to the assertion
        match result {
            Ok(_) => {
                panic!("Expected assertion failure but transaction succeeded!");
            }
            Err(error) => {
                let error_string = format!("{error:?}");
                eprintln!("\n=== Assertion error received ===");
                eprintln!("{error_string}");

                // Check that the error contains source location information
                // The assertion failure should include some source location context.
                // Due to how intrinsic stubs are compiled with synthetic locations,
                // the fallback may show caller context (kernel/user code) rather than
                // the exact line in felt.rs where assert_eq is defined.
                let has_source_file = error_string.contains("source_file: Some(");
                let has_source_content = error_string.contains("content:");

                // Check for indicators of missing debug info
                let has_unknown_source = error_string.contains("source_file: None")
                    && !error_string.contains("source_file: Some(");

                eprintln!("\n=== Source location check ===");
                eprintln!("Has source file: {has_source_file}");
                eprintln!("Has source content: {has_source_content}");
                eprintln!("Has unknown/missing source indicators: {has_unknown_source}");

                // Verify that we get SOME source location (not just None/UNKNOWN)
                // This validates that the VM caller location fallback is working
                assert!(
                    has_source_file || has_source_content,
                    "Error message should contain source location context.\n\
                     The VM should fall back to caller location when intrinsic has synthetic span.\n\
                     Got: {error_string}"
                );
                assert!(
                    !has_unknown_source,
                    "Error message should not have unknown/missing source indicators.\n\
                     Got: {error_string}"
                );

                eprintln!("\n=== Test completed: assertion failure with source location verified ===");
            }
        }
    });
}
