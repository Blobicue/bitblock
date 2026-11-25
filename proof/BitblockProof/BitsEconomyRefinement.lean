namespace BitblockProof

/-!
`BITS-ECONOMY-1`: Formal 6-opcode state-machine refinement, closed total-supply conservation,
exact integer `80/20` creator/platform split arithmetic, `lab == false` zero-mint structural
refusal, replay-guard monotonicity, Cross-Session Amendment 1 `ActorIdV1` key-rotation invariance,
non-transitive depth-1 delegation cap enforcement, and atomic rejection rollback corresponding to
`crates/research/src/bits_economy_1.rs` (`BitsGenesisConfigV1`, `BitsTransferV1`,
`BitsPurchaseSplitV1`, `BitsTestnetGrantV1`, `BitsSimulatedPayoutV1`, `BitsIdentityBindV1`,
`BitsL2AnchorV1`, `BitsDelegationGrantV1`, `BitsEconomyStateV1`, `compute_purchase_split`,
`BitsEconomyStateV1::apply_signed_tx`, and `BitsEconomyStateV1::register_purchase_delegation`).

Establishes under `lake env lean --trust=0` with zero custom axioms (`Quot.sound` and `propext`
only):
- Exact integer `80/20` creator/platform purchase split conservation
  (`bitsCreatorShare amount bps + bitsPlatformShare amount bps = amount`, floor-to-creator bounds,
  `0%` transfer tax, and canonical platform vectors);
- Duplicate-free balance-map total-supply conservation (`balTotalSupply = genesisSupply`) across
  exact transfers, 3-way splits (including arbitrary actor aliasing), all 6 opcodes, and arbitrary
  finite transaction blocks (`applyBitsBlock_preserves_totalSupply`);
- Fail-closed atomic rejection rollback: any rejected transaction leaves the entire economy state
  bit-for-bit unchanged (`applyBitsTx_atomic_rollback_on_reject`);
- Structural testnet-only enforcement (`lab == false` unconditionally refuses `TestnetGrant` and
  `SimulatedPayout`; `L2Anchor` never mutates balances and strictly enforces monotone epochs);
- Cross-Session Amendment 1 identity invariance: `IdentityBind` key rotation preserves every
  `ActorIdV1` balance and revokes every stale delegation for that principal;
- Non-transitive depth-1 delegation enforcement: delegated `PurchaseSplit` strictly obeys
  `allowPurchaseSplit`, `!revoked`, `height <= expiresAt`, `amount <= maxPerTx`, and
  `spent + amount <= maxTotal`, cannot authorize `Transfer` or `SimulatedPayout`, and rejects
  depth-2 sub-delegation chains;
- Strict replay-guard monotonicity (`usedOrders`, `usedGrants`, `usedPayouts`) and rejection of
  replayed order/grant/payout IDs;
- Powered negative controls proving unbounded basis points, non-zero transfer tax, out-of-pool
  runtime minting, and `lab = false` signup-grant admission violate the invariants.

This is an abstract mathematical model over Lean `Nat`, `Bool`, and finite lists. It does not
formally verify `rustc`/LLVM machine code, SHA-256 hashing, RFC 8032 Ed25519 cryptography, or
network liveness.
-/

/-! ## Section 1: Exact Integer 80/20 Purchase Split & Zero-Tax Arithmetic -/

/-- Basis-point denominator (`10,000 = 100.00%`). -/
def bitsBpsDenominator : Nat := 10000

/-- Default creator share (`8,000 = 80.00%`). -/
def bitsDefaultCreatorBps : Nat := 8000

/-- Integer floor creator payout `floor(amount * creatorBps / 10_000)`. -/
def bitsCreatorShare (amount creatorBps : Nat) : Nat :=
  (amount * creatorBps) / bitsBpsDenominator

/-- Exact platform treasury complement `amount - creatorShare`. -/
def bitsPlatformShare (amount creatorBps : Nat) : Nat :=
  amount - bitsCreatorShare amount creatorBps

theorem bitsCreatorShare_le_amount (amount creatorBps : Nat)
    (hbps : creatorBps ≤ bitsBpsDenominator) :
    bitsCreatorShare amount creatorBps ≤ amount := by
  unfold bitsCreatorShare bitsBpsDenominator at *
  have hmul : amount * creatorBps ≤ amount * 10000 := Nat.mul_le_mul_left amount hbps
  have hdiv : (amount * creatorBps) / 10000 ≤ (amount * 10000) / 10000 :=
    Nat.div_le_div_right hmul
  have hcancel : (amount * 10000) / 10000 = amount :=
    Nat.mul_div_cancel amount (by decide : 0 < 10000)
  omega

theorem bits_purchase_split_exact_conservation (amount creatorBps : Nat)
    (hbps : creatorBps ≤ bitsBpsDenominator) :
    bitsCreatorShare amount creatorBps + bitsPlatformShare amount creatorBps = amount := by
  have hle := bitsCreatorShare_le_amount amount creatorBps hbps
  unfold bitsPlatformShare
  omega

theorem bits_purchase_split_80_20_conservation (amount : Nat) :
    bitsCreatorShare amount bitsDefaultCreatorBps +
      bitsPlatformShare amount bitsDefaultCreatorBps = amount :=
  bits_purchase_split_exact_conservation amount bitsDefaultCreatorBps (by decide)

theorem bits_purchase_split_floor_lower_bound (amount creatorBps : Nat) :
    bitsCreatorShare amount creatorBps * bitsBpsDenominator ≤ amount * creatorBps := by
  unfold bitsCreatorShare bitsBpsDenominator
  have h := Nat.div_add_mod (amount * creatorBps) 10000
  omega

theorem bits_purchase_split_floor_upper_bound (amount creatorBps : Nat) :
    amount * creatorBps < (bitsCreatorShare amount creatorBps + 1) * bitsBpsDenominator := by
  unfold bitsCreatorShare bitsBpsDenominator
  have h := Nat.div_add_mod (amount * creatorBps) 10000
  have hmod : (amount * creatorBps) % 10000 < 10000 := Nat.mod_lt _ (by decide)
  omega

theorem bits_purchase_split_canonical_vectors :
    bitsCreatorShare 10 8000 = 8 ∧ bitsPlatformShare 10 8000 = 2 ∧
    bitsCreatorShare 250 8000 = 200 ∧ bitsPlatformShare 250 8000 = 50 ∧
    bitsCreatorShare 101 8000 = 80 ∧ bitsPlatformShare 101 8000 = 21 ∧
    bitsCreatorShare 999 8000 = 799 ∧ bitsPlatformShare 999 8000 = 200 := by
  decide

/-! ## Section 2: Duplicate-Free Balance Map & Closed Supply Conservation -/

/-- Duplicate-free association map modeling `BTreeMap<ActorIdV1, u128>`. -/
abbrev BitsBalMap := List (Nat × Nat)

/-- Key-uniqueness invariant on `BitsBalMap`. -/
def BalNoDup : BitsBalMap → Prop
  | [] => True
  | (k, _) :: rest => (∀ v, (k, v) ∉ rest) ∧ BalNoDup rest

/-- Total Bits supply across all accounts and system pools (`grantPool`, `treasury`, `escrow`). -/
def balTotalSupply : BitsBalMap → Nat
  | [] => 0
  | (_, v) :: rest => v + balTotalSupply rest

/-- Balance lookup returning `0` for absent actors. -/
def balGet : BitsBalMap → Nat → Nat
  | [], _ => 0
  | (k, v) :: rest, a => if k = a then v else balGet rest a

/-- Updates an actor's balance in-place (or inserts if absent), preserving `BalNoDup`. -/
def balSet : BitsBalMap → Nat → Nat → BitsBalMap
  | [], a, v => [(a, v)]
  | (k, cur) :: rest, a, v =>
      if k = a then (k, v) :: rest else (k, cur) :: balSet rest a v

theorem balGet_of_not_mem (m : BitsBalMap) (a : Nat) (h : ∀ v, (a, v) ∉ m) :
    balGet m a = 0 := by
  induction m with
  | nil => rfl
  | cons hd tl ih =>
      rcases hd with ⟨k, v⟩
      by_cases hka : k = a
      · subst hka
        exact False.elim (h v (List.Mem.head tl))
      · simp [balGet, hka]
        exact ih (fun v' hm => h v' (List.Mem.tail _ hm))

