#![cfg(feature = "test-sbf")]

use borsh::BorshSerialize;
use light_compressed_token_client::instructions::compress;

use light_program_test::{
    program_test::LightProgramTest, Indexer, ProgramTestConfig, RpcConnection,
};
use solana_sdk::pubkey::Pubkey;


use solana_sdk::{
    instruction::Instruction,
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

#[tokio::test]
async fn test_claim_and_decompress() {
    let config = ProgramTestConfig::new(
        true,
        Some(vec![(
            "light_compressed_claim",
            light_compressed_claim::id(),
        )]),
    );
    let mut rpc = LightProgramTest::new(config).await.unwrap();
    let payer = rpc.get_payer().insecure_clone();
    let claimant = payer.pubkey();
    let state_tree = rpc.get_state_merkle_tree().merkle_tree.clone();

    let (mint, token_account, owner) = setup_spl_token_account(&mut rpc).await;

    let (claimant_pda, bump_seed) = find_claimant_pda(claimant, mint.pubkey(), 1);
    let compress_ix = compress(
        payer.pubkey(),
        owner.pubkey(),
        token_account.pubkey(),
        mint.pubkey(),
        2,
        claimant_pda,
        state_tree,
    )
    .unwrap();

    // we compress the funds to the timelocked recipient PDA.
    rpc.create_and_send_transaction(&[compress_ix], &payer.pubkey(), &[&payer, &owner])
        .await
        .unwrap();

    return;
    // you need the validity proof lol
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

pub fn find_claimant_pda(claimant: Pubkey, mint: Pubkey, slot: u64) -> (Pubkey, u8) {
    let claimant_bytes = claimant.to_bytes();
    let mint_bytes = mint.to_bytes();
    let slot_bytes = slot.to_le_bytes();
    let seeds = &[&claimant_bytes[..32], &mint_bytes[..32], &slot_bytes[..8]];

    Pubkey::find_program_address(seeds, &light_compressed_claim::id())
}

use solana_program_test::{tokio, ProgramTest};
use solana_sdk::{program_pack::Pack, system_instruction};
use spl_token::{
    id, instruction,
    state::{Account, Mint},
};

// Create a new SPL mint, with a new mint account, token account for given owner, and funds it with tokens.
// returns (mint_account, token_account, owner)
pub async fn setup_spl_token_account(rpc: &mut LightProgramTest) -> (Keypair, Keypair, Keypair) {
    let payer = rpc.get_payer().insecure_clone();

    let mint_account = Keypair::new();
    let owner = Keypair::new();
    let token_program = &id();
    let rent = rpc.context.banks_client.get_rent().await.unwrap();
    let mint_rent = rent.minimum_balance(Mint::LEN);

    let token_mint_a_account_ix = solana_program::system_instruction::create_account(
        &payer.pubkey(),
        &mint_account.pubkey(),
        mint_rent,
        Mint::LEN as u64,
        token_program,
    );

    let token_mint_a_ix = instruction::initialize_mint(
        token_program,
        &mint_account.pubkey(),
        &owner.pubkey(),
        None,
        9,
    )
    .unwrap();

    // create mint transaction
    rpc.create_and_send_transaction(
        &[token_mint_a_account_ix, token_mint_a_ix],
        &payer.pubkey(),
        &[&payer, &mint_account],
    )
    .await
    .unwrap();

    // Create account that can hold the newly minted tokens
    let account_rent = rent.minimum_balance(Account::LEN);
    let token_account = Keypair::new();
    let new_token_account_ix = system_instruction::create_account(
        &payer.pubkey(),
        &token_account.pubkey(),
        account_rent,
        Account::LEN as u64,
        token_program,
    );

    let my_account = Keypair::new();
    let initialize_account_ix = instruction::initialize_account(
        token_program,
        &token_account.pubkey(),
        &mint_account.pubkey(),
        &my_account.pubkey(),
    )
    .unwrap();

    rpc.create_and_send_transaction(
        &[new_token_account_ix, initialize_account_ix],
        &payer.pubkey(),
        &[&payer, &token_account],
    )
    .await
    .unwrap();

    // Mint tokens into newly created account
    let mint_amount: u64 = 10;
    let mint_to_ix = instruction::mint_to(
        &token_program,
        &mint_account.pubkey(),
        &token_account.pubkey(),
        &owner.pubkey(),
        &[],
        mint_amount.clone(),
    )
    .unwrap();

    rpc.create_and_send_transaction(&[mint_to_ix], &payer.pubkey(), &[&payer, &owner])
        .await
        .unwrap();

    // Inspect account
    let token_account_info = rpc
        .context
        .banks_client
        .get_account(token_account.pubkey().clone())
        .await
        .unwrap()
        .expect("could not fetch account information");
    let account_data = Account::unpack(&token_account_info.data).unwrap();
    println!("account data: {:?}", account_data);
    assert_eq!(
        account_data.amount,
        mint_amount.clone(),
        "not correct amount"
    );

    (mint_account, token_account, owner)
}
