//! Canonical, content-addressed observations of raw Windows Advanced Color configuration.
//!
//! This carrier binds one ordered per-route configuration report to a previously collected
//! [`OutputTopologyObservation`]. It deliberately retains only raw WDDM values: the API report
//! is not decoded as HDR, a color space, compositor behavior, scanout, emitted light, or a
//! physical-pixel measurement. Numeric target IDs remain collector-local and have no canonical
//! lane; route position is the already-canonical route position in the bound topology.

use std::fmt;

use crate::output_topology::{
    MAX_OUTPUT_TOPOLOGY_ROUTES, OutputTopologyError, OutputTopologyObservation,
};
use crate::protocol::{ContentTag, TagError, TagKind, hash_canonical};

/// Schema for one raw per-route Advanced Color configuration observation.
pub const OUTPUT_COLOR_OBSERVATION_SCHEMA: &[u8] = b"bitblock/output-color-observation/v1";
/// Maximum payload admitted before allocation.
pub const MAX_OUTPUT_COLOR_PAYLOAD_BYTES: usize = 32 + 1 + 2 + MAX_OUTPUT_TOPOLOGY_ROUTES * 13;

const AVAILABLE_MARKER: u8 = 0;
const UNAVAILABLE_MARKER: u8 = 1;
const ROUTE_AVAILABLE_MARKER: u8 = 0;
const ROUTE_UNAVAILABLE_MARKER: u8 = 1;
const PARENT_DIGEST_BYTES: usize = 32;
const AVAILABLE_ROUTE_BYTES: usize = 1 + 4 + 4 + 4;
const UNAVAILABLE_ROUTE_BYTES: usize = 1 + 4;

/// Raw result of `DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO` for one canonical topology route.
///
/// The values intentionally remain opaque raw words. In particular, `flags_raw` and
/// `color_encoding_raw` must not be relabelled as a specific HDR or light-emission mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdvancedColorRouteFacts {
    Available {
        flags_raw: u32,
        color_encoding_raw: u32,
        bits_per_color_channel: u32,
    },
    Unavailable {
        error_code: u32,
    },
}

impl AdvancedColorRouteFacts {
    const fn encoded_len(self) -> usize {
        match self {
            Self::Available { .. } => AVAILABLE_ROUTE_BYTES,
            Self::Unavailable { .. } => UNAVAILABLE_ROUTE_BYTES,
        }
    }

    fn encode(self, bytes: &mut Vec<u8>) {
        match self {
            Self::Available {
                flags_raw,
                color_encoding_raw,
                bits_per_color_channel,
            } => {
                bytes.push(ROUTE_AVAILABLE_MARKER);
                bytes.extend_from_slice(&flags_raw.to_be_bytes());
                bytes.extend_from_slice(&color_encoding_raw.to_be_bytes());
                bytes.extend_from_slice(&bits_per_color_channel.to_be_bytes());
            }
            Self::Unavailable { error_code } => {
                bytes.push(ROUTE_UNAVAILABLE_MARKER);
                bytes.extend_from_slice(&error_code.to_be_bytes());
            }
        }
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, OutputColorObservationError> {
        match reader.u8()? {
            ROUTE_AVAILABLE_MARKER => Ok(Self::Available {
                flags_raw: reader.u32()?,
                color_encoding_raw: reader.u32()?,
                bits_per_color_channel: reader.u32()?,
            }),
            ROUTE_UNAVAILABLE_MARKER => Ok(Self::Unavailable {
                error_code: reader.u32()?,
            }),
            marker => Err(OutputColorObservationError::UnknownMarker {
                field: "route outcome",
                marker,
            }),
        }
    }
}

/// Full result of the color query attempt bound to one topology observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputColorFacts {
    Available {
        routes: Vec<AdvancedColorRouteFacts>,
    },
    Unavailable {
        reason_code_raw: u32,
        error_code: u32,
    },
}

