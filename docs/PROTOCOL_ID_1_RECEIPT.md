# PROTOCOL-ID-1: non-circular identifier graph, 9-field core, and two-row collision receipt v1

This receipt resolves `PS-027:PROTOCOL-ID-1@base=BOUNDED`, unlocking **`PS-070:CRYPTO-1@base=READY`**
and **`PS-079:PROFILE-D1@base=READY`**.

## Provenance

- **Merged `origin/main` subject (PR #42):** commit `c1402c644762a10a50c58bd9a5a7f2e58d87e530`
  (tree `54fc917e54eacf730a74fe2b019735a9d7e45b6d`, parent `05149c313889b3d14e30e2dfd91ab1c196ffa027`,
  CI runs `45088110484` & `45088114177`, 10/10 jobs green).
- **Executable module:** [`crates/research/src/protocol_id_1.rs`](../crates/research/src/protocol_id_1.rs)
  (`wasm32`-clean, strictly behind `--features protocol-research`; zero churn to
  `profile_bootstrap.rs`, `bitblock-core`, or `world_tag`).
- **Independent Python reference suite:** [`tools/protocol_id_1_vectors.py`](../tools/protocol_id_1_vectors.py)
  (`129/129` protocol schemas independently vectored).
- **CI & mutation evidence:** [`tests/wave8c_protocol_id_1.rs`](../tests/wave8c_protocol_id_1.rs) and
  `python3 tools/bbmutate.py protocol_id_1` (`10/10` canonical big-endian sites KILLED, 100%).
- **Audit probe:** [`examples/protocol_id_1_audit_probe.rs`](../examples/protocol_id_1_audit_probe.rs).

## Verified golden tags & normative invariants

- **Exact 9-field `CanonicalAcceptedProposalCoreV1` (`bitblock/protocol-id-1/exact-proposal-core/v1`):**
  `#bb.receipt.sha256.zaewefk4edydxkwnzxngps7h4szty4oohcrdtdpxwjrzsjxzi53q`
  (matches `AcceptedProposalCoreFieldManifestV1` verbatim while excluding all 8 forbidden
  credential and evidence fields of `AcceptedProposalCoreExcludedFieldManifestV1`).
- **Credential-normalizing `AcceptedProposalWrapperV1` (`bitblock/protocol-id-1/proposal-wrapper/v1`):**
  `#bb.receipt.sha256.aumbzbgscvkmaofwnpaqbujimukmiodgv2pbv7w26mexpr5qepxq`
  (deduplicates strict RFC 8032 Ed25519 proposer signatures over `SigningDomainTagV1::BlockProposal`
  without multiplying weight or creating duplicate parents, unions consensus evidence, and inserts
  same-wrapper availability dependencies atomically via `AtomicAvailabilityIndexV1`).
- **Two-row `TwoRowParentCollisionAuditV1` (`bitblock/protocol-id-1/parent-collision-audit/v1`):**
  - Row 1 (`ProtocolIdentityHashCompromiseV1`, `#bb.receipt.sha256.k3k4i5i3xkghpncwnwssp4jklfbsvpnqfwwzynbmqwk6hbdotraq`):
    byte-distinct lookup payload preimages under one `ProfileIdHashV1` output -> terminal safe-halt.
  - Row 2 (`ProfileOwnedAuthorityBindingCompromiseV1`, `#bb.receipt.sha256.awmkppq2clfbrf7qouyfxdeuseo7kktzcrnriii3pm4tkckk6wha`):
    byte-equal lookup payload with distinct otherwise-valid unsigned cores -> live authority-binding safe-halt.
  - Invalid lookup reconstruction rejects fail-closed as `InvalidLookupReconstruction` (strictly
    never a third collision row).
- **12-node, 17-edge non-circular identifier DAG (`PROTOCOL_ID_1_CANONICAL_DAG`):** verified acyclic
  with cross-domain, wrong-chain, wrong-version, field-omission, duplicate-field, wrong-parent,
  and self-loop/cycle rejection tests.
- **Visual verification:** `protocol_id_1_graph_chart.png` rendered on merged `c1402c6` was
  directly inspected via `read_file`.

## Non-claims

Non-circular identifiers, wrapper normalization, and two-row collision safe-halts do not establish
trusted distribution, valid validator key custody, honest execution, consensus safety, data
availability, or the cryptographic collision resistance of `ProfileIdHashV1` (`SHA-256`).