theorem balSet_not_mem_of_ne (m : BitsBalMap) (a v k : Nat) (hne : k ≠ a)
    (h : ∀ v', (k, v') ∉ m) : ∀ v', (k, v') ∉ balSet m a v := by
  induction m with
  | nil =>
      intro v' hm
      simp [balSet] at hm
      exact hne hm.1
  | cons hd tl ih =>
      rcases hd with ⟨k0, v0⟩
      intro v' hm
      by_cases hk0a : k0 = a
      · subst hk0a
        simp [balSet] at hm
        rcases hm with ⟨hk, _⟩ | htl
        · exact hne hk
        · exact h v' (List.Mem.tail _ htl)
      · simp [balSet, hk0a] at hm
        rcases hm with ⟨hk, hv⟩ | htl
        · subst hk; subst hv
          exact h v' (List.Mem.head _)
        · exact ih (fun w hw => h w (List.Mem.tail _ hw)) v' htl

theorem balSet_noDup (m : BitsBalMap) (a v : Nat) (hnd : BalNoDup m) :
    BalNoDup (balSet m a v) := by
  induction m with
  | nil =>
      simp [balSet, BalNoDup]
  | cons hd tl ih =>
      rcases hd with ⟨k0, v0⟩
      rcases hnd with ⟨hnot, htl⟩
      by_cases hk0a : k0 = a
      · subst hk0a
        simp [balSet, BalNoDup]
        exact ⟨hnot, htl⟩
      · simp [balSet, hk0a, BalNoDup]
        exact ⟨balSet_not_mem_of_ne tl a v k0 hk0a hnot, ih htl⟩

theorem balGet_balSet_same (m : BitsBalMap) (a v : Nat) :
    balGet (balSet m a v) a = v := by
  induction m with
  | nil => simp [balSet, balGet]
  | cons hd tl ih =>
      rcases hd with ⟨k0, v0⟩
      by_cases hk0a : k0 = a
      · subst hk0a; simp [balSet, balGet]
      · simp [balSet, balGet, hk0a, ih]

theorem balGet_balSet_other (m : BitsBalMap) (a b v : Nat) (hne : a ≠ b) :
    balGet (balSet m a v) b = balGet m b := by
  induction m with
  | nil =>
      simp [balSet, balGet, hne]
  | cons hd tl ih =>
      rcases hd with ⟨k0, v0⟩
      by_cases hk0a : k0 = a
      · subst hk0a; simp [balSet, balGet, hne]
      · by_cases hk0b : k0 = b
        · subst hk0b; simp [balSet, balGet, hk0a]
        · simp [balSet, balGet, hk0a, hk0b, ih]

/-- Fundamental additive conservation law for `balSet`: updating actor `a` to `v` changes
    `balTotalSupply` by exactly `v - balGet m a` (stated subtraction-free in `Nat`). -/
theorem balTotalSupply_balSet (m : BitsBalMap) (a v : Nat) (hnd : BalNoDup m) :
    balTotalSupply (balSet m a v) + balGet m a = balTotalSupply m + v := by
  induction m with
  | nil =>
      simp [balSet, balTotalSupply, balGet]
  | cons hd tl ih =>
      rcases hd with ⟨k0, v0⟩
      rcases hnd with ⟨_, htl⟩
      by_cases hk0a : k0 = a
      · subst hk0a
        simp [balSet, balTotalSupply, balGet]
        omega
      · simp [balSet, hk0a, balTotalSupply, balGet]
        have hrec := ih htl
        omega

/-- Exact two-party debit+credit helper (handles `src = dst` cleanly as well). -/
def balTransfer (m : BitsBalMap) (src dst amt : Nat) : BitsBalMap :=
  balSet (balSet m src (balGet m src - amt)) dst
    (balGet (balSet m src (balGet m src - amt)) dst + amt)

theorem balTransfer_noDup (m : BitsBalMap) (src dst amt : Nat) (hnd : BalNoDup m) :
    BalNoDup (balTransfer m src dst amt) :=
  balSet_noDup _ _ _ (balSet_noDup m src _ hnd)

theorem balTransfer_preserves_totalSupply (m : BitsBalMap) (src dst amt : Nat)
    (hnd : BalNoDup m) (hbal : amt ≤ balGet m src) :
    balTotalSupply (balTransfer m src dst amt) = balTotalSupply m := by
  dsimp only [balTransfer]
  have h1 := balTotalSupply_balSet m src (balGet m src - amt) hnd
  have hnd1 := balSet_noDup m src (balGet m src - amt) hnd
  have h2 := balTotalSupply_balSet (balSet m src (balGet m src - amt)) dst
    (balGet (balSet m src (balGet m src - amt)) dst + amt) hnd1
  omega

/-- Exact three-party atomic purchase split (handles arbitrary actor aliasing cleanly). -/
def balPurchaseSplit (m : BitsBalMap) (buyer creator treasury amt creatorBits platformBits : Nat) :
    BitsBalMap :=
  balSet
    (balSet
      (balSet m buyer (balGet m buyer - amt))
      creator
      (balGet (balSet m buyer (balGet m buyer - amt)) creator + creatorBits))
    treasury
    (balGet
      (balSet
        (balSet m buyer (balGet m buyer - amt))
        creator
        (balGet (balSet m buyer (balGet m buyer - amt)) creator + creatorBits))
      treasury + platformBits)

theorem balPurchaseSplit_noDup (m : BitsBalMap)
    (buyer creator treasury amt creatorBits platformBits : Nat) (hnd : BalNoDup m) :
    BalNoDup (balPurchaseSplit m buyer creator treasury amt creatorBits platformBits) :=
  balSet_noDup _ _ _ (balSet_noDup _ _ _ (balSet_noDup m _ _ hnd))

theorem balPurchaseSplit_preserves_totalSupply (m : BitsBalMap)
    (buyer creator treasury amt creatorBits platformBits : Nat)
    (hnd : BalNoDup m) (hbal : amt ≤ balGet m buyer)
    (hsplit : creatorBits + platformBits = amt) :
    balTotalSupply (balPurchaseSplit m buyer creator treasury amt creatorBits platformBits) =
      balTotalSupply m := by
  dsimp only [balPurchaseSplit]
  have hnd1 := balSet_noDup m buyer (balGet m buyer - amt) hnd
  have h1 := balTotalSupply_balSet m buyer (balGet m buyer - amt) hnd
  have hnd2 := balSet_noDup (balSet m buyer (balGet m buyer - amt)) creator
    (balGet (balSet m buyer (balGet m buyer - amt)) creator + creatorBits) hnd1
  have h2 := balTotalSupply_balSet (balSet m buyer (balGet m buyer - amt)) creator
    (balGet (balSet m buyer (balGet m buyer - amt)) creator + creatorBits) hnd1
  have h3 := balTotalSupply_balSet
    (balSet (balSet m buyer (balGet m buyer - amt)) creator
      (balGet (balSet m buyer (balGet m buyer - amt)) creator + creatorBits))
    treasury
    (balGet (balSet (balSet m buyer (balGet m buyer - amt)) creator
      (balGet (balSet m buyer (balGet m buyer - amt)) creator + creatorBits)) treasury + platformBits)
    hnd2
  omega

/-! ## Section 3: Complete 6-Opcode `BitsEconomyState` Machine -/

/-- Testnet grant categories (`Signup`, `Faucet`, `CarryOver`). -/
inductive BitsGrantKind where
  | signup
  | faucet
  | carryOver
deriving DecidableEq, Repr

/-- Immutable Bits genesis policy mirroring `BitsGenesisConfigV1`. -/
structure BitsConfig where
  lab : Bool
  chainId : Nat
  genesisSupply : Nat
  platformTreasury : Nat
  grantPool : Nat
  payoutEscrow : Nat
  maxSignupGrant : Nat
  maxFaucetGrant : Nat
  minPurchase : Nat
  creatorSplitBps : Nat
  minPayout : Nat
deriving DecidableEq, Repr

/-- Genesis validity predicate mirroring `BitsGenesisConfigV1::validate`. -/
structure ValidBitsConfig (cfg : BitsConfig) : Prop where
  chainIdPos : 0 < cfg.chainId
  supplyPos : 0 < cfg.genesisSupply
  minPurchasePos : 0 < cfg.minPurchase
  minPayoutPos : 0 < cfg.minPayout
  bpsBounded : cfg.creatorSplitBps ≤ bitsBpsDenominator
  treasuryNeGrant : cfg.platformTreasury ≠ cfg.grantPool
  treasuryNeEscrow : cfg.platformTreasury ≠ cfg.payoutEscrow
  grantNeEscrow : cfg.grantPool ≠ cfg.payoutEscrow

/-- Scoped non-transitive depth-1 delegation mirroring `BitsDelegationGrantV1`. -/
structure BitsDelegationGrant where
  principalActor : Nat
  delegatePk : Nat
  maxBitsPerTx : Nat
  maxBitsTotal : Nat
  spentBits : Nat
  expiresAtHeight : Nat
  allowPurchaseSplit : Bool
  revoked : Bool
deriving DecidableEq, Repr

