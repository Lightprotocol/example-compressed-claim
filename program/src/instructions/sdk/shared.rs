use light_compressed_account::compressed_account::PackedMerkleContext;
use solana_program::{account_info::AccountInfo, msg, program_error::ProgramError};

/// CPI Accounts for decompressing compressed token accounts.
pub struct CompressedTokenDecompressCpiAccounts<'a> {
    pub fee_payer: AccountInfo<'a>,
    pub authority: AccountInfo<'a>,
    pub cpi_authority_pda: AccountInfo<'a>,
    pub light_system_program: AccountInfo<'a>,
    pub registered_program_pda: AccountInfo<'a>,
    pub noop_program: AccountInfo<'a>,
    pub account_compression_authority: AccountInfo<'a>,
    pub account_compression_program: AccountInfo<'a>,
    pub self_program: AccountInfo<'a>,
    pub token_pool_pda: AccountInfo<'a>,
    pub decompress_destination: AccountInfo<'a>,
    pub token_program: AccountInfo<'a>,
    pub system_program: AccountInfo<'a>,
    pub tree_and_queue_accounts: Vec<AccountInfo<'a>>,
}

// Base number of accounts (excluding tree and queue accounts)
pub const BASE_CPI_ACCOUNT_COUNT: usize = 13;

/// Extract compressed token CPI accounts from instruction accounts
///
/// # Arguments
/// * `accounts` - The instruction accounts array
/// * `start_index` - Index where CPI accounts begin
/// * `merkle_contexts` - Merkle contexts to determine required accounts and their ordering.
///
/// # Returns
/// * `Result` containing tuple of (CPI accounts struct, remaining accounts slice).
///
/// # Errors
/// * `ProgramError::NotEnoughAccountKeys` if there aren't enough accounts
///
/// # Example
/// ```ignore
/// let (cpi_accounts, remaining) = get_decompress_cpi_accounts(accounts, 1, &[merkle_context])?;
/// ```
pub fn get_decompress_cpi_accounts<'a, 'b>(
    accounts: &'a [AccountInfo<'b>],
    start_index: usize,
    merkle_contexts: &[PackedMerkleContext],
) -> Result<
    (
        CompressedTokenDecompressCpiAccounts<'b>,
        &'a [AccountInfo<'b>],
    ),
    ProgramError,
> {
    let idx = start_index;
    // TODO: could use insert_or_get.
    let mut tree_and_queue_accounts = vec![];
    let mut seen_indices = std::collections::HashSet::new();

    for context in merkle_contexts {
        let tree_idx = idx + BASE_CPI_ACCOUNT_COUNT + context.merkle_tree_pubkey_index as usize;
        let queue_idx = idx + BASE_CPI_ACCOUNT_COUNT + context.queue_pubkey_index as usize;

        if tree_idx >= accounts.len() || queue_idx >= accounts.len() {
            msg!(
                "Account index out of bounds: tree_idx={}, queue_idx={}, accounts.len()={}",
                tree_idx,
                queue_idx,
                accounts.len()
            );
            return Err(ProgramError::NotEnoughAccountKeys);
        }

        if !seen_indices.contains(&tree_idx) {
            tree_and_queue_accounts.push(accounts[tree_idx].clone());
            seen_indices.insert(tree_idx);
        }
        if !seen_indices.contains(&queue_idx) {
            tree_and_queue_accounts.push(accounts[queue_idx].clone());
            seen_indices.insert(queue_idx);
        }
    }

    let cpi_accounts = CompressedTokenDecompressCpiAccounts {
        fee_payer: accounts[idx].clone(),
        authority: accounts[idx + 1].clone(),
        cpi_authority_pda: accounts[idx + 2].clone(),
        light_system_program: accounts[idx + 3].clone(),
        registered_program_pda: accounts[idx + 4].clone(),
        noop_program: accounts[idx + 5].clone(),
        account_compression_authority: accounts[idx + 6].clone(),
        account_compression_program: accounts[idx + 7].clone(),
        self_program: accounts[idx + 8].clone(),
        token_pool_pda: accounts[idx + 9].clone(),
        decompress_destination: accounts[idx + 10].clone(),
        token_program: accounts[idx + 11].clone(),
        system_program: accounts[idx + 12].clone(),
        tree_and_queue_accounts: tree_and_queue_accounts.clone(),
    };

    // all non-cpi accounts that may follow.
    let remaining = &accounts[idx + BASE_CPI_ACCOUNT_COUNT + tree_and_queue_accounts.len()..];

    Ok((cpi_accounts, remaining))
}

pub trait ToAccountInfos<'a> {
    fn to_account_infos(&self) -> Vec<AccountInfo<'a>>;
}

impl<'a> ToAccountInfos<'a> for CompressedTokenDecompressCpiAccounts<'a> {
    fn to_account_infos(&self) -> Vec<AccountInfo<'a>> {
        let mut accounts = vec![
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
        ];
        accounts.extend(self.tree_and_queue_accounts.iter().cloned());
        accounts
    }
}

/// Helper trait to get the compressed token program from CPI accounts
pub trait CompressedTokenProgramGetter<'a> {
    fn compressed_token_program(&self) -> &AccountInfo<'a>;
}

