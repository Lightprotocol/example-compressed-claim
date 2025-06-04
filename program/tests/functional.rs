#![cfg(feature = "test-sbf")]

use light_client::indexer::GetCompressedTokenAccountsByOwnerOrDelegateOptions;
use light_compressed_account::compressed_account::PackedMerkleContext;
use light_compressed_account::constants::ACCOUNT_COMPRESSION_PROGRAM_ID;
use light_compressed_claim::instruction::{build_claim_and_decompress_instruction, ClaimAccounts};
use light_compressed_token::mint_sdk::create_create_token_pool_instruction;
use light_compressed_token_client::instructions::compress;
use light_compressed_token_client::{get_token_pool_pda, LIGHT_SYSTEM_PROGRAM_ID};
use light_program_test::accounts::test_accounts::NOOP_PROGRAM_ID;
use light_program_test::program_test::TestRpc;
use light_program_test::{
    program_test::LightProgramTest, Indexer, ProgramTestConfig, RpcConnection,
};
use solana_program_test::tokio;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::{program_pack::Pack, system_instruction};
use spl_token::{
    id, instruction,
    state::{Account, Mint},
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
    let state_tree = rpc.test_accounts.v1_state_trees[0].merkle_tree;
    let queue = rpc.test_accounts.v1_state_trees[0].nullifier_queue;

    let (mint, token_account, owner) = setup_spl_token_account(&mut rpc).await;
    setup_token_pool(&mut rpc, &mint).await;

    let payer = rpc.get_payer().insecure_clone();
    let claimant = Keypair::new();
    let unlock_slot = 1_000;
    let amount = 2;

    let (claimant_pda, bump_seed) =
        find_claimant_pda(claimant.pubkey(), mint.pubkey(), unlock_slot);

    let compress_ix = compress(
        payer.pubkey(),
        owner.pubkey(),
        token_account.pubkey(),
        mint.pubkey(),
        amount,
        claimant_pda,
        state_tree,
    )
    .unwrap();

    // we compress 2 tokens to the timelocked recipient PDA.
    rpc.create_and_send_transaction(&[compress_ix], &payer.pubkey(), &[&payer, &owner])
        .await
        .unwrap();

    // Fetch compressed token account and validity proof.
    let options = Some(GetCompressedTokenAccountsByOwnerOrDelegateOptions {
        mint: Some(mint.pubkey()),
        cursor: None,
        limit: None,
    });
    let compressed_token_account = rpc
        .get_compressed_token_accounts_by_owner(&claimant_pda, options, None)
        .await
        .unwrap()
        .value
        .items[0]
        .clone();

    let proof = rpc
        .indexer()
        .unwrap()
        .get_validity_proof(vec![compressed_token_account.account.hash], vec![], None)
        .await
        .unwrap();

    // TODO: provide helper.
    let token_pool_pda = get_token_pool_pda(&mint.pubkey());
    let accounts = ClaimAccounts {
        claimant: claimant.pubkey(),
        fee_payer: payer.pubkey(),
        associated_airdrop_pda: claimant_pda,
        ctoken_cpi_authority_pda: Pubkey::from_str_const(
            "GXtd2izAiMJPwMEjfgTRH3d7k9mjn4Jq3JrWFv9gySYy",
        ),
        light_system_program: LIGHT_SYSTEM_PROGRAM_ID,
        registered_program_pda: Pubkey::from_str_const(
            "35hkDgaAKwMCaxRz2ocSZ6NaUrtKkyNqU6c4RV3tYJRh",
        ),
        noop_program: NOOP_PROGRAM_ID,
        account_compression_authority: Pubkey::find_program_address(
            &[b"cpi_authority"],
            &LIGHT_SYSTEM_PROGRAM_ID,
        )
        .0,
        account_compression_program: ACCOUNT_COMPRESSION_PROGRAM_ID,
        ctoken_program: Pubkey::from_str_const("cTokenmWW8bLPjZEBAUgYy3zKxQZW6VKi7bqNFEVv3m"),
        token_pool_pda,
        decompress_destination: token_account.pubkey(),
        token_program: spl_token::ID,
        system_program: solana_sdk::system_program::ID,
        state_tree,
        queue,
    };

    let packed_merkle_context = PackedMerkleContext {
        merkle_tree_pubkey_index: 0,
        queue_pubkey_index: 1,
        leaf_index: compressed_token_account.account.leaf_index,
        prove_by_index: compressed_token_account.account.prove_by_index,
    };

    let instruction = build_claim_and_decompress_instruction(
        &accounts,
        proof.value.proof.clone().into(),
        proof.value.get_root_indices()[0].unwrap(),
        packed_merkle_context,
        amount,
        None,
        mint.pubkey(),
        unlock_slot,
        bump_seed,
    );
    let instruction_clone = instruction.clone();

    // SPL token account should be without the compressed tokens.
    let account_info = rpc
        .context
        .banks_client
        .get_account(token_account.pubkey())
        .await
        .unwrap();
    let account_data = Account::unpack(&account_info.unwrap().data).unwrap();
    assert_eq!(account_data.amount, 10 - amount);

    // not yet unlocked.
    rpc.warp_to_slot(999).unwrap();
    let result = rpc
        .create_and_send_transaction(&[instruction], &payer.pubkey(), &[&payer, &claimant])
        .await;
    assert_eq!(
        result.unwrap_err().to_string(),
        "TransactionError: Error processing Instruction 0: custom program error: 0x1"
    );

    // now unlocked.
    rpc.warp_to_slot(1000).unwrap();
    rpc.create_and_send_transaction(&[instruction_clone], &payer.pubkey(), &[&payer, &claimant])
        .await
        .unwrap();

    let account_info = rpc
        .context
        .banks_client
        .get_account(token_account.pubkey())
        .await
        .unwrap();
    let account_data = Account::unpack(&account_info.unwrap().data).unwrap();
    assert_eq!(account_data.amount, 10);
}

