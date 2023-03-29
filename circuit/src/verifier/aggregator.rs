use super::{
	gen_pk,
	loader::{
		native::{NUM_BITS, NUM_LIMBS},
		Halo2LScalar, LoaderConfig,
	},
	transcript::{
		native::{PoseidonRead, PoseidonWrite, WIDTH},
		PoseidonReadChipset,
	},
};
use crate::{
	circuit::{FullRoundHasher, PartialRoundHasher},
	ecc::{
		EccAddConfig, EccDoubleConfig, EccMulConfig, EccTableSelectConfig, EccUnreducedLadderConfig,
	},
	gadgets::{
		absorb::AbsorbChip,
		bits2num::Bits2NumChip,
		main::{MainChip, MainConfig},
	},
	integer::{
		native::Integer, rns::Bn256_4_68, IntegerAddChip, IntegerDivChip, IntegerMulChip,
		IntegerReduceChip, IntegerSubChip,
	},
	params::poseidon_bn254_5x5::Params,
	poseidon::{sponge::PoseidonSpongeConfig, PoseidonConfig},
	Chip, CommonConfig, RegionCtx,
};
use halo2::{
	circuit::{Layouter, Region, SimpleFloorPlanner, Value},
	halo2curves::bn256::{Bn256, Fq, Fr, G1Affine, G1},
	plonk::{create_proof, Circuit, ConstraintSystem, Error},
	poly::{
		commitment::ParamsProver,
		kzg::{
			commitment::{KZGCommitmentScheme, ParamsKZG},
			multiopen::ProverGWC,
		},
	},
	transcript::{TranscriptReadBuffer, TranscriptWriterBuffer},
};
use rand::{thread_rng, RngCore};
use snark_verifier::{
	pcs::{
		kzg::{Gwc19, KzgAccumulator, KzgAs, KzgSuccinctVerifyingKey},
		AccumulationScheme, AccumulationSchemeProver,
	},
	system::halo2::{compile, Config},
	verifier::{
		plonk::{PlonkProtocol, PlonkSuccinctVerifier},
		SnarkVerifier,
	},
};
use std::{rc::Rc, sync::Mutex};

type PSV = PlonkSuccinctVerifier<KzgAs<Bn256, Gwc19>>;
type SVK = KzgSuccinctVerifyingKey<G1Affine>;

#[derive(Clone)]
// TODO: Make SnarkWitness and functions to convert from Snark to SnarkWitness,
// without witness function
// pub struct Snark {
// 	protocol: PlonkProtocol<G1Affine>,
// 	instances: Vec<Vec<Value<Fr>>>,
// 	proof: Value<Vec<u8>>,
// }

/// Snark witness structure
pub struct Snark {
	protocol: PlonkProtocol<G1Affine>,
	instances: Vec<Vec<Fr>>,
	proof: Vec<u8>,
}

impl Snark {
	fn new<C: Circuit<Fr>, R: RngCore>(
		params: &ParamsKZG<Bn256>, circuit: C, instances: Vec<Vec<Fr>>, rng: &mut R,
	) -> Self {
		let pk = gen_pk(params, &circuit);
		let protocol = compile(
			params,
			pk.get_vk(),
			Config::kzg().with_num_instance(vec![1]),
		);

		let instances_slice: Vec<&[Fr]> = instances.iter().map(|x| x.as_slice()).collect();
		let mut transcript = PoseidonWrite::<_, G1Affine, Bn256_4_68, Params>::new(Vec::new());
		create_proof::<KZGCommitmentScheme<Bn256>, ProverGWC<_>, _, _, _, _>(
			params,
			&pk,
			&[circuit],
			&[instances_slice.as_slice()],
			rng,
			&mut transcript,
		)
		.unwrap();
		let proof = transcript.finalize();
		Self { protocol, instances, proof }
	}
}

struct Aggregator {
	svk: SVK,
	snarks: Vec<Snark>,
	instances: Vec<Fr>,
	as_proof: Vec<u8>,
}

