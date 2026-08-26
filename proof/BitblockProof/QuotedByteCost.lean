import Std

namespace BitblockProof.QuotedByteCost

/-!
This file proves only the static representation cost of the proposed quoted-byte
codec. `Term` deliberately has no evaluator, and the byte/list encoders do not
interpret Bitblock edits. In particular, these theorems make no claim about
`on`, `off`, `toggle`, reduction fuel, simulation time, or rendering work.

The cost model is the declared naive SK-to-iota upper bound: every `I`, `K`, and
`S` leaf contributes one atom and respectively 2, 4, and 5 iota leaves.
Application nodes contribute structure, but no additional leaf cost.
-/

/-- Closed SK terms used solely as representation syntax. -/
inductive Term where
  | i
  | k
  | s
  | app (fn arg : Term)
deriving DecidableEq, Repr

/-- Number of primitive SK leaves in a representation. -/
def atoms : Term → Nat
  | .i | .k | .s => 1
  | .app fn arg => atoms fn + atoms arg

/-- Naive iota-leaf upper bound: `I = 2`, `K = 4`, and `S = 5`. -/
def iota : Term → Nat
  | .i => 2
  | .k => 4
  | .s => 5
  | .app fn arg => iota fn + iota arg

/-- Exact primitive census projections used to audit the weighted iota total. -/
def countI : Term → Nat
  | .i => 1
  | .k | .s => 0
  | .app fn arg => countI fn + countI arg

def countK : Term → Nat
  | .k => 1
  | .i | .s => 0
  | .app fn arg => countK fn + countK arg

def countS : Term → Nat
  | .s => 1
  | .i | .k => 0
  | .app fn arg => countS fn + countS arg

/-- The named tick projection of an iota representation. -/
def ticks (term : Term) : Nat := iota term - 1

/-- `FALSE = K I`, the standard optimized abstraction of `λa. λb. b`. -/
def falseTerm : Term := .app .k .i

/-- The two distinct, equal-cost bit symbols. -/
def B0 : Term := .app .i falseTerm
def B1 : Term := .app falseTerm .i

def encodeBit : Bool → Term
  | false => B0
  | true => B1

theorem B0_ne_B1 : B0 ≠ B1 := by
  decide

@[simp] theorem atoms_B0 : atoms B0 = 3 := by
  rfl

@[simp] theorem atoms_B1 : atoms B1 = 3 := by
  rfl

@[simp] theorem iota_B0 : iota B0 = 8 := by
  rfl

@[simp] theorem iota_B1 : iota B1 = 8 := by
  rfl

/-- Content changes the chosen closed term, but not either representation cost. -/
theorem bit_symbols_content_neutral_cost :
    atoms B0 = atoms B1 ∧ iota B0 = iota B1 := by
  simp

@[simp] theorem atoms_encodeBit (bit : Bool) : atoms (encodeBit bit) = 3 := by
  cases bit <;> rfl

@[simp] theorem iota_encodeBit (bit : Bool) : iota (encodeBit bit) = 8 := by
  cases bit <;> rfl

@[simp] theorem countI_encodeBit (bit : Bool) : countI (encodeBit bit) = 2 := by
  cases bit <;> rfl

@[simp] theorem countK_encodeBit (bit : Bool) : countK (encodeBit bit) = 1 := by
  cases bit <;> rfl

@[simp] theorem countS_encodeBit (bit : Bool) : countS (encodeBit bit) = 0 := by
  cases bit <;> rfl

/--
The closed SK term produced by optimized bracket abstraction of
`λx. λy. λf. f x y`. Its census is `S = 10`, `K = 10`, `I = 3`.
-/
private def app2 (fn left right : Term) : Term :=
  .app (.app fn left) right

private def abstractKAppliedToBound : Term :=
  app2 .s (.app .k .k) .i

def pairTerm : Term :=
  app2 .s
    (app2 .s (.app .k .s)
      (app2 .s (.app .k .k)
        (app2 .s (.app .k .s)
          (app2 .s (.app .k (.app .s .i)) abstractKAppliedToBound))))
    (.app .k abstractKAppliedToBound)

