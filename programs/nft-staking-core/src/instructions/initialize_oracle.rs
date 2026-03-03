use anchor_lang::{prelude::*, system_program};
use mpl_core::{
    instructions::AddCollectionExternalPluginAdapterV1CpiBuilder,
    types::{
        ExternalCheckResult, ExternalPluginAdapterInitInfo, HookableLifecycleEvent, OracleInitInfo,
        PluginAuthority, ValidationResultsOffset,
    },
    ExternalCheckResultBits, ID as MPL_CORE_ID,
};

use crate::{
    errors::StakingError,
    state::{OracleState, OracleVault, ORACLE_STATE_SEED, ORACLE_VAULT_SEED},
};

#[derive(Accounts)]
pub struct InitializeOracle<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    /// CHECK: collection validated by mpl core
    #[account(mut)]
    pub collection: UncheckedAccount<'info>,
    /// CHECK: PDA update authority
    #[account(
        seeds = [b"update_authority", collection.key().as_ref()],
        bump
    )]
    pub update_authority: UncheckedAccount<'info>,
    #[account(
        init,
        payer = admin,
        space = 8 + OracleState::LEN,
        seeds = [ORACLE_STATE_SEED, collection.key().as_ref()],
        bump
    )]
    pub oracle_state: Account<'info, OracleState>,
    #[account(
        init,
        payer = admin,
        space = 8 + OracleVault::LEN,
        seeds = [ORACLE_VAULT_SEED, collection.key().as_ref()],
        bump
    )]
    pub oracle_vault: Account<'info, OracleVault>,
    /// CHECK: mpl core program id
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

impl<'info> InitializeOracle<'info> {
    pub fn initialize_oracle(
        &mut self,
        open_hour_utc: u8,
        close_hour_utc: u8,
        boundary_window_seconds: i64,
        crank_reward_lamports: u64,
        initial_vault_lamports: u64,
        bumps: &InitializeOracleBumps,
    ) -> Result<()> {
        require!(open_hour_utc < 24, StakingError::InvalidTimestamp);
        require!(close_hour_utc < 24, StakingError::InvalidTimestamp);
        require!(boundary_window_seconds > 0, StakingError::InvalidTimestamp);

        self.oracle_state.set_inner(OracleState::init_default(
            open_hour_utc,
            close_hour_utc,
            boundary_window_seconds,
            crank_reward_lamports,
            bumps.oracle_state,
        ));

        self.oracle_vault.set_inner(OracleVault {
            bump: bumps.oracle_vault,
        });

        if initial_vault_lamports > 0 {
            let cpi_ctx = CpiContext::new(
                self.system_program.to_account_info(),
                system_program::Transfer {
                    from: self.admin.to_account_info(),
                    to: self.oracle_vault.to_account_info(),
                },
            );
            system_program::transfer(cpi_ctx, initial_vault_lamports)?;
        }

        let mut check_bits = ExternalCheckResultBits::new();
        check_bits.set_can_reject(true);

        let collection_key = self.collection.key();
        let signer_seeds = &[
            b"update_authority",
            collection_key.as_ref(),
            &[bumps.update_authority],
        ];

        AddCollectionExternalPluginAdapterV1CpiBuilder::new(
            &self.mpl_core_program.to_account_info(),
        )
        .collection(&self.collection.to_account_info())
        .payer(&self.admin.to_account_info())
        .authority(Some(&self.update_authority.to_account_info()))
        .system_program(&self.system_program.to_account_info())
        .init_info(ExternalPluginAdapterInitInfo::Oracle(OracleInitInfo {
            base_address: self.oracle_state.key(),
            init_plugin_authority: Some(PluginAuthority::UpdateAuthority),
            lifecycle_checks: vec![(
                HookableLifecycleEvent::Transfer,
                ExternalCheckResult {
                    flags: u32::from_le_bytes(check_bits.into_bytes()),
                },
            )],
            base_address_config: None,
            results_offset: Some(ValidationResultsOffset::Anchor),
        }))
        .invoke_signed(&[signer_seeds])?;

        Ok(())
    }
}
