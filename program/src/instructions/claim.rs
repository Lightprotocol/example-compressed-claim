use crate::{check_pda::check_claim_pda, constants::CTOKEN_PROGRAM_ID, error::ClaimError};
use light_compressed_account::{
    compressed_account::PackedMerkleContext, instruction_data::compressed_proof::CompressedProof,
};
use light_compressed_token_sdk::{
    cpi::{
        self, account_info::get_compressed_token_account_info,
        accounts::CompressedTokenDecompressCpiAccounts,
    },
    state::InputTokenDataWithContext,
};

use light_sdk::cpi_accounts::TransferCpiAccounts;
use solana_program::{
    account_info::AccountInfo, clock::Clock, entrypoint::ProgramResult, msg,
    program::invoke_signed, program_error::ProgramError, pubkey::Pubkey, sysvar::Sysvar,
};

#[allow(clippy::too_many_arguments)]
pub fn process_claim(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    proof: Option<CompressedProof>,
    root_index: u16,
    merkle_context: PackedMerkleContext,
    amount: u64,
    lamports: Option<u64>,
    mint: Pubkey,
    unlock_slot: u64,
    bump_seed: u8,
) -> ProgramResult {
    let claimant_info = &accounts[0];
    let fee_payer_info = &accounts[1];
    let associated_airdrop_pda_info = &accounts[2];
    let ctoken_cpi_authority_pda_info = &accounts[3];
    let light_system_program_info = &accounts[4];
    let registered_program_pda_info = &accounts[5];
    let noop_program_info = &accounts[6];
    let account_compression_authority_info = &accounts[7];
    let account_compression_program_info = &accounts[8];
    let ctoken_program_info = &accounts[9];
    let token_pool_pda_info = &accounts[10];
    let decompress_destination_info = &accounts[11];
    let token_program_info = &accounts[12];
    let system_program_info = &accounts[13];
    let state_tree_info = &accounts[14];
    let queue_info = &accounts[15];

    if accounts.len() != 16 {
        msg!("Expected 16 accounts, got {}", accounts.len());
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    // CHECK:
    if !claimant_info.is_signer {
        msg!("Claimant must be a signer");
        claimant_info.key.log();
        return Err(ProgramError::MissingRequiredSignature);
    }
    // CHECK:
    if !fee_payer_info.is_signer {
        msg!("Fee payer must be a signer");
        fee_payer_info.key.log();
        return Err(ProgramError::MissingRequiredSignature);
    }
    // CHECK:
    if ctoken_program_info.key != &CTOKEN_PROGRAM_ID {
        msg!("Invalid compressed token program.",);
        ctoken_program_info.key.log();
        return Err(ProgramError::InvalidArgument);
    }

    let ctoken_account =
        get_compressed_token_account_info(merkle_context, root_index, amount, lamports);

    // CHECK:
    let current_slot = Clock::get()?.slot;
    if current_slot < unlock_slot {
        msg!(
            "Tokens are still locked: current slot ({}) is less than unlock slot ({}).",
            current_slot,
            unlock_slot
        );
        return Err(ClaimError::TokensLocked.into());
    }

    let light_cpi_accounts = CompressedTokenDecompressCpiAccounts {
        fee_payer: fee_payer_info.clone(),
        authority: associated_airdrop_pda_info.clone(),
        cpi_authority_pda: ctoken_cpi_authority_pda_info.clone(),
        light_system_program: light_system_program_info.clone(),
        registered_program_pda: registered_program_pda_info.clone(),
        noop_program: noop_program_info.clone(),
        account_compression_authority: account_compression_authority_info.clone(),
        account_compression_program: account_compression_program_info.clone(),
        self_program: ctoken_program_info.clone(),
        token_pool_pda: token_pool_pda_info.clone(),
        decompress_destination: decompress_destination_info.clone(),
        token_program: token_program_info.clone(),
        system_program: system_program_info.clone(),
        state_merkle_tree: state_tree_info.clone(),
        queue: queue_info.clone(),
    };
    check_pda_and_claim_token(
        program_id,
        light_cpi_accounts,
        ctoken_account,
        &proof,
        claimant_info.clone(),
        mint,
        unlock_slot,
        bump_seed,
    )
}

#[allow(clippy::too_many_arguments)]
fn check_pda_and_claim_token(
    claim_program: &Pubkey,
    light_cpi_accounts: CompressedTokenDecompressCpiAccounts,
    compressed_token_account: InputTokenDataWithContext,
    proof: &Option<CompressedProof>,
    claimant: AccountInfo<'_>,
    mint: Pubkey,
    slot: u64,
    bump_seed: u8,
) -> ProgramResult {
    let claimant_bytes = claimant.key.to_bytes();
    let slot_bytes = slot.to_le_bytes();
    let mint_bytes = mint.to_bytes();

    let seeds = &[
        &claimant_bytes[..32],
        &mint_bytes[..32],
        &slot_bytes[..8],
        &[bump_seed],
    ];

    check_claim_pda(seeds, claim_program, light_cpi_accounts.authority.key)?;

    // TODO: replace with transfer cpi. (decompress_destination -> new owner)
    let instruction = cpi::instruction::decompress(
        &mint,
        vec![compressed_token_account],
        proof,
        &light_cpi_accounts,
        None,
    )?;

    let signers_seeds: &[&[&[u8]]] = &[&seeds[..]];
    invoke_signed(
        &instruction,
        &[
            light_cpi_accounts.fee_payer,
            light_cpi_accounts.authority,
            light_cpi_accounts.cpi_authority_pda,
            light_cpi_accounts.light_system_program,
            light_cpi_accounts.registered_program_pda,
            light_cpi_accounts.noop_program,
            light_cpi_accounts.account_compression_authority,
            light_cpi_accounts.account_compression_program,
            light_cpi_accounts.self_program,
            light_cpi_accounts.token_pool_pda,
            light_cpi_accounts.decompress_destination,
            light_cpi_accounts.token_program,
            light_cpi_accounts.system_program,
            light_cpi_accounts.state_merkle_tree,
            light_cpi_accounts.queue,
        ][..],
        signers_seeds,
    )?;
    Ok(())
}
