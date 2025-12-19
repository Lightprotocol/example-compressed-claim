#![cfg(feature = "test-sbf")]

use light_client::indexer::GetCompressedTokenAccountsByOwnerOrDelegateOptions;
use light_client::rpc::Rpc;
use light_compressed_account::compressed_account::PackedMerkleContext;
use light_compressed_claim::instruction::{build_claim_and_decompress_instruction, ClaimAccounts};
use light_compressed_token::mint_sdk::create_create_token_pool_instruction;
use light_ctoken_sdk::compressed_token::{
    transfer2::{
        create_transfer2_instruction, Transfer2AccountsMetaConfig, Transfer2Config, Transfer2Inputs,
    },
    CTokenAccount2,
};
use light_ctoken_sdk::ValidityProof;
use solana_sdk::instruction::AccountMeta;
use light_program_test::program_test::{LightProgramTest, TestRpc};
use light_program_test::{Indexer, ProgramTestConfig};
use solana_program_test::tokio;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::{program_pack::Pack, system_instruction};
use spl_token::{
    id, instruction,
    state::{Account, Mint},
};

const CTOKEN_PROGRAM_ID: Pubkey = solana_sdk::pubkey!("cTokenmWW8bLPjZEBAUgYy3zKxQZW6VKi7bqNFEVv3m");
const LIGHT_SYSTEM_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("SySTEM1eSU2p4BGQfQpimFEWWSC1XDFeun3Nqzz3rT7");
const ACCOUNT_COMPRESSION_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("compr6CUsB5m2jS4Y3831ztGSTnDpnKJTKS95d64XVq");
const POOL_SEED: &[u8] = b"pool";

