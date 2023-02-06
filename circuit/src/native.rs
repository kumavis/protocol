use std::collections::HashMap;

use crate::eddsa::native::{PublicKey, Signature};
use halo2::{
	arithmetic::Field,
	halo2curves::{bn256::Fr, FieldExt},
};

const NUM_NEIGHBOURS: usize = 6;
const NUM_ITERATIONS: usize = 20;
const INITIAL_SCORE: u128 = 1000;

struct Opinion {
	sig: Signature,
	message_hash: Fr,
	scores: [Fr; NUM_NEIGHBOURS],
}

impl Opinion {
	pub fn new(sig: Signature, message_hash: Fr, scores: [Fr; NUM_NEIGHBOURS]) -> Self {
		Self { sig, message_hash, scores }
	}
}

struct EigenTrustSet {
	set: [(PublicKey, Fr); NUM_NEIGHBOURS],
	ops: HashMap<PublicKey, Opinion>,
}

impl EigenTrustSet {
	pub fn new() -> Self {
		Self { set: [(PublicKey::default(), Fr::zero()); NUM_NEIGHBOURS], ops: HashMap::new() }
	}

	pub fn add_member(&mut self, pk: PublicKey) {
		let pos = self.set.iter().position(|&(x, _)| x == PublicKey::default());
		let first_available = self.set.iter().position(|&(x, _)| x == PublicKey::default());
		// Make sure not already in the set
		assert!(pos.is_none());

		let initial_score = Fr::from_u128(INITIAL_SCORE);
		let index = first_available.unwrap();
		self.set[index] = (pk, initial_score);
	}

	pub fn update_op(&mut self, from: PublicKey, op: Opinion) {
		let pos_from = self.set.iter().position(|&(x, _)| x == from);
		assert!(pos_from.is_some());
		self.ops.insert(from, op);
	}

	pub fn converge(&self) -> [Fr; NUM_NEIGHBOURS] {
		let mut s = self.set.map(|item| item.1);
		for _ in 0..NUM_ITERATIONS {
			let mut distributions = [[Fr::zero(); NUM_NEIGHBOURS]; NUM_NEIGHBOURS];
			for i in 0..NUM_NEIGHBOURS {
				let mut local_distr = [Fr::zero(); NUM_NEIGHBOURS];
				let ops_i = self.ops.get(&self.set[i].0).unwrap();
				for j in 0..NUM_NEIGHBOURS {
					let op = ops_i.scores[j] * s[i];
					local_distr[j] = op;
				}
				distributions[i] = local_distr;
			}

			let mut new_s = [Fr::zero(); NUM_NEIGHBOURS];
			for i in 0..NUM_NEIGHBOURS {
				for j in 0..NUM_NEIGHBOURS {
					new_s[i] += distributions[j][i];
				}
			}

			s = new_s;
		}

		println!("new s: {:?}", s);

		let mut sum = Fr::zero();
		for x in s.iter() {
			sum += x;
		}
		println!("sum: {:?}", sum);

		s
	}
}

#[cfg(test)]
mod test {
	use super::{EigenTrustSet, Opinion, NUM_NEIGHBOURS};
	use crate::{
		calculate_message_hash,
		eddsa::native::{sign, PublicKey, SecretKey},
	};
	use rand::thread_rng;

	#[test]
	fn test_add_member_in_initial_set() {}
	#[test]
	fn test_add_two_members_without_opinions() {}
	#[test]
	#[should_panic]
	fn test_add_two_members_with_one_opinion() {
		let mut set = EigenTrustSet::new();

		let rng = &mut thread_rng();

		let sk1 = SecretKey::random(rng);
		let sk2 = SecretKey::random(rng);

		let pk1 = sk1.public();
		let pk2 = sk2.public();

		set.add_member(pk1);
		set.add_member(pk2);

		let pks = [PublicKey::default(); NUM_NEIGHBOURS];
		let scores = [Fr::zero(); NUM_NEIGHBOURS];
		let (_, message_hashes) = calculate_message_hash(pks.to_vec(), vec![scores.to_vec()]);
		let sig = sign(&sk1, &pk1, message_hashes[0]);

		let op = Opinion::new(sig, message_hashes[0], scores);

		set.update_op(pk1, op);
		set.converge();
	}
	#[test]
	fn test_add_two_members_with_opinions() {}
	#[test]
	fn test_add_three_members_with_opinions() {}
	#[test]
	fn test_add_three_members_with_two_opinions() {}
}
