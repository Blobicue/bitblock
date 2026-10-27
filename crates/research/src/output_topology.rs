//! Canonical, content-addressed observations of the active Windows output topology.
//!
//! The canonical model deliberately begins after DisplayConfig's transient join phase. Handles,
//! adapter LUIDs, source/target IDs, and mode-table indices are collector-local lookup keys and
//! have no field here. The retained facts describe the active source and every cloned target
//! route, or an explicit acquisition failure. Exact UTF-16 code units are retained without lossy
//! Unicode conversion.

use std::cmp::Ordering;
use std::fmt;

use crate::protocol::{ContentTag, TagError, TagKind, hash_canonical};

/// Canonical schema string for active monitor output topology observations.
pub const OUTPUT_TOPOLOGY_OBSERVATION_SCHEMA: &[u8] = b"bitblock/output-topology-observation/v1";
/// Maximum byte length admitted for a canonical output topology observation payload.
pub const MAX_OUTPUT_TOPOLOGY_PAYLOAD_BYTES: usize = 65_536;
/// Maximum UTF-16 code unit count admitted for a topology device string.
pub const MAX_OUTPUT_TOPOLOGY_UTF16_UNITS: usize = 8_192;
/// Maximum number of cloned target routes admitted in one output topology observation.
pub const MAX_OUTPUT_TOPOLOGY_ROUTES: usize = 256;
/// Maximum number of DisplayConfig acquisition attempts retained in one observation.
pub const MAX_OUTPUT_TOPOLOGY_ATTEMPTS: usize = 256;

const AVAILABLE_MARKER: u8 = 0;
const UNAVAILABLE_MARKER: u8 = 1;
const ACQUISITION_ATTEMPT_BYTES: usize = 32;
const MIN_ROUTE_BYTES: usize = 1 + 4 + 4 + 4 + 1 + 4 + 4 + 4 + 8 + 4 + 48 + 1;

/// A nonempty sequence of exact UTF-16 code units.
///
/// Embedded NULs are rejected because Windows fixed buffers use the first NUL as their boundary.
/// Unpaired surrogates are accepted and retained exactly.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Utf16Text {
    units: Vec<u16>,
}

impl Utf16Text {
    /// Validates and wraps an owned vector of non-empty, NUL-free UTF-16 code units.
    pub fn new(units: Vec<u16>) -> Result<Self, OutputTopologyError> {
        validate_utf16_units(&units)?;
        Ok(Self { units })
    }

    /// Validates and clones a slice of non-empty, NUL-free UTF-16 code units into `Utf16Text`.
    pub fn from_units(units: &[u16]) -> Result<Self, OutputTopologyError> {
        validate_utf16_units(units)?;
        let mut owned = Vec::new();
        owned.try_reserve_exact(units.len()).map_err(|_| {
            OutputTopologyError::AllocationFailed {
                requested: units.len(),
            }
        })?;
        owned.extend_from_slice(units);
        Ok(Self { units: owned })
    }

    /// Returns the underlying slice of exact UTF-16 code units.
    pub fn units(&self) -> &[u16] {
        &self.units
    }

    /// Consumes this `Utf16Text` and returns its owned vector of UTF-16 code units.
    pub fn into_units(self) -> Vec<u16> {
        self.units
    }

    fn encoded_len(&self) -> Result<usize, OutputTopologyError> {
        2_usize
            .checked_add(
                self.units
                    .len()
                    .checked_mul(2)
                    .ok_or(OutputTopologyError::LengthOverflow)?,
            )
            .ok_or(OutputTopologyError::LengthOverflow)
    }

    fn encode(&self, writer: &mut Writer) {
        writer.u16(
            u16::try_from(self.units.len())
                .expect("validated UTF-16 length is representable as u16"),
        );
        for unit in &self.units {
            writer.u16(*unit);
        }
    }

    fn decode(reader: &mut Reader<'_>, field: &'static str) -> Result<Self, OutputTopologyError> {
        let unit_count = usize::from(reader.u16()?);
        if unit_count == 0 {
            return Err(OutputTopologyError::EmptyUtf16 { field });
        }
        if unit_count > MAX_OUTPUT_TOPOLOGY_UTF16_UNITS {
            return Err(OutputTopologyError::Utf16TooLong {
                field,
                actual: unit_count,
                maximum: MAX_OUTPUT_TOPOLOGY_UTF16_UNITS,
            });
        }
        let byte_count = unit_count
            .checked_mul(2)
            .ok_or(OutputTopologyError::LengthOverflow)?;
        reader.require(byte_count)?;
        let mut units = Vec::new();
        units
            .try_reserve_exact(unit_count)
            .map_err(|_| OutputTopologyError::AllocationFailed {
                requested: unit_count,
            })?;
        for index in 0..unit_count {
            let unit = reader.u16()?;
            if unit == 0 {
                return Err(OutputTopologyError::EmbeddedUtf16Nul { field, index });
            }
            units.push(unit);
        }
        Ok(Self { units })
    }
}

fn validate_utf16_units(units: &[u16]) -> Result<(), OutputTopologyError> {
    if units.is_empty() {
        return Err(OutputTopologyError::EmptyUtf16 {
            field: "UTF-16 text",
        });
    }
    if units.len() > MAX_OUTPUT_TOPOLOGY_UTF16_UNITS || units.len() > usize::from(u16::MAX) {
        return Err(OutputTopologyError::Utf16TooLong {
            field: "UTF-16 text",
            actual: units.len(),
            maximum: MAX_OUTPUT_TOPOLOGY_UTF16_UNITS.min(usize::from(u16::MAX)),
        });
    }
    if let Some(index) = units.iter().position(|unit| *unit == 0) {
        return Err(OutputTopologyError::EmbeddedUtf16Nul {
            field: "UTF-16 text",
            index,
        });
    }
    Ok(())
}

/// Signed 32-bit desktop coordinate rectangle for monitor, work, and client bounds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RectI32 {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl RectI32 {
    fn encode(self, writer: &mut Writer) {
        writer.i32(self.left);
        writer.i32(self.top);
        writer.i32(self.right);
        writer.i32(self.bottom);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, OutputTopologyError> {
        Ok(Self {
            left: reader.i32()?,
            top: reader.i32()?,
            right: reader.i32()?,
            bottom: reader.i32()?,
        })
    }
}

/// Unsigned 32-bit width and height dimensions in physical or desktop pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PixelSize {
    pub width: u32,
    pub height: u32,
}

impl PixelSize {
    fn encode(self, writer: &mut Writer) {
        writer.u32(self.width);
        writer.u32(self.height);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, OutputTopologyError> {
        Ok(Self {
            width: reader.u32()?,
            height: reader.u32()?,
        })
    }
}

/// A Windows rational retained exactly. Reduction is used only while comparing route keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct RawRational {
    pub numerator: u32,
    pub denominator: u32,
}

impl RawRational {
    fn normalized_key(self) -> (u8, u32, u32) {
        if self.denominator == 0 {
            return (1, self.numerator, 0);
        }
        let divisor = gcd(self.numerator, self.denominator);
        (0, self.numerator / divisor, self.denominator / divisor)
    }

    fn canonical_cmp(self, other: Self) -> Ordering {
        self.normalized_key()
            .cmp(&other.normalized_key())
            .then_with(|| self.numerator.cmp(&other.numerator))
            .then_with(|| self.denominator.cmp(&other.denominator))
    }

