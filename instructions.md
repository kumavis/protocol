# Many Participants Test

This test is designed to test the protocol with many participants. It is a simulation of a real-world scenario where there are many peers in the network.

## Requirements

### Environment

- [Rust](https://www.rust-lang.org/tools/install)
- [Solidity Compiler Version Manager](https://github.com/alloy-rs/svm-rs)

```bash
cargo install svm-rs
svm install 0.8.17
```

### Wallet

You need a wallet to submit attestations and sign them. Please use a new one since this is a test. You can download the [Metamask](https://metamask.io/) extension for your browser and create a new wallet.

Go to `settings -> privacy-> display seed phrase`. Once you have it, setup your `.env`:

```bash
MNEMONIC="patrol harsh coast shoot crisp cheap ... ... ... ... ... ..."
```

**Don't send funds to this wallet**

## Submitting Attestations

1. Open a new terminal and install this project:

```bash
git clone git@github.com:eigen-trust/protocol.git
cd protocol
```

2. Build the release version of the crate so we can run it from the `target` directory:

```bash
cargo build --release
```

3. Update the configuration to use our shared anvil instance.

```bash
./target/release/eigentrust-cli update --node http://34.230.44.97:3000
```

4. Submit an attestation. In this step you should attest to different peers in each attestation, to add new members to the set.

```bash
./target/release/eigentrust-cli attest --to 0x70997970C51812dc3A010C7d01b50e0d17dc79C8 --score 5
```

5. Calculate the scores

```bash
./target/release/eigentrust-cli scores
```
