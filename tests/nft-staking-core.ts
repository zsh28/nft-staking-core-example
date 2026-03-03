import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { NftStakingCore } from "../target/types/nft_staking_core";
import { SystemProgram } from "@solana/web3.js";
import { MPL_CORE_PROGRAM_ID } from "@metaplex-foundation/mpl-core";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { assert } from "chai";

const MILLISECONDS_PER_DAY = 86_400_000;
const POINTS_PER_STAKED_NFT_PER_DAY = 10_000_000;
const FREEZE_PERIOD_IN_DAYS = 7;
const ORACLE_OPEN_HOUR = 9;
const ORACLE_CLOSE_HOUR = 17;
const ORACLE_BOUNDARY_WINDOW_SECONDS = 600;
const CRANK_REWARD_LAMPORTS = 1_000_000;
const INITIAL_ORACLE_VAULT_LAMPORTS = 2_000_000;
const MPL_CORE_PUBKEY = new anchor.web3.PublicKey(
  MPL_CORE_PROGRAM_ID.toString()
);

describe("nft-staking-core", function () {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.nftStakingCore as Program<NftStakingCore>;

  const collectionKeypair = anchor.web3.Keypair.generate();
  const nftTransferKeypair = anchor.web3.Keypair.generate();
  const nftBurnKeypair = anchor.web3.Keypair.generate();
  const recipient = anchor.web3.Keypair.generate();
  const cranker = anchor.web3.Keypair.generate();

  const updateAuthority = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("update_authority"), collectionKeypair.publicKey.toBuffer()],
    program.programId
  )[0];

  const config = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("config"), collectionKeypair.publicKey.toBuffer()],
    program.programId
  )[0];

  const rewardsMint = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("rewards"), config.toBuffer()],
    program.programId
  )[0];

  const oracleState = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("oracle_state"), collectionKeypair.publicKey.toBuffer()],
    program.programId
  )[0];

  const oracleVault = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("oracle_vault"), collectionKeypair.publicKey.toBuffer()],
    program.programId
  )[0];

  const providerRewardsAta = getAssociatedTokenAddressSync(
    rewardsMint,
    provider.wallet.publicKey,
    false,
    TOKEN_PROGRAM_ID,
    ASSOCIATED_TOKEN_PROGRAM_ID
  );

  const recipientRewardsAta = getAssociatedTokenAddressSync(
    rewardsMint,
    recipient.publicKey,
    false,
    TOKEN_PROGRAM_ID,
    ASSOCIATED_TOKEN_PROGRAM_ID
  );

  async function advanceTime(params: {
    absoluteEpoch?: number;
    absoluteSlot?: number;
    absoluteTimestamp?: number;
  }): Promise<void> {
    const rpcResponse = await fetch(provider.connection.rpcEndpoint, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "surfnet_timeTravel",
        params: [params],
      }),
    });

    const result = (await rpcResponse.json()) as { error?: any };
    if (result.error) {
      throw new Error(`Time travel failed: ${JSON.stringify(result.error)}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }

  async function currentUnixMillis(): Promise<number> {
    const slot = await provider.connection.getSlot("confirmed");
    const blockTime = await provider.connection.getBlockTime(slot);
    const unixSeconds = blockTime ?? Math.floor(Date.now() / 1000);
    return unixSeconds * 1000;
  }

  async function travelForwardDays(days: number): Promise<void> {
    const nowMs = await currentUnixMillis();
    await advanceTime({
      absoluteTimestamp: nowMs + days * MILLISECONDS_PER_DAY,
    });
  }

  function nextUtcHourAfter(baseTimestampMs: number, hourUtc: number): number {
    const baseDate = new Date(baseTimestampMs);
    let target = Date.UTC(
      baseDate.getUTCFullYear(),
      baseDate.getUTCMonth(),
      baseDate.getUTCDate(),
      hourUtc,
      0,
      0,
      0
    );
    if (target <= baseTimestampMs) {
      target += MILLISECONDS_PER_DAY;
    }
    return target;
  }

  async function tokenBalanceOrZero(
    pubkey: anchor.web3.PublicKey
  ): Promise<number> {
    try {
      const balance = await provider.connection.getTokenAccountBalance(pubkey);
      return Number(balance.value.amount);
    } catch {
      return 0;
    }
  }

  before(async function () {
    const mplCoreInfo = await provider.connection.getAccountInfo(
      MPL_CORE_PUBKEY
    );
    if (!mplCoreInfo?.executable) {
      this.skip();
      return;
    }

    const rpcResponse = await fetch(provider.connection.rpcEndpoint, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "surfnet_timeTravel",
        params: [{ absoluteTimestamp: Date.now() + 1000 }],
      }),
    });

    const result = (await rpcResponse.json()) as { error?: any };
    if (result.error) {
      this.skip();
    }
  });

  it("Creates collection and initializes config/oracle", async () => {
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(
        recipient.publicKey,
        2 * anchor.web3.LAMPORTS_PER_SOL
      )
    );
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(
        cranker.publicKey,
        2 * anchor.web3.LAMPORTS_PER_SOL
      )
    );

    await program.methods
      .createCollection("Test Collection", "https://example.com/collection")
      .accountsPartial({
        payer: provider.wallet.publicKey,
        collection: collectionKeypair.publicKey,
        updateAuthority,
        systemProgram: SystemProgram.programId,
        mplCoreProgram: MPL_CORE_PUBKEY,
      })
      .signers([collectionKeypair])
      .rpc();

    await program.methods
      .initializeConfig(POINTS_PER_STAKED_NFT_PER_DAY, FREEZE_PERIOD_IN_DAYS)
      .accountsPartial({
        admin: provider.wallet.publicKey,
        collection: collectionKeypair.publicKey,
        updateAuthority,
        config,
        rewardsMint,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    await program.methods
      .initializeOracle(
        ORACLE_OPEN_HOUR,
        ORACLE_CLOSE_HOUR,
        new anchor.BN(ORACLE_BOUNDARY_WINDOW_SECONDS),
        new anchor.BN(CRANK_REWARD_LAMPORTS),
        new anchor.BN(INITIAL_ORACLE_VAULT_LAMPORTS)
      )
      .accountsPartial({
        admin: provider.wallet.publicKey,
        collection: collectionKeypair.publicKey,
        updateAuthority,
        oracleState,
        oracleVault,
        mplCoreProgram: MPL_CORE_PUBKEY,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  });

  it("Mints two NFTs and stakes both", async () => {
    await program.methods
      .mintNft("Transfer NFT", "https://example.com/transfer")
      .accountsPartial({
        user: provider.wallet.publicKey,
        nft: nftTransferKeypair.publicKey,
        collection: collectionKeypair.publicKey,
        updateAuthority,
        systemProgram: SystemProgram.programId,
        mplCoreProgram: MPL_CORE_PUBKEY,
      })
      .signers([nftTransferKeypair])
      .rpc();

    await program.methods
      .mintNft("Burn NFT", "https://example.com/burn")
      .accountsPartial({
        user: provider.wallet.publicKey,
        nft: nftBurnKeypair.publicKey,
        collection: collectionKeypair.publicKey,
        updateAuthority,
        systemProgram: SystemProgram.programId,
        mplCoreProgram: MPL_CORE_PUBKEY,
      })
      .signers([nftBurnKeypair])
      .rpc();

    await program.methods
      .stake()
      .accountsPartial({
        user: provider.wallet.publicKey,
        updateAuthority,
        config,
        nft: nftTransferKeypair.publicKey,
        collection: collectionKeypair.publicKey,
        systemProgram: SystemProgram.programId,
        mplCoreProgram: MPL_CORE_PUBKEY,
      })
      .rpc();

    await program.methods
      .stake()
      .accountsPartial({
        user: provider.wallet.publicKey,
        updateAuthority,
        config,
        nft: nftBurnKeypair.publicKey,
        collection: collectionKeypair.publicKey,
        systemProgram: SystemProgram.programId,
        mplCoreProgram: MPL_CORE_PUBKEY,
      })
      .rpc();
  });

  it("Claims rewards without unstaking", async () => {
    await travelForwardDays(2);

    const before = await tokenBalanceOrZero(providerRewardsAta);

    await program.methods
      .claimRewards()
      .accountsPartial({
        user: provider.wallet.publicKey,
        updateAuthority,
        config,
        rewardsMint,
        userRewardsAta: providerRewardsAta,
        nft: nftTransferKeypair.publicKey,
        collection: collectionKeypair.publicKey,
        mplCoreProgram: MPL_CORE_PUBKEY,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .rpc();

    const after = await tokenBalanceOrZero(providerRewardsAta);
    assert.isAbove(after, before, "claim_rewards should mint rewards");
  });

  it("Blocks transfer outside time window and allows after cranking into window", async () => {
    const now = await currentUnixMillis();
    const outsideWindow = nextUtcHourAfter(now, 3);
    await advanceTime({ absoluteTimestamp: outsideWindow });

    await program.methods
      .crankOracle()
      .accountsPartial({
        caller: provider.wallet.publicKey,
        collection: collectionKeypair.publicKey,
        oracleState,
        oracleVault,
      })
      .rpc();

    let failed = false;
    try {
      await program.methods
        .transferNft()
        .accountsPartial({
          user: provider.wallet.publicKey,
          newOwner: recipient.publicKey,
          nft: nftTransferKeypair.publicKey,
          collection: collectionKeypair.publicKey,
          updateAuthority,
          oracleState,
          mplCoreProgram: MPL_CORE_PUBKEY,
          systemProgram: SystemProgram.programId,
        })
        .rpc();
    } catch {
      failed = true;
    }
    assert.isTrue(
      failed,
      "transfer should fail when oracle is rejecting transfers"
    );

    const atBoundary = nextUtcHourAfter(outsideWindow, ORACLE_OPEN_HOUR);
    await advanceTime({ absoluteTimestamp: atBoundary });

    const crankBefore = await provider.connection.getBalance(cranker.publicKey);
    await program.methods
      .crankOracle()
      .accountsPartial({
        caller: cranker.publicKey,
        collection: collectionKeypair.publicKey,
        oracleState,
        oracleVault,
      })
      .signers([cranker])
      .rpc();
    const crankAfter = await provider.connection.getBalance(cranker.publicKey);

    assert.equal(
      crankAfter - crankBefore,
      CRANK_REWARD_LAMPORTS,
      "cranker should receive boundary reward"
    );

    await program.methods
      .transferNft()
      .accountsPartial({
        user: provider.wallet.publicKey,
        newOwner: recipient.publicKey,
        nft: nftTransferKeypair.publicKey,
        collection: collectionKeypair.publicKey,
        updateAuthority,
        oracleState,
        mplCoreProgram: MPL_CORE_PUBKEY,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  });

  it("Unstakes transferred NFT as recipient and burns second staked NFT", async () => {
    await travelForwardDays(8);

    await program.methods
      .unstake()
      .accountsPartial({
        user: recipient.publicKey,
        updateAuthority,
        config,
        rewardsMint,
        userRewardsAta: recipientRewardsAta,
        nft: nftTransferKeypair.publicKey,
        collection: collectionKeypair.publicKey,
        mplCoreProgram: MPL_CORE_PUBKEY,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .signers([recipient])
      .rpc();

    const beforeBurn = await tokenBalanceOrZero(providerRewardsAta);

    await program.methods
      .burnStakedNft()
      .accountsPartial({
        user: provider.wallet.publicKey,
        updateAuthority,
        config,
        rewardsMint,
        userRewardsAta: providerRewardsAta,
        nft: nftBurnKeypair.publicKey,
        collection: collectionKeypair.publicKey,
        mplCoreProgram: MPL_CORE_PUBKEY,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .rpc();

    const afterBurn = await tokenBalanceOrZero(providerRewardsAta);
    assert.isAbove(
      afterBurn,
      beforeBurn,
      "burn_staked_nft should mint bonus rewards"
    );
  });
});
