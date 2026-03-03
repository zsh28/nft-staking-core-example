# NFT Staking Core

`nft-staking-core` is an Anchor program that lets users stake Metaplex Core NFTs and earn SPL token rewards, while keeping assets non-custodial.

It uses Metaplex Core plugins to store staking state directly on assets and collections, enforce transfer restrictions, and gate transfers through an Oracle-based schedule.

## What the program does

- Creates and manages a program-controlled Metaplex Core collection.
- Mints Core NFTs into that collection.
- Stakes NFTs by freezing them and marking staking attributes on-chain.
- Lets users claim rewards while still staked.
- Lets users unstake after a configured freeze period.
- Lets users burn staked NFTs for pending rewards plus a one-time bonus.
- Tracks collection-level `total_staked` as a collection attribute.
- Adds an external Oracle adapter that can approve/reject transfers based on UTC time window.
- Includes a permissionless crank that updates Oracle transfer state and can reward the caller near boundaries.

## Plugin usage

The program relies on these Metaplex Core plugins:

- `Attributes` (asset): stores `staked`, `staked_at`, and `last_claimed_at`.
- `FreezeDelegate` (asset): freezes NFT while staked, thaws on unstake.
- `BurnDelegate` (asset): allows delegated burn flow for staked burn-to-earn.
- `Attributes` (collection): stores and updates `total_staked`.
- External Oracle Adapter (collection): validates `Transfer` lifecycle with reject support.

## Program accounts and state

### `Config`

PDA scoped per collection.

```rust
#[account]
pub struct Config {
    pub points_per_stake: u32,
    pub freeze_period: u8,
    pub rewards_bump: u8,
    pub config_bump: u8,
}
```

`Config` is also mint authority for the rewards mint PDA.

### `OracleState`

PDA scoped per collection. Stores:

- transfer validation bytes used by Oracle adapter
- UTC open/close hour
- crank reward boundary window
- crank reward lamports

### `OracleVault`

PDA scoped per collection. Holds lamports used to pay boundary crank rewards.

## Instructions

- `create_collection(name, uri)`

  - Creates Core collection with program-derived update authority.

- `mint_nft(name, uri)`

  - Mints Core NFT into the collection.

- `initialize_config(points_per_stake, freeze_period)`

  - Initializes staking config PDA and rewards mint PDA.

- `stake()`

  - Verifies ownership/collection authority.
  - Sets `staked=true`, `staked_at=now`, `last_claimed_at=now`.
  - Freezes NFT via `FreezeDelegate` plugin.
  - Ensures burn delegate plugin exists.
  - Increments collection `total_staked`.

- `claim_rewards()`

  - Keeps NFT staked.
  - Mints rewards for full days since `last_claimed_at`.
  - Updates only `last_claimed_at`.

- `unstake()`

  - Requires freeze period elapsed since `staked_at`.
  - Mints full-day rewards.
  - Sets `staked=false`, resets stake timestamps.
  - Thaws NFT.
  - Decrements collection `total_staked`.

- `burn_staked_nft()`

  - Computes pending rewards plus one-time burn bonus.
  - Mints reward tokens.
  - Burns NFT with Metaplex Core burn CPI.
  - Decrements collection `total_staked`.

- `initialize_oracle(open_hour_utc, close_hour_utc, boundary_window_seconds, crank_reward_lamports, initial_vault_lamports)`

  - Initializes Oracle state/vault PDAs.
  - Attaches Oracle external plugin adapter to collection for Transfer lifecycle checks.

- `crank_oracle()`

  - Permissionless.
  - Reads current on-chain time and updates transfer state:
    - approved inside window
    - rejected outside window
  - Pays caller from Oracle vault near open/close boundary.

- `transfer_nft()`
  - Program path for transfer CPI.
  - Passes Oracle account as remaining account so transfer is validated by Oracle adapter.

## PDA map

| Account          | Seeds                              |
| ---------------- | ---------------------------------- |
| Update authority | `["update_authority", collection]` |
| Config           | `["config", collection]`           |
| Rewards mint     | `["rewards", config]`              |
| Oracle state     | `["oracle_state", collection]`     |
| Oracle vault     | `["oracle_vault", collection]`     |

## Test setup

Current tests are in `tests/nft-staking-core.ts` and cover:

- collection/config/oracle initialization
- stake + claim without unstake
- oracle-gated transfer behavior
- unstake and burn reward paths

### Run

```bash
yarn install
anchor build
anchor test --skip-local-validator
```

Notes:

- Tests use `surfnet_timeTravel`, so they require Surfnet-compatible RPC.
- If Surfnet or MPL Core runtime support is missing, tests will be skipped/pending.

## Dependencies

```toml
anchor-lang = "0.32.1"
anchor-spl = "0.32.1"
mpl-core = { version = "0.11.1", features = ["anchor"] }
```
