use crate::{error::ClaimError, instruction::ClaimProgramInstruction};
use borsh::BorshDeserialize;
use light_compressed_account::compressed_account::PackedMerkleContext;
use light_ctoken_interface::instructions::transfer2::MultiInputTokenDataWithContext;
use light_ctoken_sdk::{
    compressed_token::{
        transfer2::{
            create_transfer2_instruction, Transfer2AccountsMetaConfig, Transfer2Config,
            Transfer2Inputs,
        },
        CTokenAccount2,
    },
    ValidityProof,
};
use solana_program::{
    account_info::AccountInfo,
    clock::Clock,
    entrypoint::ProgramResult,
    instruction::AccountMeta,
    msg,
    program::invoke_signed,
    program_error::ProgramError,
    pubkey,
    pubkey::Pubkey,
    sysvar::Sysvar,
};

const CTOKEN_PROGRAM_ID: Pubkey = pubkey!("cTokenmWW8bLPjZEBAUgYy3zKxQZW6VKi7bqNFEVv3m");

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let instruction = ClaimProgramInstruction::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    match instruction {
        ClaimProgramInstruction::Claim {
            validity_proof,
            merkle_context,
            root_index,
            amount,
            mint,
            unlock_slot,
            bump_seed,
            // Indices into packed_accounts
            owner_index,
            mint_index,
            destination_index,
            pool_account_index,
            pool_index,
            pool_bump,
        } => process_claim(
            program_id,
            accounts,
            validity_proof,
            merkle_context,
            root_index,
            amount,
            mint,
            unlock_slot,
            bump_seed,
            owner_index,
            mint_index,
            destination_index,
            pool_account_index,
            pool_index,
            pool_bump,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn process_claim(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    validity_proof: ValidityProof,
    merkle_context: PackedMerkleContext,
    root_index: u16,
    amount: u64,
    mint: Pubkey,
    unlock_slot: u64,
    bump_seed: u8,
    owner_index: u8,
    mint_index: u8,
    destination_index: u8,
    pool_account_index: u8,
    pool_index: u8,
    pool_bump: u8,
) -> ProgramResult {
    // Account layout:
    // 0: claimant (signer)
    // 1: fee_payer (signer)
    // 2: associated_airdrop_pda (authority for the compressed tokens)
    // 3: ctoken_program
    // 4: ctoken_cpi_authority_pda
    // 5: light_system_program
    // 6: registered_program_pda
    // 7: account_compression_authority
    // 8: account_compression_program
    // 9: system_program
    // 10+: packed_accounts (merkle tree, queue, mint, owner, destination, pool, etc.)

    let claimant_info = &accounts[0];
    let fee_payer_info = &accounts[1];
    let associated_airdrop_pda_info = &accounts[2];
    let ctoken_program_info = &accounts[3];

    // CHECK: claimant must be signer
    if !claimant_info.is_signer {
        msg!("Claimant must be a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    // CHECK: fee_payer must be signer
    if !fee_payer_info.is_signer {
        msg!("Fee payer must be a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    // CHECK: ctoken program ID
    if ctoken_program_info.key != &CTOKEN_PROGRAM_ID {
        msg!("Invalid compressed token program");
        return Err(ProgramError::InvalidArgument);
    }

    // CHECK: unlock slot
    let current_slot = Clock::get()?.slot;
    if current_slot < unlock_slot {
        msg!(
            "Tokens are still locked: current slot ({}) is less than unlock slot ({}).",
            current_slot,
            unlock_slot
        );
        return Err(ClaimError::TokensLocked.into());
    }

    // Verify the PDA
    let claimant_bytes = claimant_info.key.to_bytes();
    let mint_bytes = mint.to_bytes();
    let slot_bytes = unlock_slot.to_le_bytes();
    let seeds = &[
        &claimant_bytes[..32],
        &mint_bytes[..32],
        &slot_bytes[..8],
        &[bump_seed],
    ];
    check_claim_pda(seeds, program_id, associated_airdrop_pda_info.key)?;

    // Build MultiInputTokenDataWithContext from instruction parameters
    let input_token_data = MultiInputTokenDataWithContext {
        owner: owner_index,
        amount,
        has_delegate: false,
        delegate: 0,
        mint: mint_index,
        version: 3, // V2 for batched Merkle trees
        merkle_context,
        root_index,
    };

    // Create CTokenAccount2 and set up SPL decompression
    let mut token_account = CTokenAccount2::new(vec![input_token_data])
        .map_err(|_| ProgramError::InvalidAccountData)?;

    token_account
        .decompress_spl(amount, destination_index, pool_account_index, pool_index, pool_bump)
        .map_err(|_| ProgramError::InvalidAccountData)?;

    // Build packed account metas from accounts[10..]
    // Force owner account to be signer for invoke_signed to work
    let packed_accounts = &accounts[10..];
    let packed_account_metas: Vec<AccountMeta> = packed_accounts
        .iter()
        .enumerate()
        .map(|(i, info)| AccountMeta {
            pubkey: *info.key,
            is_signer: info.is_signer || i == owner_index as usize,
            is_writable: info.is_writable,
        })
        .collect();

    // Build the transfer2 instruction
    let meta_config = Transfer2AccountsMetaConfig::new(*fee_payer_info.key, packed_account_metas);
    let transfer_config = Transfer2Config::default().filter_zero_amount_outputs();

    let inputs = Transfer2Inputs {
        meta_config,
        token_accounts: vec![token_account],
        transfer_config,
        validity_proof,
        ..Default::default()
    };

    let instruction =
        create_transfer2_instruction(inputs).map_err(|_| ProgramError::InvalidInstructionData)?;

    // Invoke with PDA signer
    let signers_seeds: &[&[&[u8]]] = &[seeds];
    invoke_signed(&instruction, accounts, signers_seeds)?;

    Ok(())
}

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