/-- Union of all six canonical `BitsTxPayloadV1` opcodes (`0x01..=0x06`). -/
inductive BitsTxPayload where
  | transfer (src dst amountBits : Nat)
  | purchaseSplit (buyer creator treasury amountBits creatorBps orderId : Nat)
  | testnetGrant (recipient recipientPk amountBits : Nat) (kind : BitsGrantKind) (grantId : Nat)
  | simulatedPayout (creator amountBits payoutRequestId : Nat)
  | identityBind (actorId newPk externalProvider externalIdHash : Nat) (popValid : Bool)
  | l2Anchor (appId epochId l2Root eventCount : Nat)
deriving DecidableEq, Repr

/-- Canonical 1-byte opcode discriminant (`1..=6`) mirroring `BitsTxPayloadV1::opcode`. -/
def BitsTxPayload.opcode : BitsTxPayload → Nat
  | .transfer _ _ _ => 1
  | .purchaseSplit _ _ _ _ _ _ => 2
  | .testnetGrant _ _ _ _ _ => 3
  | .simulatedPayout _ _ _ => 4
  | .identityBind _ _ _ _ _ => 5
  | .l2Anchor _ _ _ _ => 6

/-- Abstract transaction envelope mirroring `BitsSignedTxV1`. -/
structure BitsTx where
  chainId : Nat
  senderActor : Nat
  signerPk : Nat
  seq : Nat
  validUntilHeight : Nat
  sigValid : Bool
  payload : BitsTxPayload
deriving DecidableEq, Repr

/-- Deterministic execution disposition mirroring `BitsDispositionV1`. -/
inductive BitsDisposition where
  | admitted
  | rejectedTestnetOnly
  | rejectedUnauthorized
  | rejectedReplayOrNonce
  | rejectedInsufficientBalance
  | rejectedPolicy
deriving DecidableEq, Repr

/-- Complete state mirroring `BitsEconomyStateV1`. -/
structure BitsState where
  cfg : BitsConfig
  balances : BitsBalMap
  nonces : List (Nat × Nat)
  actorKeys : List (Nat × Nat)
  delegations : List BitsDelegationGrant
  grantRelayers : List Nat
  usedOrders : List Nat
  usedGrants : List Nat
  usedPayouts : List Nat
  l2Epochs : List (Nat × Nat)
deriving DecidableEq, Repr

/-- State well-formedness invariant (`ValidBitsConfig` + `BalNoDup` + closed supply conservation). -/
structure ValidBitsState (st : BitsState) : Prop where
  cfgValid : ValidBitsConfig st.cfg
  balNoDup : BalNoDup st.balances
  conserved : balTotalSupply st.balances = st.cfg.genesisSupply

def assocGetNat : List (Nat × Nat) → Nat → Nat
  | [], _ => 0
  | (k0, v0) :: rest, k => if k0 = k then v0 else assocGetNat rest k

def assocFindOpt : List (Nat × Nat) → Nat → Option Nat
  | [], _ => none
  | (k0, v0) :: rest, k => if k0 = k then some v0 else assocFindOpt rest k

def assocSetNat : List (Nat × Nat) → Nat → Nat → List (Nat × Nat)
  | [], k, v => [(k, v)]
  | (k0, v0) :: rest, k, v => if k0 = k then (k0, v) :: rest else (k0, v0) :: assocSetNat rest k v

/-- Checks whether `pk` is currently the active delegate of any principal. -/
def isActiveDelegate (dels : List BitsDelegationGrant) (pk : Nat) : Bool :=
  dels.any (fun g => decide (g.delegatePk = pk))

/-- Registers a direct non-transitive depth-1 purchase delegation (mirrors `register_purchase_delegation`). -/
def registerDelegation (st : BitsState) (g : BitsDelegationGrant) :
    BitsDisposition × BitsState :=
  if !g.allowPurchaseSplit || g.revoked || g.maxBitsPerTx == 0 || g.maxBitsTotal == 0 ||
      g.maxBitsPerTx > g.maxBitsTotal || g.spentBits != 0 then
    (.rejectedPolicy, st)
  else
    match assocFindOpt st.actorKeys g.principalActor with
    | some principalPk =>
        if isActiveDelegate st.delegations principalPk then
          (.rejectedUnauthorized, st)
        else
          (.admitted, { st with delegations := g :: st.delegations })
    | none =>
        (.admitted, { st with delegations := g :: st.delegations })

/-- Revokes every delegation belonging to `actor` (triggered automatically on key rotation). -/
def revokeActorDelegations (dels : List BitsDelegationGrant) (actor : Nat) :
    List BitsDelegationGrant :=
  dels.map (fun g => if g.principalActor = actor then { g with revoked := true } else g)

/-- Attempts to consume bounded depth-1 purchase delegation allowance for `(buyer, signerPk)`. -/
def consumePurchaseDelegation (dels : List BitsDelegationGrant) (buyer signerPk amt blockHeight : Nat) :
    Option (List BitsDelegationGrant) :=
  match dels with
  | [] => none
  | g :: rest =>
      if g.principalActor = buyer ∧ g.delegatePk = signerPk then
        if !g.revoked && g.allowPurchaseSplit && blockHeight ≤ g.expiresAtHeight &&
            amt ≤ g.maxBitsPerTx && g.spentBits + amt ≤ g.maxBitsTotal then
          some ({ g with spentBits := g.spentBits + amt } :: rest)
        else
          none
      else
        match consumePurchaseDelegation rest buyer signerPk amt blockHeight with
        | none => none
        | some rest' => some (g :: rest')

/-! ### Per-opcode atomic transition helpers -/

def applyTransfer (st : BitsState) (tx : BitsTx) (src dst amt : Nat) :
    BitsDisposition × BitsState :=
  if amt = 0 ∨ src = dst then
    (.rejectedPolicy, st)
  else if src ≠ tx.senderActor ∨ assocFindOpt st.actorKeys src ≠ some tx.signerPk then
    (.rejectedUnauthorized, st)
  else if balGet st.balances src < amt then
    (.rejectedInsufficientBalance, st)
  else
    (.admitted, { st with
      balances := balTransfer st.balances src dst amt
      nonces := assocSetNat st.nonces tx.senderActor tx.seq })

def applyPurchaseSplit (st : BitsState) (tx : BitsTx) (blockHeight : Nat)
    (buyer creator treasury amt bps orderId : Nat) : BitsDisposition × BitsState :=
  if amt = 0 ∨ buyer = creator ∨ treasury ≠ st.cfg.platformTreasury ∨
      amt < st.cfg.minPurchase ∨ bps ≠ st.cfg.creatorSplitBps ∨ bps > bitsBpsDenominator then
    (.rejectedPolicy, st)
  else if buyer ≠ tx.senderActor then
    (.rejectedUnauthorized, st)
  else if orderId ∈ st.usedOrders then
    (.rejectedReplayOrNonce, st)
  else
    match (if assocFindOpt st.actorKeys buyer = some tx.signerPk then
        some st.delegations
      else
        consumePurchaseDelegation st.delegations buyer tx.signerPk amt blockHeight) with
    | none => (.rejectedUnauthorized, st)
    | some nextDels =>
        if balGet st.balances buyer < amt then
          (.rejectedInsufficientBalance, st)
        else
          (.admitted, { st with
            balances := balPurchaseSplit st.balances buyer creator treasury amt
              (bitsCreatorShare amt bps) (bitsPlatformShare amt bps)
            nonces := assocSetNat st.nonces tx.senderActor tx.seq
            delegations := nextDels
            usedOrders := orderId :: st.usedOrders })

def applyTestnetGrant (st : BitsState) (tx : BitsTx)
    (recipient recipientPk amt : Nat) (kind : BitsGrantKind) (grantId : Nat) :
    BitsDisposition × BitsState :=
  if st.cfg.lab = false then
    (.rejectedTestnetOnly, st)
  else if amt = 0 then
    (.rejectedPolicy, st)
  else if tx.senderActor ≠ st.cfg.grantPool ∨ tx.signerPk ∉ st.grantRelayers then
    (.rejectedUnauthorized, st)
  else if grantId ∈ st.usedGrants then
    (.rejectedReplayOrNonce, st)
  else if (kind = .signup ∧ amt > st.cfg.maxSignupGrant) ∨
      (kind = .faucet ∧ amt > st.cfg.maxFaucetGrant) then
    (.rejectedPolicy, st)
  else if balGet st.balances st.cfg.grantPool < amt then
    (.rejectedInsufficientBalance, st)
  else
    let nextKeys :=
      match assocFindOpt st.actorKeys recipient with
      | some _ => st.actorKeys
      | none => assocSetNat st.actorKeys recipient recipientPk
    (.admitted, { st with
      balances := balTransfer st.balances st.cfg.grantPool recipient amt
      nonces := assocSetNat st.nonces tx.senderActor tx.seq
      actorKeys := nextKeys
      usedGrants := grantId :: st.usedGrants })

