use anchor_lang::prelude::*;

pub const ORACLE_VALIDATION_V1: u8 = 1;
pub const EXTERNAL_VALIDATION_APPROVED: u8 = 0;
pub const EXTERNAL_VALIDATION_REJECTED: u8 = 1;
pub const EXTERNAL_VALIDATION_PASS: u8 = 2;

pub const ORACLE_STATE_SEED: &[u8] = b"oracle_state";
pub const ORACLE_VAULT_SEED: &[u8] = b"oracle_vault";

#[account]
pub struct OracleState {
    pub validation_results: [u8; 5],
    pub open_hour_utc: u8,
    pub close_hour_utc: u8,
    pub boundary_window_seconds: i64,
    pub crank_reward_lamports: u64,
    pub bump: u8,
}

impl OracleState {
    pub const LEN: usize = 128;

    pub fn init_default(
        open_hour_utc: u8,
        close_hour_utc: u8,
        boundary_window_seconds: i64,
        crank_reward_lamports: u64,
        bump: u8,
    ) -> Self {
        Self {
            validation_results: [
                ORACLE_VALIDATION_V1,
                EXTERNAL_VALIDATION_PASS,
                EXTERNAL_VALIDATION_REJECTED,
                EXTERNAL_VALIDATION_PASS,
                EXTERNAL_VALIDATION_PASS,
            ],
            open_hour_utc,
            close_hour_utc,
            boundary_window_seconds,
            crank_reward_lamports,
            bump,
        }
    }

    pub fn set_transfer_state(&mut self, transfer_result: u8) {
        self.validation_results[0] = ORACLE_VALIDATION_V1;
        self.validation_results[2] = transfer_result;
    }
}

#[account]
pub struct OracleVault {
    pub bump: u8,
}

impl OracleVault {
    pub const LEN: usize = 1;
}