impl Clone for Aggregator {
	fn clone(&self) -> Self {
		Self {
			svk: self.svk.clone(),
			snarks: self.snarks.clone(),
			instances: self.instances.clone(),
			as_proof: self.as_proof.clone(),
		}
	}
}

impl Aggregator {
	pub fn new(params: &ParamsKZG<Bn256>, snarks: Vec<Snark>) -> Self {
		let svk = params.get_g()[0].into();

		let mut plonk_proofs = Vec::new();
		for snark in &snarks {
			let mut transcript_read: PoseidonRead<_, G1Affine, Bn256_4_68, Params> =
				PoseidonRead::init(snark.proof.as_slice());
			let proof = PSV::read_proof(
				&svk, &snark.protocol, &snark.instances, &mut transcript_read,
			)
			.unwrap();
			let res = PSV::verify(&svk, &snark.protocol, &snark.instances, &proof).unwrap();
			plonk_proofs.extend(res);
		}

		let mut transcript_write =
			PoseidonWrite::<Vec<u8>, G1Affine, Bn256_4_68, Params>::new(Vec::new());
		let rng = &mut thread_rng();
		let accumulator = KzgAs::<Bn256, Gwc19>::create_proof(
			&Default::default(),
			&plonk_proofs,
			&mut transcript_write,
			rng,
		)
		.unwrap();
		let as_proof = transcript_write.finalize();

		let KzgAccumulator { lhs, rhs } = accumulator;
		let instances = [lhs.x, lhs.y, rhs.x, rhs.y]
			.map(|v| Integer::<_, _, NUM_LIMBS, NUM_BITS, Bn256_4_68>::from_w(v).limbs)
			.concat();

		Self { svk, snarks, instances, as_proof }
	}
}

#[derive(Clone)]
struct AggregatorConfig {
	pub(crate) common: CommonConfig,
	pub(crate) main: MainConfig,
	pub(crate) poseidon_sponge: PoseidonSpongeConfig,
	pub(crate) ecc_mul_scalar: EccMulConfig,
}

impl Circuit<Fr> for Aggregator {
	type Config = AggregatorConfig;
	type FloorPlanner = SimpleFloorPlanner;

	fn without_witnesses(&self) -> Self {
		Self {
			svk: Value::<KzgSuccinctVerifyingKey<G1Affine>>::unknown(),
			snarks: Value::unknown(),
			instances: Value::unknown(),
			as_proof: Value::unknown(),
		}
		// TODO: Return Value::unknown() for each value, after Implementing
		// SnarkWitness Self::clone(self)
	}

	/// Configure
	fn configure(meta: &mut ConstraintSystem<Fr>) -> Self::Config {
		let common = CommonConfig::new(meta);
		let main_selector = MainChip::configure(&common, meta);
		let main = MainConfig::new(main_selector);

		let full_round_selector = FullRoundHasher::configure(&common, meta);
		let partial_round_selector = PartialRoundHasher::configure(&common, meta);
		let poseidon = PoseidonConfig::new(full_round_selector, partial_round_selector);

		let absorb_selector = AbsorbChip::<Fr, WIDTH>::configure(&common, meta);
		let poseidon_sponge = PoseidonSpongeConfig::new(poseidon.clone(), absorb_selector);

		let bits2num = Bits2NumChip::configure(&common, meta);

		let int_red =
			IntegerReduceChip::<Fq, Fr, NUM_LIMBS, NUM_BITS, Bn256_4_68>::configure(&common, meta);
		let int_add =
			IntegerAddChip::<Fq, Fr, NUM_LIMBS, NUM_BITS, Bn256_4_68>::configure(&common, meta);
		let int_sub =
			IntegerSubChip::<Fq, Fr, NUM_LIMBS, NUM_BITS, Bn256_4_68>::configure(&common, meta);
		let int_mul =
			IntegerMulChip::<Fq, Fr, NUM_LIMBS, NUM_BITS, Bn256_4_68>::configure(&common, meta);
		let int_div =
			IntegerDivChip::<Fq, Fr, NUM_LIMBS, NUM_BITS, Bn256_4_68>::configure(&common, meta);

		let ladder = EccUnreducedLadderConfig::new(int_add, int_sub, int_mul, int_div);
		let add = EccAddConfig::new(int_red, int_sub, int_mul, int_div);
		let double = EccDoubleConfig::new(int_red, int_add, int_sub, int_mul, int_div);
		let table_select = EccTableSelectConfig::new(main.clone());
		let ecc_mul_scalar = EccMulConfig::new(ladder, add, double, table_select, bits2num);

		AggregatorConfig { common, main, poseidon_sponge, ecc_mul_scalar }
	}