impl OutputColorFacts {
    fn validate(&self) -> Result<(), OutputColorObservationError> {
        let Self::Available { routes } = self else {
            return Ok(());
        };
        if routes.is_empty() {
            return Err(OutputColorObservationError::EmptyRoutes);
        }
        if routes.len() > MAX_OUTPUT_TOPOLOGY_ROUTES || routes.len() > usize::from(u16::MAX) {
            return Err(OutputColorObservationError::TooManyRoutes {
                actual: routes.len(),
                maximum: MAX_OUTPUT_TOPOLOGY_ROUTES,
            });
        }
        Ok(())
    }

    fn encoded_len(&self) -> Result<usize, OutputColorObservationError> {
        let length = match self {
            Self::Available { routes } => {
                let route_bytes = routes.iter().try_fold(0_usize, |total, route| {
                    total
                        .checked_add(route.encoded_len())
                        .ok_or(OutputColorObservationError::LengthOverflow)
                })?;
                PARENT_DIGEST_BYTES
                    .checked_add(1 + 2)
                    .and_then(|total| total.checked_add(route_bytes))
                    .ok_or(OutputColorObservationError::LengthOverflow)?
            }
            Self::Unavailable { .. } => PARENT_DIGEST_BYTES
                .checked_add(1 + 4 + 4)
                .ok_or(OutputColorObservationError::LengthOverflow)?,
        };
        if length > MAX_OUTPUT_COLOR_PAYLOAD_BYTES {
            return Err(OutputColorObservationError::PayloadTooLarge {
                actual: length,
                maximum: MAX_OUTPUT_COLOR_PAYLOAD_BYTES,
            });
        }
        Ok(length)
    }

    fn encode(&self, parent_topology: ContentTag, bytes: &mut Vec<u8>) {
        bytes.extend_from_slice(&parent_topology.digest());
        match self {
            Self::Available { routes } => {
                bytes.push(AVAILABLE_MARKER);
                bytes.extend_from_slice(
                    &u16::try_from(routes.len())
                        .expect("validated route count fits the canonical u16 lane")
                        .to_be_bytes(),
                );
                for route in routes {
                    route.encode(bytes);
                }
            }
            Self::Unavailable {
                reason_code_raw,
                error_code,
            } => {
                bytes.push(UNAVAILABLE_MARKER);
                bytes.extend_from_slice(&reason_code_raw.to_be_bytes());
                bytes.extend_from_slice(&error_code.to_be_bytes());
            }
        }
    }
}

/// A tag-bound raw Advanced Color configuration observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputColorObservation {
    tag: ContentTag,
    parent_topology: ContentTag,
    facts: OutputColorFacts,
}

impl OutputColorObservation {
    /// Binds one raw color result for every canonical route in an available parent topology.
    pub fn new_available(
        parent_topology: &OutputTopologyObservation,
        routes: Vec<AdvancedColorRouteFacts>,
    ) -> Result<Self, OutputColorObservationError> {
        parent_topology
            .verify()
            .map_err(OutputColorObservationError::ParentTopology)?;
        let Some(parent_routes) = parent_topology.available().map(|facts| &facts.routes) else {
            return Err(OutputColorObservationError::ParentUnavailableForAvailableFacts);
        };
        if routes.len() != parent_routes.len() {
            return Err(OutputColorObservationError::RouteCountMismatch {
                expected: parent_routes.len(),
                actual: routes.len(),
            });
        }
        Self::from_facts(
            parent_topology.tag(),
            OutputColorFacts::Available { routes },
        )
    }

    /// Retains a failed/unavailable color query while still naming the topology it was about.
    pub fn new_unavailable(
        parent_topology: &OutputTopologyObservation,
        reason_code_raw: u32,
        error_code: u32,
    ) -> Result<Self, OutputColorObservationError> {
        parent_topology
            .verify()
            .map_err(OutputColorObservationError::ParentTopology)?;
        Self::from_facts(
            parent_topology.tag(),
            OutputColorFacts::Unavailable {
                reason_code_raw,
                error_code,
            },
        )
    }