def applySimulatedPayout (st : BitsState) (tx : BitsTx)
    (creator amt payoutId : Nat) : BitsDisposition × BitsState :=
  if st.cfg.lab = false then
    (.rejectedTestnetOnly, st)
  else if amt = 0 ∨ amt < st.cfg.minPayout then
    (.rejectedPolicy, st)
  else if creator ≠ tx.senderActor ∨ assocFindOpt st.actorKeys creator ≠ some tx.signerPk then
    (.rejectedUnauthorized, st)
  else if payoutId ∈ st.usedPayouts then
    (.rejectedReplayOrNonce, st)
  else if balGet st.balances creator < amt then
    (.rejectedInsufficientBalance, st)
  else
    (.admitted, { st with
      balances := balTransfer st.balances creator st.cfg.payoutEscrow amt
      nonces := assocSetNat st.nonces tx.senderActor tx.seq
      usedPayouts := payoutId :: st.usedPayouts })

def applyIdentityBind (st : BitsState) (tx : BitsTx)
    (actorId newPk : Nat) (popValid : Bool) : BitsDisposition × BitsState :=
  if actorId ≠ tx.senderActor ∨ popValid = false then
    (.rejectedUnauthorized, st)
  else
    match assocFindOpt st.actorKeys actorId with
    | some curPk =>
        if tx.signerPk ≠ curPk then
          (.rejectedUnauthorized, st)
        else if newPk = curPk then
          (.admitted, { st with nonces := assocSetNat st.nonces tx.senderActor tx.seq })
        else
          (.admitted, { st with
            nonces := assocSetNat st.nonces tx.senderActor tx.seq
            actorKeys := assocSetNat st.actorKeys actorId newPk
            delegations := revokeActorDelegations st.delegations actorId })
    | none =>
        if tx.signerPk ≠ newPk then
          (.rejectedUnauthorized, st)
        else
          (.admitted, { st with
            nonces := assocSetNat st.nonces tx.senderActor tx.seq
            actorKeys := assocSetNat st.actorKeys actorId newPk })

def applyL2Anchor (st : BitsState) (tx : BitsTx) (appId epochId : Nat) :
    BitsDisposition × BitsState :=
  if tx.signerPk ∉ st.grantRelayers then
    (.rejectedUnauthorized, st)
  else
    match assocFindOpt st.l2Epochs appId with
    | some prevEpoch =>
        if epochId ≤ prevEpoch then
          (.rejectedReplayOrNonce, st)
        else
          (.admitted, { st with
            nonces := assocSetNat st.nonces tx.senderActor tx.seq
            l2Epochs := assocSetNat st.l2Epochs appId epochId })
    | none =>
        (.admitted, { st with
          nonces := assocSetNat st.nonces tx.senderActor tx.seq
          l2Epochs := assocSetNat st.l2Epochs appId epochId })

/-- Atomic 6-opcode state transition mirroring `BitsEconomyStateV1::apply_signed_tx`. -/
def applyBitsTx (st : BitsState) (tx : BitsTx) (blockHeight : Nat) :
    BitsDisposition × BitsState :=
  if tx.chainId ≠ st.cfg.chainId ∨ tx.sigValid = false then
    (.rejectedUnauthorized, st)
  else if tx.validUntilHeight ≠ 0 ∧ blockHeight > tx.validUntilHeight then
    (.rejectedReplayOrNonce, st)
  else if tx.seq ≠ assocGetNat st.nonces tx.senderActor + 1 then
    (.rejectedReplayOrNonce, st)
  else
    match tx.payload with
    | .transfer src dst amt => applyTransfer st tx src dst amt
    | .purchaseSplit b c t amt bps oid => applyPurchaseSplit st tx blockHeight b c t amt bps oid
    | .testnetGrant r pk amt kind gid => applyTestnetGrant st tx r pk amt kind gid
    | .simulatedPayout c amt pid => applySimulatedPayout st tx c amt pid
    | .identityBind a pk _ _ pop => applyIdentityBind st tx a pk pop
    | .l2Anchor app ep _ _ => applyL2Anchor st tx app ep

/-- Sequential execution of a block of signed Bits transactions. -/
def applyBitsBlock (st : BitsState) (txs : List BitsTx) (blockHeight : Nat) : BitsState :=
  match txs with
  | [] => st
  | tx :: rest => applyBitsBlock (applyBitsTx st tx blockHeight).2 rest blockHeight

/-! ## Section 4: State-Machine Refinement & Invariant Theorems -/

theorem bitsPayload_opcode_range (p : BitsTxPayload) :
    1 ≤ p.opcode ∧ p.opcode ≤ 6 := by
  cases p <;> simp [BitsTxPayload.opcode]

theorem applyTransfer_cfg (st : BitsState) (tx : BitsTx) (s d a : Nat) :
    (applyTransfer st tx s d a).2.cfg = st.cfg := by
  unfold applyTransfer
  split
  · rfl
  · split
    · rfl
    · split <;> rfl

theorem applyPurchaseSplit_cfg (st : BitsState) (tx : BitsTx) (h b c t a bps oid : Nat) :
    (applyPurchaseSplit st tx h b c t a bps oid).2.cfg = st.cfg := by
  unfold applyPurchaseSplit
  split
  · rfl
  · split
    · rfl
    · split
      · rfl
      · split
        · rfl
        · split <;> rfl

theorem applyTestnetGrant_cfg (st : BitsState) (tx : BitsTx) (r pk a : Nat) (k : BitsGrantKind) (gid : Nat) :
    (applyTestnetGrant st tx r pk a k gid).2.cfg = st.cfg := by
  unfold applyTestnetGrant
  split
  · rfl
  · split
    · rfl
    · split
      · rfl
      · split
        · rfl
        · split
          · rfl
          · split <;> rfl

theorem applySimulatedPayout_cfg (st : BitsState) (tx : BitsTx) (c a pid : Nat) :
    (applySimulatedPayout st tx c a pid).2.cfg = st.cfg := by
  unfold applySimulatedPayout
  split
  · rfl
  · split
    · rfl
    · split
      · rfl
      · split
        · rfl
        · split <;> rfl

theorem applyIdentityBind_cfg (st : BitsState) (tx : BitsTx) (a pk : Nat) (pop : Bool) :
    (applyIdentityBind st tx a pk pop).2.cfg = st.cfg := by
  unfold applyIdentityBind
  split
  · rfl
  · split
    · split
      · rfl
      · split <;> rfl
    · split <;> rfl

theorem applyL2Anchor_cfg (st : BitsState) (tx : BitsTx) (app ep : Nat) :
    (applyL2Anchor st tx app ep).2.cfg = st.cfg := by
  unfold applyL2Anchor
  split
  · rfl
  · split
    · split <;> rfl
    · rfl

theorem applyBitsTx_preserves_cfg (st : BitsState) (tx : BitsTx) (h : Nat) :
    (applyBitsTx st tx h).2.cfg = st.cfg := by
  unfold applyBitsTx
  split
  · rfl
  · split
    · rfl
    · split
      · rfl
      · split
        · exact applyTransfer_cfg st tx _ _ _
        · exact applyPurchaseSplit_cfg st tx h _ _ _ _ _ _
        · exact applyTestnetGrant_cfg st tx _ _ _ _ _
        · exact applySimulatedPayout_cfg st tx _ _ _
        · exact applyIdentityBind_cfg st tx _ _ _
        · exact applyL2Anchor_cfg st tx _ _

theorem applyTransfer_rollback (st : BitsState) (tx : BitsTx) (s d a : Nat)
    (hrej : (applyTransfer st tx s d a).1 ≠ .admitted) :
    (applyTransfer st tx s d a).2 = st := by
  unfold applyTransfer at hrej ⊢
  by_cases h1 : a = 0 ∨ s = d
  · simp [h1]
  · simp only [if_neg h1] at hrej ⊢
    by_cases h2 : s ≠ tx.senderActor ∨ assocFindOpt st.actorKeys s ≠ some tx.signerPk
    · simp [h2]
    · simp only [if_neg h2] at hrej ⊢
      by_cases h3 : balGet st.balances s < a
      · simp [h3]
      · simp only [if_neg h3] at hrej ⊢
        exact False.elim (hrej rfl)

