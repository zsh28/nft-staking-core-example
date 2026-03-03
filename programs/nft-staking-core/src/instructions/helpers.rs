use anchor_lang::prelude::*;
use mpl_core::{
    accounts::BaseCollectionV1,
    fetch_plugin,
    instructions::{AddCollectionPluginV1CpiBuilder, UpdateCollectionPluginV1CpiBuilder},
    types::{Attribute, Attributes, Plugin, PluginAuthority, PluginType},
};

use crate::errors::StakingError;

pub const SECONDS_PER_DAY: i64 = 86_400;

pub fn upsert_attribute(attributes: &mut Vec<Attribute>, key: &str, value: String) {
    if let Some(attribute) = attributes.iter_mut().find(|attribute| attribute.key == key) {
        attribute.value = value;
        return;
    }

    attributes.push(Attribute {
        key: key.to_string(),
        value,
    });
}

pub fn get_attribute<'a>(attributes: &'a [Attribute], key: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|attribute| attribute.key == key)
        .map(|attribute| attribute.value.as_str())
}

pub fn parse_i64_attribute(attributes: &[Attribute], key: &str) -> Result<i64> {
    let value = get_attribute(attributes, key).ok_or(StakingError::MissingStakeAttribute)?;
    value
        .parse::<i64>()
        .map_err(|_| error!(StakingError::InvalidTimestamp))
}

pub fn calculate_days_elapsed(start_timestamp: i64, end_timestamp: i64) -> Result<i64> {
    let elapsed_seconds = end_timestamp
        .checked_sub(start_timestamp)
        .ok_or(StakingError::InvalidTimestamp)?;

    elapsed_seconds
        .checked_div(SECONDS_PER_DAY)
        .ok_or(StakingError::InvalidTimestamp.into())
}

pub fn sync_collection_total_staked<'a>(
    mpl_core_program: &AccountInfo<'a>,
    collection: &AccountInfo<'a>,
    payer: &AccountInfo<'a>,
    authority: &AccountInfo<'a>,
    system_program: &AccountInfo<'a>,
    signer_seeds: &[&[&[u8]]],
    delta: i64,
) -> Result<()> {
    let _ = BaseCollectionV1::try_from(collection)?;

    match fetch_plugin::<BaseCollectionV1, Attributes>(collection, PluginType::Attributes) {
        Ok((_, mut fetched_attributes, _)) => {
            let current_total = get_attribute(&fetched_attributes.attribute_list, "total_staked")
                .unwrap_or("0")
                .parse::<i64>()
                .map_err(|_| error!(StakingError::InvalidTimestamp))?;

            let next_total = current_total
                .checked_add(delta)
                .ok_or(StakingError::Overflow)?;
            require!(next_total >= 0, StakingError::Underflow);

            upsert_attribute(
                &mut fetched_attributes.attribute_list,
                "total_staked",
                next_total.to_string(),
            );

            UpdateCollectionPluginV1CpiBuilder::new(mpl_core_program)
                .collection(collection)
                .payer(payer)
                .authority(Some(authority))
                .system_program(system_program)
                .plugin(Plugin::Attributes(fetched_attributes))
                .invoke_signed(signer_seeds)?;
        }
        Err(_) => {
            let start_total = 0_i64.checked_add(delta).ok_or(StakingError::Overflow)?;
            require!(start_total >= 0, StakingError::Underflow);

            AddCollectionPluginV1CpiBuilder::new(mpl_core_program)
                .collection(collection)
                .payer(payer)
                .authority(Some(authority))
                .system_program(system_program)
                .plugin(Plugin::Attributes(Attributes {
                    attribute_list: vec![Attribute {
                        key: "total_staked".to_string(),
                        value: start_total.to_string(),
                    }],
                }))
                .init_authority(PluginAuthority::UpdateAuthority)
                .invoke_signed(signer_seeds)?;
        }
    }

    Ok(())
}
