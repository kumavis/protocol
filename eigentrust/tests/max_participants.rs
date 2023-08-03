use eigentrust::{
	attestation::AttestationRaw,
	eth::{address_from_public_key, deploy_as, ecdsa_secret_from_mnemonic},
	Client, ClientConfig,
};
use ethers::{
	core::{types::TransactionRequest, utils::Anvil},
	prelude::rand::Rng,
	providers::{Http, Middleware, Provider},
};
use ethers::{prelude::rand, types::Address};
use secp256k1::Secp256k1;
use std::convert::TryFrom;

const FUNDS_MNEMONIC: &'static str = "test test test test test test test test test test test junk";
const TEST_MNEMONIC: &'static str =
	"sausage type strategy swift ability warm cheap cousin hamster best ignore bamboo";

#[tokio::test]
async fn test_get_attestations() {
	let anvil = Anvil::new().spawn();

	let provider = Provider::<Http>::try_from("http://localhost:8545".to_string()).unwrap();
	let provider_accounts = provider.get_accounts().await.unwrap();
	let from = provider_accounts[0];

	let secp = Secp256k1::new();
	let generated_accounts = ecdsa_secret_from_mnemonic(TEST_MNEMONIC, 128).unwrap();
	let new_accounts_addresses: Vec<Address> = generated_accounts
		.iter()
		.map(|secret| {
			let public_key = secp256k1::PublicKey::from_secret_key(&secp, secret);
			address_from_public_key(&public_key)
		})
		.collect();

	for to in new_accounts_addresses.clone() {
		let tx = TransactionRequest::new().to(to).value(1000000).from(from);
		provider.send_transaction(tx, None).await.unwrap();
	}

	let config = ClientConfig {
		as_address: "0x5fbdb2315678afecb367f032d93f642f64180aa3".to_string(),
		band_id: "38922764296632428858395574229367".to_string(),
		band_th: "500".to_string(),
		band_url: "http://localhost:3000".to_string(),
		chain_id: "31337".to_string(),
		domain: "0x0000000000000000000000000000000000000000".to_string(),
		node_url: "http://localhost:8545".to_string(),
	};
	let client = Client::new(config, FUNDS_MNEMONIC.to_string());

	// Deploy attestation station
	let as_address = deploy_as(client.get_signer()).await.unwrap();

	// Update config with new addresses and instantiate client
	let config = ClientConfig {
		as_address: format!("{:?}", as_address),
		band_id: "38922764296632428858395574229367".to_string(),
		band_th: "500".to_string(),
		band_url: "http://localhost:3000".to_string(),
		chain_id: "31337".to_string(),
		domain: "0x0000000000000000000000000000000000000000".to_string(),
		node_url: "http://localhost:8545".to_string(),
	};
	let client = Client::new(config, FUNDS_MNEMONIC.to_string());

	let domain_input = [
		0xff, 0x61, 0x4a, 0x6d, 0x59, 0x56, 0x2a, 0x42, 0x37, 0x72, 0x37, 0x76, 0x32, 0x4d, 0x36,
		0x53, 0x62, 0x6d, 0x35, 0xff,
	];

	let mut attestation_futures = Vec::new();

	for (index, attested) in new_accounts_addresses.iter().enumerate() {
		for n in 0..128 {
			if n == index {
				continue;
			}

			let mut rng = rand::thread_rng();
			let value: u8 = rng.gen::<u8>() / 128;
			let attestation =
				AttestationRaw::new(attested.to_fixed_bytes(), domain_input, value, [0; 32]);

			let client_clone = client.clone();
			let future = tokio::spawn(async move {
				client_clone.attest(attestation, n as u32).await.unwrap();
			});

			attestation_futures.push(future);
		}
	}

	for future in attestation_futures {
		match future.await {
			Ok(result) => {
				println!("result: {:#?}", result);
			},
			Err(_) => {},
		}
	}

	let attestations = client.get_attestations().await.unwrap();

	println!("attestations: {:#?}", attestations);

	let scores = client.calculate_scores(attestations).await.unwrap();

	println!("scores: {:#?}", scores);

	drop(anvil);
}