    fn encode(self, writer: &mut Writer) {
        writer.u32(self.numerator);
        writer.u32(self.denominator);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, OutputTopologyError> {
        Ok(Self {
            numerator: reader.u32()?,
            denominator: reader.u32()?,
        })
    }
}

fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.max(1)
}

/// One bounded acquisition attempt. Codes and flags remain raw so future Windows values survive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AcquisitionAttempt {
    pub operation_code_raw: u32,
    pub profile_code_raw: u32,
    pub request_flags_raw: u32,
    pub retry_ordinal: u32,
    pub outcome_code_raw: u32,
    pub error_code: u32,
    pub reported_path_capacity: u32,
    pub reported_mode_capacity: u32,
}

impl AcquisitionAttempt {
    fn encode(self, writer: &mut Writer) {
        writer.u32(self.operation_code_raw);
        writer.u32(self.profile_code_raw);
        writer.u32(self.request_flags_raw);
        writer.u32(self.retry_ordinal);
        writer.u32(self.outcome_code_raw);
        writer.u32(self.error_code);
        writer.u32(self.reported_path_capacity);
        writer.u32(self.reported_mode_capacity);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, OutputTopologyError> {
        Ok(Self {
            operation_code_raw: reader.u32()?,
            profile_code_raw: reader.u32()?,
            request_flags_raw: reader.u32()?,
            retry_ordinal: reader.u32()?,
            outcome_code_raw: reader.u32()?,
            error_code: reader.u32()?,
            reported_path_capacity: reader.u32()?,
            reported_mode_capacity: reader.u32()?,
        })
    }
}

/// The profile, flags, retries, errors, and capacity reports used to acquire one observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcquisitionFacts {
    pub profile_code_raw: u32,
    pub request_flags_raw: u32,
    pub attempts: Vec<AcquisitionAttempt>,
}

impl AcquisitionFacts {
    fn validate(&self) -> Result<(), OutputTopologyError> {
        if self.attempts.is_empty() {
            return Err(OutputTopologyError::EmptyAttempts);
        }
        if self.attempts.len() > MAX_OUTPUT_TOPOLOGY_ATTEMPTS
            || self.attempts.len() > usize::from(u16::MAX)
        {
            return Err(OutputTopologyError::TooManyAttempts {
                actual: self.attempts.len(),
                maximum: MAX_OUTPUT_TOPOLOGY_ATTEMPTS,
            });
        }
        for (index, attempt) in self.attempts.iter().enumerate() {
            if attempt.retry_ordinal != index as u32 {
                return Err(OutputTopologyError::NonCanonicalRetryOrdinal {
                    index,
                    actual: attempt.retry_ordinal,
                });
            }
        }
        Ok(())
    }

    fn encoded_len(&self) -> Result<usize, OutputTopologyError> {
        4_usize
            .checked_add(4)
            .and_then(|length| length.checked_add(2))
            .and_then(|length| {
                self.attempts
                    .len()
                    .checked_mul(ACQUISITION_ATTEMPT_BYTES)
                    .and_then(|attempt_bytes| length.checked_add(attempt_bytes))
            })
            .ok_or(OutputTopologyError::LengthOverflow)
    }

    fn encode(&self, writer: &mut Writer) {
        writer.u32(self.profile_code_raw);
        writer.u32(self.request_flags_raw);
        writer.u16(
            u16::try_from(self.attempts.len())
                .expect("validated acquisition attempt count is representable"),
        );
        for attempt in &self.attempts {
            attempt.encode(writer);
        }
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, OutputTopologyError> {
        let profile_code_raw = reader.u32()?;
        let request_flags_raw = reader.u32()?;
        let count = usize::from(reader.u16()?);
        if count == 0 {
            return Err(OutputTopologyError::EmptyAttempts);
        }
        if count > MAX_OUTPUT_TOPOLOGY_ATTEMPTS {
            return Err(OutputTopologyError::TooManyAttempts {
                actual: count,
                maximum: MAX_OUTPUT_TOPOLOGY_ATTEMPTS,
            });
        }
        let required = count
            .checked_mul(ACQUISITION_ATTEMPT_BYTES)
            .ok_or(OutputTopologyError::LengthOverflow)?;
        reader.require(required)?;
        let mut attempts = Vec::new();
        attempts
            .try_reserve_exact(count)
            .map_err(|_| OutputTopologyError::AllocationFailed { requested: count })?;
        for index in 0..count {
            let attempt = AcquisitionAttempt::decode(reader)?;
            if attempt.retry_ordinal != index as u32 {
                return Err(OutputTopologyError::NonCanonicalRetryOrdinal {
                    index,
                    actual: attempt.retry_ordinal,
                });
            }
            attempts.push(attempt);
        }
        Ok(Self {
            profile_code_raw,
            request_flags_raw,
            attempts,
        })
    }
}

/// DisplayConfig desktop source mode position, dimensions, and raw pixel format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceModeFacts {
    pub position_x: i32,
    pub position_y: i32,
    pub width: u32,
    pub height: u32,
    pub pixel_format_raw: u32,
}

impl SourceModeFacts {
    fn encode(self, writer: &mut Writer) {
        writer.i32(self.position_x);
        writer.i32(self.position_y);
        writer.u32(self.width);
        writer.u32(self.height);
        writer.u32(self.pixel_format_raw);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, OutputTopologyError> {
        Ok(Self {
            position_x: reader.i32()?,
            position_y: reader.i32()?,
            width: reader.u32()?,
            height: reader.u32()?,
            pixel_format_raw: reader.u32()?,
        })
    }
}

/// GDI device name, desktop rectangles, and mode facts for the active display source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFacts {
    pub gdi_device_name: Utf16Text,
    pub client_rect: RectI32,
    pub monitor_rect: RectI32,
    pub work_rect: RectI32,
    pub monitor_flags_raw: u32,
    pub mode: SourceModeFacts,
    pub status_flags_raw: u32,
}

impl SourceFacts {
    /// Returns whether the monitor flags mark this display source as the primary desktop monitor.
    pub const fn primary(&self) -> bool {
        self.monitor_flags_raw & 1 != 0
    }

    fn validate(&self) -> Result<(), OutputTopologyError> {
        validate_utf16_units(self.gdi_device_name.units())
    }

    fn encoded_len(&self) -> Result<usize, OutputTopologyError> {
        self.gdi_device_name
            .encoded_len()?
            .checked_add(48 + 4 + 20 + 4)
            .ok_or(OutputTopologyError::LengthOverflow)
    }

    fn encode(&self, writer: &mut Writer) {
        self.gdi_device_name.encode(writer);
        self.client_rect.encode(writer);
        self.monitor_rect.encode(writer);
        self.work_rect.encode(writer);
        writer.u32(self.monitor_flags_raw);
        self.mode.encode(writer);
        writer.u32(self.status_flags_raw);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, OutputTopologyError> {
        Ok(Self {
            gdi_device_name: Utf16Text::decode(reader, "source GDI device name")?,
            client_rect: RectI32::decode(reader)?,
            monitor_rect: RectI32::decode(reader)?,
            work_rect: RectI32::decode(reader)?,
            monitor_flags_raw: reader.u32()?,
            mode: SourceModeFacts::decode(reader)?,
            status_flags_raw: reader.u32()?,
        })
    }
}

/// Manufacturer identifier and product code extracted from a monitor EDID descriptor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdidIdentity {
    pub manufacturer_id: u16,
    pub product_code: u16,
}

