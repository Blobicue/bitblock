# CONSENSUS-D1: deterministic-finality PoS BFT and DA/retention decision v1

This decision record resolves `PS-053:CONSENSUS-D1@base=BOUNDED` and
`PS-054:CONSENSUS-D1@mechanism-choice=BOUNDED`, conditioned on `VALIDATOR-ECON-D1` (`CE-04`).

## Provenance

- **Merged `origin/main` subject (PR #26):** commit `ac503dce78ffb3951ea489ddb1d2cc35f4a1f178`
  (tree `950e59927c847833892b9b2debfe64b276227ae3`, CI runs `45009961375` & `45009964425`).
- **Executable contract:** [`crates/research/src/consensus_gov_d1.rs`](../crates/research/src/consensus_gov_d1.rs).
- **Independent Python vectors:** [`tools/pre_genesis_d1_vectors.py`](../tools/pre_genesis_d1_vectors.py).
- **CI integration & mutation evidence:** [`tests/pre_genesis_d1_contracts.rs`](../tests/pre_genesis_d1_contracts.rs)
  and `python3 tools/bbmutate.py consensus_gov_d1` (`15/15` sites KILLED).

## Selected design (`ConsensusPolicyDescriptorV1`)

- **Canonical default policy tag:** `#bb.policy.sha256.7g333rrr5tpxptksywvawnlgbqvyppsfpxkefmtzanv6rfsz5tla`
- **Mechanism (`HotStuffTwoPhaseBftPosV1 = 1`):** partial-synchrony deterministic-finality
  2-phase BFT with bonded Proof-of-Stake validator weights (staged operationally behind
  `SEQUENCER-1` before live multi-validator rollout).
- **Quorum rule:** strict integer supermajority `3 * signed_weight > 2 * total_weight` (`n >= 3f + 1`
  by weight, `<= 128` active validators in v1) using explicit strict Ed25519 signer attribution
  over `VoteSignDocV1` (`bitblock/consensus-d1/vote-sign-doc/v1`).
- **Non-self-referential validator set (`InitialValidatorSetV1`):** sorted unique `validator_id`,
  strictly distinct `vote_key` and `proposal_key` per validator across the entire set, positive
  weights, and canonical `active_validator_root` (`bitblock/consensus-d1/validator-set-root/v1`).
  `verify_quorum_certificate` rejects wrong active roots, unknown signers, duplicate signers,
  wrong-role keys, and `<= 2/3` weight.
- **Data availability & sync:** full-possession **data-before-vote** is mandatory on every voting
  validator; validators and full nodes keep rolling history `>= unbonding_delay_epochs` (`28`
  epochs default vs `21`-epoch unbonding delay); fresh nodes bootstrap via weak-subjectivity
  authenticated state checkpoints; full genesis-to-tip **Archive nodes** remain optional and
  separate.
- **v1 scope boundary (`PROFILE-SCOPE-D0` inputs):** single `i64³` world per chain, 100% public
  transparent state (no shielded/ZK state in v1), and zero external bridges or oracles admitted.

## Rejected alternatives

- **Governed PoA-only BFT in v1:** rejected as the target L1 consensus mechanism in favor of
  explicitly specifying bonded PoS (`VALIDATOR-ECON-D1`) up front while still staging through
  `SEQUENCER-1` during bring-up.
- **Probabilistic SHA-256 Proof-of-Work:** rejected due to probabilistic reorg/burn-reversal risk
  and absence of deterministic finality.
- **Permanent centralized sequencer only:** rejected as final L1 goal (retained strictly as an
  intermediate milestone `SEQUENCER-1`).
- **Erasure-coded DAS or mandatory genesis-forever validator storage:** rejected in v1 to avoid
  large DAS cryptographic/networking TCB and unbounded validator disk growth.

## Non-claims

A frozen pre-genesis consensus descriptor is not a liveness/safety proof, running network,
validator schedule implementation (`VALIDATOR-SCHEDULE-1`), or `CONSENSUS-1` runtime (all remain
`LOCKED`).