/// Find token pool PDA with bump
fn find_token_pool_pda_with_bump(mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[POOL_SEED, mint.as_ref()], &CTOKEN_PROGRAM_ID)
}

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
    let state_tree = rpc.test_accounts.v2_state_trees[0].merkle_tree;
    let queue = rpc.test_accounts.v2_state_trees[0].output_queue;

    let (mint, token_account, owner) = setup_spl_token_account(&mut rpc).await;
    setup_token_pool(&mut rpc, &mint).await;

    let payer = rpc.get_payer().insecure_clone();
    let claimant = Keypair::new();
    let unlock_slot = 1_000;
    let amount = 2;

    let (claimant_pda, bump_seed) =
        find_claimant_pda(claimant.pubkey(), mint.pubkey(), unlock_slot);

    // Get pool PDA with bump for compress
    let (token_pool_pda_compress, pool_bump_compress) = find_token_pool_pda_with_bump(&mint.pubkey());

    // Build packed accounts for V2 compress instruction
    // Indices: 0=tree, 1=queue, 2=mint, 3=recipient, 4=source, 5=pool, 6=spl_token, 7=authority
    let compress_packed_accounts = vec![
        AccountMeta::new(state_tree, false),                  // 0: merkle tree
        AccountMeta::new(queue, false),                       // 1: output queue
        AccountMeta::new_readonly(mint.pubkey(), false),      // 2: mint
        AccountMeta::new_readonly(claimant_pda, false),       // 3: recipient (owner of compressed tokens)
        AccountMeta::new(token_account.pubkey(), false),      // 4: source SPL token account
        AccountMeta::new(token_pool_pda_compress, false),     // 5: token pool
        AccountMeta::new_readonly(spl_token::id(), false),    // 6: SPL token program
        AccountMeta::new_readonly(owner.pubkey(), true),      // 7: authority (signer)
    ];

    // Create CTokenAccount2 with compress_spl for V2 compress
    let mut ctoken_account = CTokenAccount2::new_empty(
        3, // owner_index (claimant_pda - recipient of compressed tokens)
        2, // mint_index
    );
    ctoken_account
        .compress_spl(
            amount,
            4,                   // source_or_recipient_index (SPL token account)
            7,                   // authority index (owner signs)
            5,                   // pool_account_index
            0,                   // pool_index
            pool_bump_compress,  // bump
        )
        .unwrap();

    // Build meta config
    let meta_config = Transfer2AccountsMetaConfig::new(payer.pubkey(), compress_packed_accounts);
    let transfer_config = Transfer2Config::new();

    let inputs = Transfer2Inputs {
        token_accounts: vec![ctoken_account],
        validity_proof: ValidityProof::default(), // No proof needed for pure compress
        transfer_config,
        meta_config,
        in_lamports: None,
        out_lamports: None,
        output_queue: 1, // queue is at index 1 in packed_accounts
    };

    let compress_ix = create_transfer2_instruction(inputs).unwrap();

    // Compress 2 tokens to the timelocked recipient PDA
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

    // Get pool PDA with bump
    let (token_pool_pda, pool_bump) = find_token_pool_pda_with_bump(&mint.pubkey());

    // Build packed accounts in the correct order
    // Indices: 0=tree, 1=queue, 2=mint, 3=owner, 4=destination, 5=pool, 6=spl_token
    let packed_accounts = vec![
        (state_tree, false, true),              // 0: merkle tree
        (queue, false, true),                   // 1: nullifier queue
        (mint.pubkey(), false, false),          // 2: mint
        (claimant_pda, false, false),           // 3: owner (airdrop PDA)
        (token_account.pubkey(), false, true),  // 4: destination SPL account
        (token_pool_pda, false, true),          // 5: token pool
        (spl_token::id(), false, false),        // 6: SPL token program
    ];

    let accounts = ClaimAccounts {
        claimant: claimant.pubkey(),
        fee_payer: payer.pubkey(),
        associated_airdrop_pda: claimant_pda,
        ctoken_program: CTOKEN_PROGRAM_ID,
        ctoken_cpi_authority_pda: Pubkey::from_str_const(
            "GXtd2izAiMJPwMEjfgTRH3d7k9mjn4Jq3JrWFv9gySYy",
        ),
        light_system_program: LIGHT_SYSTEM_PROGRAM_ID,
        registered_program_pda: Pubkey::from_str_const(
            "35hkDgaAKwMCaxRz2ocSZ6NaUrtKkyNqU6c4RV3tYJRh",
        ),
        account_compression_authority: Pubkey::find_program_address(
            &[b"cpi_authority"],
            &LIGHT_SYSTEM_PROGRAM_ID,
        )
        .0,
        account_compression_program: ACCOUNT_COMPRESSION_PROGRAM_ID,
        system_program: solana_sdk::system_program::ID,
        packed_accounts,
    };

    // For V2 batched trees, accounts in the output queue should have prove_by_index = true
    // The local test indexer may not set this correctly, so we force it based on
    // the account being newly compressed (leaf_index 0 in output queue)
    let prove_by_index = true; // Account is in output queue, proven by index not by ZK proof

    let packed_merkle_context = PackedMerkleContext {
        merkle_tree_pubkey_index: 0,
        queue_pubkey_index: 1,
        leaf_index: compressed_token_account.account.leaf_index,
        prove_by_index,
    };

    // Build the claim instruction with new API
    let validity_proof: ValidityProof = proof.value.proof.clone().into();
    // For V2 batched trees, root_index may be None when prove_by_index is true
    let root_index = proof.value.get_root_indices()[0].unwrap_or(0);

    let instruction = build_claim_and_decompress_instruction(
        &accounts,
        validity_proof.clone(),
        packed_merkle_context,
        root_index,
        amount,
        mint.pubkey(),
        unlock_slot,
        bump_seed,
        3,         // owner_index
        2,         // mint_index
        4,         // destination_index
        5,         // pool_account_index
        0,         // pool_index
        pool_bump,
    );
    let instruction_clone = instruction.clone();

    // SPL token account should be without the compressed tokens.
    let account_info = rpc.get_account(token_account.pubkey()).await.unwrap();
    let account_data = Account::unpack(&account_info.unwrap().data).unwrap();
    assert_eq!(account_data.amount, 10 - amount);

    // Not yet unlocked - should fail.
    rpc.warp_to_slot(999).unwrap();
    let result = rpc
        .create_and_send_transaction(&[instruction], &payer.pubkey(), &[&payer, &claimant])
        .await;
    assert_eq!(
        result.unwrap_err().to_string(),
        "TransactionError: Error processing Instruction 0: custom program error: 0x1"
    );

    // Now unlocked - should succeed.
    rpc.warp_to_slot(1000).unwrap();
    rpc.create_and_send_transaction(&[instruction_clone], &payer.pubkey(), &[&payer, &claimant])
        .await
        .unwrap();

    let account_info = rpc.get_account(token_account.pubkey()).await.unwrap();
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

    // Get rent using the Rpc trait method
    let mint_rent = rpc
        .get_minimum_balance_for_rent_exemption(Mint::LEN)
        .await
        .unwrap();

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
    let account_rent = rpc
        .get_minimum_balance_for_rent_exemption(Account::LEN)
        .await
        .unwrap();
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
        token_program,
        &mint_account.pubkey(),
        &token_account.pubkey(),
        &owner.pubkey(),
        &[],
        mint_amount,
    )
    .unwrap();

    rpc.create_and_send_transaction(&[mint_to_ix], &payer.pubkey(), &[&payer, &owner])
        .await
        .unwrap();

    let token_account_info = rpc
        .get_account(token_account.pubkey())
        .await
        .unwrap()
        .expect("could not fetch account information");
    let account_data = Account::unpack(&token_account_info.data).unwrap();

    assert_eq!(account_data.amount, mint_amount, "not correct amount");
    assert_eq!(account_data.mint, mint_account.pubkey(), "not correct mint");
    assert_eq!(
        account_data.owner,
        payer.pubkey(),
        "not correct owner (payer)"
    );
    assert_eq!(account_data.owner, owner.pubkey(), "not correct owner");

    (mint_account, token_account, owner)
}