/// Result of querying `DISPLAYCONFIG_TARGET_DEVICE_NAME` for one active route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetIdentityObservation {
    Available {
        device_path: Utf16Text,
        friendly_annotation: Option<Utf16Text>,
        name_flags_raw: u32,
        edid: Option<EdidIdentity>,
        connector_instance: u32,
        reported_output_technology_raw: u32,
    },
    Unavailable {
        reason_code_raw: u32,
        error_code: u32,
    },
}

impl TargetIdentityObservation {
    fn validate(&self) -> Result<(), OutputTopologyError> {
        match self {
            Self::Available {
                device_path,
                friendly_annotation,
                ..
            } => {
                validate_utf16_units(device_path.units())?;
                if let Some(annotation) = friendly_annotation {
                    validate_utf16_units(annotation.units())?;
                }
                Ok(())
            }
            Self::Unavailable { .. } => Ok(()),
        }
    }

    fn encoded_len(&self) -> Result<usize, OutputTopologyError> {
        match self {
            Self::Available {
                device_path,
                friendly_annotation,
                edid,
                ..
            } => {
                let mut length = 1_usize
                    .checked_add(device_path.encoded_len()?)
                    .and_then(|value| value.checked_add(1 + 4 + 1 + 4 + 4))
                    .ok_or(OutputTopologyError::LengthOverflow)?;
                if let Some(annotation) = friendly_annotation {
                    length = length
                        .checked_add(annotation.encoded_len()?)
                        .ok_or(OutputTopologyError::LengthOverflow)?;
                }
                if edid.is_some() {
                    length = length
                        .checked_add(4)
                        .ok_or(OutputTopologyError::LengthOverflow)?;
                }
                Ok(length)
            }
            Self::Unavailable { .. } => Ok(1 + 4 + 4),
        }
    }

    fn stable_cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (
                Self::Available {
                    device_path: left_path,
                    name_flags_raw: left_flags,
                    edid: left_edid,
                    connector_instance: left_connector,
                    reported_output_technology_raw: left_technology,
                    ..
                },
                Self::Available {
                    device_path: right_path,
                    name_flags_raw: right_flags,
                    edid: right_edid,
                    connector_instance: right_connector,
                    reported_output_technology_raw: right_technology,
                    ..
                },
            ) => left_path
                .cmp(right_path)
                .then_with(|| left_flags.cmp(right_flags))
                .then_with(|| left_edid.cmp(right_edid))
                .then_with(|| left_connector.cmp(right_connector))
                .then_with(|| left_technology.cmp(right_technology)),
            (Self::Available { .. }, Self::Unavailable { .. }) => Ordering::Less,
            (Self::Unavailable { .. }, Self::Available { .. }) => Ordering::Greater,
            (
                Self::Unavailable {
                    reason_code_raw: left_reason,
                    error_code: left_error,
                },
                Self::Unavailable {
                    reason_code_raw: right_reason,
                    error_code: right_error,
                },
            ) => left_reason
                .cmp(right_reason)
                .then_with(|| left_error.cmp(right_error)),
        }
    }

    fn annotation_cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (
                Self::Available {
                    friendly_annotation: left,
                    ..
                },
                Self::Available {
                    friendly_annotation: right,
                    ..
                },
            ) => left.cmp(right),
            _ => Ordering::Equal,
        }
    }

    fn encode(&self, writer: &mut Writer) {
        match self {
            Self::Available {
                device_path,
                friendly_annotation,
                name_flags_raw,
                edid,
                connector_instance,
                reported_output_technology_raw,
            } => {
                writer.u8(AVAILABLE_MARKER);
                device_path.encode(writer);
                writer.boolean(friendly_annotation.is_some());
                if let Some(annotation) = friendly_annotation {
                    annotation.encode(writer);
                }
                writer.u32(*name_flags_raw);
                writer.boolean(edid.is_some());
                if let Some(identity) = edid {
                    writer.u16(identity.manufacturer_id);
                    writer.u16(identity.product_code);
                }
                writer.u32(*connector_instance);
                writer.u32(*reported_output_technology_raw);
            }
            Self::Unavailable {
                reason_code_raw,
                error_code,
            } => {
                writer.u8(UNAVAILABLE_MARKER);
                writer.u32(*reason_code_raw);
                writer.u32(*error_code);
            }
        }
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, OutputTopologyError> {
        match reader.u8()? {
            AVAILABLE_MARKER => {
                let device_path = Utf16Text::decode(reader, "target device path")?;
                let friendly_annotation = if reader.boolean("friendly annotation presence")? {
                    Some(Utf16Text::decode(reader, "target friendly annotation")?)
                } else {
                    None
                };
                let name_flags_raw = reader.u32()?;
                let edid = if reader.boolean("EDID identity validity")? {
                    Some(EdidIdentity {
                        manufacturer_id: reader.u16()?,
                        product_code: reader.u16()?,
                    })
                } else {
                    None
                };
                Ok(Self::Available {
                    device_path,
                    friendly_annotation,
                    name_flags_raw,
                    edid,
                    connector_instance: reader.u32()?,
                    reported_output_technology_raw: reader.u32()?,
                })
            }
            UNAVAILABLE_MARKER => Ok(Self::Unavailable {
                reason_code_raw: reader.u32()?,
                error_code: reader.u32()?,
            }),
            marker => Err(OutputTopologyError::UnknownMarker {
                field: "target identity outcome",
                marker,
            }),
        }
    }
}

/// Pixel clock rate, sync frequencies, and active/total dimensions for a display target.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TargetSignalTiming {
    pub pixel_rate: u64,
    pub h_sync: RawRational,
    pub v_sync: RawRational,
    pub active_size: PixelSize,
    pub total_size: PixelSize,
    pub video_standard_raw: u32,
    pub scanline_ordering_raw: u32,
}

impl TargetSignalTiming {
    fn canonical_cmp(self, other: Self) -> Ordering {
        self.pixel_rate
            .cmp(&other.pixel_rate)
            .then_with(|| self.h_sync.canonical_cmp(other.h_sync))
            .then_with(|| self.v_sync.canonical_cmp(other.v_sync))
            .then_with(|| self.active_size.cmp(&other.active_size))
            .then_with(|| self.total_size.cmp(&other.total_size))
            .then_with(|| self.video_standard_raw.cmp(&other.video_standard_raw))
            .then_with(|| self.scanline_ordering_raw.cmp(&other.scanline_ordering_raw))
    }

    fn encode(self, writer: &mut Writer) {
        writer.u64(self.pixel_rate);
        self.h_sync.encode(writer);
        self.v_sync.encode(writer);
        self.active_size.encode(writer);
        self.total_size.encode(writer);
        writer.u32(self.video_standard_raw);
        writer.u32(self.scanline_ordering_raw);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, OutputTopologyError> {
        Ok(Self {
            pixel_rate: reader.u64()?,
            h_sync: RawRational::decode(reader)?,
            v_sync: RawRational::decode(reader)?,
            active_size: PixelSize::decode(reader)?,
            total_size: PixelSize::decode(reader)?,
            video_standard_raw: reader.u32()?,
            scanline_ordering_raw: reader.u32()?,
        })
    }
}

/// Source size, desktop region, and clipping rectangle for a target desktop image.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DesktopImageFacts {
    pub path_source_size: PixelSize,
    pub desktop_image_region: RectI32,
    pub desktop_image_clip: RectI32,
}

