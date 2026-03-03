use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{mint_to_checked, Mint, MintToChecked, TokenAccount, TokenInterface},
};
use mpl_core::{
    accounts::{BaseAssetV1, BaseCollectionV1},
    fetch_plugin,
    instructions::UpdatePluginV1CpiBuilder,
    types::{Attribute, Attributes, Plugin, PluginType, UpdateAuthority},
    ID as MPL_CORE_ID,
};

use super::helpers::{
    calculate_days_elapsed, get_attribute, parse_i64_attribute, upsert_attribute, SECONDS_PER_DAY,
};
use crate::{errors::StakingError, state::Config};

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    /// CHECK: PDA update authority
    #[account(
        seeds = [b"update_authority", collection.key().as_ref()],
        bump
    )]
    pub update_authority: UncheckedAccount<'info>,
    #[account(
        seeds = [b"config", collection.key().as_ref()],
        bump = config.config_bump
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        seeds = [b"rewards", config.key().as_ref()],
        bump = config.rewards_bump
    )]
    pub rewards_mint: InterfaceAccount<'info, Mint>,
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = rewards_mint,
        associated_token::authority = user,
    )]
    pub user_rewards_ata: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: checked by mpl core
    #[account(mut)]
    pub nft: UncheckedAccount<'info>,
    /// CHECK: checked by mpl core
    #[account(mut)]
    pub collection: UncheckedAccount<'info>,
    /// CHECK: mpl core program id
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> ClaimRewards<'info> {
    pub fn claim_rewards(&mut self, bumps: &ClaimRewardsBumps) -> Result<()> {
        let base_asset = BaseAssetV1::try_from(&self.nft.to_account_info())?;
        require!(
            base_asset.owner == self.user.key(),
            StakingError::InvalidOwner
        );
        require!(
            base_asset.update_authority == UpdateAuthority::Collection(self.collection.key()),
            StakingError::InvalidAuthority
        );

        let base_collection = BaseCollectionV1::try_from(&self.collection.to_account_info())?;
        require!(
            base_collection.update_authority == self.update_authority.key(),
            StakingError::InvalidAuthority
        );

        let collection_key = self.collection.key();
        let signer_seeds = &[
            b"update_authority",
            collection_key.as_ref(),
            &[bumps.update_authority],
        ];

        let current_timestamp = Clock::get()?.unix_timestamp;

        let fetched_attributes = match fetch_plugin::<BaseAssetV1, Attributes>(
            &self.nft.to_account_info(),
            PluginType::Attributes,
        ) {
            Ok((_, attributes, _)) => attributes,
            Err(_) => return Err(StakingError::NotStaked.into()),
        };

        let mut attribute_list: Vec<Attribute> = fetched_attributes.attribute_list.clone();
        require!(
            get_attribute(&attribute_list, "staked") == Some("true"),
            StakingError::NotStaked
        );

        let staked_at = parse_i64_attribute(&attribute_list, "staked_at")?;
        let last_claimed_at = get_attribute(&attribute_list, "last_claimed_at")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(staked_at);

        let claimable_days = calculate_days_elapsed(last_claimed_at, current_timestamp)?;
        require!(claimable_days > 0, StakingError::NoRewardsToClaim);

        let amount = (claimable_days as u64)
            .checked_mul(self.config.points_per_stake as u64)
            .ok_or(StakingError::Overflow)?;

        let next_claimed_at = last_claimed_at
            .checked_add(
                claimable_days
                    .checked_mul(SECONDS_PER_DAY)
                    .ok_or(StakingError::Overflow)?,
            )
            .ok_or(StakingError::Overflow)?;

        upsert_attribute(
            &mut attribute_list,
            "last_claimed_at",
            next_claimed_at.to_string(),
        );

        UpdatePluginV1CpiBuilder::new(&self.mpl_core_program.to_account_info())
            .asset(&self.nft.to_account_info())
            .collection(Some(&self.collection.to_account_info()))
            .payer(&self.user.to_account_info())
            .authority(Some(&self.update_authority.to_account_info()))
            .system_program(&self.system_program.to_account_info())
            .plugin(Plugin::Attributes(Attributes { attribute_list }))
            .invoke_signed(&[signer_seeds])?;

        let config_seeds = &[
            b"config",
            collection_key.as_ref(),
            &[self.config.config_bump],
        ];
        let config_signer_seeds = &[&config_seeds[..]];

        let cpi_accounts = MintToChecked {
            mint: self.rewards_mint.to_account_info(),
            to: self.user_rewards_ata.to_account_info(),
            authority: self.config.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            self.token_program.to_account_info(),
            cpi_accounts,
            config_signer_seeds,
        );
        mint_to_checked(cpi_ctx, amount, self.rewards_mint.decimals)?;

        Ok(())
    }
}
