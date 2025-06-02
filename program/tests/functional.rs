#![cfg(feature = "test-sbf")]

use light_compressed_account::compressed_account::{pack_merkle_context, PackedMerkleContext};

use light_compressed_account::constants::ACCOUNT_COMPRESSION_PROGRAM_ID;
use light_compressed_claim::instruction::{build_claim_and_decompress_instruction, ClaimAccounts};
use light_compressed_token_client::instructions::compress;
use light_compressed_token_client::{
    get_cpi_authority_pda, get_token_pool_pda, LIGHT_SYSTEM_PROGRAM_ID,
};
use light_program_test::accounts::test_accounts::NOOP_PROGRAM_ID;
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
    let payer = rpc.get_payer().insecure_clone();
    let claimant = payer.pubkey();
    let state_tree = rpc.get_state_merkle_tree().merkle_tree.clone();
    let queue = rpc.get_state_merkle_tree().nullifier_queue.clone();

    let (mint, token_account, owner) = setup_spl_token_account(&mut rpc).await;

    let unlock_slot = 1;
    let amount = 2;

    let (claimant_pda, bump_seed) = find_claimant_pda(claimant, mint.pubkey(), unlock_slot);
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

    // we compress the funds to the timelocked recipient PDA.
    rpc.create_and_send_transaction(&[compress_ix], &payer.pubkey(), &[&payer, &owner])
        .await
        .unwrap();

    let compressed_token_accounts = rpc
        .get_compressed_token_accounts_by_owner(&claimant_pda, Some(mint.pubkey()))
        .await
        .unwrap();

    assert_eq!(compressed_token_accounts.len(), 1);

    let compressed_token_account = compressed_token_accounts[0].clone();

    let proof = rpc
        .indexer()
        .unwrap()
        .get_validity_proof(
            vec![compressed_token_account.compressed_account.hash().unwrap()],
            vec![],
        )
        .await
        .unwrap();

    let token_pool_pda = get_token_pool_pda(&mint.pubkey());

    // TODO: provide helper.
    let accounts = ClaimAccounts {
        claimant,
        fee_payer: payer.pubkey(),
        associated_airdrop_pda: claimant_pda,
        ctoken_cpi_authority_pda: get_cpi_authority_pda().0,
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

    let root_index = proof.root_indices[0];
    let merkle_context = compressed_token_account.compressed_account.merkle_context;

    // let mut remaining_accounts = RemainingAccount::default();
    // let packed_merkle_context = PackedMerkleContext(merkle_context);

    // TODO: ix_data needs to accept better index. just one is best i think!
    let instruction = build_claim_and_decompress_instruction(
        &accounts,
        None,
        root_index,
        PackedMerkleContext::default(),
        amount,
        mint.pubkey(),
        unlock_slot,
        bump_seed,
    );

    println!("instruction: {:?}", instruction);

    rpc.create_and_send_transaction(&[instruction], &payer.pubkey(), &[&payer, &owner])
        .await
        .unwrap();

    let account_info = rpc
        .context
        .banks_client
        .get_account(token_account.pubkey())
        .await
        .unwrap();
    let account_data = Account::unpack(&account_info.unwrap().data).unwrap();
    println!("token account data: {:?}", account_data);
}

pub fn find_claimant_pda(claimant: Pubkey, mint: Pubkey, slot: u64) -> (Pubkey, u8) {
    let claimant_bytes = claimant.to_bytes();
    let mint_bytes = mint.to_bytes();
    let slot_bytes = slot.to_le_bytes();
    let seeds = &[&claimant_bytes[..32], &mint_bytes[..32], &slot_bytes[..8]];

    Pubkey::find_program_address(seeds, &light_compressed_claim::id())
}

// Create a new SPL mint, with a new mint account, token account for given
// owner, and funds it with tokens. returns (mint_account, token_account, owner)
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
