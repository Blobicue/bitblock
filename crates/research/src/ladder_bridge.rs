//! Verifier-first quoted-byte bridge into the pinned LadderLang cost model.
//!
//! This module admits exactly one source schema: a canonical standalone [`Edit`]. It verifies and
//! re-tags the edit before quoting the *complete* content-tag preimage. The quote is then expanded
//! into a closed LadderLang `Lam` shape under one content-tagged policy. The resulting cost is a
//! static representation upper bound; it is not edit execution, beta fuel, simulation work,
//! renderer work, currency value, or consensus weight.

use std::fmt;

use crate::program::{EDIT_CANONICAL_LEN, EDIT_SCHEMA, Edit, EditCodecError};
use crate::protocol::{ContentTag, TagError, TagKind, encode_canonical_preimage, hash_canonical};

/// Domain prefix prepended to canonical content-tag preimages before quoting into the LadderLang `Lam` representation.
pub const BRIDGE_NAME: &[u8] = b"bb-lam/v1";
/// Canonical protocol schema string for the LadderLang quoted-byte representation policy tag.
pub const BRIDGE_POLICY_SCHEMA: &[u8] = b"bitblock/ladderlang/quoted-byte-policy/v1";
/// Canonical protocol schema string for expanded closed LadderLang `Lam` prefix-encoded terms.
pub const LADDER_TERM_SCHEMA: &[u8] = b"bitblock/ladderlang/lam-prefix-vla-v1";
/// Canonical protocol schema string for verified LadderLang quoted-byte cost and transition receipts.
pub const LADDER_QUOTE_RECEIPT_SCHEMA: &[u8] = b"bitblock/ladderlang/quote-receipt/v1";

const BRIDGE_POLICY_PAYLOAD: &[u8] = concat!(
    "bridge=bb-lam/v1;",
    "ladderlang-commit=59042265167b8f40700a891d890fde8015bcade9;",
    "source=full-content-tag-preimage;quoted-prefix=bb-lam/v1;",
    "bit-order=msb-first;B0=App(I,FALSE);B1=App(FALSE,I);B(0)=B0;B(1)=B1;",
    "Byte(b7..b0)=Pair(B(b7),Pair(B(b6),Pair(B(b5),Pair(B(b4),",
    "Pair(B(b3),Pair(B(b2),Pair(B(b1),B(b0)))))));",
    "Bytes([])=NIL,Bytes(x::xs)=App(Byte(x),Bytes(xs));",
    "LamPrefix=Var(n):octet(00)||u64be(n),Lam(x):octet(01)||x,",
    "App(f,a):octet(02)||f||a;",
    "cost=Lam.costChecked;scope=representation-only"
)
.as_bytes();

const EDIT_TAG_PREIMAGE_BYTES: usize = 103;
const EDIT_QUOTED_BYTES: usize = BRIDGE_NAME.len() + EDIT_TAG_PREIMAGE_BYTES;
const RECEIPT_PAYLOAD_BYTES: u64 = 3 * 32 + 4 * 8;

const VAR_NODE: u8 = 0;
const LAM_NODE: u8 = 1;
const APP_NODE: u8 = 2;

/// One candidate canonical object presented to the bridge verifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BridgeObject<'a> {
    kind: TagKind,
    schema: &'a [u8],
    payload: &'a [u8],
}

impl<'a> BridgeObject<'a> {
    /// Constructs a candidate `BridgeObject` from a claimed `TagKind`, schema slice, and canonical payload slice.
    pub const fn new(kind: TagKind, schema: &'a [u8], payload: &'a [u8]) -> Self {
        Self {
            kind,
            schema,
            payload,
        }
    }

    /// Returns the claimed `TagKind` domain of this candidate LadderLang bridge object.
    pub const fn kind(self) -> TagKind {
        self.kind
    }

    /// Returns the claimed canonical schema byte slice of this candidate LadderLang bridge object.
    pub const fn schema(self) -> &'a [u8] {
        self.schema
    }

    /// Returns the borrowed canonical payload bytes presented to the LadderLang bridge verifier.
    pub const fn payload(self) -> &'a [u8] {
        self.payload
    }
}

