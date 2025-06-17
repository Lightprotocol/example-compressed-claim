use crate::instructions::sdk::{self, decompress, get_decompress_cpi_accounts, ToAccountInfos};
use crate::{
    check_pda::check_claim_pda, constants::CTOKEN_PROGRAM_ID, error::ClaimError,
    instructions::sdk::CompressedTokenProgramGetter,
};
use light_compressed_account::{
    compressed_account::PackedMerkleContext, instruction_data::compressed_proof::CompressedProof,
};
use light_compressed_token_sdk::{
    cpi::account_info::get_compressed_token_account_info, state::InputTokenDataWithContext,
};
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

    let (light_cpi_accounts, _remaining) =
        get_decompress_cpi_accounts(accounts, 1, &[merkle_context])?;

    // CHECK:
    if !claimant_info.is_signer {
        msg!("Claimant must be a signer");
        claimant_info.key.log();
        return Err(ProgramError::MissingRequiredSignature);
    }
    // CHECK:
    if !light_cpi_accounts.fee_payer.is_signer {
        msg!("Fee payer must be a signer");
        light_cpi_accounts.fee_payer.key.log();
        return Err(ProgramError::MissingRequiredSignature);
    }
    // CHECK:
    if light_cpi_accounts.compressed_token_program().key != &CTOKEN_PROGRAM_ID {
        msg!("Invalid compressed token program.",);
        light_cpi_accounts.compressed_token_program().key.log();
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
    light_cpi_accounts: crate::instructions::sdk::CompressedTokenDecompressCpiAccounts,
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

    let instruction = sdk::decompress(
        &mint,        // mint
        claimant.key, // owner
        vec![compressed_token_account],
        proof,
        &light_cpi_accounts,
        None,
    )?;

    let signers_seeds: &[&[&[u8]]] = &[&seeds[..]];
    let account_infos = light_cpi_accounts.to_account_infos();
    invoke_signed(&instruction, &account_infos[..], signers_seeds)?;
    Ok(())
}