@[simp] theorem atoms_pairTerm : atoms pairTerm = 23 := by
  rfl

@[simp] theorem iota_pairTerm : iota pairTerm = 96 := by
  rfl

@[simp] theorem countI_pairTerm : countI pairTerm = 3 := by
  rfl

@[simp] theorem countK_pairTerm : countK pairTerm = 10 := by
  rfl

@[simp] theorem countS_pairTerm : countS pairTerm = 10 := by
  rfl

/-- One fixed `PAIR` node applied to its two represented children. -/
def pairNode (left right : Term) : Term :=
  .app (.app pairTerm left) right

@[simp] theorem atoms_pairNode (left right : Term) :
    atoms (pairNode left right) = 23 + atoms left + atoms right := by
  simp [pairNode, atoms]

@[simp] theorem iota_pairNode (left right : Term) :
    iota (pairNode left right) = 96 + iota left + iota right := by
  simp [pairNode, iota]

@[simp] theorem countI_pairNode (left right : Term) :
    countI (pairNode left right) = 3 + countI left + countI right := by
  simp [pairNode, countI]

@[simp] theorem countK_pairNode (left right : Term) :
    countK (pairNode left right) = 10 + countK left + countK right := by
  simp [pairNode, countK]

@[simp] theorem countS_pairNode (left right : Term) :
    countS (pairNode left right) = 10 + countS left + countS right := by
  simp [pairNode, countS]

/-- Exactly eight bits, with index zero designated the most-significant bit. -/
abbrev Byte := Fin 8 → Bool

/-- An MSB-first right spine containing exactly seven `PAIR` nodes. -/
def encodeByte (byte : Byte) : Term :=
  pairNode (encodeBit (byte 0))
    (pairNode (encodeBit (byte 1))
      (pairNode (encodeBit (byte 2))
        (pairNode (encodeBit (byte 3))
          (pairNode (encodeBit (byte 4))
            (pairNode (encodeBit (byte 5))
              (pairNode (encodeBit (byte 6)) (encodeBit (byte 7))))))))

/-- Deliberately wrong association, retained as a powered structural control. -/
def encodeByteLeftMutant (byte : Byte) : Term :=
  pairNode
    (pairNode
      (pairNode
        (pairNode
          (pairNode
            (pairNode
              (pairNode (encodeBit (byte 0)) (encodeBit (byte 1)))
              (encodeBit (byte 2)))
            (encodeBit (byte 3)))
          (encodeBit (byte 4)))
        (encodeBit (byte 5)))
      (encodeBit (byte 6)))
    (encodeBit (byte 7))

/-- Deliberately wrong bit order, retained as a powered structural control. -/
def encodeByteReversedMutant (byte : Byte) : Term :=
  pairNode (encodeBit (byte 7))
    (pairNode (encodeBit (byte 6))
      (pairNode (encodeBit (byte 5))
        (pairNode (encodeBit (byte 4))
          (pairNode (encodeBit (byte 3))
            (pairNode (encodeBit (byte 2))
              (pairNode (encodeBit (byte 1)) (encodeBit (byte 0))))))))

def asymmetricByte : Byte := fun index => index = 0 || index = 3 || index = 6

theorem byte_pair_association_control :
    encodeByte asymmetricByte ≠ encodeByteLeftMutant asymmetricByte := by
  decide

theorem byte_bit_order_control :
    encodeByte asymmetricByte ≠ encodeByteReversedMutant asymmetricByte := by
  decide

@[simp] theorem atoms_encodeByte (byte : Byte) : atoms (encodeByte byte) = 185 := by
  simp [encodeByte]

@[simp] theorem iota_encodeByte (byte : Byte) : iota (encodeByte byte) = 736 := by
  simp [encodeByte]

@[simp] theorem countI_encodeByte (byte : Byte) : countI (encodeByte byte) = 37 := by
  simp [encodeByte]

@[simp] theorem countK_encodeByte (byte : Byte) : countK (encodeByte byte) = 78 := by
  simp [encodeByte]

@[simp] theorem countS_encodeByte (byte : Byte) : countS (encodeByte byte) = 70 := by
  simp [encodeByte]