/// Independently checkable static projections of one expanded quoted-byte term.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LadderMetrics {
    quoted_bytes: u64,
    lam_prefix_bytes: u64,
    lam_size: u64,
    lam_variable_nodes: u64,
    lam_application_nodes: u64,
    binder_depth: u64,
    total_binders: u64,
    computed_layer: u64,
    s_atoms: u64,
    k_atoms: u64,
    i_atoms: u64,
    sk_application_nodes: u64,
    sk_surviving_variables: u64,
    atoms_upper: u64,
    iota_upper: u64,
    ticks: u64,
}

impl LadderMetrics {
    fn for_quote_len(length: usize) -> Result<Self, BridgeError> {
        let length = u64::try_from(length).map_err(|_| BridgeError::ArithmeticOverflow)?;
        let linear = |coefficient: u64, constant: u64| {
            coefficient
                .checked_mul(length)
                .and_then(|value| value.checked_add(constant))
                .ok_or(BridgeError::ArithmeticOverflow)
        };
        let s_atoms = linear(70, 0)?;
        let k_atoms = linear(78, 1)?;
        let i_atoms = linear(37, 1)?;
        let atoms_upper = linear(185, 2)?;
        let iota_upper = linear(736, 6)?;
        let ticks = iota_upper
            .checked_sub(1)
            .ok_or(BridgeError::ArithmeticOverflow)?;

        let metrics = Self {
            quoted_bytes: length,
            lam_prefix_bytes: linear(415, 11)?,
            lam_size: linear(119, 3)?,
            lam_variable_nodes: linear(37, 1)?,
            lam_application_nodes: linear(37, 0)?,
            binder_depth: 3,
            total_binders: linear(45, 2)?,
            computed_layer: 4,
            s_atoms,
            k_atoms,
            i_atoms,
            sk_application_nodes: atoms_upper
                .checked_sub(1)
                .ok_or(BridgeError::ArithmeticOverflow)?,
            sk_surviving_variables: 0,
            atoms_upper,
            iota_upper,
            ticks,
        };
        if metrics.atoms_upper
            != metrics
                .s_atoms
                .checked_add(metrics.k_atoms)
                .and_then(|value| value.checked_add(metrics.i_atoms))
                .ok_or(BridgeError::ArithmeticOverflow)?
            || metrics.iota_upper
                != metrics
                    .s_atoms
                    .checked_mul(5)
                    .and_then(|value| {
                        metrics
                            .k_atoms
                            .checked_mul(4)
                            .and_then(|k| value.checked_add(k))
                    })
                    .and_then(|value| {
                        metrics
                            .i_atoms
                            .checked_mul(2)
                            .and_then(|i| value.checked_add(i))
                    })
                    .ok_or(BridgeError::ArithmeticOverflow)?
        {
            return Err(BridgeError::CostInvariant);
        }
        Ok(metrics)
    }

    /// Returns the total count of quoted preimage bytes (including `BRIDGE_NAME`) expanded into LadderLang `Lam`.
    pub const fn quoted_bytes(self) -> u64 {
        self.quoted_bytes
    }

    /// Returns the exact byte length of the expanded closed LadderLang `Lam` prefix-encoded term.
    pub const fn lam_prefix_bytes(self) -> u64 {
        self.lam_prefix_bytes
    }

    /// Returns the total AST node count of the expanded closed LadderLang `Lam` representation.
    pub const fn lam_size(self) -> u64 {
        self.lam_size
    }

    /// Returns the count of De Bruijn `Var` nodes inside the expanded LadderLang `Lam` term.
    pub const fn lam_variable_nodes(self) -> u64 {
        self.lam_variable_nodes
    }

    /// Returns the count of binary `App` nodes inside the expanded LadderLang `Lam` term.
    pub const fn lam_application_nodes(self) -> u64 {
        self.lam_application_nodes
    }

