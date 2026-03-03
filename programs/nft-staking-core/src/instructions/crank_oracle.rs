use anchor_lang::prelude::*;

use crate::{
    errors::StakingError,
    state::{
        OracleState, OracleVault, EXTERNAL_VALIDATION_APPROVED, EXTERNAL_VALIDATION_REJECTED,
        ORACLE_STATE_SEED, ORACLE_VAULT_SEED,
    },
};

const SECONDS_PER_HOUR: i64 = 3600;
const SECONDS_PER_DAY: i64 = 86_400;

#[derive(Accounts)]
pub struct CrankOracle<'info> {
    #[account(mut)]
    pub caller: Signer<'info>,
    /// CHECK: collection key seed source
    pub collection: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [ORACLE_STATE_SEED, collection.key().as_ref()],
        bump = oracle_state.bump
    )]
    pub oracle_state: Account<'info, OracleState>,
    #[account(
        mut,
        seeds = [ORACLE_VAULT_SEED, collection.key().as_ref()],
        bump = oracle_vault.bump
    )]
    pub oracle_vault: Account<'info, OracleVault>,
}

impl<'info> CrankOracle<'info> {
    pub fn crank_oracle(&mut self) -> Result<()> {
        let unix_timestamp = Clock::get()?.unix_timestamp;
        let seconds_of_day =
            ((unix_timestamp % SECONDS_PER_DAY) + SECONDS_PER_DAY) % SECONDS_PER_DAY;
        let hour_utc = (seconds_of_day / SECONDS_PER_HOUR) as u8;

        let in_open_window = if self.oracle_state.open_hour_utc <= self.oracle_state.close_hour_utc
        {
            hour_utc >= self.oracle_state.open_hour_utc
                && hour_utc < self.oracle_state.close_hour_utc
        } else {
            hour_utc >= self.oracle_state.open_hour_utc
                || hour_utc < self.oracle_state.close_hour_utc
        };

        let transfer_state = if in_open_window {
            EXTERNAL_VALIDATION_APPROVED
        } else {
            EXTERNAL_VALIDATION_REJECTED
        };
        self.oracle_state.set_transfer_state(transfer_state);

        let open_seconds = self.oracle_state.open_hour_utc as i64 * SECONDS_PER_HOUR;
        let close_seconds = self.oracle_state.close_hour_utc as i64 * SECONDS_PER_HOUR;
        let boundary_window = self.oracle_state.boundary_window_seconds;

        let close_to_open = (seconds_of_day - open_seconds).abs() <= boundary_window;
        let close_to_close = (seconds_of_day - close_seconds).abs() <= boundary_window;

        if close_to_open || close_to_close {
            let reward = self.oracle_state.crank_reward_lamports;
            if reward > 0 {
                let vault_lamports = self.oracle_vault.to_account_info().lamports();
                require!(
                    vault_lamports >= reward,
                    StakingError::InsufficientOracleVaultBalance
                );

                **self
                    .oracle_vault
                    .to_account_info()
                    .try_borrow_mut_lamports()? -= reward;
                **self.caller.to_account_info().try_borrow_mut_lamports()? += reward;
            }
        }

        Ok(())
    }
}
