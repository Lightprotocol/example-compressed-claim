use crate::error::ClaimError;
use borsh::ser::BorshSerialize;
use light_compressed_account_anchorless::{
    compressed_account::PackedMerkleContext,
    instruction_data::compressed_proof::CompressedProof,
    token::{CompressedTokenInstructionDataTransfer, InputTokenDataWithContext},
};
use solana_program::instruction::{AccountMeta, Instruction};
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
/// 1. `[]` The mint account                        (read-only).
/// 2. `[signer]` The fee payer account             
/// 3. `[]` The associated airdrop pda              (read-only).
/// 4. `[]` The ctoken cpi authority pda            (read-only).
/// 5. `[]` The light system program pda            (read-only).
/// 6. `[]` The registered program pda              (read-only).
/// 7. `[]` The noop program pda                    (read-only).
/// 8. `[]` The account compression authority pda   (read-only).
/// 9. `[]` The account compression program pda     (read-only).
/// 10. `[]` The self program pda                   (read-only).
/// 11. `[]` The token pool pda                     (read-only).
/// 12. `[]` The decompress destination account     (writeable).
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
    let mint_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let fee_payer_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let associated_airdrop_pda_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let ctoken_cpi_authority_pda_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let light_system_program_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let registered_program_pda_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let noop_program_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let account_compression_authority_info: &AccountInfo<'_> =
        next_account_info(account_info_iter)?;
    let account_compression_program_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let self_program_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
    let token_pool_pda_info: &AccountInfo<'_> = next_account_info(account_info_iter)?;
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

    // TODO: ixdata
    let proof = &CompressedProof::default();
    let merkle_context = PackedMerkleContext::default();
    let amount = 0;
    let root_index = 0;
    let unlock_slot = u64::from_le_bytes(instruction_data.try_into().unwrap());
    let bump_seed = 0;

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

    check_claim_pda(
        program_id,
        claimant_info.key,
        mint_info.key,
        &bump_seed,
        &unlock_slot,
        associated_airdrop_pda_info.key,
    )?;

    decompress_token(
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

// caller program.
fn decompress_token<'a>(
    claim_program: &Pubkey,
    light_cpi_accounts: CompressedTokenDecompressCpiAccounts,
    compressed_token_accounts: Vec<InputTokenDataWithContext>,
    proof: &CompressedProof,
    claimant: AccountInfo<'a>,
    mint: AccountInfo<'a>,
    slot: u64,
    bump_seed: u8,
) -> ProgramResult {
    let signer_bytes = claimant.key.to_bytes();
    let claim_bytes = claim_program.to_bytes();
    let slot_bytes = slot.to_le_bytes();
    let seeds = [
        &claim_bytes[..32],
        &signer_bytes[..32],
        &slot_bytes[..8],
        &[bump_seed],
    ];

    let instruction = decompress_token_instruction(
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

/// CPI Accounts for decompressing compressed token accounts.
pub struct CompressedTokenDecompressCpiAccounts<'a> {
    fee_payer: AccountInfo<'a>,
    authority: AccountInfo<'a>,
    cpi_authority_pda: AccountInfo<'a>,
    light_system_program: AccountInfo<'a>,
    registered_program_pda: AccountInfo<'a>,
    noop_program: AccountInfo<'a>,
    account_compression_authority: AccountInfo<'a>,
    account_compression_program: AccountInfo<'a>,
    self_program: AccountInfo<'a>,
    token_pool_pda: AccountInfo<'a>,
    decompress_destination: AccountInfo<'a>,
    token_program: AccountInfo<'a>,
    system_program: AccountInfo<'a>,
}

// TODO: Move to SDK.
/// Return Instruction to decompress compressed token accounts.
pub fn decompress_token_instruction(
    mint: &Pubkey,
    compressed_token_accounts: Vec<InputTokenDataWithContext>,
    proof: &CompressedProof,
    light_cpi_accounts: &CompressedTokenDecompressCpiAccounts,
) -> Result<Instruction, ProgramError> {
    let data = decompress_token_instruction_data(mint, proof, compressed_token_accounts);

    let accounts = vec![
        AccountMeta::new(*light_cpi_accounts.fee_payer.key, true),
        AccountMeta::new_readonly(*light_cpi_accounts.authority.key, true), //TODO: CHECK SIGNING
        AccountMeta::new_readonly(*light_cpi_accounts.cpi_authority_pda.key, true),
        AccountMeta::new_readonly(*light_cpi_accounts.light_system_program.key, false),
        AccountMeta::new_readonly(*light_cpi_accounts.registered_program_pda.key, false),
        AccountMeta::new_readonly(*light_cpi_accounts.noop_program.key, false),
        AccountMeta::new_readonly(*light_cpi_accounts.account_compression_authority.key, false),
        AccountMeta::new_readonly(*light_cpi_accounts.account_compression_program.key, false),
        AccountMeta::new_readonly(*light_cpi_accounts.self_program.key, false),
        AccountMeta::new(*light_cpi_accounts.token_pool_pda.key, false),
        AccountMeta::new(*light_cpi_accounts.decompress_destination.key, false),
        AccountMeta::new_readonly(*light_cpi_accounts.token_program.key, false),
        AccountMeta::new_readonly(*light_cpi_accounts.system_program.key, false),
    ];

    Ok(Instruction {
        program_id: *light_cpi_accounts.token_program.key,
        accounts,
        data,
    })
}

// TODO: Move to SDK.
/// Return Instruction Data to decompress compressed token accounts.
pub fn decompress_token_instruction_data(
    mint: &Pubkey,
    proof: &CompressedProof,
    compressed_token_accounts: Vec<InputTokenDataWithContext>,
) -> Vec<u8> {
    let amount = compressed_token_accounts
        .iter()
        .map(|data| data.amount)
        .sum();

    let compressed_token_instruction_data_transfer = CompressedTokenInstructionDataTransfer {
        proof: Some(*proof),
        mint: *mint,
        delegated_transfer: None,
        input_token_data_with_context: compressed_token_accounts,
        output_compressed_accounts: Vec::new(),
        is_compress: false,
        compress_or_decompress_amount: Some(amount),
        cpi_context: None,
        lamports_change_account_merkle_tree_index: None,
    };

    let mut inputs = Vec::new();

    compressed_token_instruction_data_transfer
        .serialize(&mut inputs)
        .unwrap();
    inputs
}

/// CHECK:
/// 1. PDA belongs to the claimant.
/// 2. PDA belongs to mint.
/// 3. PDA owned by program.
/// 4. PDA derived from passed slot
///
fn check_claim_pda(
    claim_program: &Pubkey,
    claimant: &Pubkey,
    mint: &Pubkey,
    bump_seed: &u8,
    unlock_slot: &u64,
    airdrop_account: &Pubkey,
) -> Result<(), ProgramError> {
    let seed_slot = &unlock_slot.to_le_bytes();
    let claimant_bytes = claimant.to_bytes();
    let mint_bytes = mint.to_bytes();

    let derived_pda = Pubkey::create_program_address(
        &[
            seed_slot,
            &claimant_bytes[..32],
            &mint_bytes[..32],
            &[*bump_seed],
        ],
        claim_program,
    )?;

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
