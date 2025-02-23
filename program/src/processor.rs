use crate::error::CustomError;
use light_compressed_account_anchorless::{
    compressed_account::{
        CompressedAccount, CompressedAccountData, PackedCompressedAccountWithMerkleContext,
        PackedMerkleContext,
    },
    hash_to_bn254_field_size_be,
    instruction_data::{
        compressed_proof::CompressedProof, cpi_context::CompressedCpiContext,
        data::OutputCompressedAccountWithPackedContext, invoke_cpi::InstructionDataInvokeCpi,
    },
    token::{CompressedTokenInstructionDataTransfer, InputTokenDataWithContext},
};
use solana_program::account_info::AccountInfo;
use solana_program::instruction::AccountMeta;
use solana_program::instruction::Instruction;
use solana_program::program::invoke_signed;
use solana_program::pubkey;
use solana_program::{
    account_info::next_account_info, clock::Clock, entrypoint::ProgramResult, msg,
    program_error::ProgramError, pubkey::Pubkey, sysvar::Sysvar,
};

use std::slice::Iter;

// TODO HARDCODE FOR CTOKEN
const CTOKEN_CPI_AUTHORITY_PDA: Pubkey = pubkey!("GXtd2izAiMJPwMEjfgTRH3d7k9mjn4Jq3JrWFv9gySYy");
// pub fn get_cpi_authority_pda() -> (Pubkey, u8) {
//     Pubkey::find_program_address(&[b"cpi_authority"], &pubkey!("cTokenmWW8bLPjZEBAUgYy3zKxQZW6VKi7bqNFEVv3m"))
// }