    fn from_facts(
        parent_topology: ContentTag,
        facts: OutputColorFacts,
    ) -> Result<Self, OutputColorObservationError> {
        facts.validate()?;
        let mut observation = Self {
            tag: ContentTag::from_digest(TagKind::OutputColorObservation, [0; 32]),
            parent_topology,
            facts,
        };
        observation.tag = observation.recompute_tag()?;
        Ok(observation)
    }

    pub const fn tag(&self) -> ContentTag {
        self.tag
    }

    /// Full typed tag of the topology whose canonical route order indexes this observation.
    pub const fn parent_topology(&self) -> ContentTag {
        self.parent_topology
    }

    pub fn facts(&self) -> &OutputColorFacts {
        &self.facts
    }

    pub fn routes(&self) -> Option<&[AdvancedColorRouteFacts]> {
        match &self.facts {
            OutputColorFacts::Available { routes } => Some(routes),
            OutputColorFacts::Unavailable { .. } => None,
        }
    }

    pub fn encode_canonical(&self) -> Result<Vec<u8>, OutputColorObservationError> {
        self.facts.validate()?;
        let length = self.facts.encoded_len()?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| OutputColorObservationError::AllocationFailed { requested: length })?;
        self.facts.encode(self.parent_topology, &mut bytes);
        debug_assert_eq!(bytes.len(), length);
        Ok(bytes)
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, OutputColorObservationError> {
        if bytes.is_empty() || bytes.len() > MAX_OUTPUT_COLOR_PAYLOAD_BYTES {
            return Err(OutputColorObservationError::WrongPayloadLength {
                actual: bytes.len(),
                maximum: MAX_OUTPUT_COLOR_PAYLOAD_BYTES,
            });
        }
        let mut reader = Reader::new(bytes);
        let parent_topology = ContentTag::from_digest(TagKind::OutputTopology, reader.digest()?);
        let facts = match reader.u8()? {
            AVAILABLE_MARKER => {
                let route_count = usize::from(reader.u16()?);
                if route_count == 0 {
                    return Err(OutputColorObservationError::EmptyRoutes);
                }
                if route_count > MAX_OUTPUT_TOPOLOGY_ROUTES {
                    return Err(OutputColorObservationError::TooManyRoutes {
                        actual: route_count,
                        maximum: MAX_OUTPUT_TOPOLOGY_ROUTES,
                    });
                }
                reader.require(
                    route_count
                        .checked_mul(UNAVAILABLE_ROUTE_BYTES)
                        .ok_or(OutputColorObservationError::LengthOverflow)?,
                )?;
                let mut routes = Vec::new();
                routes.try_reserve_exact(route_count).map_err(|_| {
                    OutputColorObservationError::AllocationFailed {
                        requested: route_count,
                    }
                })?;
                for _ in 0..route_count {
                    routes.push(AdvancedColorRouteFacts::decode(&mut reader)?);
                }
                OutputColorFacts::Available { routes }
            }
            UNAVAILABLE_MARKER => OutputColorFacts::Unavailable {
                reason_code_raw: reader.u32()?,
                error_code: reader.u32()?,
            },
            marker => {
                return Err(OutputColorObservationError::UnknownMarker {
                    field: "observation outcome",
                    marker,
                });
            }
        };
        reader.finish()?;
        let observation = Self::from_facts(parent_topology, facts)?;
        if observation.encode_canonical()?.as_slice() != bytes {
            return Err(OutputColorObservationError::NonCanonicalEncoding);
        }
        Ok(observation)
    }

    /// Checks only this carrier's syntax, canonical encoding, type, and content tag.
    ///
    /// The parent bytes remain external. Use [`Self::verify_against`] before treating a route
    /// index as bound to a topology route.
    pub fn verify(&self) -> Result<(), OutputColorObservationError> {
        self.facts.validate()?;
        if self.parent_topology.kind() != TagKind::OutputTopology {
            return Err(OutputColorObservationError::WrongParentTagKind {
                actual: self.parent_topology.kind(),
            });
        }
        if self.tag.kind() != TagKind::OutputColorObservation {
            return Err(OutputColorObservationError::WrongTagKind {
                actual: self.tag.kind(),
            });
        }
        if self.tag != self.recompute_tag()? {
            return Err(OutputColorObservationError::TagMismatch);
        }
        Ok(())
    }