theorem applyPurchaseSplit_rollback (st : BitsState) (tx : BitsTx) (h b c t a bps oid : Nat)
    (hrej : (applyPurchaseSplit st tx h b c t a bps oid).1 ≠ .admitted) :
    (applyPurchaseSplit st tx h b c t a bps oid).2 = st := by
  unfold applyPurchaseSplit at hrej ⊢
  by_cases h1 : a = 0 ∨ b = c ∨ t ≠ st.cfg.platformTreasury ∨ a < st.cfg.minPurchase ∨
      bps ≠ st.cfg.creatorSplitBps ∨ bps > bitsBpsDenominator
  · simp [h1]
  · simp only [if_neg h1] at hrej ⊢
    by_cases h2 : b ≠ tx.senderActor
    · simp [h2]
    · simp only [if_neg h2] at hrej ⊢
      by_cases h3 : oid ∈ st.usedOrders
      · simp [h3]
      · simp only [if_neg h3] at hrej ⊢
        cases hauth : (if assocFindOpt st.actorKeys b = some tx.signerPk then some st.delegations
            else consumePurchaseDelegation st.delegations b tx.signerPk a h) with
        | none => simp
        | some nextDels =>
            simp only [hauth] at hrej ⊢
            by_cases h4 : balGet st.balances b < a
            · simp [h4]
            · simp only [if_neg h4] at hrej ⊢
              exact False.elim (hrej rfl)

theorem applyTestnetGrant_rollback (st : BitsState) (tx : BitsTx) (r pk a : Nat) (k : BitsGrantKind) (gid : Nat)
    (hrej : (applyTestnetGrant st tx r pk a k gid).1 ≠ .admitted) :
    (applyTestnetGrant st tx r pk a k gid).2 = st := by
  unfold applyTestnetGrant at hrej ⊢
  by_cases h1 : st.cfg.lab = false
  · simp [h1]
  · simp only [if_neg h1] at hrej ⊢
    by_cases h2 : a = 0
    · simp [h2]
    · simp only [if_neg h2] at hrej ⊢
      by_cases h3 : tx.senderActor ≠ st.cfg.grantPool ∨ tx.signerPk ∉ st.grantRelayers
      · simp [h3]
      · simp only [if_neg h3] at hrej ⊢
        by_cases h4 : gid ∈ st.usedGrants
        · simp [h4]
        · simp only [if_neg h4] at hrej ⊢
          by_cases h5 : (k = .signup ∧ a > st.cfg.maxSignupGrant) ∨
              (k = .faucet ∧ a > st.cfg.maxFaucetGrant)
          · simp [h5]
          · simp only [if_neg h5] at hrej ⊢
            by_cases h6 : balGet st.balances st.cfg.grantPool < a
            · simp [h6]
            · simp only [if_neg h6] at hrej ⊢
              exact False.elim (hrej rfl)

theorem applySimulatedPayout_rollback (st : BitsState) (tx : BitsTx) (c a pid : Nat)
    (hrej : (applySimulatedPayout st tx c a pid).1 ≠ .admitted) :
    (applySimulatedPayout st tx c a pid).2 = st := by
  unfold applySimulatedPayout at hrej ⊢
  by_cases h1 : st.cfg.lab = false
  · simp [h1]
  · simp only [if_neg h1] at hrej ⊢
    by_cases h2 : a = 0 ∨ a < st.cfg.minPayout
    · simp [h2]
    · simp only [if_neg h2] at hrej ⊢
      by_cases h3 : c ≠ tx.senderActor ∨ assocFindOpt st.actorKeys c ≠ some tx.signerPk
      · simp [h3]
      · simp only [if_neg h3] at hrej ⊢
        by_cases h4 : pid ∈ st.usedPayouts
        · simp [h4]
        · simp only [if_neg h4] at hrej ⊢
          by_cases h5 : balGet st.balances c < a
          · simp [h5]
          · simp only [if_neg h5] at hrej ⊢
            exact False.elim (hrej rfl)

theorem applyIdentityBind_rollback (st : BitsState) (tx : BitsTx) (a pk : Nat) (pop : Bool)
    (hrej : (applyIdentityBind st tx a pk pop).1 ≠ .admitted) :
    (applyIdentityBind st tx a pk pop).2 = st := by
  unfold applyIdentityBind at hrej ⊢
  by_cases h1 : a ≠ tx.senderActor ∨ pop = false
  · simp [h1]
  · simp only [if_neg h1] at hrej ⊢
    cases hk : assocFindOpt st.actorKeys a with
    | some curPk =>
        simp only [hk] at hrej ⊢
        by_cases h2 : tx.signerPk ≠ curPk
        · simp [h2]
        · simp only [if_neg h2] at hrej ⊢
          by_cases h3 : pk = curPk
          · simp only [if_pos h3] at hrej ⊢
            exact False.elim (hrej rfl)
          · simp only [if_neg h3] at hrej ⊢
            exact False.elim (hrej rfl)
    | none =>
        simp only [hk] at hrej ⊢
        by_cases h2 : tx.signerPk ≠ pk
        · simp [h2]
        · simp only [if_neg h2] at hrej ⊢
          exact False.elim (hrej rfl)

theorem applyL2Anchor_rollback (st : BitsState) (tx : BitsTx) (app ep : Nat)
    (hrej : (applyL2Anchor st tx app ep).1 ≠ .admitted) :
    (applyL2Anchor st tx app ep).2 = st := by
  unfold applyL2Anchor at hrej ⊢
  by_cases h1 : tx.signerPk ∉ st.grantRelayers
  · simp [h1]
  · simp only [if_neg h1] at hrej ⊢
    cases he : assocFindOpt st.l2Epochs app with
    | some prev =>
        simp only [he] at hrej ⊢
        by_cases h2 : ep ≤ prev
        · simp [h2]
        · simp only [if_neg h2] at hrej ⊢
          exact False.elim (hrej rfl)
    | none =>
        simp only [he] at hrej ⊢
        exact False.elim (hrej rfl)

/-- Atomic rollback: any rejected transaction leaves the entire economy state unchanged. -/
theorem applyBitsTx_atomic_rollback_on_reject (st : BitsState) (tx : BitsTx) (h : Nat)
    (hrej : (applyBitsTx st tx h).1 ≠ .admitted) :
    (applyBitsTx st tx h).2 = st := by
  unfold applyBitsTx at hrej ⊢
  by_cases h1 : tx.chainId ≠ st.cfg.chainId ∨ tx.sigValid = false
  · simp [h1]
  · simp only [if_neg h1] at hrej ⊢
    by_cases h2 : tx.validUntilHeight ≠ 0 ∧ h > tx.validUntilHeight
    · simp [h2]
    · simp only [if_neg h2] at hrej ⊢
      by_cases h3 : tx.seq ≠ assocGetNat st.nonces tx.senderActor + 1
      · simp [h3]
      · simp only [if_neg h3] at hrej ⊢
        cases hp : tx.payload with
        | transfer s d a =>
            simp only [hp] at hrej ⊢
            exact applyTransfer_rollback st tx s d a hrej
        | purchaseSplit b c t a bps oid =>
            simp only [hp] at hrej ⊢
            exact applyPurchaseSplit_rollback st tx h b c t a bps oid hrej
        | testnetGrant r pk a k gid =>
            simp only [hp] at hrej ⊢
            exact applyTestnetGrant_rollback st tx r pk a k gid hrej
        | simulatedPayout c a pid =>
            simp only [hp] at hrej ⊢
            exact applySimulatedPayout_rollback st tx c a pid hrej
        | identityBind a pk _ _ pop =>
            simp only [hp] at hrej ⊢
            exact applyIdentityBind_rollback st tx a pk pop hrej
        | l2Anchor app ep _ _ =>
            simp only [hp] at hrej ⊢
            exact applyL2Anchor_rollback st tx app ep hrej

theorem applyTransfer_valid (st : BitsState) (tx : BitsTx) (s d a : Nat) (hv : ValidBitsState st) :
    ValidBitsState (applyTransfer st tx s d a).2 := by
  unfold applyTransfer
  split
  · exact hv
  · split
    · exact hv
    · split
      · exact hv
      · rename_i hnotlt
        have hle : a ≤ balGet st.balances s := Nat.not_lt.mp hnotlt
        exact {
          cfgValid := hv.cfgValid
          balNoDup := balTransfer_noDup st.balances s d a hv.balNoDup
          conserved := by
            rw [balTransfer_preserves_totalSupply st.balances s d a hv.balNoDup hle]
            exact hv.conserved
        }

