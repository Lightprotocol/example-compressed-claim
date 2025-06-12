//! Program entrypoint
#![cfg(not(feature = "no-entrypoint"))]
use solana_program::{
    account_info::AccountInfo, entrypoint, entrypoint::ProgramResult, pubkey::Pubkey,
};

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    crate::processor::process_instruction(program_id, accounts, instruction_data)
}

solana_program::declare_id!("7UHB3CfWv7SugNhfdyP7aeZJPMjnpd9zJ7xYkHozB3Na");

mod check_pda;
mod constants;
mod error;
pub mod instruction;
mod instructions;
pub mod processor;

#[cfg(not(target_os = "solana"))]
pub use instruction::client;

pub use solana_program;
