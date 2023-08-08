use eigentrust::{
	attestation::AttestationRaw,
	eth::{address_from_public_key, deploy_as, ecdsa_secret_from_mnemonic},
	Client, ClientConfig,
};
use ethers::{
	core::{types::TransactionRequest, utils::Anvil},
	prelude::{rand::Rng, SignerMiddleware},
	providers::{Http, Middleware, Provider},
	signers::{LocalWallet, Signer},
};
use ethers::{prelude::rand, types::Address};
use futures::future::join_all;
use secp256k1::Secp256k1;
use std::convert::TryFrom;

const TEST_MNEMONIC: &'static str =
	"sausage type strategy swift ability warm cheap cousin hamster best ignore bamboo";
const LOCAL_NODE_URL: &'static str = "http://localhost:8545";
const LOCAL_NODE_CHAIN_ID: &'static str = "31337";
const NEIGHBOURS: u8 = 128;

#[tokio::test]
async fn test_max_participants() {
	let anvil = Anvil::new().spawn();

	let provider = Provider::<Http>::try_from(LOCAL_NODE_URL.to_string()).unwrap();
	let provider_accounts = provider.get_accounts().await.unwrap();
	let funded_account = provider_accounts[0];

	let secp = Secp256k1::new();
	let gen_secret_keys = ecdsa_secret_from_mnemonic(TEST_MNEMONIC, NEIGHBOURS as u32).unwrap();
	let gen_addresses: Vec<Address> = gen_secret_keys
		.iter()
		.map(|secret| {
			let public_key = secp256k1::PublicKey::from_secret_key(&secp, secret);
			address_from_public_key(&public_key)
		})
		.collect();

	// Fund accounts
	for to in gen_addresses.clone() {
		let tx = TransactionRequest::new().to(to).value("100000000000000").from(funded_account);
		provider.send_transaction(tx, None).await.unwrap();
	}

	let mut config = ClientConfig {
		as_address: "0x5fbdb2315678afecb367f032d93f642f64180aa3".to_string(),
		band_id: "38922764296632428858395574229367".to_string(),
		band_th: "500".to_string(),
		band_url: "http://localhost:3000".to_string(),
		chain_id: LOCAL_NODE_CHAIN_ID.to_string(),
		domain: "0x0000000000000000000000000000000000000000".to_string(),
		node_url: LOCAL_NODE_URL.to_string(),
	};
	let mut client = Client::new(config.clone(), TEST_MNEMONIC.to_string());

	// Deploy attestation station
	let as_address = deploy_as(client.get_signer()).await.unwrap();
	config.as_address = format!("{:?}", as_address);
	client.set_config(config);

	let domain_input = [
		0xff, 0x61, 0x4a, 0x6d, 0x59, 0x56, 0x2a, 0x42, 0x37, 0x72, 0x37, 0x76, 0x32, 0x4d, 0x36,
		0x53, 0x62, 0x6d, 0x35, 0xff,
	];

	// Tasks: 128*128 = 16384
	// These are a lot of tasks to be handled at once by futures.
	// So let's do them in 128 batches of 128.

	for i in 0..2 {
		let mut tasks = Vec::with_capacity(NEIGHBOURS.into());

		for j in 0..NEIGHBOURS {
			if i == j {
				continue;
			}

			let mut rng = rand::thread_rng();
			let score: u8 = rng.gen::<u8>() / 32;

			let attestation = AttestationRaw::new(
				gen_addresses[j as usize].to_fixed_bytes(),
				domain_input,
				score,
				[0; 32],
			);

			let pk_string: String = gen_secret_keys[j as usize].display_secret().to_string();

			// Setup wallet
			let wallet: LocalWallet = pk_string.parse().unwrap();

			// Setup signer
			let signer = SignerMiddleware::new(
				provider.clone(),
				wallet.with_chain_id(LOCAL_NODE_CHAIN_ID.parse::<u64>().unwrap()),
			);

			let mut cloned_client = client.clone();
			cloned_client.set_signer(signer);

			let handle = async move { cloned_client.attest(attestation, j as u32).await };

			tasks.push(handle);
		}

		let outputs = join_all(tasks).await;

		for res in &outputs {
			match res {
				Ok(_) => (),
				Err(e) => println!("Task failed with error: {:?}", e),
			}
		}
	}

	let attestations = client.get_attestations().await.unwrap();

	println!("Attestations: {:#?}", attestations.len());

	let scores = client.calculate_scores(attestations).await.unwrap();

	println!("Scores: {:#?}", scores);

	drop(anvil);
}
