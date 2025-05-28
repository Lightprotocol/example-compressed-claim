#![cfg(feature = "test-sbf")]

use {
    light_compressed_account::compressed_account::PackedMerkleContext,
    light_compressed_claim::{
        instruction::{build_claim_and_decompress_instruction, ClaimAccounts},
        processor,
    },
    solana_program_test::*,
    solana_sdk::{pubkey::Pubkey, signature::Signer},
};

// TODO: 1) add instruction builder (serialize, deserialize), 2) setup with test validator
fn program_test() -> ProgramTest {
    ProgramTest::new(
        "light-compressed-claim",
        light_compressed_claim::id(),
        processor!(processor::process_instruction),
    )
}

#[tokio::test]
async fn test_claim_and_decompress() {
    let context = program_test().start_with_context().await;

    // Setup test accounts
    let claimant = context.payer.pubkey();
    let fee_payer = context.payer.pubkey();
    let mint = Pubkey::new_unique();

    let accounts = ClaimAccounts {
        claimant: Pubkey::new_unique(),
        fee_payer: Pubkey::new_unique(),
        associated_airdrop_pda: Pubkey::new_unique(),
        ctoken_cpi_authority_pda: Pubkey::new_unique(),
        light_system_program: Pubkey::new_unique(),
        registered_program_pda: Pubkey::new_unique(),
        noop_program: Pubkey::new_unique(),
        account_compression_authority: Pubkey::new_unique(),
        account_compression_program: Pubkey::new_unique(),
        ctoken_program: Pubkey::new_unique(),
        token_pool_pda: Pubkey::new_unique(),
        decompress_destination: Pubkey::new_unique(),
        token_program: Pubkey::new_unique(),
        system_program: Pubkey::new_unique(),
        state_tree: Pubkey::new_unique(),
        queue: Pubkey::new_unique(),
    };

    let mint = Pubkey::new_unique();
    let root_index = 42;
    let merkle_context = PackedMerkleContext::default();
    let amount = 1000;
    let unlock_slot = 12345;
    let bump_seed = 1;

    let instruction = build_claim_and_decompress_instruction(
        &accounts,
        None,
        root_index,
        merkle_context,
        amount,
        mint,
        unlock_slot,
        bump_seed,
    );

    println!("instruction: {:?}", instruction);
    // TODO: Create necessary accounts and submit transaction
    // let transaction = Transaction::new_signed_with_payer(
    //     &[instruction],
    //     Some(&fee_payer),
    //     &[&context.payer],
    //     context.last_blockhash,
    // );
    // context.banks_client.process_transaction(transaction).await.unwrap();
}