use spl_token_metadata_interface::borsh::BorshSerialize;
/// Processes the "claim_and_decompress" instruction
///
/// Expected Accounts:
/// 0. `[signer]` The claimant's account.
/// 1. `[]` The mint account (read-only).
/// 2. `[]` The associated airdrop pda. (read-only, uninitialized)
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let account_info_iter: &mut Iter<'_, AccountInfo<'_>> = &mut accounts.iter();

    // Account 0: Signer, claimant
    let signer = next_account_info(account_info_iter)?;

    // CHECK:
    if !signer.is_signer {
        msg!("Missing required signature.");
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Account 1: Mint account
    let mint_account = next_account_info(account_info_iter)?;

    // Account 2: Airdrop PDA
    let airdrop_account = next_account_info(account_info_iter)?;

    // CHECK:
    if airdrop_account.lamports() > 0 || airdrop_account.data_len() > 0 {
        msg!("Airdrop PDA must be uninitialized.");
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    // Account 3: Destination account
    let destination = next_account_info(account_info_iter)?;

    // Account 4: Token pool PDA
    let token_pool_pda = next_account_info(account_info_iter)?;

    // CHECK:
    if instruction_data.len() != 8 {
        msg!("Invalid instruction data length; expected 8 bytes.");
        return Err(ProgramError::InvalidInstructionData);
    }
    let unlock_slot = u64::from_le_bytes(instruction_data.try_into().unwrap());

    // Get current slot
    let current_slot = Clock::get()?.slot;

    // CHECK:
    if current_slot < unlock_slot {
        msg!(
            "Tokens are still locked: current slot ({}) is less than unlock slot ({}).",
            current_slot,
            unlock_slot
        );
        return Err(CustomError::TokensLocked.into());
    }

    // Derive airdrop PDA
    let seed_slot = &unlock_slot.to_le_bytes();
    let seeds = &[seed_slot, signer.key.as_ref(), mint_account.key.as_ref()];
    let (derived_pda, _bump) = Pubkey::find_program_address(seeds, program_id);

    // CHECK:
    if derived_pda != *airdrop_account.key {
        msg!(
            "Invalid airdrop PDA provided. Expected: {}. Found: {}.",
            derived_pda,
            airdrop_account.key
        );
        return Err(CustomError::InvalidPDA.into());
    }

    let bump = 0;

    // get the account(s) to decompress.
    let input_token_data_with_context = InputTokenDataWithContext {
        amount,
        delegate_index: None,
        merkle_context: PackedMerkleContext::default(),
        root_index,
        lamports: None,
        tlv: None,
    };

    // TODO: must get the input account.
    // TODO: add decompress.
    decompress_token(
        program_id,
        signer,
        mint_account,
        airdrop_account,
        token_pool_pda,
        destination,
        bump,
        root_index,
        proof,
        input_token_data_with_context,
    );
    // Log success
    msg!("Claim successful.");

    Ok(())
}

// caller program.
fn decompress_token<'a>(
    claim_program: &Pubkey,
    signer: AccountInfo<'a>,
    token_pool_pda: AccountInfo<'a>,
    mint: AccountInfo<'a>,
    destination: AccountInfo<'a>,
    associated_airdrop_pda: AccountInfo<'a>,
    bump_seed: u8,
    slot: u64,
    root_index: u16,
    proof: &CompressedProof,
    compressed_token_accounts: Vec<InputTokenDataWithContext>,
    ctoken_cpi_authority_pda: AccountInfo<'a>,

    token_program: AccountInfo<'a>,
    system_program: AccountInfo<'a>,
) -> ProgramResult {
    let signer_bytes = signer.key.to_bytes();
    let claim_bytes = claim_program.to_bytes();
    let slot_bytes = slot.to_le_bytes();
    let seeds = [
        &claim_bytes[..32],
        &signer_bytes[..32],
        &slot_bytes[..8],
        &[bump_seed],
    ];

    let light_cpi_accounts = CompressedTokenDecompressCpiAccounts {
        fee_payer: signer,
        authority: associated_airdrop_pda,
        cpi_authority_pda: ctoken_cpi_authority_pda,
        light_system_program: signer,
        registered_program_pda: signer,
        noop_program: signer,
        account_compression_authority: signer,
        account_compression_program: signer,
        self_program: signer,
        token_pool_pda: signer,
        decompress_destination: signer,
        token_program,
        system_program,
    };

    // ACCS RIGHT
    let instruction = decompress_token_instruction(
        claim_program,
        token_program,
        token_pool_pda,
        mint,
        destination,
        associated_airdrop_pda,
        compressed_token_accounts,
        proof,
        &light_cpi_accounts
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

//TODO: move to sdk. "decompress_spl_account"
pub fn decompress_token_instruction(
    claim_program: &Pubkey,
    token_program: &Pubkey,
    token_pool_pda: &Pubkey,
    mint: &Pubkey,
    destination: &Pubkey, // user
    associated_airdrop_pda: &Pubkey,
    compressed_token_accounts: Vec<InputTokenDataWithContext>,
    proof: CompressedProof,
    cpi_accounts: &mut Iter<'_, AccountInfo>,
) -> Result<Instruction, ProgramError> {
    let data = decompress_token_instruction_data(mint, proof, compressed_token_accounts);

    let accounts = vec![
        AccountMeta::new(*token_pool_pda, false),
        AccountMeta::new(*mint, false),
        AccountMeta::new(*destination, false),
        AccountMeta::new_readonly(*associated_airdrop_pda, true),
        AccountMeta::new_readonly(*cpi_accounts.next().unwrap().key, false), // registered_program_pda
        AccountMeta::new_readonly(*cpi_accounts.next().unwrap().key, false), // noop_program
        AccountMeta::new_readonly(*cpi_accounts.next().unwrap().key, false), // account_compression_authority
        AccountMeta::new_readonly(*cpi_accounts.next().unwrap().key, false), // account_compression_program
        AccountMeta::new(*claim_program, false),                             // self_program
        AccountMeta::new_readonly(*cpi_accounts.next().unwrap().key, false), // cpi_authority_pda
        AccountMeta::new_readonly(*cpi_accounts.next().unwrap().key, false), // light_system_program
    ];

    Ok(Instruction {
        program_id: *token_program,
        accounts,
        data,
    })
}

//TODO: move to sdk. "decompress_token_instruction_data"
pub fn decompress_token_instruction_data(
    mint: &Pubkey,
    proof: CompressedProof,
    compressed_token_accounts: Vec<InputTokenDataWithContext>,
) -> Vec<u8> {
    // decompress all.
    let amount = compressed_token_accounts
        .iter()
        .map(|data| data.amount)
        .sum();

    let compressed_token_instruction_data_transfer = CompressedTokenInstructionDataTransfer {
        proof: Some(proof),
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
    use borsh::ser::BorshSerialize;
    compressed_token_instruction_data_transfer
        .serialize(&mut inputs)
        .unwrap();
    inputs
}