impl<'a> CompressedTokenProgramGetter<'a> for CompressedTokenDecompressCpiAccounts<'a> {
    fn compressed_token_program(&self) -> &AccountInfo<'a> {
        &self.self_program
    }
}

#[cfg(feature = "anchor")]
use anchor_lang::AnchorSerialize;
#[cfg(not(feature = "anchor"))]
use borsh::BorshSerialize as AnchorSerialize;
use light_compressed_account::instruction_data::{
    compressed_proof::CompressedProof, cpi_context::CompressedCpiContext,
};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use light_compressed_token_sdk::state::{
    CompressedTokenInstructionDataTransfer, InputTokenDataWithContext,
};

// TODO: v2
/// Return Instruction to decompress compressed token accounts.
pub fn decompress(
    mint: &Pubkey,
    owner: &Pubkey,
    compressed_token_accounts: Vec<InputTokenDataWithContext>,
    proof: &Option<CompressedProof>,
    light_cpi_accounts: &CompressedTokenDecompressCpiAccounts,
    cpi_context: Option<&CompressedCpiContext>,
) -> Result<Instruction, ProgramError> {
    decompress_with_amounts(
        mint,
        owner,
        compressed_token_accounts,
        proof,
        light_cpi_accounts,
        cpi_context,
        None,
        None,
    )
}

/// Return Instruction to decompress compressed token accounts with optional partial amounts.
pub fn decompress_with_amounts(
    mint: &Pubkey,
    owner: &Pubkey,
    compressed_token_accounts: Vec<InputTokenDataWithContext>,
    proof: &Option<CompressedProof>,
    light_cpi_accounts: &CompressedTokenDecompressCpiAccounts,
    cpi_context: Option<&CompressedCpiContext>,
    decompress_amount: Option<u64>,
    decompress_lamports: Option<u64>,
) -> Result<Instruction, ProgramError> {
    let data = decompress_token_instruction_data(
        mint,
        proof,
        owner,
        compressed_token_accounts,
        cpi_context,
        decompress_amount,
        decompress_lamports,
    );

    let mut accounts = vec![
        AccountMeta::new(*light_cpi_accounts.fee_payer.key, true),
        AccountMeta::new_readonly(*light_cpi_accounts.authority.key, true),
        AccountMeta::new_readonly(*light_cpi_accounts.cpi_authority_pda.key, false),
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
    accounts.extend(
        light_cpi_accounts
            .tree_and_queue_accounts
            .iter()
            .map(|acc| AccountMeta::new(*acc.key, false)),
    );

    Ok(Instruction {
        program_id: *light_cpi_accounts.self_program.key,
        accounts,
        data,
    })
}

/// Return Instruction Data to decompress compressed token accounts with optional partial amounts.
pub fn decompress_token_instruction_data(
    mint: &Pubkey,
    proof: &Option<CompressedProof>,
    owner: &Pubkey,
    compressed_token_accounts: Vec<InputTokenDataWithContext>,
    cpi_context: Option<&CompressedCpiContext>,
    decompress_amount: Option<u64>,
    decompress_lamports: Option<u64>,
) -> Vec<u8> {
    let total_amount: u64 = compressed_token_accounts
        .iter()
        .map(|data| data.amount)
        .sum();

    let total_lamports: u64 = compressed_token_accounts
        .iter()
        .map(|data| data.lamports.unwrap_or(0))
        .sum();

    let amount_to_decompress = decompress_amount.unwrap_or(total_amount);
    let lamports_to_decompress = decompress_lamports.unwrap_or(total_lamports);

    // Create output compressed account if partial decompression
    let output_compressed_accounts = if amount_to_decompress < total_amount {
        let output = light_compressed_token_sdk::state::PackedTokenTransferOutputData {
            owner: *owner,
            amount: total_amount - amount_to_decompress,
            lamports: Some(total_lamports - lamports_to_decompress)
                .filter(|lamports| *lamports > 0),
            merkle_tree_index: compressed_token_accounts[0]
                .merkle_context
                .merkle_tree_pubkey_index as u8, // TODO: v2 support.
            tlv: None,
        };
        vec![output]
    } else {
        Vec::new()
    };

    let compressed_token_instruction_data_transfer = CompressedTokenInstructionDataTransfer {
        proof: *proof,
        mint: *mint,
        delegated_transfer: None,
        input_token_data_with_context: compressed_token_accounts,
        output_compressed_accounts,
        is_compress: false,
        compress_or_decompress_amount: Some(amount_to_decompress),
        cpi_context: cpi_context.copied(),
        lamports_change_account_merkle_tree_index: None,
        with_transaction_hash: false,
    };

    let mut inputs = Vec::new();
    // transfer discriminator
    inputs.extend_from_slice(&[163, 52, 200, 231, 140, 3, 69, 186]);

    let mut serialized_data = Vec::new();
    compressed_token_instruction_data_transfer
        .serialize(&mut serialized_data)
        .unwrap();

    // Add length buffer
    inputs.extend_from_slice(&(serialized_data.len() as u32).to_le_bytes());
    inputs.extend_from_slice(&serialized_data);
    inputs
}