    /// Resolves this observation against the exact topology object it names.
    pub fn verify_against(
        &self,
        parent_topology: &OutputTopologyObservation,
    ) -> Result<(), OutputColorObservationError> {
        self.verify()?;
        parent_topology
            .verify()
            .map_err(OutputColorObservationError::ParentTopology)?;
        if self.parent_topology != parent_topology.tag() {
            return Err(OutputColorObservationError::ParentTagMismatch {
                expected: self.parent_topology,
                actual: parent_topology.tag(),
            });
        }
        if let OutputColorFacts::Available { routes } = &self.facts {
            let Some(parent_routes) = parent_topology.available().map(|facts| &facts.routes) else {
                return Err(OutputColorObservationError::ParentUnavailableForAvailableFacts);
            };
            if routes.len() != parent_routes.len() {
                return Err(OutputColorObservationError::RouteCountMismatch {
                    expected: parent_routes.len(),
                    actual: routes.len(),
                });
            }
        }
        Ok(())
    }

    fn recompute_tag(&self) -> Result<ContentTag, OutputColorObservationError> {
        let bytes = self.encode_canonical()?;
        hash_canonical(
            TagKind::OutputColorObservation,
            OUTPUT_COLOR_OBSERVATION_SCHEMA,
            bytes.len() as u64,
            |sink| sink(&bytes),
        )
        .map_err(OutputColorObservationError::Tag)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputColorObservationError {
    WrongPayloadLength {
        actual: usize,
        maximum: usize,
    },
    PayloadTooLarge {
        actual: usize,
        maximum: usize,
    },
    UnexpectedEnd {
        needed: usize,
        remaining: usize,
    },
    TrailingBytes {
        actual: usize,
    },
    LengthOverflow,
    AllocationFailed {
        requested: usize,
    },
    EmptyRoutes,
    TooManyRoutes {
        actual: usize,
        maximum: usize,
    },
    UnknownMarker {
        field: &'static str,
        marker: u8,
    },
    NonCanonicalEncoding,
    WrongParentTagKind {
        actual: TagKind,
    },
    ParentTopology(OutputTopologyError),
    ParentTagMismatch {
        expected: ContentTag,
        actual: ContentTag,
    },
    ParentUnavailableForAvailableFacts,
    RouteCountMismatch {
        expected: usize,
        actual: usize,
    },
    WrongTagKind {
        actual: TagKind,
    },
    TagMismatch,
    Tag(TagError),
}

impl fmt::Display for OutputColorObservationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongPayloadLength { actual, maximum } => write!(
                formatter,
                "output-color payload has {actual} bytes; valid range is 1..={maximum}"
            ),
            Self::PayloadTooLarge { actual, maximum } => write!(
                formatter,
                "output-color payload requires {actual} bytes, exceeding {maximum}"
            ),
            Self::UnexpectedEnd { needed, remaining } => write!(
                formatter,
                "output-color needs {needed} more bytes but only {remaining} remain"
            ),
            Self::TrailingBytes { actual } => {
                write!(formatter, "output-color has {actual} trailing bytes")
            }
            Self::LengthOverflow => formatter.write_str("output-color length overflowed"),
            Self::AllocationFailed { requested } => write!(
                formatter,
                "could not allocate {requested} bounded output-color elements or bytes"
            ),
            Self::EmptyRoutes => {
                formatter.write_str("available output-color facts require at least one route")
            }
            Self::TooManyRoutes { actual, maximum } => write!(
                formatter,
                "output-color has {actual} routes, exceeding {maximum}"
            ),
            Self::UnknownMarker { field, marker } => {
                write!(
                    formatter,
                    "{field} uses unknown output-color marker {marker}"
                )
            }
            Self::NonCanonicalEncoding => {
                formatter.write_str("output-color bytes are not canonical")
            }
            Self::WrongParentTagKind { actual } => write!(
                formatter,
                "output-color parent has {} tag kind instead of output-topology",
                actual.name()
            ),
            Self::ParentTopology(source) => {
                write!(formatter, "bound output topology is invalid: {source}")
            }
            Self::ParentTagMismatch { expected, actual } => write!(
                formatter,
                "output-color names topology {expected}, not supplied topology {actual}"
            ),
            Self::ParentUnavailableForAvailableFacts => formatter.write_str(
                "available output-color facts require an available bound output topology",
            ),
            Self::RouteCountMismatch { expected, actual } => write!(
                formatter,
                "output-color has {actual} routes but its bound topology has {expected}"
            ),
            Self::WrongTagKind { actual } => write!(
                formatter,
                "output-color has {} tag kind instead of output-color-observation",
                actual.name()
            ),
            Self::TagMismatch => {
                formatter.write_str("output-color tag does not match its canonical facts")
            }
            Self::Tag(source) => write!(formatter, "could not tag output-color facts: {source}"),
        }
    }
}