pub fn find_claimant_pda(claimant: Pubkey, mint: Pubkey, slot: u64) -> (Pubkey, u8) {
    let claimant_bytes = claimant.to_bytes();
    let mint_bytes = mint.to_bytes();
    let slot_bytes = slot.to_le_bytes();
    let seeds = &[&claimant_bytes[..32], &mint_bytes[..32], &slot_bytes[..8]];

    Pubkey::find_program_address(seeds, &light_compressed_claim::id())
}

pub async fn setup_token_pool(rpc: &mut LightProgramTest, mint: &Keypair) {
    let payer = rpc.get_payer().insecure_clone();
    let create_token_pool_ix =
        create_create_token_pool_instruction(&payer.pubkey(), &mint.pubkey(), false);
    rpc.create_and_send_transaction(&[create_token_pool_ix], &payer.pubkey(), &[&payer])
        .await
        .unwrap();
}

/// Creates a new SPL mint and a token account, and funds it with tokens.
///
/// Returns (mint_account, token_account, owner)
pub async fn setup_spl_token_account(rpc: &mut LightProgramTest) -> (Keypair, Keypair, Keypair) {
    let payer = rpc.get_payer().insecure_clone();

    let mint_account = Keypair::new();
    let owner = payer.insecure_clone();
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

    let initialize_account_ix = instruction::initialize_account(
        token_program,
        &token_account.pubkey(),
        &mint_account.pubkey(),
        &owner.pubkey(),
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

    let token_account_info = rpc
        .context
        .banks_client
        .get_account(token_account.pubkey().clone())
        .await
        .unwrap()
        .expect("could not fetch account information");
    let account_data = Account::unpack(&token_account_info.data).unwrap();

    assert_eq!(
        account_data.amount,
        mint_amount.clone(),
        "not correct amount"
    );
    assert_eq!(account_data.mint, mint_account.pubkey(), "not correct mint");
    assert_eq!(
        account_data.owner,
        payer.pubkey(),
        "not correct owner (payer)"
    );
    assert_eq!(account_data.owner, owner.pubkey(), "not correct owner");

    (mint_account, token_account, owner)
}
