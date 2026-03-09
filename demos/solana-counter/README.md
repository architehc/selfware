# Solana Counter Demo

This demo showcases selfware autonomously building, testing, and deploying a Solana counter program using the Anchor framework -- from zero to deployed in a single prompt.

## What This Demo Does

Selfware receives a single prompt and autonomously:

1. Scaffolds an Anchor project
2. Writes a counter program with initialize, increment, and decrement instructions
3. Writes comprehensive tests
4. Builds the program
5. Deploys to a local Solana validator
6. Runs the test suite to verify everything works

## Prerequisites

- **Rust** (1.70+): https://rustup.rs
- **Node.js** (18+): https://nodejs.org
- **Yarn**: `npm install -g yarn`
- **Selfware**: `cargo install selfware` (or build from this repo)

## Quick Start

### 1. Install Solana and Anchor tooling

```bash
./setup.sh
```

This installs:
- Solana CLI (stable release via Anza)
- Anchor CLI (built from source)
- Generates a local keypair
- Starts a local validator and airdrops SOL

### 2. Run the demo

```bash
./run_demo.sh
```

Selfware will read `prompt.md` and execute every step autonomously. Watch as it generates the program, writes tests, builds, deploys, and verifies.

### 3. Inspect the output

After the demo completes, you will find a new `counter/` directory containing the full Anchor project. See [expected_output.md](expected_output.md) for what the generated project should look like.

## Step-by-Step Walkthrough

If you want to understand what selfware does at each stage:

### Stage 1 -- Project Scaffold
Selfware runs `anchor init counter` to create the standard Anchor project structure:
```
counter/
  Anchor.toml
  Cargo.toml
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

### Stage 2 -- Program Implementation
Selfware replaces the generated `lib.rs` with a counter program containing:
- A `Counter` account struct with a `count: u64` field and an `authority: Pubkey`
- `initialize` -- creates the counter account, sets count to 0
- `increment` -- increases count by 1
- `decrement` -- decreases count by 1, with a check to prevent underflow below 0

### Stage 3 -- Test Suite
Selfware writes TypeScript tests in `tests/counter.ts` that:
- Initialize a counter and verify count starts at 0
- Increment multiple times and verify the count
- Decrement and verify the count
- Attempt to decrement below 0 and verify it fails gracefully

### Stage 4 -- Build and Deploy
Selfware runs:
```bash
anchor build        # Compiles the program to BPF bytecode
anchor deploy       # Deploys to the running local validator
```

### Stage 5 -- Test Execution
Selfware runs:
```bash
anchor test --skip-local-validator   # Tests against the already-running validator
```

All tests should pass, confirming the program works end-to-end.

## Configuration

The demo creates a `selfware.toml` pointing at a local LM Studio endpoint. Edit `run_demo.sh` to change the model endpoint or parameters for your setup.

## Troubleshooting

| Problem | Solution |
|---------|----------|
| `solana: command not found` | Run `setup.sh` or add Solana to your PATH |
| `anchor: command not found` | Run `setup.sh` -- Anchor builds from source and takes a few minutes |
| Validator not running | `solana-test-validator &` then `sleep 5` |
| Insufficient SOL | `solana airdrop 100` (localnet only) |
| Build fails with BPF errors | Ensure you have the latest Solana CLI: `solana-install update` |
