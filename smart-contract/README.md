# Marketify Anchor program

This folder contains the Anchor program source for the Marketify marketplace.

Quick steps to build and generate the IDL (requires Rust + Anchor):

1. Install Rust and the Solana toolchain. See https://docs.solana.com/cli/install-solana-cli-tools
2. Install Anchor CLI (requires npm and Rust):

   npm install -g @project-serum/anchor-cli

3. From the repository root run the provided PowerShell script (Windows):

   pwsh .\scripts\build-anchor.ps1

4. The generated IDL will be placed at `smart-contract/target/idl/marketify.json` after a successful build. Copy that file to the frontend (example: `app/anchor/marketify.json`) so the client can use the Anchor IDL.

Notes:
- The program ID in `Anchor.toml` is already set to the on-chain ID used by the frontend: `FSoNXDpgsYZkp3VtPjPWPR2cQ5PMPt16SmLFm75A7FYh`.
- If you want to use a different cluster, update `Anchor.toml` `provider.cluster` accordingly.
