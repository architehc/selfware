# Expected Output

After selfware completes the demo, the following artifacts should be produced.

## Project Structure

```
counter/
  Anchor.toml
  Cargo.toml
  package.json
  tsconfig.json
  programs/
    counter/
      Cargo.toml
      src/
        lib.rs
  tests/
    counter.ts
  migrations/
    deploy.ts
```

## Counter Program (`programs/counter/src/lib.rs`)

The generated program should contain something equivalent to:

```rust
use anchor_lang::prelude::*;

declare_id!("...");  // Auto-generated program ID

#[program]
pub mod counter {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let counter = &mut ctx.accounts.counter;
        counter.count = 0;
        counter.authority = ctx.accounts.authority.key();
        Ok(())
    }

    pub fn increment(ctx: Context<Update>) -> Result<()> {
        let counter = &mut ctx.accounts.counter;
        counter.count += 1;
        Ok(())
    }

    pub fn decrement(ctx: Context<Update>) -> Result<()> {
        let counter = &mut ctx.accounts.counter;
        require!(counter.count > 0, CounterError::AlreadyZero);
        counter.count -= 1;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 8 + 32,
    )]
    pub counter: Account<'info, Counter>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Update<'info> {
    #[account(mut, has_one = authority)]
    pub counter: Account<'info, Counter>,
    pub authority: Signer<'info>,
}

#[account]
pub struct Counter {
    pub count: u64,
    pub authority: Pubkey,
}

#[error_code]
pub enum CounterError {
    #[msg("Counter is already at zero")]
    AlreadyZero,
}
```

## Test File (`tests/counter.ts`)

The generated tests should cover:

```typescript
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Counter } from "../target/types/counter";
import { expect } from "chai";

describe("counter", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.Counter as Program<Counter>;
  const counter = anchor.web3.Keypair.generate();

  it("initializes the counter", async () => {
    await program.methods
      .initialize()
      .accounts({
        counter: counter.publicKey,
        authority: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([counter])
      .rpc();

    const account = await program.account.counter.fetch(counter.publicKey);
    expect(account.count.toNumber()).to.equal(0);
  });

  it("increments the counter", async () => {
    await program.methods
      .increment()
      .accounts({
        counter: counter.publicKey,
        authority: provider.wallet.publicKey,
      })
      .rpc();

    const account = await program.account.counter.fetch(counter.publicKey);
    expect(account.count.toNumber()).to.equal(1);
  });

  it("decrements the counter", async () => {
    await program.methods
      .decrement()
      .accounts({
        counter: counter.publicKey,
        authority: provider.wallet.publicKey,
      })
      .rpc();

    const account = await program.account.counter.fetch(counter.publicKey);
    expect(account.count.toNumber()).to.equal(0);
  });

  it("fails to decrement below zero", async () => {
    try {
      await program.methods
        .decrement()
        .accounts({
          counter: counter.publicKey,
          authority: provider.wallet.publicKey,
        })
        .rpc();
      expect.fail("should have thrown");
    } catch (err) {
      expect(err.toString()).to.include("AlreadyZero");
    }
  });
});
```

## Build Output

```
$ anchor build
BPF SDK: ~/.local/share/solana/install/active_release/bin/sdk/sbf
cargo-build-sbf child: rustup toolchain list -v
...
To deploy this program:
  $ solana program deploy /path/to/counter/target/deploy/counter.so
The program address will default to this keypair (override with --program-id):
  /path/to/counter/target/deploy/counter-keypair.json
```

## Deploy Output

```
$ anchor deploy
Deploying cluster: http://localhost:8899
Upgrade authority: /Users/.../.config/solana/id.json
Deploying program "counter"...
Program path: /path/to/counter/target/deploy/counter.so
Program Id: <PROGRAM_ID>

Deploy success
```

## Test Output

```
$ anchor test --skip-local-validator

  counter
    ✓ initializes the counter (420ms)
    ✓ increments the counter (415ms)
    ✓ decrements the counter (412ms)
    ✓ fails to decrement below zero (205ms)

  4 passing (1.5s)
```