impl DesktopImageFacts {
    fn encode(self, writer: &mut Writer) {
        self.path_source_size.encode(writer);
        self.desktop_image_region.encode(writer);
        self.desktop_image_clip.encode(writer);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, OutputTopologyError> {
        Ok(Self {
            path_source_size: PixelSize::decode(reader)?,
            desktop_image_region: RectI32::decode(reader)?,
            desktop_image_clip: RectI32::decode(reader)?,
        })
    }
}

/// One active source-to-target route. Path-level and target-name technologies are distinct facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetRouteFacts {
    pub identity: TargetIdentityObservation,
    pub output_technology_raw: u32,
    pub target_available: bool,
    pub status_flags_raw: u32,
    pub rotation_raw: u32,
    pub scaling_raw: u32,
    pub refresh_rate: RawRational,
    pub path_flags_raw: u32,
    pub signal: TargetSignalTiming,
    pub desktop_image: Option<DesktopImageFacts>,
}

impl TargetRouteFacts {
    fn validate(&self) -> Result<(), OutputTopologyError> {
        self.identity.validate()
    }

    fn encoded_len(&self) -> Result<usize, OutputTopologyError> {
        self.identity
            .encoded_len()?
            .checked_add(4 + 1 + 4 + 4 + 4 + 8 + 4 + 48 + 1)
            .and_then(|length| {
                if self.desktop_image.is_some() {
                    length.checked_add(40)
                } else {
                    Some(length)
                }
            })
            .ok_or(OutputTopologyError::LengthOverflow)
    }

    /// Compares route-defining facts while excluding the human-friendly annotation.
    pub fn stable_cmp(&self, other: &Self) -> Ordering {
        self.identity
            .stable_cmp(&other.identity)
            .then_with(|| self.output_technology_raw.cmp(&other.output_technology_raw))
            .then_with(|| self.target_available.cmp(&other.target_available))
            .then_with(|| self.status_flags_raw.cmp(&other.status_flags_raw))
            .then_with(|| self.rotation_raw.cmp(&other.rotation_raw))
            .then_with(|| self.scaling_raw.cmp(&other.scaling_raw))
            .then_with(|| self.refresh_rate.canonical_cmp(other.refresh_rate))
            .then_with(|| self.path_flags_raw.cmp(&other.path_flags_raw))
            .then_with(|| self.signal.canonical_cmp(other.signal))
            .then_with(|| self.desktop_image.cmp(&other.desktop_image))
    }

    pub(crate) fn canonical_cmp(&self, other: &Self) -> Ordering {
        self.stable_cmp(other)
            .then_with(|| self.identity.annotation_cmp(&other.identity))
    }

    fn encode(&self, writer: &mut Writer) {
        self.identity.encode(writer);
        writer.u32(self.output_technology_raw);
        writer.boolean(self.target_available);
        writer.u32(self.status_flags_raw);
        writer.u32(self.rotation_raw);
        writer.u32(self.scaling_raw);
        self.refresh_rate.encode(writer);
        writer.u32(self.path_flags_raw);
        self.signal.encode(writer);
        writer.boolean(self.desktop_image.is_some());
        if let Some(desktop_image) = self.desktop_image {
            desktop_image.encode(writer);
        }
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, OutputTopologyError> {
        Ok(Self {
            identity: TargetIdentityObservation::decode(reader)?,
            output_technology_raw: reader.u32()?,
            target_available: reader.boolean("target availability")?,
            status_flags_raw: reader.u32()?,
            rotation_raw: reader.u32()?,
            scaling_raw: reader.u32()?,
            refresh_rate: RawRational::decode(reader)?,
            path_flags_raw: reader.u32()?,
            signal: TargetSignalTiming::decode(reader)?,
            desktop_image: if reader.boolean("desktop-image presence")? {
                Some(DesktopImageFacts::decode(reader)?)
            } else {
                None
            },
        })
    }
}

/// Acquisition trace, active source facts, and canonically ordered target routes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvailableTopologyFacts {
    pub acquisition: AcquisitionFacts,
    pub source: SourceFacts,
    pub routes: Vec<TargetRouteFacts>,
}

/// Acquisition trace and raw failure codes when output topology collection is unavailable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnavailableTopologyFacts {
    pub acquisition: AcquisitionFacts,
    pub reason_code_raw: u32,
    pub error_code: u32,
}

/// Outcome of an active monitor output topology query, either available or unavailable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputTopologyFacts {
    Available(AvailableTopologyFacts),
    Unavailable(UnavailableTopologyFacts),
}

impl OutputTopologyFacts {
    fn validate(&self) -> Result<(), OutputTopologyError> {
        match self {
            Self::Available(facts) => {
                facts.acquisition.validate()?;
                facts.source.validate()?;
                validate_routes(&facts.routes)
            }
            Self::Unavailable(facts) => facts.acquisition.validate(),
        }
    }

    fn encoded_len(&self) -> Result<usize, OutputTopologyError> {
        let length = match self {
            Self::Available(facts) => {
                let mut length = 1_usize
                    .checked_add(facts.acquisition.encoded_len()?)
                    .and_then(|value| value.checked_add(facts.source.encoded_len().ok()?))
                    .and_then(|value| value.checked_add(2))
                    .ok_or(OutputTopologyError::LengthOverflow)?;
                for route in &facts.routes {
                    length = length
                        .checked_add(4)
                        .and_then(|value| value.checked_add(route.encoded_len().ok()?))
                        .ok_or(OutputTopologyError::LengthOverflow)?;
                }
                length
            }
            Self::Unavailable(facts) => 1_usize
                .checked_add(facts.acquisition.encoded_len()?)
                .and_then(|value| value.checked_add(8))
                .ok_or(OutputTopologyError::LengthOverflow)?,
        };
        if length > MAX_OUTPUT_TOPOLOGY_PAYLOAD_BYTES {
            return Err(OutputTopologyError::PayloadTooLarge {
                actual: length,
                maximum: MAX_OUTPUT_TOPOLOGY_PAYLOAD_BYTES,
            });
        }
        Ok(length)
    }

    fn encode(&self, writer: &mut Writer) -> Result<(), OutputTopologyError> {
        match self {
            Self::Available(facts) => {
                writer.u8(AVAILABLE_MARKER);
                facts.acquisition.encode(writer);
                facts.source.encode(writer);
                writer.u16(
                    u16::try_from(facts.routes.len())
                        .expect("validated route count is representable"),
                );
                for route in &facts.routes {
                    let route_len = route.encoded_len()?;
                    writer.u32(
                        u32::try_from(route_len)
                            .expect("bounded route length is representable as u32"),
                    );
                    route.encode(writer);
                }
            }
            Self::Unavailable(facts) => {
                writer.u8(UNAVAILABLE_MARKER);
                facts.acquisition.encode(writer);
                writer.u32(facts.reason_code_raw);
                writer.u32(facts.error_code);
            }
        }
        Ok(())
    }
}

/// A tag-bound active-output observation. Its tag is never truncated for authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputTopologyObservation {
    tag: ContentTag,
    facts: OutputTopologyFacts,
}