theorem applyPurchaseSplit_valid (st : BitsState) (tx : BitsTx) (h b c t a bps oid : Nat)
    (hv : ValidBitsState st) :
    ValidBitsState (applyPurchaseSplit st tx h b c t a bps oid).2 := by
  unfold applyPurchaseSplit
  split
  · exact hv
  · rename_i hpol
    have hbps : bps ≤ bitsBpsDenominator :=
      Nat.le_of_not_lt (fun hgt => hpol (Or.inr (Or.inr (Or.inr (Or.inr (Or.inr hgt))))))
    split
    · exact hv
    · split
      · exact hv
      · split
        · exact hv
        · split
          · exact hv
          · rename_i hnotlt
            have hle : a ≤ balGet st.balances b := Nat.not_lt.mp hnotlt
            have hsplit := bits_purchase_split_exact_conservation a bps hbps
            exact {
              cfgValid := hv.cfgValid
              balNoDup := balPurchaseSplit_noDup st.balances b c t a
                (bitsCreatorShare a bps) (bitsPlatformShare a bps) hv.balNoDup
              conserved := by
                rw [balPurchaseSplit_preserves_totalSupply st.balances b c t a
                  (bitsCreatorShare a bps) (bitsPlatformShare a bps) hv.balNoDup hle hsplit]
                exact hv.conserved
            }

theorem applyTestnetGrant_valid (st : BitsState) (tx : BitsTx)
    (r pk a : Nat) (k : BitsGrantKind) (gid : Nat) (hv : ValidBitsState st) :
    ValidBitsState (applyTestnetGrant st tx r pk a k gid).2 := by
  unfold applyTestnetGrant
  split
  · exact hv
  · split
    · exact hv
    · split
      · exact hv
      · split
        · exact hv
        · split
          · exact hv
          · split
            · exact hv
            · rename_i hnotlt
              have hle : a ≤ balGet st.balances st.cfg.grantPool := Nat.not_lt.mp hnotlt
              exact {
                cfgValid := hv.cfgValid
                balNoDup := balTransfer_noDup st.balances st.cfg.grantPool r a hv.balNoDup
                conserved := by
                  rw [balTransfer_preserves_totalSupply st.balances st.cfg.grantPool r a hv.balNoDup hle]
                  exact hv.conserved
              }

theorem applySimulatedPayout_valid (st : BitsState) (tx : BitsTx) (c a pid : Nat)
    (hv : ValidBitsState st) :
    ValidBitsState (applySimulatedPayout st tx c a pid).2 := by
  unfold applySimulatedPayout
  split
  · exact hv
  · split
    · exact hv
    · split
      · exact hv
      · split
        · exact hv
        · split
          · exact hv
          · rename_i hnotlt
            have hle : a ≤ balGet st.balances c := Nat.not_lt.mp hnotlt
            exact {
              cfgValid := hv.cfgValid
              balNoDup := balTransfer_noDup st.balances c st.cfg.payoutEscrow a hv.balNoDup
              conserved := by
                rw [balTransfer_preserves_totalSupply st.balances c st.cfg.payoutEscrow a hv.balNoDup hle]
                exact hv.conserved
            }

theorem applyIdentityBind_valid (st : BitsState) (tx : BitsTx) (a pk : Nat) (pop : Bool)
    (hv : ValidBitsState st) :
    ValidBitsState (applyIdentityBind st tx a pk pop).2 := by
  unfold applyIdentityBind
  split
  · exact hv
  · split
    · split
      · exact hv
      · split
        · exact { cfgValid := hv.cfgValid, balNoDup := hv.balNoDup, conserved := hv.conserved }
        · exact { cfgValid := hv.cfgValid, balNoDup := hv.balNoDup, conserved := hv.conserved }
    · split
      · exact hv
      · exact { cfgValid := hv.cfgValid, balNoDup := hv.balNoDup, conserved := hv.conserved }

theorem applyL2Anchor_valid (st : BitsState) (tx : BitsTx) (app ep : Nat)
    (hv : ValidBitsState st) :
    ValidBitsState (applyL2Anchor st tx app ep).2 := by
  unfold applyL2Anchor
  split
  · exact hv
  · split
    · split
      · exact hv
      · exact { cfgValid := hv.cfgValid, balNoDup := hv.balNoDup, conserved := hv.conserved }
    · exact { cfgValid := hv.cfgValid, balNoDup := hv.balNoDup, conserved := hv.conserved }

/-- Every transaction across all 6 opcodes preserves `ValidBitsState` (closed supply conservation). -/
theorem applyBitsTx_preserves_valid (st : BitsState) (tx : BitsTx) (h : Nat)
    (hv : ValidBitsState st) :
    ValidBitsState (applyBitsTx st tx h).2 := by
  unfold applyBitsTx
  split
  · exact hv
  · split
    · exact hv
    · split
      · exact hv
      · split
        · exact applyTransfer_valid st tx _ _ _ hv
        · exact applyPurchaseSplit_valid st tx h _ _ _ _ _ _ hv
        · exact applyTestnetGrant_valid st tx _ _ _ _ _ hv
        · exact applySimulatedPayout_valid st tx _ _ _ hv
        · exact applyIdentityBind_valid st tx _ _ _ hv
        · exact applyL2Anchor_valid st tx _ _ hv

theorem applyBitsTx_preserves_totalSupply (st : BitsState) (tx : BitsTx) (h : Nat)
    (hv : ValidBitsState st) :
    balTotalSupply (applyBitsTx st tx h).2.balances = st.cfg.genesisSupply := by
  have hv' := applyBitsTx_preserves_valid st tx h hv
  have hcfg := applyBitsTx_preserves_cfg st tx h
  rw [← hcfg]
  exact hv'.conserved

/-- Multi-transaction / full-block closed supply conservation by induction. -/
theorem applyBitsBlock_preserves_valid (st : BitsState) (txs : List BitsTx) (h : Nat)
    (hv : ValidBitsState st) :
    ValidBitsState (applyBitsBlock st txs h) := by
  induction txs generalizing st with
  | nil => exact hv
  | cons tx rest ih =>
      exact ih (applyBitsTx st tx h).2 (applyBitsTx_preserves_valid st tx h hv)

theorem applyBitsBlock_preserves_totalSupply (st : BitsState) (txs : List BitsTx) (h : Nat)
    (hv : ValidBitsState st) :
    balTotalSupply (applyBitsBlock st txs h).balances = st.cfg.genesisSupply := by
  induction txs generalizing st with
  | nil => exact hv.conserved
  | cons tx rest ih =>
      have hcfg := applyBitsTx_preserves_cfg st tx h
      have hv1 := applyBitsTx_preserves_valid st tx h hv
      have hrec := ih (applyBitsTx st tx h).2 hv1
      rw [hcfg] at hrec
      exact hrec

/-! ## Section 5: Testnet-Only Gate, L2 Anchor Zero-Mint & Amendment 1 Key Rotation -/

theorem testnetGrant_never_admitted_when_lab_false (st : BitsState) (tx : BitsTx) (h : Nat)
    (r pk amt : Nat) (kind : BitsGrantKind) (gid : Nat)
    (hlab : st.cfg.lab = false)
    (hp : tx.payload = .testnetGrant r pk amt kind gid) :
    (applyBitsTx st tx h).1 ≠ .admitted := by
  unfold applyBitsTx
  rw [hp]; dsimp only
  split
  · intro hc; cases hc
  · split
    · intro hc; cases hc
    · split
      · intro hc; cases hc
      · unfold applyTestnetGrant
        rw [if_pos hlab]
        intro hc; cases hc

theorem simulatedPayout_never_admitted_when_lab_false (st : BitsState) (tx : BitsTx) (h : Nat)
    (c amt pid : Nat) (hlab : st.cfg.lab = false)
    (hp : tx.payload = .simulatedPayout c amt pid) :
    (applyBitsTx st tx h).1 ≠ .admitted := by
  unfold applyBitsTx
  rw [hp]; dsimp only
  split
  · intro hc; cases hc
  · split
    · intro hc; cases hc
    · split
      · intro hc; cases hc
      · unfold applySimulatedPayout
        rw [if_pos hlab]
        intro hc; cases hc

theorem l2Anchor_never_mutates_balances (st : BitsState) (tx : BitsTx) (h : Nat)
    (appId epochId l2Root evCount : Nat)
    (hp : tx.payload = .l2Anchor appId epochId l2Root evCount) :
    (applyBitsTx st tx h).2.balances = st.balances := by
  unfold applyBitsTx
  rw [hp]; dsimp only
  split
  · rfl
  · split
    · rfl
    · split
      · rfl
      · unfold applyL2Anchor
        split
        · rfl
        · split
          · split <;> rfl
          · rfl