/-- Exact, content-neutral representation cost of every fixed-width byte. -/
theorem byte_representation_cost (byte : Byte) :
    atoms (encodeByte byte) = 185 ∧ iota (encodeByte byte) = 736 := by
  simp

/-- The list terminator is representation-identical to `FALSE = K I`. -/
def nilTerm : Term := falseTerm

/-- Bytes are joined by application nodes in a fixed right spine ending in `NIL`. -/
def encodeBytes : List Byte → Term
  | [] => nilTerm
  | byte :: rest => .app (encodeByte byte) (encodeBytes rest)

def encodeBytesLeftMutant (bytes : List Byte) : Term :=
  bytes.foldl (fun rest byte => .app rest (encodeByte byte)) nilTerm

theorem byte_list_association_control :
    encodeBytes [asymmetricByte, fun _ => false] ≠
      encodeBytesLeftMutant [asymmetricByte, fun _ => false] := by
  decide

@[simp] theorem atoms_nilTerm : atoms nilTerm = 2 := by
  rfl

@[simp] theorem iota_nilTerm : iota nilTerm = 6 := by
  rfl

theorem atoms_encodeBytes (bytes : List Byte) :
    atoms (encodeBytes bytes) = 185 * bytes.length + 2 := by
  induction bytes with
  | nil => rfl
  | cons byte rest ih =>
      simp only [encodeBytes, atoms, atoms_encodeByte, List.length_cons]
      rw [ih, Nat.mul_succ]
      omega

theorem iota_encodeBytes (bytes : List Byte) :
    iota (encodeBytes bytes) = 736 * bytes.length + 6 := by
  induction bytes with
  | nil => rfl
  | cons byte rest ih =>
      simp only [encodeBytes, iota, iota_encodeByte, List.length_cons]
      rw [ih, Nat.mul_succ]
      omega

theorem countI_encodeBytes (bytes : List Byte) :
    countI (encodeBytes bytes) = 37 * bytes.length + 1 := by
  induction bytes with
  | nil => rfl
  | cons byte rest ih =>
      simp only [encodeBytes, countI, countI_encodeByte, List.length_cons]
      rw [ih, Nat.mul_succ]
      omega

theorem countK_encodeBytes (bytes : List Byte) :
    countK (encodeBytes bytes) = 78 * bytes.length + 1 := by
  induction bytes with
  | nil => rfl
  | cons byte rest ih =>
      simp only [encodeBytes, countK, countK_encodeByte, List.length_cons]
      rw [ih, Nat.mul_succ]
      omega

theorem countS_encodeBytes (bytes : List Byte) :
    countS (encodeBytes bytes) = 70 * bytes.length := by
  induction bytes with
  | nil => rfl
  | cons byte rest ih =>
      simp only [encodeBytes, countS, countS_encodeByte, List.length_cons]
      rw [ih, Nat.mul_succ]
      omega

theorem ticks_encodeBytes (bytes : List Byte) :
    ticks (encodeBytes bytes) = 736 * bytes.length + 5 := by
  rw [ticks, iota_encodeBytes]
  omega

/--
The bounded codec theorem. It quantifies over finite byte lists and establishes
only their static representation cost; it does not state executable semantics.
-/
theorem quoted_bytes_representation_cost (bytes : List Byte) :
    countS (encodeBytes bytes) = 70 * bytes.length ∧
    countK (encodeBytes bytes) = 78 * bytes.length + 1 ∧
    countI (encodeBytes bytes) = 37 * bytes.length + 1 ∧
    atoms (encodeBytes bytes) = 185 * bytes.length + 2 ∧
    iota (encodeBytes bytes) = 736 * bytes.length + 6 ∧
    ticks (encodeBytes bytes) = 736 * bytes.length + 5 := by
  exact ⟨countS_encodeBytes bytes, countK_encodeBytes bytes,
    countI_encodeBytes bytes, atoms_encodeBytes bytes,
    iota_encodeBytes bytes, ticks_encodeBytes bytes⟩

end BitblockProof.QuotedByteCost
