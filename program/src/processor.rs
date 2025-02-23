use crate::error::ClaimError;
use borsh::BorshDeserialize;
use light_compressed_account::{
    compressed_account::PackedMerkleContext, instruction_data::compressed_proof::CompressedProof,
};
use light_compressed_token_sdk::{
    cpi, cpi::accounts::CompressedTokenDecompressCpiAccounts, state::InputTokenDataWithContext,
};
use solana_program::program::invoke_signed;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::Sysvar,
};

/// Processes the "claim_and_decompress" instruction
///
/// Expected Accounts:
/// 0. `[signer]` The claimant's account.
/// 1. `[signer]` The fee payer account.
/// 2. `[]` The mint account.
/// 3. `[]` The associated airdrop pda.
/// 4. `[]` The ctoken cpi authority pda.
/// 5. `[]` The light system program pda.
/// 6. `[]` The registered program pda              (read-only).
/// 7. `[]` The noop program pda                    (read-only).
/// 8. `[]` The account compression authority pda   (read-only).
/// 9. `[]` The account compression program pda     (read-only).
/// 10. `[]` The self program pda                   (read-only).
/// 11. `[]` The token pool pda                     
/// 12. `[]` The decompress destination account   
/// 13. `[]` The token program                      (read-only).
/// 14. `[]` The system program                     (read-only).
///
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let claimant_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let fee_payer_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let mint_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    // owner of locked tokens, tied to mint, claimant, and unlock slot:
    let associated_airdrop_pda_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    // Default protocol accounts required for cpi:
    let ctoken_cpi_authority_pda_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let light_system_program_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let registered_program_pda_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let noop_program_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let account_compression_authority_info: &AccountInfo<'_> =
        next_account_info(account_info_iter)?;
    let account_compression_program_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let self_program_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    // one of the mint's protocol-owned token pools:
    let token_pool_pda_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    // can be any spl token account:
    let decompress_destination_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let token_program_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let system_program_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;

    // CHECK:
    if !claimant_info.is_signer {
        msg!("Claimant must be a signer.");
        return Err(ProgramError::MissingRequiredSignature);
    }
    if !fee_payer_info.is_signer {
        msg!("Fee payer must be a signer.");
        return Err(ProgramError::MissingRequiredSignature);
    }

    // CHECK:
    if associated_airdrop_pda_info.lamports() > 0 || associated_airdrop_pda_info.data_len() > 0 {
        msg!("Airdrop PDA must be uninitialized.");
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    let light_context_accounts = CompressedTokenDecompressCpiAccounts {
        fee_payer: fee_payer_info.clone(),
        authority: associated_airdrop_pda_info.clone(),
        cpi_authority_pda: ctoken_cpi_authority_pda_info.clone(),
        light_system_program: light_system_program_info.clone(),
        registered_program_pda: registered_program_pda_info.clone(),
        noop_program: noop_program_info.clone(),
        account_compression_authority: account_compression_authority_info.clone(),
        account_compression_program: account_compression_program_info.clone(),
        self_program: self_program_info.clone(),
        token_pool_pda: token_pool_pda_info.clone(),
        decompress_destination: decompress_destination_info.clone(),
        token_program: token_program_info.clone(),
        system_program: system_program_info.clone(),
    };

    let data = InstructionData::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;

    let proof = &data.proof;
    let root_index = data.root_index;
    let merkle_context = data.merkle_context;
    let amount = data.amount;
    let unlock_slot = data.unlock_slot;
    let bump_seed = data.bump_seed;

    // get the compressed token account
    let compressed_token_account = InputTokenDataWithContext {
        amount,
        delegate_index: None,
        merkle_context,
        root_index,
        lamports: None,
        tlv: None,
    };

    // Get current slot
    let current_slot = Clock::get()?.slot;

    // CHECK:
    if current_slot < unlock_slot {
        msg!(
            "Tokens are still locked: current slot ({}) is less than unlock slot ({}).",
            current_slot,
            unlock_slot
        );
        return Err(ClaimError::TokensLocked.into());
    }

    check_pda_and_decompress_token(
        program_id,
        light_context_accounts,
        vec![compressed_token_account],
        proof,
        claimant_info.clone(),
        mint_info.clone(),
        unlock_slot,
        bump_seed,
    )
}

/// CHECK:
/// 1. PDA derived from claimant.
/// 2. PDA derived from mint.
/// 3. PDA owned by claim_program.
/// 4. PDA derived from slot.
///
/// If check passes, decompresses the compressed token account to destination account.
fn check_pda_and_decompress_token<'a>(
    claim_program: &Pubkey,
    light_cpi_accounts: CompressedTokenDecompressCpiAccounts,
    compressed_token_accounts: Vec<InputTokenDataWithContext>,
    proof: &CompressedProof,
    claimant: AccountInfo<'a>,
    mint: AccountInfo<'a>,
    slot: u64,
    bump_seed: u8,
) -> ProgramResult {
    let claimant_bytes = claimant.key.to_bytes();
    let slot_bytes = slot.to_le_bytes();
    let mint_bytes = mint.key.to_bytes();

    let seeds = &[
        &claimant_bytes[..32],
        &mint_bytes[..32],
        &slot_bytes[..8],
        &[bump_seed],
    ];

    check_claim_pda(seeds, claim_program, light_cpi_accounts.authority.key)?;

    let instruction = cpi::instruction::decompress_token_instruction(
        &mint.key,
        compressed_token_accounts,
        proof,
        &light_cpi_accounts,
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
        ][..],
        signers_seeds,
    )?;
    Ok(())
}

/// CHECK: associated_airdrop_pda must be derived from claimant, mint, and slot.
fn check_claim_pda(
    seeds: &[&[u8]],
    claim_program: &Pubkey,
    airdrop_account: &Pubkey,
) -> Result<(), ProgramError> {
    let derived_pda =
        Pubkey::create_program_address(seeds, claim_program).expect("Invalid PDA seeds.");

    if derived_pda != *airdrop_account {
        msg!(
            "Invalid airdrop PDA provided. Expected: {}. Found: {}.",
            derived_pda,
            airdrop_account
        );
        return Err(ClaimError::InvalidPDA.into());
    }

    Ok(())
}

#[derive(BorshDeserialize)]
struct InstructionData {
    // validity proof info
    proof: CompressedProof,
    root_index: u16,
    // compressed token account info
    merkle_context: PackedMerkleContext,
    amount: u64,
    // inputs
    unlock_slot: u64,
    bump_seed: u8,
}