theorem l2Anchor_rejects_non_increasing_epoch (st : BitsState) (tx : BitsTx) (h : Nat)
    (appId epochId l2Root evCount prevEpoch : Nat)
    (hp : tx.payload = .l2Anchor appId epochId l2Root evCount)
    (hprev : assocFindOpt st.l2Epochs appId = some prevEpoch)
    (hle : epochId ≤ prevEpoch) :
    (applyBitsTx st tx h).1 ≠ .admitted := by
  unfold applyBitsTx
  rw [hp]; dsimp only
  split
  · intro hc; cases hc
  · split
    · intro hc; cases hc
    · split
      · intro hc; cases hc
      · unfold applyL2Anchor
        split
        · intro hc; cases hc
        · rw [hprev]
          dsimp only
          rw [if_pos hle]
          intro hc; cases hc

/-- Cross-Session Amendment 1: `IdentityBind` (initial bind or key rotation) never changes any
    `ActorIdV1` balance (`balances' = balances`). -/
theorem identityBind_preserves_all_balances (st : BitsState) (tx : BitsTx) (h : Nat)
    (actorId newPk prov extHash : Nat) (popValid : Bool)
    (hp : tx.payload = .identityBind actorId newPk prov extHash popValid) :
    (applyBitsTx st tx h).2.balances = st.balances := by
  unfold applyBitsTx
  rw [hp]; dsimp only
  split
  · rfl
  · split
    · rfl
    · split
      · rfl
      · unfold applyIdentityBind
        split
        · rfl
        · split
          · split
            · rfl
            · split <;> rfl
          · split <;> rfl

/-- On key rotation (`newPk ≠ curPk`), every delegation for `actorId` becomes `revoked = true`. -/
theorem revokeActorDelegations_marks_all_revoked (dels : List BitsDelegationGrant) (actor : Nat) :
    ∀ g ∈ revokeActorDelegations dels actor, g.principalActor = actor → g.revoked = true := by
  intro g hg hprin
  unfold revokeActorDelegations at hg
  rw [List.mem_map] at hg
  rcases hg with ⟨g0, _, hEq⟩
  by_cases hp0 : g0.principalActor = actor
  · simp only [if_pos hp0] at hEq
    subst hEq
    rfl
  · simp only [if_neg hp0] at hEq
    subst hEq
    exact False.elim (hp0 hprin)

theorem assocFindOpt_assocSetNat_same (m : List (Nat × Nat)) (k v : Nat) :
    assocFindOpt (assocSetNat m k v) k = some v := by
  induction m with
  | nil => simp [assocSetNat, assocFindOpt]
  | cons hd tl ih =>
      rcases hd with ⟨k0, v0⟩
      by_cases hk : k0 = k
      · subst hk; simp [assocSetNat, assocFindOpt]
      · simp [assocSetNat, assocFindOpt, hk, ih]

theorem identityBind_rotation_updates_key_and_revokes_delegations
    (st : BitsState) (tx : BitsTx) (h : Nat)
    (actorId curPk newPk prov extHash : Nat)
    (hchain : tx.chainId = st.cfg.chainId)
    (hsig : tx.sigValid = true)
    (hvalid : tx.validUntilHeight = 0 ∨ h ≤ tx.validUntilHeight)
    (hseq : tx.seq = assocGetNat st.nonces tx.senderActor + 1)
    (hsender : actorId = tx.senderActor)
    (hcur : assocFindOpt st.actorKeys actorId = some curPk)
    (hsigner : tx.signerPk = curPk)
    (hneq : newPk ≠ curPk)
    (hp : tx.payload = .identityBind actorId newPk prov extHash true) :
    (applyBitsTx st tx h).1 = .admitted ∧
    assocFindOpt (applyBitsTx st tx h).2.actorKeys actorId = some newPk ∧
    (applyBitsTx st tx h).2.balances = st.balances ∧
    (∀ g ∈ (applyBitsTx st tx h).2.delegations, g.principalActor = actorId → g.revoked = true) := by
  have h1 : ¬ (tx.chainId ≠ st.cfg.chainId ∨ tx.sigValid = false) := by
    intro hc; rcases hc with hne | hf
    · exact hne hchain
    · rw [hsig] at hf; cases hf
  have h2 : ¬ (tx.validUntilHeight ≠ 0 ∧ h > tx.validUntilHeight) := by
    intro ⟨hnz, hgt⟩
    rcases hvalid with hz | hle
    · exact hnz hz
    · omega
  have h3 : ¬ (tx.seq ≠ assocGetNat st.nonces tx.senderActor + 1) := by
    intro hne; exact hne hseq
  have h4 : ¬ (actorId ≠ tx.senderActor ∨ true = false) := by
    intro hc; rcases hc with hne | hf
    · exact hne hsender
    · cases hf
  have h5 : ¬ (tx.signerPk ≠ curPk) := by intro hne; exact hne hsigner
  unfold applyBitsTx
  rw [if_neg h1, if_neg h2, if_neg h3, hp]
  dsimp only
  unfold applyIdentityBind
  rw [if_neg h4, hcur]
  dsimp only
  rw [if_neg h5, if_neg hneq]
  refine ⟨rfl, assocFindOpt_assocSetNat_same st.actorKeys actorId newPk, rfl, ?_⟩
  exact revokeActorDelegations_marks_all_revoked st.delegations actorId

/-! ## Section 6: Delegation Cap Soundness, Non-Transitivity & Replay Monotonicity -/

