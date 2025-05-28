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

mod error;
pub mod instruction;
pub mod processor;
pub use solana_program;

solana_program::declare_id!("7UHB3CfWv7SugNhfdyP7aeZJPMjnpd9zJ7xYkHozB3Na");