impl std::error::Error for OutputColorObservationError {}

struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn require(&self, needed: usize) -> Result<(), OutputColorObservationError> {
        let remaining = self.bytes.len().saturating_sub(self.cursor);
        if needed > remaining {
            return Err(OutputColorObservationError::UnexpectedEnd { needed, remaining });
        }
        Ok(())
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], OutputColorObservationError> {
        self.require(length)?;
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(OutputColorObservationError::LengthOverflow)?;
        let value = &self.bytes[self.cursor..end];
        self.cursor = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, OutputColorObservationError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, OutputColorObservationError> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().expect("fixed u16 byte lane"),
        ))
    }

    fn u32(&mut self) -> Result<u32, OutputColorObservationError> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().expect("fixed u32 byte lane"),
        ))
    }

    fn digest(&mut self) -> Result<[u8; PARENT_DIGEST_BYTES], OutputColorObservationError> {
        Ok(self
            .take(PARENT_DIGEST_BYTES)?
            .try_into()
            .expect("fixed digest byte lane"))
    }

    fn finish(self) -> Result<(), OutputColorObservationError> {
        let trailing = self.bytes.len().saturating_sub(self.cursor);
        if trailing == 0 {
            Ok(())
        } else {
            Err(OutputColorObservationError::TrailingBytes { actual: trailing })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output_topology::{
        AcquisitionAttempt, AcquisitionFacts, PixelSize, RawRational, RectI32, SourceFacts,
        SourceModeFacts, TargetIdentityObservation, TargetRouteFacts, TargetSignalTiming,
        Utf16Text,
    };

    fn acquisition() -> AcquisitionFacts {
        AcquisitionFacts {
            profile_code_raw: 7,
            request_flags_raw: 0x8000_0001,
            attempts: vec![AcquisitionAttempt {
                operation_code_raw: 1,
                profile_code_raw: 7,
                request_flags_raw: 0x8000_0001,
                retry_ordinal: 0,
                outcome_code_raw: 1,
                error_code: 0,
                reported_path_capacity: 2,
                reported_mode_capacity: 3,
            }],
        }
    }

    fn source() -> SourceFacts {
        SourceFacts {
            gdi_device_name: Utf16Text::new(vec![b'D' as u16]).unwrap(),
            client_rect: RectI32::default(),
            monitor_rect: RectI32::default(),
            work_rect: RectI32::default(),
            monitor_flags_raw: 1,
            mode: SourceModeFacts {
                position_x: 0,
                position_y: 0,
                width: 1,
                height: 1,
                pixel_format_raw: 1,
            },
            status_flags_raw: 1,
        }
    }

    fn route(index: u32) -> TargetRouteFacts {
        TargetRouteFacts {
            identity: TargetIdentityObservation::Unavailable {
                reason_code_raw: index,
                error_code: index ^ 0x8000_0000,
            },
            output_technology_raw: 0xf000_0001,
            target_available: true,
            status_flags_raw: 1,
            rotation_raw: 0xf000_0002,
            scaling_raw: 0xf000_0003,
            refresh_rate: RawRational {
                numerator: 60,
                denominator: 1,
            },
            path_flags_raw: 1,
            signal: TargetSignalTiming {
                pixel_rate: 1,
                h_sync: RawRational {
                    numerator: 1,
                    denominator: 1,
                },
                v_sync: RawRational {
                    numerator: 1,
                    denominator: 1,
                },
                active_size: PixelSize {
                    width: 1,
                    height: 1,
                },
                total_size: PixelSize {
                    width: 1,
                    height: 1,
                },
                video_standard_raw: 0,
                scanline_ordering_raw: 0,
            },
            desktop_image: None,
        }
    }

    fn available_parent(route_count: usize) -> OutputTopologyObservation {
        OutputTopologyObservation::new_available(
            acquisition(),
            source(),
            (0..route_count).map(|index| route(index as u32)).collect(),
        )
        .unwrap()
    }

    #[test]
    fn available_and_unavailable_round_trip_with_raw_words() {
        let parent = available_parent(2);
        let observation = OutputColorObservation::new_available(
            &parent,
            vec![
                AdvancedColorRouteFacts::Available {
                    flags_raw: 0x8000_000f,
                    color_encoding_raw: 0xffff_fffe,
                    bits_per_color_channel: u32::MAX,
                },
                AdvancedColorRouteFacts::Unavailable { error_code: 1168 },
            ],
        )
        .unwrap();
        let bytes = observation.encode_canonical().unwrap();
        assert!(bytes.len() <= MAX_OUTPUT_COLOR_PAYLOAD_BYTES);
        let decoded = OutputColorObservation::decode_canonical(&bytes).unwrap();
        assert_eq!(decoded, observation);
        assert_eq!(decoded.tag().kind(), TagKind::OutputColorObservation);
        decoded.verify().unwrap();
        decoded.verify_against(&parent).unwrap();

        let unavailable_parent =
            OutputTopologyObservation::new_unavailable(acquisition(), 8, 87).unwrap();
        let unavailable =
            OutputColorObservation::new_unavailable(&unavailable_parent, 9, 88).unwrap();
        let decoded =
            OutputColorObservation::decode_canonical(&unavailable.encode_canonical().unwrap())
                .unwrap();
        assert_eq!(decoded, unavailable);
        decoded.verify_against(&unavailable_parent).unwrap();
    }

    #[test]
    fn available_facts_require_matching_available_parent_routes() {
        let parent = available_parent(2);
        assert_eq!(
            OutputColorObservation::new_available(
                &parent,
                vec![AdvancedColorRouteFacts::Unavailable { error_code: 1 }],
            ),
            Err(OutputColorObservationError::RouteCountMismatch {
                expected: 2,
                actual: 1,
            })
        );

        let unavailable_parent =
            OutputTopologyObservation::new_unavailable(acquisition(), 8, 87).unwrap();
        assert_eq!(
            OutputColorObservation::new_available(
                &unavailable_parent,
                vec![AdvancedColorRouteFacts::Unavailable { error_code: 1 }],
            ),
            Err(OutputColorObservationError::ParentUnavailableForAvailableFacts)
        );
    }

    #[test]
    fn parent_and_route_binding_fail_closed() {
        let parent = available_parent(2);
        let observation = OutputColorObservation::new_available(
            &parent,
            vec![
                AdvancedColorRouteFacts::Unavailable { error_code: 1 },
                AdvancedColorRouteFacts::Unavailable { error_code: 2 },
            ],
        )
        .unwrap();
        let different_parent = available_parent(1);
        assert!(matches!(
            observation.verify_against(&different_parent),
            Err(OutputColorObservationError::ParentTagMismatch { .. })
        ));

        let mut forged = observation.clone();
        forged.facts = OutputColorFacts::Available {
            routes: vec![AdvancedColorRouteFacts::Unavailable { error_code: 1 }],
        };
        assert_eq!(
            forged.verify(),
            Err(OutputColorObservationError::TagMismatch)
        );
    }

    #[test]
    fn malformed_and_noncanonical_bytes_are_refused() {
        let parent = available_parent(1);
        let observation = OutputColorObservation::new_available(
            &parent,
            vec![AdvancedColorRouteFacts::Available {
                flags_raw: 1,
                color_encoding_raw: 2,
                bits_per_color_channel: 3,
            }],
        )
        .unwrap();
        let bytes = observation.encode_canonical().unwrap();
        assert!(matches!(
            OutputColorObservation::decode_canonical(&bytes[..bytes.len() - 1]),
            Err(OutputColorObservationError::UnexpectedEnd { .. })
        ));

        let mut bad_outcome = bytes.clone();
        bad_outcome[PARENT_DIGEST_BYTES] = 2;
        assert!(matches!(
            OutputColorObservation::decode_canonical(&bad_outcome),
            Err(OutputColorObservationError::UnknownMarker {
                field: "observation outcome",
                marker: 2,
            })
        ));

        let mut bad_route = bytes.clone();
        bad_route[PARENT_DIGEST_BYTES + 1 + 2] = 2;
        assert!(matches!(
            OutputColorObservation::decode_canonical(&bad_route),
            Err(OutputColorObservationError::UnknownMarker {
                field: "route outcome",
                marker: 2,
            })
        ));

        let mut zero_routes = bytes.clone();
        zero_routes[PARENT_DIGEST_BYTES + 1..PARENT_DIGEST_BYTES + 3]
            .copy_from_slice(&0_u16.to_be_bytes());
        assert_eq!(
            OutputColorObservation::decode_canonical(&zero_routes),
            Err(OutputColorObservationError::EmptyRoutes)
        );

        let mut trailing = bytes;
        trailing.push(0);
        assert!(matches!(
            OutputColorObservation::decode_canonical(&trailing),
            Err(OutputColorObservationError::TrailingBytes { actual: 1 })
        ));
    }

    #[test]
    fn independent_python_known_answer_vector_decodes_and_retags() {
        const VECTOR: [u8; 53] = [
            0x75, 0x20, 0xbc, 0x7b, 0x7d, 0xae, 0x8e, 0x9c, 0x1d, 0x37, 0x12, 0x39, 0xb4, 0x56,
            0xf9, 0xbf, 0x03, 0x78, 0xbc, 0x51, 0xe8, 0x58, 0xf2, 0x20, 0xeb, 0xeb, 0x1d, 0x58,
            0xb3, 0x6e, 0x70, 0x4f, 0x00, 0x00, 0x02, 0x00, 0x80, 0x00, 0x00, 0x0f, 0xff, 0xff,
            0xff, 0xfe, 0xff, 0xff, 0xff, 0xff, 0x01, 0x00, 0x00, 0x04, 0x90,
        ];
        let observation = OutputColorObservation::decode_canonical(&VECTOR).unwrap();
        assert_eq!(observation.encode_canonical().unwrap(), VECTOR);
        assert_eq!(
            observation.tag().to_string(),
            "#bb.output-color-observation.sha256.aw75drwp3saetgieirtggy6k6okgjiqgdamuqfl5xb2wtqslicgq"
        );
        assert_eq!(
            observation.routes(),
            Some(
                [
                    AdvancedColorRouteFacts::Available {
                        flags_raw: 0x8000_000f,
                        color_encoding_raw: 0xffff_fffe,
                        bits_per_color_channel: u32::MAX,
                    },
                    AdvancedColorRouteFacts::Unavailable { error_code: 1168 },
                ]
                .as_slice()
            )
        );
    }
}
