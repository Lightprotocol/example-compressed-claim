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
use solana_program::{
    account_info::AccountInfo, clock::Clock, entrypoint::ProgramResult, msg,
    program::invoke_signed, program_error::ProgramError, pubkey::Pubkey, sysvar::Sysvar,
};

// TODO: move to light-compressed-token-sdk
// TODO: deal with queue and tree separately (needs dynamic)
/// Number of accounts required for compressed token CPI
const CTOKEN_CPI_ACCOUNT_COUNT: usize = 15;

/// Extract compressed token CPI accounts from instruction accounts
///
/// # Arguments
/// * `accounts` - The instruction accounts array
/// * `start_index` - Index where CPI accounts begin
///
/// # Returns
/// * Tuple of (CPI accounts struct, remaining accounts slice)
///
/// # Example
/// ```ignore
/// let (cpi_accounts, remaining) = get_cpi_accounts!(accounts, 1)?;
/// ```
macro_rules! get_cpi_accounts {
    ($accounts:expr, $start_index:expr) => {{
        let accounts = $accounts;
        let start_idx = $start_index;
        let end_idx = start_idx + CTOKEN_CPI_ACCOUNT_COUNT;

        if accounts.len() < end_idx {
            msg!(
                "Not enough accounts for CPI: expected at least {}, got {}",
                end_idx,
                accounts.len()
            );
            Err(ProgramError::NotEnoughAccountKeys)
        } else {
            let cpi_accounts = CompressedTokenDecompressCpiAccounts {
                fee_payer: accounts[start_idx].clone(),
                authority: accounts[start_idx + 1].clone(),
                cpi_authority_pda: accounts[start_idx + 2].clone(),
                light_system_program: accounts[start_idx + 3].clone(),
                registered_program_pda: accounts[start_idx + 4].clone(),
                noop_program: accounts[start_idx + 5].clone(),
                account_compression_authority: accounts[start_idx + 6].clone(),
                account_compression_program: accounts[start_idx + 7].clone(),
                self_program: accounts[start_idx + 8].clone(),
                token_pool_pda: accounts[start_idx + 9].clone(),
                decompress_destination: accounts[start_idx + 10].clone(),
                token_program: accounts[start_idx + 11].clone(),
                system_program: accounts[start_idx + 12].clone(),
                state_merkle_tree: accounts[start_idx + 13].clone(),
                queue: accounts[start_idx + 14].clone(),
            };

            let remaining = &accounts[end_idx..];
            Ok::<_, ProgramError>((cpi_accounts, remaining))
        }
    }};
}

/// Helper trait to convert CPI accounts to array for invoke_signed
trait ToAccountInfos<'a> {
    fn to_account_infos(&self) -> [AccountInfo<'a>; CTOKEN_CPI_ACCOUNT_COUNT];
}

impl<'a> ToAccountInfos<'a> for CompressedTokenDecompressCpiAccounts<'a> {
    fn to_account_infos(&self) -> [AccountInfo<'a>; CTOKEN_CPI_ACCOUNT_COUNT] {
        [
            self.fee_payer.clone(),
            self.authority.clone(),
            self.cpi_authority_pda.clone(),
            self.light_system_program.clone(),
            self.registered_program_pda.clone(),
            self.noop_program.clone(),
            self.account_compression_authority.clone(),
            self.account_compression_program.clone(),
            self.self_program.clone(),
            self.token_pool_pda.clone(),
            self.decompress_destination.clone(),
            self.token_program.clone(),
            self.system_program.clone(),
            self.state_merkle_tree.clone(),
            self.queue.clone(),
        ]
    }
}

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

    let (light_cpi_accounts, _remaining) = get_cpi_accounts!(accounts, 1)?;

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
    if light_cpi_accounts.self_program.key != &CTOKEN_PROGRAM_ID {
        msg!("Invalid compressed token program.",);
        light_cpi_accounts.self_program.key.log();
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
        &light_cpi_accounts.to_account_infos()[..],
        signers_seeds,
    )?;
    Ok(())
}