	/// Synthesize the circuit.
	fn synthesize(
		&self, config: Self::Config, mut layouter: impl Layouter<Fr>,
	) -> Result<(), Error> {
		let layouter_rc = Rc::new(Mutex::new(layouter.namespace(|| "loader")));
		let loader_config = LoaderConfig::<G1Affine, _, Bn256_4_68>::new(
			layouter_rc, config.common, config.ecc_mul_scalar, config.main, config.poseidon_sponge,
		);

		let mut accumulators = Vec::new();
		for snark in self.snarks {
			let protocol = snark.protocol.loaded(&loader_config);
			let mut transcript_read: PoseidonReadChipset<&[u8], G1Affine, _, Bn256_4_68, Params> =
				PoseidonReadChipset::new(snark.proof.as_slice(), loader_config);
			let mut instances: Vec<Vec<Halo2LScalar<G1Affine, _, Bn256_4_68>>> = Vec::new();
			layouter.assign_region(
				|| "assign_instances",
				|region: Region<'_, Fr>| {
					let mut ctx = RegionCtx::new(region, 0);
					for i in 0..snark.instances.len() {
						for j in 0..snark.instances[i].len() {
							let assigned = ctx.assign_advice(
								config.common.advice[0],
								Value::known(snark.instances[i][j]),
							)?;
							// Check the correctness of the ctx.next()
							ctx.next();
							let new = Halo2LScalar::new(assigned, loader_config);
							instances[i].push(new);
						}
					}
					Ok(())
				},
			);

			let proof = PlonkSuccinctVerifier::read_proof(
				&self.svk, &protocol, &instances, &mut transcript_read,
			)
			.unwrap();

			let res =
				PlonkSuccinctVerifier::verify(&self.svk, &protocol, &instances, &proof).unwrap();

			accumulators.extend(res);
		}

		let mut transcript: PoseidonReadChipset<&[u8], G1Affine, _, Bn256_4_68, Params> =
			PoseidonReadChipset::new(&self.as_proof, loader_config);
		let proof =
			KzgAs::<Bn256, Gwc19>::read_proof(&Default::default(), &accumulators, &mut transcript)
				.unwrap();
		let accumulator =
			KzgAs::<Bn256, Gwc19>::verify(&Default::default(), &accumulators, &proof).unwrap();

		let lhs_x = accumulator.lhs.inner.x;
		let lhs_y = accumulator.lhs.inner.y;

		let rhs_x = accumulator.rhs.inner.x;
		let rhs_y = accumulator.rhs.inner.y;

		let mut row = 0;
		for limb in lhs_x.limbs {
			layouter.constrain_instance(limb.cell(), config.common.instance, row)?;
			row += 1;
		}
		for limb in lhs_y.limbs {
			layouter.constrain_instance(limb.cell(), config.common.instance, row)?;
			row += 1;
		}
		for limb in rhs_x.limbs {
			layouter.constrain_instance(limb.cell(), config.common.instance, row)?;
			row += 1;
		}
		for limb in rhs_y.limbs {
			layouter.constrain_instance(limb.cell(), config.common.instance, row)?;
			row += 1;
		}
		Ok(())
	}
}