impl OutputTopologyObservation {
    /// Validates, canonically sorts target routes, and constructs an available topology observation.
    pub fn new_available(
        acquisition: AcquisitionFacts,
        source: SourceFacts,
        mut routes: Vec<TargetRouteFacts>,
    ) -> Result<Self, OutputTopologyError> {
        validate_route_members(&routes)?;
        routes.sort_by(TargetRouteFacts::canonical_cmp);
        reject_duplicate_routes(&routes)?;
        Self::from_facts(OutputTopologyFacts::Available(AvailableTopologyFacts {
            acquisition,
            source,
            routes,
        }))
    }

    /// Validates and constructs an unavailable `OutputTopologyObservation` with failure codes.
    pub fn new_unavailable(
        acquisition: AcquisitionFacts,
        reason_code_raw: u32,
        error_code: u32,
    ) -> Result<Self, OutputTopologyError> {
        Self::from_facts(OutputTopologyFacts::Unavailable(UnavailableTopologyFacts {
            acquisition,
            reason_code_raw,
            error_code,
        }))
    }

    fn from_facts(facts: OutputTopologyFacts) -> Result<Self, OutputTopologyError> {
        facts.validate()?;
        let mut observation = Self {
            tag: ContentTag::from_digest(TagKind::OutputTopology, [0; 32]),
            facts,
        };
        observation.tag = observation.recompute_tag()?;
        Ok(observation)
    }

    /// Returns the canonical content tag identifying this output topology observation.
    pub const fn tag(&self) -> ContentTag {
        self.tag
    }

    /// Returns the underlying `OutputTopologyFacts` variant for this observation.
    pub fn facts(&self) -> &OutputTopologyFacts {
        &self.facts
    }

    /// Returns the `AvailableTopologyFacts` when output topology collection succeeded.
    pub fn available(&self) -> Option<&AvailableTopologyFacts> {
        match &self.facts {
            OutputTopologyFacts::Available(facts) => Some(facts),
            OutputTopologyFacts::Unavailable(_) => None,
        }
    }

    /// Returns the `UnavailableTopologyFacts` when output topology collection failed.
    pub fn unavailable(&self) -> Option<&UnavailableTopologyFacts> {
        match &self.facts {
            OutputTopologyFacts::Available(_) => None,
            OutputTopologyFacts::Unavailable(facts) => Some(facts),
        }
    }

    /// Stable topology equality intentionally excludes acquisition mechanics and annotations.
    pub fn stable_topology_eq(&self, other: &Self) -> bool {
        let (Some(left), Some(right)) = (self.available(), other.available()) else {
            return false;
        };
        left.source == right.source
            && left.routes.len() == right.routes.len()
            && left
                .routes
                .iter()
                .zip(&right.routes)
                .all(|(left_route, right_route)| {
                    left_route.stable_cmp(right_route) == Ordering::Equal
                })
    }

    /// Validates and encodes this `OutputTopologyObservation` into canonical payload bytes.
    pub fn try_encode_canonical(&self) -> Result<Vec<u8>, OutputTopologyError> {
        self.facts.validate()?;
        let length = self.facts.encoded_len()?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| OutputTopologyError::AllocationFailed { requested: length })?;
        let mut writer = Writer::new(bytes);
        self.facts.encode(&mut writer)?;
        let bytes = writer.finish();
        debug_assert_eq!(bytes.len(), length);
        Ok(bytes)
    }

    /// Decodes and validates an `OutputTopologyObservation` from canonical payload bytes.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, OutputTopologyError> {
        if bytes.is_empty() {
            return Err(OutputTopologyError::WrongPayloadLength {
                actual: 0,
                maximum: MAX_OUTPUT_TOPOLOGY_PAYLOAD_BYTES,
            });
        }
        if bytes.len() > MAX_OUTPUT_TOPOLOGY_PAYLOAD_BYTES {
            return Err(OutputTopologyError::WrongPayloadLength {
                actual: bytes.len(),
                maximum: MAX_OUTPUT_TOPOLOGY_PAYLOAD_BYTES,
            });
        }
        let mut reader = Reader::new(bytes);
        let facts = match reader.u8()? {
            AVAILABLE_MARKER => {
                let acquisition = AcquisitionFacts::decode(&mut reader)?;
                let source = SourceFacts::decode(&mut reader)?;
                let route_count = usize::from(reader.u16()?);
                if route_count == 0 {
                    return Err(OutputTopologyError::EmptyRoutes);
                }
                if route_count > MAX_OUTPUT_TOPOLOGY_ROUTES {
                    return Err(OutputTopologyError::TooManyRoutes {
                        actual: route_count,
                        maximum: MAX_OUTPUT_TOPOLOGY_ROUTES,
                    });
                }
                let minimum_required = route_count
                    .checked_mul(4 + MIN_ROUTE_BYTES)
                    .ok_or(OutputTopologyError::LengthOverflow)?;
                reader.require(minimum_required)?;
                let mut routes = Vec::new();
                routes.try_reserve_exact(route_count).map_err(|_| {
                    OutputTopologyError::AllocationFailed {
                        requested: route_count,
                    }
                })?;
                for _ in 0..route_count {
                    let route_len = usize::try_from(reader.u32()?)
                        .map_err(|_| OutputTopologyError::LengthOverflow)?;
                    if route_len < MIN_ROUTE_BYTES {
                        return Err(OutputTopologyError::RouteLengthTooSmall {
                            actual: route_len,
                            minimum: MIN_ROUTE_BYTES,
                        });
                    }
                    if route_len > MAX_OUTPUT_TOPOLOGY_PAYLOAD_BYTES {
                        return Err(OutputTopologyError::RouteLengthTooLarge {
                            actual: route_len,
                            maximum: MAX_OUTPUT_TOPOLOGY_PAYLOAD_BYTES,
                        });
                    }
                    let mut route_reader = reader.subreader(route_len)?;
                    let route = TargetRouteFacts::decode(&mut route_reader)?;
                    route_reader.finish()?;
                    routes.push(route);
                }
                validate_routes(&routes)?;
                OutputTopologyFacts::Available(AvailableTopologyFacts {
                    acquisition,
                    source,
                    routes,
                })
            }
            UNAVAILABLE_MARKER => {
                let acquisition = AcquisitionFacts::decode(&mut reader)?;
                OutputTopologyFacts::Unavailable(UnavailableTopologyFacts {
                    acquisition,
                    reason_code_raw: reader.u32()?,
                    error_code: reader.u32()?,
                })
            }
            marker => {
                return Err(OutputTopologyError::UnknownMarker {
                    field: "observation outcome",
                    marker,
                });
            }
        };
        reader.finish()?;
        let observation = Self::from_facts(facts)?;
        if observation.try_encode_canonical()?.as_slice() != bytes {
            return Err(OutputTopologyError::NonCanonicalEncoding);
        }
        Ok(observation)
    }

    /// Re-verifies the topology facts, tag kind, and canonical content tag of this observation.
    pub fn verify(&self) -> Result<(), OutputTopologyError> {
        self.facts.validate()?;
        if self.tag.kind() != TagKind::OutputTopology {
            return Err(OutputTopologyError::WrongTagKind {
                actual: self.tag.kind(),
            });
        }
        if self.tag != self.recompute_tag()? {
            return Err(OutputTopologyError::TagMismatch);
        }
        Ok(())
    }

    fn recompute_tag(&self) -> Result<ContentTag, OutputTopologyError> {
        let bytes = self.try_encode_canonical()?;
        hash_canonical(
            TagKind::OutputTopology,
            OUTPUT_TOPOLOGY_OBSERVATION_SCHEMA,
            bytes.len() as u64,
            |sink| sink(&bytes),
        )
        .map_err(OutputTopologyError::Tag)
    }
}