    /// Returns the maximum nested lambda binder depth of the quoted-byte LadderLang `Lam` term.
    pub const fn binder_depth(self) -> u64 {
        self.binder_depth
    }

    /// Returns the total count of `Lam` abstraction binders across the expanded LadderLang term.
    pub const fn total_binders(self) -> u64 {
        self.total_binders
    }

    /// Returns the static LadderLang stratification layer computed for the closed quoted-byte term.
    pub const fn computed_layer(self) -> u64 {
        self.computed_layer
    }

    /// Returns the projected count of `S` combinator atoms after LadderLang bracket abstraction.
    pub const fn s_atoms(self) -> u64 {
        self.s_atoms
    }

    /// Returns the projected count of `K` combinator atoms after LadderLang bracket abstraction.
    pub const fn k_atoms(self) -> u64 {
        self.k_atoms
    }

    /// Returns the projected count of `I` combinator atoms after LadderLang bracket abstraction.
    pub const fn i_atoms(self) -> u64 {
        self.i_atoms
    }

    /// Returns the projected count of combinator application nodes in the lowered SKI tree.
    pub const fn sk_application_nodes(self) -> u64 {
        self.sk_application_nodes
    }

    /// Returns the count of free variables surviving SKI lowering (always zero for closed quotes).
    pub const fn sk_surviving_variables(self) -> u64 {
        self.sk_surviving_variables
    }

    /// Returns the static upper bound on total SKI combinator atoms (`S + K + I`) for the quote.
    pub const fn atoms_upper(self) -> u64 {
        self.atoms_upper
    }

    /// Returns the static upper bound on weighted Iota combinator cost (`5*S + 4*K + 2*I`).
    pub const fn iota_upper(self) -> u64 {
        self.iota_upper
    }

    /// Returns the static representation tick count (`iota_upper - 1`) under the pinned LadderLang cost model.
    pub const fn ticks(self) -> u64 {
        self.ticks
    }
}

/// A verified source quote and its compact, content-addressed receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedLadderQuote {
    source_tag: ContentTag,
    policy_tag: ContentTag,
    term_tag: ContentTag,
    receipt_tag: ContentTag,
    quoted_bytes: Vec<u8>,
    metrics: LadderMetrics,
}

impl VerifiedLadderQuote {
    /// Returns the verified canonical `ContentTag` of the source `Edit` admitted by the bridge.
    pub const fn source_tag(&self) -> ContentTag {
        self.source_tag
    }

    /// Returns the `ContentTag` of the pinned LadderLang quoted-byte representation policy.
    pub const fn policy_tag(&self) -> ContentTag {
        self.policy_tag
    }

    /// Returns the `ContentTag` committing to the expanded closed LadderLang `Lam` prefix stream.
    pub const fn term_tag(&self) -> ContentTag {
        self.term_tag
    }

    /// Returns the `ContentTag` of the compact LadderLang quote receipt binding source, policy, term, and cost.
    pub const fn receipt_tag(&self) -> ContentTag {
        self.receipt_tag
    }

    /// Returns the complete quoted byte sequence (`BRIDGE_NAME` followed by the canonical tag preimage).
    pub fn quoted_bytes(&self) -> &[u8] {
        &self.quoted_bytes
    }

    /// Returns the static `LadderMetrics` cost projections computed for this verified quote.
    pub const fn metrics(&self) -> LadderMetrics {
        self.metrics
    }

    /// Streams the exact expanded closed-Lam preorder named by [`Self::term_tag`].
    pub fn encode_expanded_lam(&self, sink: &mut dyn FnMut(&[u8])) {
        encode_bytes_term(&self.quoted_bytes, sink);
    }
}

/// Refusal error returned when verifying or quoting a candidate object across the LadderLang bridge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeError {
    UnsupportedKind { actual: TagKind },
    UnsupportedSchema,
    SourceTagKind { actual: TagKind },
    SourceTagMismatch,
    Edit(EditCodecError),
    NonCanonicalEdit,
    UnexpectedSourcePreimageLength { expected: usize, actual: usize },
    ArithmeticOverflow,
    CostInvariant,
    Protocol(TagError),
}

