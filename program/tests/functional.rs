#![cfg(feature = "test-sbf")]

use {light_compressed_claim::processor, solana_program_test::*};

fn program_test() -> ProgramTest {
    ProgramTest::new(
        "light-compressed-claim",
        light_compressed_claim::id(),
        processor!(processor::process_instruction),
    )
}

#[tokio::test]
async fn test_claim_and_decompress() {
    // TODO
}