fn validate_route_members(routes: &[TargetRouteFacts]) -> Result<(), OutputTopologyError> {
    if routes.is_empty() {
        return Err(OutputTopologyError::EmptyRoutes);
    }
    if routes.len() > MAX_OUTPUT_TOPOLOGY_ROUTES || routes.len() > usize::from(u16::MAX) {
        return Err(OutputTopologyError::TooManyRoutes {
            actual: routes.len(),
            maximum: MAX_OUTPUT_TOPOLOGY_ROUTES,
        });
    }
    for route in routes {
        route.validate()?;
    }
    Ok(())
}

fn reject_duplicate_routes(routes: &[TargetRouteFacts]) -> Result<(), OutputTopologyError> {
    if routes
        .windows(2)
        .any(|pair| pair[0].stable_cmp(&pair[1]).is_eq())
    {
        return Err(OutputTopologyError::DuplicateRoute);
    }
    Ok(())
}

fn validate_routes(routes: &[TargetRouteFacts]) -> Result<(), OutputTopologyError> {
    validate_route_members(routes)?;
    reject_duplicate_routes(routes)?;
    for pair in routes.windows(2) {
        if pair[0].canonical_cmp(&pair[1]) != Ordering::Less {
            return Err(OutputTopologyError::NonCanonicalRouteOrder);
        }
    }
    Ok(())
}

/// Validation or canonical codec failure for an `OutputTopologyObservation`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputTopologyError {
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
    EmptyUtf16 {
        field: &'static str,
    },
    Utf16TooLong {
        field: &'static str,
        actual: usize,
        maximum: usize,
    },
    EmbeddedUtf16Nul {
        field: &'static str,
        index: usize,
    },
    EmptyAttempts,
    TooManyAttempts {
        actual: usize,
        maximum: usize,
    },
    NonCanonicalRetryOrdinal {
        index: usize,
        actual: u32,
    },
    EmptyRoutes,
    TooManyRoutes {
        actual: usize,
        maximum: usize,
    },
    DuplicateRoute,
    NonCanonicalRouteOrder,
    RouteLengthTooSmall {
        actual: usize,
        minimum: usize,
    },
    RouteLengthTooLarge {
        actual: usize,
        maximum: usize,
    },
    UnknownMarker {
        field: &'static str,
        marker: u8,
    },
    NonCanonicalEncoding,
    WrongTagKind {
        actual: TagKind,
    },
    TagMismatch,
    Tag(TagError),
}

impl fmt::Display for OutputTopologyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongPayloadLength { actual, maximum } => write!(
                formatter,
                "output topology payload has {actual} bytes; valid range is 1..={maximum}"
            ),
            Self::PayloadTooLarge { actual, maximum } => write!(
                formatter,
                "output topology payload requires {actual} bytes, exceeding {maximum}"
            ),
            Self::UnexpectedEnd { needed, remaining } => write!(
                formatter,
                "output topology needs {needed} more bytes but only {remaining} remain"
            ),
            Self::TrailingBytes { actual } => {
                write!(formatter, "output topology has {actual} trailing bytes")
            }
            Self::LengthOverflow => formatter.write_str("output topology length overflowed"),
            Self::AllocationFailed { requested } => write!(
                formatter,
                "could not allocate {requested} bounded output-topology elements or bytes"
            ),
            Self::EmptyUtf16 { field } => write!(formatter, "{field} must not be empty"),
            Self::Utf16TooLong {
                field,
                actual,
                maximum,
            } => write!(
                formatter,
                "{field} has {actual} UTF-16 units, exceeding {maximum}"
            ),
            Self::EmbeddedUtf16Nul { field, index } => {
                write!(
                    formatter,
                    "{field} contains an embedded NUL at unit {index}"
                )
            }
            Self::EmptyAttempts => {
                formatter.write_str("output topology requires at least one acquisition attempt")
            }
            Self::TooManyAttempts { actual, maximum } => write!(
                formatter,
                "output topology has {actual} acquisition attempts, exceeding {maximum}"
            ),
            Self::NonCanonicalRetryOrdinal { index, actual } => write!(
                formatter,
                "acquisition attempt {index} carries retry ordinal {actual}"
            ),
            Self::EmptyRoutes => {
                formatter.write_str("available output topology requires at least one target route")
            }
            Self::TooManyRoutes { actual, maximum } => write!(
                formatter,
                "output topology has {actual} target routes, exceeding {maximum}"
            ),
            Self::DuplicateRoute => {
                formatter.write_str("output topology contains a duplicate encoded target route")
            }
            Self::NonCanonicalRouteOrder => {
                formatter.write_str("target routes are not in canonical set order")
            }
            Self::RouteLengthTooSmall { actual, minimum } => write!(
                formatter,
                "target route declares {actual} bytes, below minimum {minimum}"
            ),
            Self::RouteLengthTooLarge { actual, maximum } => write!(
                formatter,
                "target route declares {actual} bytes, exceeding {maximum}"
            ),
            Self::UnknownMarker { field, marker } => {
                write!(formatter, "{field} uses unknown marker {marker}")
            }
            Self::NonCanonicalEncoding => {
                formatter.write_str("output topology bytes are not canonical")
            }
            Self::WrongTagKind { actual } => write!(
                formatter,
                "output topology has {} tag kind instead of output-topology",
                actual.name()
            ),
            Self::TagMismatch => {
                formatter.write_str("output topology tag does not match its canonical facts")
            }
            Self::Tag(source) => write!(formatter, "could not tag output topology: {source}"),
        }
    }
}

impl std::error::Error for OutputTopologyError {}

struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn boolean(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.cursor
    }

    fn require(&self, needed: usize) -> Result<(), OutputTopologyError> {
        if self.remaining() < needed {
            return Err(OutputTopologyError::UnexpectedEnd {
                needed,
                remaining: self.remaining(),
            });
        }
        Ok(())
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], OutputTopologyError> {
        self.require(length)?;
        let start = self.cursor;
        self.cursor += length;
        Ok(&self.bytes[start..self.cursor])
    }

    fn subreader(&mut self, length: usize) -> Result<Reader<'a>, OutputTopologyError> {
        Ok(Self::new(self.take(length)?))
    }

    fn finish(self) -> Result<(), OutputTopologyError> {
        if self.remaining() != 0 {
            return Err(OutputTopologyError::TrailingBytes {
                actual: self.remaining(),
            });
        }
        Ok(())
    }

    fn u8(&mut self) -> Result<u8, OutputTopologyError> {
        Ok(self.take(1)?[0])
    }

    fn boolean(&mut self, field: &'static str) -> Result<bool, OutputTopologyError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            marker => Err(OutputTopologyError::UnknownMarker { field, marker }),
        }
    }

    fn u16(&mut self) -> Result<u16, OutputTopologyError> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().expect("two-byte lane"),
        ))
    }

    fn u32(&mut self) -> Result<u32, OutputTopologyError> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().expect("four-byte lane"),
        ))
    }

    fn i32(&mut self) -> Result<i32, OutputTopologyError> {
        Ok(i32::from_be_bytes(
            self.take(4)?.try_into().expect("four-byte lane"),
        ))
    }

    fn u64(&mut self) -> Result<u64, OutputTopologyError> {
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().expect("eight-byte lane"),
        ))
    }
}