theorem consumePurchaseDelegation_requires_active_within_caps
    (dels dels' : List BitsDelegationGrant) (buyer signerPk amt h : Nat)
    (hres : consumePurchaseDelegation dels buyer signerPk amt h = some dels') :
    ∃ g ∈ dels,
      g.principalActor = buyer ∧ g.delegatePk = signerPk ∧
      g.revoked = false ∧ g.allowPurchaseSplit = true ∧
      h ≤ g.expiresAtHeight ∧ amt ≤ g.maxBitsPerTx ∧ g.spentBits + amt ≤ g.maxBitsTotal := by
  induction dels generalizing dels' with
  | nil =>
      simp [consumePurchaseDelegation] at hres
  | cons g rest ih =>
      unfold consumePurchaseDelegation at hres
      by_cases hm : g.principalActor = buyer ∧ g.delegatePk = signerPk
      · rw [if_pos hm] at hres
        by_cases hcond : (!g.revoked && g.allowPurchaseSplit && decide (h ≤ g.expiresAtHeight) &&
            decide (amt ≤ g.maxBitsPerTx) && decide (g.spentBits + amt ≤ g.maxBitsTotal)) = true
        · simp only [Bool.and_eq_true, Bool.not_eq_true', decide_eq_true_eq] at hcond
          rcases hcond with ⟨⟨⟨⟨hrev, hall⟩, hexp⟩, hptx⟩, htot⟩
          exact ⟨g, List.Mem.head rest, hm.1, hm.2, hrev, hall, hexp, hptx, htot⟩
        · rw [if_neg hcond] at hres
          cases hres
      · rw [if_neg hm] at hres
        cases hrec : consumePurchaseDelegation rest buyer signerPk amt h with
        | none =>
            rw [hrec] at hres
            cases hres
        | some r' =>
            rcases ih r' hrec with ⟨g', hmem, hprops⟩
            exact ⟨g', List.Mem.tail g hmem, hprops⟩

theorem delegation_cannot_authorize_transfer (st : BitsState) (tx : BitsTx) (h : Nat)
    (src dst amt : Nat)
    (hp : tx.payload = .transfer src dst amt)
    (hnotPrin : assocFindOpt st.actorKeys src ≠ some tx.signerPk) :
    (applyBitsTx st tx h).1 ≠ .admitted := by
  unfold applyBitsTx
  rw [hp]; dsimp only
  split
  · intro hc; cases hc
  · split
    · intro hc; cases hc
    · split
      · intro hc; cases hc
      · unfold applyTransfer
        split
        · intro hc; cases hc
        · rw [if_pos (Or.inr hnotPrin)]
          intro hc; cases hc

theorem delegation_cannot_authorize_payout (st : BitsState) (tx : BitsTx) (h : Nat)
    (creator amt pid : Nat)
    (hp : tx.payload = .simulatedPayout creator amt pid)
    (hnotPrin : assocFindOpt st.actorKeys creator ≠ some tx.signerPk) :
    (applyBitsTx st tx h).1 ≠ .admitted := by
  unfold applyBitsTx
  rw [hp]; dsimp only
  split
  · intro hc; cases hc
  · split
    · intro hc; cases hc
    · split
      · intro hc; cases hc
      · unfold applySimulatedPayout
        split
        · intro hc; cases hc
        · split
          · intro hc; cases hc
          · rw [if_pos (Or.inr hnotPrin)]
            intro hc; cases hc

/-- Non-transitive depth-2 delegation is structurally refused by `registerDelegation`. -/
theorem registerDelegation_rejects_depth2_transitive
    (st : BitsState) (g1 g2 : BitsDelegationGrant) (midPk : Nat)
    (hg1 : g1 ∈ st.delegations)
    (hmid : assocFindOpt st.actorKeys g2.principalActor = some midPk)
    (hchain : g1.delegatePk = midPk) :
    (registerDelegation st g2).1 ≠ .admitted := by
  unfold registerDelegation
  split
  · intro hc; cases hc
  · rw [hmid]; dsimp only
    have hany : isActiveDelegate st.delegations midPk = true := by
      unfold isActiveDelegate
      rw [List.any_eq_true]
      exact ⟨g1, hg1, by simp [hchain]⟩
    rw [if_pos hany]
    intro hc; cases hc

theorem duplicate_order_rejected (st : BitsState) (tx : BitsTx) (h : Nat)
    (buyer creator treasury amt bps orderId : Nat)
    (hp : tx.payload = .purchaseSplit buyer creator treasury amt bps orderId)
    (hseen : orderId ∈ st.usedOrders) :
    (applyBitsTx st tx h).1 ≠ .admitted := by
  unfold applyBitsTx
  rw [hp]; dsimp only
  split
  · intro hc; cases hc
  · split
    · intro hc; cases hc
    · split
      · intro hc; cases hc
      · unfold applyPurchaseSplit
        split
        · intro hc; cases hc
        · split
          · intro hc; cases hc
          · intro hc; cases hc

theorem duplicate_grant_rejected (st : BitsState) (tx : BitsTx) (h : Nat)
    (r pk amt : Nat) (kind : BitsGrantKind) (gid : Nat)
    (hp : tx.payload = .testnetGrant r pk amt kind gid)
    (hseen : gid ∈ st.usedGrants) :
    (applyBitsTx st tx h).1 ≠ .admitted := by
  unfold applyBitsTx
  rw [hp]; dsimp only
  split
  · intro hc; cases hc
  · split
    · intro hc; cases hc
    · split
      · intro hc; cases hc
      · unfold applyTestnetGrant
        split
        · intro hc; cases hc
        · split
          · intro hc; cases hc
          · split
            · intro hc; cases hc
            · intro hc; cases hc

theorem duplicate_payout_rejected (st : BitsState) (tx : BitsTx) (h : Nat)
    (creator amt pid : Nat)
    (hp : tx.payload = .simulatedPayout creator amt pid)
    (hseen : pid ∈ st.usedPayouts) :
    (applyBitsTx st tx h).1 ≠ .admitted := by
  unfold applyBitsTx
  rw [hp]; dsimp only
  split
  · intro hc; cases hc
  · split
    · intro hc; cases hc
    · split
      · intro hc; cases hc
      · unfold applySimulatedPayout
        split
        · intro hc; cases hc
        · split
          · intro hc; cases hc
          · split
            · intro hc; cases hc
            · intro hc; cases hc

theorem usedOrders_monotone (st : BitsState) (tx : BitsTx) (h oid : Nat)
    (hmem : oid ∈ st.usedOrders) :
    oid ∈ (applyBitsTx st tx h).2.usedOrders := by
  unfold applyBitsTx
  split
  · exact hmem
  · split
    · exact hmem
    · split
      · exact hmem
      · split
        · unfold applyTransfer
          split
          · exact hmem
          · split
            · exact hmem
            · split <;> exact hmem
        · unfold applyPurchaseSplit
          split
          · exact hmem
          · split
            · exact hmem
            · split
              · exact hmem
              · split
                · exact hmem
                · split
                  · exact hmem
                  · exact List.Mem.tail _ hmem
        · unfold applyTestnetGrant
          split
          · exact hmem
          · split
            · exact hmem
            · split
              · exact hmem
              · split
                · exact hmem
                · split
                  · exact hmem
                  · split <;> exact hmem
        · unfold applySimulatedPayout
          split
          · exact hmem
          · split
            · exact hmem
            · split
              · exact hmem
              · split
                · exact hmem
                · split <;> exact hmem
        · unfold applyIdentityBind
          split
          · exact hmem
          · split
            · split
              · exact hmem
              · split <;> exact hmem
            · split <;> exact hmem
        · unfold applyL2Anchor
          split
          · exact hmem
          · split
            · split <;> exact hmem
            · exact hmem

theorem usedGrants_monotone (st : BitsState) (tx : BitsTx) (h gid : Nat)
    (hmem : gid ∈ st.usedGrants) :
    gid ∈ (applyBitsTx st tx h).2.usedGrants := by
  unfold applyBitsTx
  split
  · exact hmem
  · split
    · exact hmem
    · split
      · exact hmem
      · split
        · unfold applyTransfer
          split
          · exact hmem
          · split
            · exact hmem
            · split <;> exact hmem
        · unfold applyPurchaseSplit
          split
          · exact hmem
          · split
            · exact hmem
            · split
              · exact hmem
              · split
                · exact hmem
                · split <;> exact hmem
        · unfold applyTestnetGrant
          split
          · exact hmem
          · split
            · exact hmem
            · split
              · exact hmem
              · split
                · exact hmem
                · split
                  · exact hmem
                  · split
                    · exact hmem
                    · exact List.Mem.tail _ hmem
        · unfold applySimulatedPayout
          split
          · exact hmem
          · split
            · exact hmem
            · split
              · exact hmem
              · split
                · exact hmem
                · split <;> exact hmem
        · unfold applyIdentityBind
          split
          · exact hmem
          · split
            · split
              · exact hmem
              · split <;> exact hmem
            · split <;> exact hmem
        · unfold applyL2Anchor
          split
          · exact hmem
          · split
            · split <;> exact hmem
            · exact hmem

theorem usedPayouts_monotone (st : BitsState) (tx : BitsTx) (h pid : Nat)
    (hmem : pid ∈ st.usedPayouts) :
    pid ∈ (applyBitsTx st tx h).2.usedPayouts := by
  unfold applyBitsTx
  split
  · exact hmem
  · split
    · exact hmem
    · split
      · exact hmem
      · split
        · unfold applyTransfer
          split
          · exact hmem
          · split
            · exact hmem
            · split <;> exact hmem
        · unfold applyPurchaseSplit
          split
          · exact hmem
          · split
            · exact hmem
            · split
              · exact hmem
              · split
                · exact hmem
                · split <;> exact hmem
        · unfold applyTestnetGrant
          split
          · exact hmem
          · split
            · exact hmem
            · split
              · exact hmem
              · split
                · exact hmem
                · split
                  · exact hmem
                  · split <;> exact hmem
        · unfold applySimulatedPayout
          split
          · exact hmem
          · split
            · exact hmem
            · split
              · exact hmem
              · split
                · exact hmem
                · split
                  · exact hmem
                  · exact List.Mem.tail _ hmem
        · unfold applyIdentityBind
          split
          · exact hmem
          · split
            · split
              · exact hmem
              · split <;> exact hmem
            · split <;> exact hmem
        · unfold applyL2Anchor
          split
          · exact hmem
          · split
            · split <;> exact hmem
            · exact hmem

/-! ## Section 7: Powered Negative Controls -/

/-- Negative control 1: any `creatorBps > 10_000` can over-credit `creatorShare > amount`,
    breaking exact split conservation. -/
theorem unbounded_bps_breaks_split_conservation :
    ∃ amount bps, bps > bitsBpsDenominator ∧ bitsCreatorShare amount bps > amount := by
  exact ⟨10000, 10001, by decide, by decide⟩

/-- Negative control 2: any positive transfer tax `tax > 0` strictly violates zero-tax
    two-party conservation (`recipientCredit = amount - tax < amount`). -/
theorem positive_transfer_tax_violates_zero_tax_transfer (amount tax : Nat)
    (hpos : 0 < tax) (hle : tax ≤ amount) :
    amount - tax < amount := by
  omega

/-- Negative control 3: minting `amt > 0` out of thin air (crediting `recipient` without
    debiting `grantPool`) strictly breaks closed `genesisSupply` conservation. -/
theorem unbacked_mint_breaks_closed_conservation (m : BitsBalMap) (recipient amt : Nat)
    (hnd : BalNoDup m) (hpos : 0 < amt) :
    balTotalSupply (balSet m recipient (balGet m recipient + amt)) ≠ balTotalSupply m := by
  have h := balTotalSupply_balSet m recipient (balGet m recipient + amt) hnd
  omega

/-- Negative control 4: a `lab = false` genesis config cannot admit any `TestnetGrant`. -/
theorem lab_false_cannot_admit_signup_grant (st : BitsState) (tx : BitsTx) (h r pk gid : Nat)
    (hlab : st.cfg.lab = false)
    (hp : tx.payload = .testnetGrant r pk 1000 .signup gid) :
    (applyBitsTx st tx h).1 ≠ .admitted :=
  testnetGrant_never_admitted_when_lab_false st tx h r pk 1000 .signup gid hlab hp

end BitblockProof
