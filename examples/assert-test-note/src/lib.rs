// Test note script that intentionally fails an assertion
// Used for testing source location information in error messages
#![no_std]

use miden::*;
#[note_script]
fn run(_arg: Word) {
    let value = Felt::from_u32(42);
    let expected = Felt::from_u32(100);

    assert_eq(value, expected);
}