impl fmt::Display for BridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::UnsupportedKind { actual } => {
                write!(
                    formatter,
                    "bb-lam/v1 does not admit source kind {}",
                    actual.name()
                )
            }
            Self::UnsupportedSchema => formatter.write_str(
                "bb-lam/v1 admits only bitblock/edit/op-u8-i64-be/v1 at this checkpoint",
            ),
            Self::SourceTagKind { actual } => write!(
                formatter,
                "claimed source tag has kind {}, expected edit",
                actual.name()
            ),
            Self::SourceTagMismatch => {
                formatter.write_str("claimed source tag does not name the canonical edit")
            }
            Self::Edit(error) => error.fmt(formatter),
            Self::NonCanonicalEdit => {
                formatter.write_str("decoded edit did not round-trip to identical bytes")
            }
            Self::UnexpectedSourcePreimageLength { expected, actual } => write!(
                formatter,
                "canonical Edit tag preimage requires {expected} bytes, found {actual}"
            ),
            Self::ArithmeticOverflow => {
                formatter.write_str("quoted-byte cost arithmetic overflowed u64")
            }
            Self::CostInvariant => formatter.write_str("quoted-byte cost projections disagree"),
            Self::Protocol(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for BridgeError {}

impl From<EditCodecError> for BridgeError {
    fn from(error: EditCodecError) -> Self {
        Self::Edit(error)
    }
}

impl From<TagError> for BridgeError {
    fn from(error: TagError) -> Self {
        Self::Protocol(error)
    }
}

/// The full named policy for the content-neutral quoted-byte representation.
pub fn bridge_policy_tag() -> ContentTag {
    hash_canonical(
        TagKind::Policy,
        BRIDGE_POLICY_SCHEMA,
        BRIDGE_POLICY_PAYLOAD.len() as u64,
        |sink| sink(BRIDGE_POLICY_PAYLOAD),
    )
    .expect("static bridge policy length must match")
}

/// Verifies and quotes one standalone Edit using its self-derived source tag.
pub fn quote_edit(edit: Edit) -> Result<VerifiedLadderQuote, BridgeError> {
    let payload = edit.encode_canonical();
    quote_object(
        BridgeObject::new(TagKind::Edit, EDIT_SCHEMA, &payload),
        edit.content_tag(),
    )
}

/// Verifies the exact kind, schema, canonical round trip, and claimed source tag before pricing.
pub fn quote_object(
    source: BridgeObject<'_>,
    claimed_source_tag: ContentTag,
) -> Result<VerifiedLadderQuote, BridgeError> {
    if source.kind != TagKind::Edit {
        return Err(BridgeError::UnsupportedKind {
            actual: source.kind,
        });
    }
    if source.schema != EDIT_SCHEMA {
        return Err(BridgeError::UnsupportedSchema);
    }
    if claimed_source_tag.kind() != TagKind::Edit {
        return Err(BridgeError::SourceTagKind {
            actual: claimed_source_tag.kind(),
        });
    }

    let edit = Edit::decode_canonical(source.payload)?;
    if edit.encode_canonical().as_slice() != source.payload {
        return Err(BridgeError::NonCanonicalEdit);
    }
    if edit.content_tag() != claimed_source_tag {
        return Err(BridgeError::SourceTagMismatch);
    }

    let source_preimage = encode_canonical_preimage(
        TagKind::Edit,
        EDIT_SCHEMA,
        EDIT_CANONICAL_LEN as u64,
        |sink| sink(source.payload),
    )?;
    if source_preimage.len() != EDIT_TAG_PREIMAGE_BYTES {
        return Err(BridgeError::UnexpectedSourcePreimageLength {
            expected: EDIT_TAG_PREIMAGE_BYTES,
            actual: source_preimage.len(),
        });
    }
    let mut quoted_bytes = Vec::with_capacity(EDIT_QUOTED_BYTES);
    quoted_bytes.extend_from_slice(BRIDGE_NAME);
    quoted_bytes.extend_from_slice(&source_preimage);
    if quoted_bytes.len() != EDIT_QUOTED_BYTES {
        return Err(BridgeError::ArithmeticOverflow);
    }

    let metrics = LadderMetrics::for_quote_len(quoted_bytes.len())?;
    let term_tag = hash_canonical(
        TagKind::LadderTerm,
        LADDER_TERM_SCHEMA,
        metrics.lam_prefix_bytes,
        |sink| encode_bytes_term(&quoted_bytes, sink),
    )?;
    let policy_tag = bridge_policy_tag();
    let receipt_tag = hash_canonical(
        TagKind::LadderQuoteReceipt,
        LADDER_QUOTE_RECEIPT_SCHEMA,
        RECEIPT_PAYLOAD_BYTES,
        |sink| {
            sink(&claimed_source_tag.digest());
            sink(&policy_tag.digest());
            sink(&term_tag.digest());
            sink(&metrics.quoted_bytes.to_be_bytes());
            sink(&metrics.atoms_upper.to_be_bytes());
            sink(&metrics.iota_upper.to_be_bytes());
            sink(&metrics.ticks.to_be_bytes());
        },
    )?;

    Ok(VerifiedLadderQuote {
        source_tag: claimed_source_tag,
        policy_tag,
        term_tag,
        receipt_tag,
        quoted_bytes,
        metrics,
    })
}

fn emit_node(node: u8, sink: &mut dyn FnMut(&[u8])) {
    sink(&[node]);
}

fn encode_var(index: u64, sink: &mut dyn FnMut(&[u8])) {
    emit_node(VAR_NODE, sink);
    sink(&index.to_be_bytes());
}

fn encode_i(sink: &mut dyn FnMut(&[u8])) {
    emit_node(LAM_NODE, sink);
    encode_var(0, sink);
}

fn encode_false(sink: &mut dyn FnMut(&[u8])) {
    emit_node(LAM_NODE, sink);
    emit_node(LAM_NODE, sink);
    encode_var(0, sink);
}

fn encode_pair(sink: &mut dyn FnMut(&[u8])) {
    emit_node(LAM_NODE, sink);
    emit_node(LAM_NODE, sink);
    emit_node(LAM_NODE, sink);
    emit_node(APP_NODE, sink);
    emit_node(APP_NODE, sink);
    encode_var(0, sink);
    encode_var(2, sink);
    encode_var(1, sink);
}

fn encode_bit(bit: bool, sink: &mut dyn FnMut(&[u8])) {
    emit_node(APP_NODE, sink);
    if bit {
        encode_false(sink);
        encode_i(sink);
    } else {
        encode_i(sink);
        encode_false(sink);
    }
}

fn encode_byte_tail(byte: u8, index: u8, sink: &mut dyn FnMut(&[u8])) {
    let bit = byte & (1 << (7 - index)) != 0;
    if index == 7 {
        encode_bit(bit, sink);
        return;
    }
    emit_node(APP_NODE, sink);
    emit_node(APP_NODE, sink);
    encode_pair(sink);
    encode_bit(bit, sink);
    encode_byte_tail(byte, index + 1, sink);
}

fn encode_byte(byte: u8, sink: &mut dyn FnMut(&[u8])) {
    encode_byte_tail(byte, 0, sink);
}

fn encode_bytes_term(bytes: &[u8], sink: &mut dyn FnMut(&[u8])) {
    for &byte in bytes {
        emit_node(APP_NODE, sink);
        encode_byte(byte, sink);
    }
    encode_false(sink); // NIL is exactly FALSE in the pinned LadderLang checkout.
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::world::Coord;

    fn vector_edit() -> Edit {
        Edit::new(
            crate::program::Operation::Toggle,
            Coord::new(i64::MIN, -2, i64::MAX),
        )
    }

    fn expanded_bytes(quote: &VerifiedLadderQuote) -> Vec<u8> {
        let mut bytes = Vec::new();
        quote.encode_expanded_lam(&mut |chunk| bytes.extend_from_slice(chunk));
        bytes
    }

    fn app_bytes(function: Vec<u8>, argument: Vec<u8>) -> Vec<u8> {
        let mut term = Vec::with_capacity(1 + function.len() + argument.len());
        term.push(APP_NODE);
        term.extend(function);
        term.extend(argument);
        term
    }

    fn encoded_term(encode: impl FnOnce(&mut dyn FnMut(&[u8]))) -> Vec<u8> {
        let mut term = Vec::new();
        encode(&mut |chunk| term.extend_from_slice(chunk));
        term
    }

    fn pair_bytes(left: Vec<u8>, right: Vec<u8>) -> Vec<u8> {
        app_bytes(app_bytes(encoded_term(encode_pair), left), right)
    }

    fn bit_bytes(bit: bool) -> Vec<u8> {
        encoded_term(|sink| encode_bit(bit, sink))
    }

    fn left_associated_byte(byte: u8) -> Vec<u8> {
        let bits = (0..8)
            .map(|index| bit_bytes(byte & (1 << (7 - index)) != 0))
            .collect::<Vec<_>>();
        bits.into_iter()
            .reduce(pair_bytes)
            .expect("a byte has eight bits")
    }

    fn bytes_with_first_byte(bytes: &[u8], first: impl FnOnce(u8) -> Vec<u8>) -> Vec<u8> {
        let tail = encoded_term(|sink| encode_bytes_term(&bytes[1..], sink));
        app_bytes(first(bytes[0]), tail)
    }

    fn tag_expanded(bytes: &[u8]) -> ContentTag {
        hash_canonical(
            TagKind::LadderTerm,
            LADDER_TERM_SCHEMA,
            bytes.len() as u64,
            |sink| sink(bytes),
        )
        .unwrap()
    }

    #[test]
    fn edit_quote_matches_independent_python_and_node_vectors() {
        let quote = quote_edit(vector_edit()).unwrap();
        let metrics = quote.metrics();
        let quote_sha: [u8; 32] = Sha256::digest(quote.quoted_bytes()).into();

        assert_eq!(BRIDGE_POLICY_PAYLOAD.len(), 481);
        assert_eq!(quote.quoted_bytes().len(), 112);
        assert_eq!(expanded_bytes(&quote).len(), 46_491);
        assert_eq!(
            quote_sha,
            [
                0xef, 0x7a, 0x38, 0x59, 0xdc, 0xaf, 0xf2, 0x1d, 0x47, 0x31, 0x6d, 0xb8, 0x83, 0x88,
                0x64, 0x6a, 0x84, 0xc6, 0xd5, 0xcc, 0x4a, 0x89, 0x70, 0xff, 0xd4, 0x93, 0xcd, 0xec,
                0x0f, 0x65, 0x68, 0x87,
            ]
        );
        assert_eq!(
            quote.source_tag().to_string(),
            "#bb.edit.sha256.l3qxygkk2tikiyma5zkvaznrgzvu46ulmtdse56c2tf4r24ugzdq"
        );
        assert_eq!(
            quote.policy_tag().to_string(),
            "#bb.policy.sha256.veaii6flkafmvwll7rafobhr3dttrjgn6syxvipbzppxbuauaoyq"
        );
        assert_eq!(
            quote.term_tag().to_string(),
            "#bb.ladder-term.sha256.jarqhm3djti62ufflcz7d3xyyvmyv4hku4optzuoarqoqegicndq"
        );
        assert_eq!(
            quote.receipt_tag().to_string(),
            "#bb.ladder-quote-receipt.sha256.5c2ipojlcn2vlpoeabhck5tvnd6rqclwgfjyd2rafkkyepuczxja"
        );
        assert_eq!(
            metrics,
            LadderMetrics {
                quoted_bytes: 112,
                lam_prefix_bytes: 46_491,
                lam_size: 13_331,
                lam_variable_nodes: 4_145,
                lam_application_nodes: 4_144,
                binder_depth: 3,
                total_binders: 5_042,
                computed_layer: 4,
                s_atoms: 7_840,
                k_atoms: 8_737,
                i_atoms: 4_145,
                sk_application_nodes: 20_721,
                sk_surviving_variables: 0,
                atoms_upper: 20_722,
                iota_upper: 82_438,
                ticks: 82_437,
            }
        );
    }

    #[test]
    fn verifier_refuses_wrong_kind_schema_tag_length_and_operation() {
        let edit = vector_edit();
        let payload = edit.encode_canonical();
        let tag = edit.content_tag();

        assert_eq!(
            quote_object(
                BridgeObject::new(TagKind::World, EDIT_SCHEMA, &payload),
                tag
            ),
            Err(BridgeError::UnsupportedKind {
                actual: TagKind::World,
            })
        );
        assert_eq!(
            quote_object(
                BridgeObject::new(TagKind::Edit, b"bitblock/edit/wrong/v1", &payload),
                tag,
            ),
            Err(BridgeError::UnsupportedSchema)
        );
        let world_tag = hash_canonical(TagKind::World, EDIT_SCHEMA, payload.len() as u64, |sink| {
            sink(&payload)
        })
        .unwrap();
        assert_eq!(
            quote_object(
                BridgeObject::new(TagKind::Edit, EDIT_SCHEMA, &payload),
                world_tag,
            ),
            Err(BridgeError::SourceTagKind {
                actual: TagKind::World,
            })
        );
        let other_tag = Edit::new(crate::program::Operation::On, edit.coord).content_tag();
        assert_eq!(
            quote_object(
                BridgeObject::new(TagKind::Edit, EDIT_SCHEMA, &payload),
                other_tag,
            ),
            Err(BridgeError::SourceTagMismatch)
        );
        assert!(matches!(
            quote_object(
                BridgeObject::new(TagKind::Edit, EDIT_SCHEMA, &payload[..24]),
                tag,
            ),
            Err(BridgeError::Edit(EditCodecError::WrongLength { .. }))
        ));
        let mut invalid_operation = payload;
        invalid_operation[0] = 3;
        assert_eq!(
            quote_object(
                BridgeObject::new(TagKind::Edit, EDIT_SCHEMA, &invalid_operation),
                tag,
            ),
            Err(BridgeError::Edit(EditCodecError::UnknownOperation(3)))
        );
    }

    #[test]
    fn structure_tag_separates_prefix_association_bit_order_and_list_spine() {
        let quote = quote_edit(vector_edit()).unwrap();
        let canonical = expanded_bytes(&quote);

        let without_prefix = encoded_term(|sink| {
            encode_bytes_term(&quote.quoted_bytes()[BRIDGE_NAME.len()..], sink)
        });
        let left_byte = bytes_with_first_byte(quote.quoted_bytes(), left_associated_byte);
        let reversed_byte = bytes_with_first_byte(quote.quoted_bytes(), |byte| {
            encoded_term(|sink| encode_byte(byte.reverse_bits(), sink))
        });

        let mut left_list = encoded_term(encode_false);
        for &byte in quote.quoted_bytes() {
            left_list = app_bytes(left_list, encoded_term(|sink| encode_byte(byte, sink)));
        }

        for mutant in [without_prefix, left_byte, reversed_byte, left_list] {
            assert_ne!(mutant, canonical);
            assert_ne!(tag_expanded(&mutant), quote.term_tag());
        }
    }

    #[test]
    fn cost_is_content_neutral_but_tags_are_not() {
        let on = quote_edit(Edit::new(
            crate::program::Operation::On,
            Coord::new(0, 0, 0),
        ))
        .unwrap();
        let off = quote_edit(Edit::new(
            crate::program::Operation::Off,
            Coord::new(i64::MAX, i64::MIN, -1),
        ))
        .unwrap();

        assert_eq!(on.metrics(), off.metrics());
        assert_ne!(on.source_tag(), off.source_tag());
        assert_ne!(on.term_tag(), off.term_tag());
        assert_ne!(on.receipt_tag(), off.receipt_tag());
    }
}