const _: () = assert!(ACQUISITION_ATTEMPT_BYTES == 8 * 4);
const _: () = assert!(MIN_ROUTE_BYTES == 87);

#[cfg(test)]
mod tests {
    use super::*;

    fn text(units: &[u16]) -> Utf16Text {
        Utf16Text::from_units(units).unwrap()
    }

    fn acquisition(seed: u32) -> AcquisitionFacts {
        AcquisitionFacts {
            profile_code_raw: 0xa000_0000 | seed,
            request_flags_raw: 0xf000_0000 | seed,
            attempts: vec![
                AcquisitionAttempt {
                    operation_code_raw: 1,
                    profile_code_raw: 2,
                    request_flags_raw: 0x8000_0000,
                    retry_ordinal: 0,
                    outcome_code_raw: 3,
                    error_code: 122,
                    reported_path_capacity: 1,
                    reported_mode_capacity: 2,
                },
                AcquisitionAttempt {
                    operation_code_raw: 4,
                    profile_code_raw: 2,
                    request_flags_raw: 0x8000_0000,
                    retry_ordinal: 1,
                    outcome_code_raw: 5,
                    error_code: 0,
                    reported_path_capacity: 2,
                    reported_mode_capacity: 5,
                },
            ],
        }
    }

    fn source() -> SourceFacts {
        SourceFacts {
            gdi_device_name: text(&[
                b'\\' as u16,
                b'\\' as u16,
                b'.' as u16,
                b'\\' as u16,
                b'D' as u16,
            ]),
            client_rect: RectI32 {
                left: 10,
                top: 20,
                right: 810,
                bottom: 620,
            },
            monitor_rect: RectI32 {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            work_rect: RectI32 {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1040,
            },
            monitor_flags_raw: 0x8000_0001,
            mode: SourceModeFacts {
                position_x: 0,
                position_y: 0,
                width: 1920,
                height: 1080,
                pixel_format_raw: 0xffff_0001,
            },
            status_flags_raw: 0x8000_0042,
        }
    }

    fn route(name: u16, refresh: RawRational) -> TargetRouteFacts {
        TargetRouteFacts {
            identity: TargetIdentityObservation::Available {
                device_path: text(&[0xd800, b'P' as u16, name]),
                friendly_annotation: Some(text(&[b'M' as u16, name])),
                name_flags_raw: 0xf0f0_00f0,
                edid: Some(EdidIdentity {
                    manufacturer_id: 0x1234,
                    product_code: name,
                }),
                connector_instance: u32::from(name),
                reported_output_technology_raw: 0xffff_ff01,
            },
            output_technology_raw: 0xffff_ff02,
            target_available: true,
            status_flags_raw: 0x8000_0001,
            rotation_raw: 0xffff_0002,
            scaling_raw: 0xffff_0003,
            refresh_rate: refresh,
            path_flags_raw: 0xdead_beef,
            signal: TargetSignalTiming {
                pixel_rate: 148_500_000,
                h_sync: RawRational {
                    numerator: 67_500,
                    denominator: 1,
                },
                v_sync: refresh,
                active_size: PixelSize {
                    width: 1920,
                    height: 1080,
                },
                total_size: PixelSize {
                    width: 2200,
                    height: 1125,
                },
                video_standard_raw: 0xffff_0010,
                scanline_ordering_raw: 0xffff_0020,
            },
            desktop_image: Some(DesktopImageFacts {
                path_source_size: PixelSize {
                    width: 1920,
                    height: 1080,
                },
                desktop_image_region: RectI32 {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1080,
                },
                desktop_image_clip: RectI32 {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1080,
                },
            }),
        }
    }

    fn available(routes: Vec<TargetRouteFacts>) -> OutputTopologyObservation {
        OutputTopologyObservation::new_available(acquisition(1), source(), routes).unwrap()
    }

    #[test]
    fn available_and_unavailable_round_trip_and_verify() {
        let available = available(vec![
            route(
                2,
                RawRational {
                    numerator: 60,
                    denominator: 1,
                },
            ),
            route(
                1,
                RawRational {
                    numerator: 120,
                    denominator: 2,
                },
            ),
        ]);
        let bytes = available.try_encode_canonical().unwrap();
        assert!(bytes.len() <= MAX_OUTPUT_TOPOLOGY_PAYLOAD_BYTES);
        let decoded = OutputTopologyObservation::decode_canonical(&bytes).unwrap();
        assert_eq!(decoded, available);
        assert_eq!(decoded.tag().kind(), TagKind::OutputTopology);
        decoded.verify().unwrap();

        let unavailable =
            OutputTopologyObservation::new_unavailable(acquisition(2), 7, 87).unwrap();
        let decoded = OutputTopologyObservation::decode_canonical(
            &unavailable.try_encode_canonical().unwrap(),
        )
        .unwrap();
        assert_eq!(decoded, unavailable);
        assert_eq!(decoded.unavailable().unwrap().reason_code_raw, 7);
        decoded.verify().unwrap();
    }

    #[test]
    fn route_permutations_have_one_canonical_tag() {
        let first = route(
            1,
            RawRational {
                numerator: 60,
                denominator: 1,
            },
        );
        let second = route(
            2,
            RawRational {
                numerator: 75,
                denominator: 1,
            },
        );
        let left = available(vec![first.clone(), second.clone()]);
        let right = available(vec![second, first]);
        assert_eq!(left.tag(), right.tag());
        assert_eq!(
            left.try_encode_canonical().unwrap(),
            right.try_encode_canonical().unwrap()
        );
    }

    #[test]
    fn every_cloned_target_route_is_retained() {
        let observation = available(vec![
            route(
                10,
                RawRational {
                    numerator: 60,
                    denominator: 1,
                },
            ),
            route(
                11,
                RawRational {
                    numerator: 60,
                    denominator: 1,
                },
            ),
            route(
                12,
                RawRational {
                    numerator: 60,
                    denominator: 1,
                },
            ),
        ]);
        assert_eq!(observation.available().unwrap().routes.len(), 3);
        let decoded = OutputTopologyObservation::decode_canonical(
            &observation.try_encode_canonical().unwrap(),
        )
        .unwrap();
        assert_eq!(decoded.available().unwrap().routes.len(), 3);
    }

    #[test]
    fn duplicate_encoded_routes_are_rejected() {
        let duplicate = route(
            1,
            RawRational {
                numerator: 60,
                denominator: 1,
            },
        );
        assert_eq!(
            OutputTopologyObservation::new_available(
                acquisition(1),
                source(),
                vec![duplicate.clone(), duplicate]
            ),
            Err(OutputTopologyError::DuplicateRoute)
        );
    }

    #[test]
    fn friendly_annotations_cannot_create_distinct_routes() {
        let first = route(
            1,
            RawRational {
                numerator: 60,
                denominator: 1,
            },
        );
        let mut relabeled = first.clone();
        let TargetIdentityObservation::Available {
            friendly_annotation,
            ..
        } = &mut relabeled.identity
        else {
            panic!("route fixture must have an available target identity");
        };
        *friendly_annotation = Some(text(&[b'O' as u16, b't' as u16, b'h' as u16]));

        assert_eq!(
            OutputTopologyObservation::new_available(
                acquisition(1),
                source(),
                vec![first, relabeled]
            ),
            Err(OutputTopologyError::DuplicateRoute)
        );
    }

    #[test]
    fn exact_utf16_preserves_unpaired_surrogates_and_rejects_nul_ambiguity() {
        let exact = Utf16Text::new(vec![0xd800, 0xdc00, 0xdfff]).unwrap();
        assert_eq!(exact.units(), &[0xd800, 0xdc00, 0xdfff]);
        assert_eq!(
            Utf16Text::new(vec![b'A' as u16, 0, b'B' as u16]),
            Err(OutputTopologyError::EmbeddedUtf16Nul {
                field: "UTF-16 text",
                index: 1
            })
        );
    }

    #[test]
    fn unknown_raw_flags_and_enum_words_round_trip() {
        let observation = available(vec![route(
            3,
            RawRational {
                numerator: u32::MAX,
                denominator: 0,
            },
        )]);
        let decoded = OutputTopologyObservation::decode_canonical(
            &observation.try_encode_canonical().unwrap(),
        )
        .unwrap();
        let route = &decoded.available().unwrap().routes[0];
        assert_eq!(route.output_technology_raw, 0xffff_ff02);
        assert_eq!(route.rotation_raw, 0xffff_0002);
        assert_eq!(route.refresh_rate.numerator, u32::MAX);
        assert_eq!(route.refresh_rate.denominator, 0);
    }

    #[test]
    fn malformed_lengths_and_boolean_markers_fail_closed() {
        let observation = available(vec![route(
            1,
            RawRational {
                numerator: 60,
                denominator: 1,
            },
        )]);
        let bytes = observation.try_encode_canonical().unwrap();
        assert!(matches!(
            OutputTopologyObservation::decode_canonical(&bytes[..bytes.len() - 1]),
            Err(OutputTopologyError::UnexpectedEnd { .. })
                | Err(OutputTopologyError::RouteLengthTooLarge { .. })
        ));

        let route_fixture = route(
            1,
            RawRational {
                numerator: 60,
                denominator: 1,
            },
        );
        let target_available_offset = 1
            + acquisition(1).encoded_len().unwrap()
            + source().encoded_len().unwrap()
            + 2
            + 4
            + route_fixture.identity.encoded_len().unwrap()
            + 4;
        let mut marker = bytes;
        marker[target_available_offset] = 2;
        assert!(matches!(
            OutputTopologyObservation::decode_canonical(&marker),
            Err(OutputTopologyError::UnknownMarker {
                field: "target availability",
                marker: 2
            })
        ));

        let mut oversized_name = observation.try_encode_canonical().unwrap();
        let source_name_offset = 1 + acquisition(1).encoded_len().unwrap();
        oversized_name[source_name_offset..source_name_offset + 2]
            .copy_from_slice(&u16::MAX.to_be_bytes());
        assert!(matches!(
            OutputTopologyObservation::decode_canonical(&oversized_name),
            Err(OutputTopologyError::Utf16TooLong { .. })
        ));
    }

    #[test]
    fn transient_join_ids_have_no_canonical_lane() {
        fn collect_with_transient_ids(
            _hmonitor: usize,
            _adapter_luid: u64,
            _source_id: u32,
            _target_id: u32,
            _source_mode_index: u32,
            _target_mode_index: u32,
        ) -> OutputTopologyObservation {
            available(vec![route(
                1,
                RawRational {
                    numerator: 60,
                    denominator: 1,
                },
            )])
        }

        let first = collect_with_transient_ids(1, 2, 3, 4, 5, 6);
        let second = collect_with_transient_ids(usize::MAX, u64::MAX, 97, 98, 99, 100);
        assert_eq!(first.tag(), second.tag());
        assert_eq!(
            first.try_encode_canonical().unwrap(),
            second.try_encode_canonical().unwrap()
        );
    }

    #[test]
    fn normalized_rationals_only_control_comparison_raw_values_still_encode() {
        let sixty = route(
            1,
            RawRational {
                numerator: 60,
                denominator: 1,
            },
        );
        let doubled = route(
            1,
            RawRational {
                numerator: 120,
                denominator: 2,
            },
        );
        assert_ne!(sixty, doubled);
        let observation = available(vec![doubled.clone(), sixty.clone()]);
        assert_eq!(observation.available().unwrap().routes.len(), 2);
        assert_eq!(observation.available().unwrap().routes[0], sixty);
        assert_eq!(observation.available().unwrap().routes[1], doubled);
    }

    #[test]
    fn stable_comparison_excludes_acquisition_and_friendly_annotation() {
        let left = available(vec![route(
            1,
            RawRational {
                numerator: 60,
                denominator: 1,
            },
        )]);
        let mut changed_route = route(
            1,
            RawRational {
                numerator: 60,
                denominator: 1,
            },
        );
        if let TargetIdentityObservation::Available {
            friendly_annotation,
            ..
        } = &mut changed_route.identity
        {
            *friendly_annotation = Some(text(&[b'N' as u16, b'e' as u16, b'w' as u16]));
        }
        let right = OutputTopologyObservation::new_available(
            acquisition(99),
            source(),
            vec![changed_route],
        )
        .unwrap();
        assert_ne!(left.tag(), right.tag());
        assert!(left.stable_topology_eq(&right));
    }

    #[test]
    fn comparison_critical_path_flag_changes_stable_topology() {
        let left = available(vec![route(
            1,
            RawRational {
                numerator: 60,
                denominator: 1,
            },
        )]);
        let mut changed = route(
            1,
            RawRational {
                numerator: 60,
                denominator: 1,
            },
        );
        changed.path_flags_raw ^= 1;
        let right = available(vec![changed]);
        assert_ne!(left.tag(), right.tag());
        assert!(!left.stable_topology_eq(&right));
    }

    #[test]
    fn target_identity_failure_is_explicit_and_nonfatal() {
        let mut target = route(
            1,
            RawRational {
                numerator: 60,
                denominator: 1,
            },
        );
        target.identity = TargetIdentityObservation::Unavailable {
            reason_code_raw: 1,
            error_code: 1168,
        };
        let observation = available(vec![target]);
        assert!(matches!(
            observation.available().unwrap().routes[0].identity,
            TargetIdentityObservation::Unavailable {
                reason_code_raw: 1,
                error_code: 1168
            }
        ));
        OutputTopologyObservation::decode_canonical(&observation.try_encode_canonical().unwrap())
            .unwrap();
    }

    #[test]
    fn route_and_attempt_bounds_are_checked_before_allocation() {
        let mut too_many_attempts = acquisition(1);
        too_many_attempts.attempts =
            vec![AcquisitionAttempt::default(); MAX_OUTPUT_TOPOLOGY_ATTEMPTS + 1];
        assert!(matches!(
            OutputTopologyObservation::new_unavailable(too_many_attempts, 1, 1),
            Err(OutputTopologyError::TooManyAttempts { .. })
        ));
        let too_many_routes = vec![
            route(
                1,
                RawRational {
                    numerator: 60,
                    denominator: 1
                }
            );
            MAX_OUTPUT_TOPOLOGY_ROUTES + 1
        ];
        assert!(matches!(
            OutputTopologyObservation::new_available(acquisition(1), source(), too_many_routes),
            Err(OutputTopologyError::TooManyRoutes { .. })
        ));
        assert!(matches!(
            OutputTopologyObservation::decode_canonical(&vec![
                0;
                MAX_OUTPUT_TOPOLOGY_PAYLOAD_BYTES + 1
            ]),
            Err(OutputTopologyError::WrongPayloadLength { .. })
        ));
    }
}
