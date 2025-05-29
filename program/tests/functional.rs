#![cfg(feature = "test-sbf")]

use borsh::BorshSerialize;
use light_compressed_account::{
    address::derive_address, compressed_account::CompressedAccountWithMerkleContext,
    hashv_to_bn254_field_size_be,
};
use light_program_test::{
    program_test::LightProgramTest, AddressWithTree, Indexer, ProgramTestConfig, RpcConnection,
    RpcError,
};
// use light_sdk::instruction::{
//     account_meta::CompressedAccountMeta,
//     accounts::SystemAccountMetaConfig,
//     merkle_context::{pack_address_merkle_context, AddressMerkleContext},
//     pack_accounts::PackedAccounts,
// };

use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use {
    light_compressed_account::compressed_account::PackedMerkleContext,
    light_compressed_claim::{
        instruction::{build_claim_and_decompress_instruction, ClaimAccounts},
        processor,
    },
    solana_program_test::*,
};

fn program_test() -> ProgramTest {
    ProgramTest::new(
        "light-compressed-claim",
        light_compressed_claim::id(),
        processor!(processor::process_instruction),
    )
}

#[tokio::test]
async fn test_claim_and_decompress() {
    let config = ProgramTestConfig::new_v2(
        true,
        Some(vec![(
            "light_compressed_claim",
            light_compressed_claim::id(),
        )]),
    );
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();

    // Setup test accounts
    let claimant = payer.pubkey();
    let fee_payer = payer.pubkey();
    let mint = Pubkey::new_unique();

    // you need the validity proof lol! 
    let compressed_pda = rpc
    .indexer()
    .unwrap()
    .get_compressed_accounts_by_owner_v2(&light_compressed_claim::id())
    .await
    .unwrap()[0]
    .clone();

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
