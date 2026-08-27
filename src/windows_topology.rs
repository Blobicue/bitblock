//! Bounded Windows active-output topology collection.
//!
//! Handles, adapter LUIDs, source/target identifiers, clone-group identifiers, path positions,
//! and mode-table indices remain collector-local join keys. None enter the canonical observation.

use std::fmt;
use std::mem::size_of;
use std::ptr;

use crate::output_color::{
    AdvancedColorRouteFacts, OutputColorObservation, OutputColorObservationError,
};
use crate::output_topology::{
    AcquisitionAttempt, AcquisitionFacts, DesktopImageFacts, EdidIdentity,
    MAX_OUTPUT_TOPOLOGY_ROUTES, OutputTopologyError, OutputTopologyObservation, PixelSize,
    RawRational, RectI32, SourceFacts, SourceModeFacts, TargetIdentityObservation,
    TargetRouteFacts, TargetSignalTiming, Utf16Text,
};
use windows_sys::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
    DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO,
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_MODE_INFO_TYPE_DESKTOP_IMAGE,
    DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE, DISPLAYCONFIG_MODE_INFO_TYPE_TARGET,
    DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME,
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS,
    QDC_VIRTUAL_MODE_AWARE, QDC_VIRTUAL_REFRESH_RATE_AWARE, QueryDisplayConfig,
};
use windows_sys::Win32::Foundation::{
    ERROR_INSUFFICIENT_BUFFER, ERROR_INVALID_PARAMETER, ERROR_SUCCESS, GetLastError, HWND, LUID,
    POINT, RECT, SetLastError,
};
use windows_sys::Win32::Graphics::Gdi::{
    ClientToScreen, DISPLAYCONFIG_MAXPATH, DISPLAYCONFIG_PATH_ACTIVE,
    DISPLAYCONFIG_PATH_DESKTOP_IMAGE_IDX_INVALID, DISPLAYCONFIG_PATH_MODE_IDX_INVALID,
    DISPLAYCONFIG_PATH_SOURCE_MODE_IDX_INVALID, DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE,
    DISPLAYCONFIG_PATH_TARGET_MODE_IDX_INVALID, GetMonitorInfoW, HMONITOR,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MONITORINFOEXW, MonitorFromWindow,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

pub const PRIMARY_QUERY_FLAGS: u32 =
    QDC_ONLY_ACTIVE_PATHS | QDC_VIRTUAL_MODE_AWARE | QDC_VIRTUAL_REFRESH_RATE_AWARE;
pub const FALLBACK_QUERY_FLAGS: u32 = QDC_ONLY_ACTIVE_PATHS | QDC_VIRTUAL_MODE_AWARE;
pub const MAX_SIZE_QUERY_CYCLES: usize = 4;
pub const MAX_QUERY_PATHS: u32 = DISPLAYCONFIG_MAXPATH;
pub const MAX_QUERY_MODES: u32 = MAX_QUERY_PATHS * 3;

const PROFILE_VRR: u32 = u32::from_be_bytes(*b"VRR1");
const PROFILE_VMODE: u32 = u32::from_be_bytes(*b"VMO1");
const OP_MONITOR: u32 = u32::from_be_bytes(*b"MONI");
const OP_MONITOR_INFO: u32 = u32::from_be_bytes(*b"MIFO");
const OP_CLIENT_RECT: u32 = u32::from_be_bytes(*b"CLIR");
const OP_SIZE: u32 = u32::from_be_bytes(*b"SIZE");
const OP_QUERY: u32 = u32::from_be_bytes(*b"QURY");
const OP_SOURCE_NAMES: u32 = u32::from_be_bytes(*b"SNAM");
const OP_TARGET_NAMES: u32 = u32::from_be_bytes(*b"TNAM");
const OP_VALIDATE: u32 = u32::from_be_bytes(*b"VALI");
const PASS: u32 = u32::from_be_bytes(*b"PASS");
const FAIL: u32 = u32::from_be_bytes(*b"FAIL");
const RETRY: u32 = u32::from_be_bytes(*b"RETR");
const FALLBACK: u32 = u32::from_be_bytes(*b"FALL");
const CAP_REFUSAL: u32 = u32::from_be_bytes(*b"CAP!");
const PARTIAL: u32 = u32::from_be_bytes(*b"PART");

const R_MONITOR: u32 = u32::from_be_bytes(*b"MON0");
const R_MONITOR_INFO: u32 = u32::from_be_bytes(*b"MINF");
const R_CLIENT_RECT: u32 = u32::from_be_bytes(*b"CREC");
const R_SIZE: u32 = u32::from_be_bytes(*b"SIZF");
const R_QUERY: u32 = u32::from_be_bytes(*b"QRYF");
const R_QUERY_CAP: u32 = u32::from_be_bytes(*b"QCAP");
const R_RETRY_EXHAUSTED: u32 = u32::from_be_bytes(*b"RETX");
const R_QUERY_SHAPE: u32 = u32::from_be_bytes(*b"QSHP");
const R_SOURCE_CALL: u32 = u32::from_be_bytes(*b"SCAL");
const R_SOURCE_STRING: u32 = u32::from_be_bytes(*b"SSTR");
const R_SOURCE_MISSING: u32 = u32::from_be_bytes(*b"SNF0");
const R_SOURCE_AMBIGUOUS: u32 = u32::from_be_bytes(*b"SAMB");
const R_MODE_REF: u32 = u32::from_be_bytes(*b"MREF");
const R_MODE_KIND: u32 = u32::from_be_bytes(*b"MKND");
const R_MODE_OWNER: u32 = u32::from_be_bytes(*b"MOWN");
const R_SOURCE_CONFLICT: u32 = u32::from_be_bytes(*b"SCON");
const R_PATH_SHAPE: u32 = u32::from_be_bytes(*b"PSHP");
const R_ROUTE_CAP: u32 = u32::from_be_bytes(*b"RCAP");
const I_CALL: u32 = u32::from_be_bytes(*b"TCAL");
const I_STRING: u32 = u32::from_be_bytes(*b"TSTR");
const I_PATH: u32 = u32::from_be_bytes(*b"TPTH");
const C_TOPOLOGY_UNAVAILABLE: u32 = u32::from_be_bytes(*b"CTOP");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AdapterKey {
    low: u32,
    high: i32,
}

impl AdapterKey {
    fn from_luid(value: LUID) -> Self {
        Self {
            low: value.LowPart,
            high: value.HighPart,
        }
    }

    fn into_luid(self) -> LUID {
        LUID {
            LowPart: self.low,
            HighPart: self.high,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceKey {
    adapter: AdapterKey,
    id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TargetKey {
    adapter: AdapterKey,
    id: u32,
}

/// Collector-local raw result of one Windows Advanced Color query.
///
/// No target ID or decoded mode survives into the canonical output-color observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RawAdvancedColorInfo {
    flags_raw: u32,
    color_encoding_raw: u32,
    bits_per_color_channel: u32,
}

#[derive(Clone, Copy, Debug)]
struct RawMonitorInfo {
    monitor_rect: RectI32,
    work_rect: RectI32,
    flags_raw: u32,
    device_name: [u16; 32],
}

#[derive(Clone, Copy, Debug)]
struct SizeCall {
    error_code: u32,
    path_count: u32,
    mode_count: u32,
}

#[derive(Clone, Copy, Debug)]
struct QueryCall {
    error_code: u32,
    path_count: u32,
    mode_count: u32,
}

trait DisplayTopologyApi {
    fn monitor_from_window(&mut self, window: usize) -> Result<usize, u32>;
    fn monitor_info(&mut self, monitor: usize) -> Result<RawMonitorInfo, u32>;
    fn client_screen_rect(&mut self, window: usize) -> Result<RectI32, u32>;
    fn buffer_sizes(&mut self, flags: u32) -> SizeCall;
    fn query(
        &mut self,
        flags: u32,
        paths: &mut [DISPLAYCONFIG_PATH_INFO],
        modes: &mut [DISPLAYCONFIG_MODE_INFO],
    ) -> QueryCall;
    fn source_name(&mut self, key: SourceKey) -> Result<[u16; 32], u32>;
    fn target_name(&mut self, key: TargetKey) -> Result<DISPLAYCONFIG_TARGET_DEVICE_NAME, u32>;
    fn advanced_color(&mut self, key: TargetKey) -> Result<RawAdvancedColorInfo, u32>;
}

struct WinApi;

impl DisplayTopologyApi for WinApi {
    fn monitor_from_window(&mut self, window: usize) -> Result<usize, u32> {
        // SAFETY: the public wrapper requires a live HWND for this synchronous call.
        unsafe { SetLastError(ERROR_SUCCESS) };
        // SAFETY: same contract; the API does not retain the HWND.
        let monitor = unsafe { MonitorFromWindow(window as HWND, MONITOR_DEFAULTTONEAREST) };
        if monitor.is_null() {
            // SAFETY: reads this thread's error value after the failed call.
            Err(unsafe { GetLastError() })
        } else {
            Ok(monitor as usize)
        }
    }

    fn monitor_info(&mut self, monitor: usize) -> Result<RawMonitorInfo, u32> {
        let mut native = MONITORINFOEXW::default();
        native.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        // SAFETY: `native` begins with MONITORINFO and stays live for the call.
        unsafe { SetLastError(ERROR_SUCCESS) };
        // SAFETY: monitor was returned by MonitorFromWindow in this observation.
        let ok = unsafe {
            GetMonitorInfoW(
                monitor as HMONITOR,
                (&mut native as *mut MONITORINFOEXW).cast::<MONITORINFO>(),
            )
        };
        if ok == 0 {
            // SAFETY: reads this thread's error value after the failed call.
            return Err(unsafe { GetLastError() });
        }
        Ok(RawMonitorInfo {
            monitor_rect: rect(native.monitorInfo.rcMonitor),
            work_rect: rect(native.monitorInfo.rcWork),
            flags_raw: native.monitorInfo.dwFlags,
            device_name: native.szDevice,
        })
    }

    fn client_screen_rect(&mut self, window: usize) -> Result<RectI32, u32> {
        let mut client = RECT::default();
        // SAFETY: HWND contract and writable synchronous output.
        unsafe { SetLastError(ERROR_SUCCESS) };
        // SAFETY: the call does not retain either argument.
        if unsafe { GetClientRect(window as HWND, &mut client) } == 0 {
            // SAFETY: reads this thread's error value after failure.
            return Err(unsafe { GetLastError() });
        }
        let Some(width) = client
            .right
            .checked_sub(client.left)
            .filter(|value| *value > 0)
        else {
            return Err(ERROR_SUCCESS);
        };
        let Some(height) = client
            .bottom
            .checked_sub(client.top)
            .filter(|value| *value > 0)
        else {
            return Err(ERROR_SUCCESS);
        };
        let mut origin = POINT {
            x: client.left,
            y: client.top,
        };
        // SAFETY: HWND contract and writable synchronous output.
        unsafe { SetLastError(ERROR_SUCCESS) };
        // SAFETY: the call does not retain either argument.
        if unsafe { ClientToScreen(window as HWND, &mut origin) } == 0 {
            // SAFETY: reads this thread's error value after failure.
            return Err(unsafe { GetLastError() });
        }
        let Some(right) = origin.x.checked_add(width) else {
            return Err(ERROR_SUCCESS);
        };
        let Some(bottom) = origin.y.checked_add(height) else {
            return Err(ERROR_SUCCESS);
        };
        Ok(RectI32 {
            left: origin.x,
            top: origin.y,
            right,
            bottom,
        })
    }

    fn buffer_sizes(&mut self, flags: u32) -> SizeCall {
        let mut path_count = 0;
        let mut mode_count = 0;
        // SAFETY: both outputs are valid for this synchronous call.
        let error_code =
            unsafe { GetDisplayConfigBufferSizes(flags, &mut path_count, &mut mode_count) };
        SizeCall {
            error_code,
            path_count,
            mode_count,
        }
    }

    fn query(
        &mut self,
        flags: u32,
        paths: &mut [DISPLAYCONFIG_PATH_INFO],
        modes: &mut [DISPLAYCONFIG_MODE_INFO],
    ) -> QueryCall {
        let mut path_count = paths.len() as u32;
        let mut mode_count = modes.len() as u32;
        // SAFETY: bounded nonempty slices are writable for their declared counts. Active-path
        // queries require a null topology-ID pointer.
        let error_code = unsafe {
            QueryDisplayConfig(
                flags,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                ptr::null_mut(),
            )
        };
        QueryCall {
            error_code,
            path_count,
            mode_count,
        }
    }

    fn source_name(&mut self, key: SourceKey) -> Result<[u16; 32], u32> {
        let mut packet = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
        packet.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
        packet.header.size = size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
        packet.header.adapterId = key.adapter.into_luid();
        packet.header.id = key.id;
        // SAFETY: header describes the complete writable packet following it.
        let result = unsafe { DisplayConfigGetDeviceInfo(&mut packet.header) };
        if result == ERROR_SUCCESS as i32 {
            Ok(packet.viewGdiDeviceName)
        } else {
            Err(result as u32)
        }
    }

    fn target_name(&mut self, key: TargetKey) -> Result<DISPLAYCONFIG_TARGET_DEVICE_NAME, u32> {
        let mut packet = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
        packet.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
        packet.header.size = size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
        packet.header.adapterId = key.adapter.into_luid();
        packet.header.id = key.id;
        // SAFETY: header describes the complete writable packet following it.
        let result = unsafe { DisplayConfigGetDeviceInfo(&mut packet.header) };
        if result == ERROR_SUCCESS as i32 {
            Ok(packet)
        } else {
            Err(result as u32)
        }
    }

    fn advanced_color(&mut self, key: TargetKey) -> Result<RawAdvancedColorInfo, u32> {
        let mut packet = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO::default();
        packet.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO;
        packet.header.size = size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>() as u32;
        packet.header.adapterId = key.adapter.into_luid();
        packet.header.id = key.id;
        // SAFETY: header describes the complete writable packet following it.
        let result = unsafe { DisplayConfigGetDeviceInfo(&mut packet.header) };
        if result == ERROR_SUCCESS as i32 {
            // SAFETY: the successful documented request initializes this raw 32-bit union lane.
            let flags_raw = unsafe { packet.Anonymous.value };
            Ok(RawAdvancedColorInfo {
                flags_raw,
                color_encoding_raw: packet.colorEncoding as u32,
                bits_per_color_channel: packet.bitsPerColorChannel,
            })
        } else {
            Err(result as u32)
        }
    }
}

/// Paired active-output configuration reports from one shared topology collection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveOutputWithColor {
    topology: OutputTopologyObservation,
    color: OutputColorObservation,
}

impl ActiveOutputWithColor {
    /// Builds one verified paired observation from already-bound topology and color objects.
    ///
    /// This does not perform a Windows call; it is useful when a caller already owns the exact
    /// pair and needs the same validation boundary as the live collector.
    pub fn new(
        topology: OutputTopologyObservation,
        color: OutputColorObservation,
    ) -> Result<Self, ActiveOutputObservationError> {
        let observation = Self { topology, color };
        observation.verify()?;
        Ok(observation)
    }

    pub const fn topology(&self) -> &OutputTopologyObservation {
        &self.topology
    }

    pub const fn color(&self) -> &OutputColorObservation {
        &self.color
    }

    pub fn verify(&self) -> Result<(), ActiveOutputObservationError> {
        self.topology
            .verify()
            .map_err(ActiveOutputObservationError::Topology)?;
        self.color
            .verify_against(&self.topology)
            .map_err(ActiveOutputObservationError::Color)
    }
}

#[derive(Debug)]
pub enum ActiveOutputObservationError {
    Topology(OutputTopologyError),
    Color(OutputColorObservationError),
}

impl fmt::Display for ActiveOutputObservationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Topology(source) => write!(formatter, "active output topology failed: {source}"),
            Self::Color(source) => write!(formatter, "active output color failed: {source}"),
        }
    }
}

impl std::error::Error for ActiveOutputObservationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Topology(source) => Some(source),
            Self::Color(source) => Some(source),
        }
    }
}

struct CollectedActiveOutput {
    topology: OutputTopologyObservation,
    target_keys: Option<Vec<TargetKey>>,
}

/// Collects the active output selected by `window` without canonicalizing transient join keys.
///
/// API failures produce typed-unavailable observations. `Err` is reserved for failure to build
/// the bounded canonical object itself.
///
/// # Safety
///
/// `window` must be a live HWND owned by the calling thread for this synchronous observation.
pub unsafe fn observe_active_output(
    window: HWND,
) -> Result<OutputTopologyObservation, OutputTopologyError> {
    let mut api = WinApi;
    observe_active_output_with(&mut api, window as usize)
}

/// Collects one active topology and its raw per-route Advanced Color configuration together.
///
/// The topology and color reports share one QueryDisplayConfig collection. Per-target Advanced
/// Color failures become explicit unavailable route facts; they do not erase an otherwise usable
/// topology observation.
///
/// # Safety
///
/// `window` must be a live HWND owned by the calling thread for this synchronous observation.
pub unsafe fn observe_active_output_with_color(
    window: HWND,
) -> Result<ActiveOutputWithColor, ActiveOutputObservationError> {
    let mut api = WinApi;
    observe_active_output_with_color_with(&mut api, window as usize)
}

fn observe_active_output_with<A: DisplayTopologyApi>(
    api: &mut A,
    window: usize,
) -> Result<OutputTopologyObservation, OutputTopologyError> {
    Ok(collect_active_output_with(api, window)?.topology)
}

fn observe_active_output_with_color_with<A: DisplayTopologyApi>(
    api: &mut A,
    window: usize,
) -> Result<ActiveOutputWithColor, ActiveOutputObservationError> {
    let CollectedActiveOutput {
        topology,
        target_keys,
    } = collect_active_output_with(api, window).map_err(ActiveOutputObservationError::Topology)?;
    let color = match target_keys {
        Some(target_keys) => {
            let mut routes = Vec::new();
            routes.try_reserve_exact(target_keys.len()).map_err(|_| {
                ActiveOutputObservationError::Color(OutputColorObservationError::AllocationFailed {
                    requested: target_keys.len(),
                })
            })?;
            for key in target_keys {
                routes.push(match api.advanced_color(key) {
                    Ok(raw) => AdvancedColorRouteFacts::Available {
                        flags_raw: raw.flags_raw,
                        color_encoding_raw: raw.color_encoding_raw,
                        bits_per_color_channel: raw.bits_per_color_channel,
                    },
                    Err(error_code) => AdvancedColorRouteFacts::Unavailable { error_code },
                });
            }
            OutputColorObservation::new_available(&topology, routes)
                .map_err(ActiveOutputObservationError::Color)?
        }
        None => OutputColorObservation::new_unavailable(&topology, C_TOPOLOGY_UNAVAILABLE, 0)
            .map_err(ActiveOutputObservationError::Color)?,
    };
    ActiveOutputWithColor::new(topology, color)
}

fn collect_active_output_with<A: DisplayTopologyApi>(
    api: &mut A,
    window: usize,
) -> Result<CollectedActiveOutput, OutputTopologyError> {
    let mut attempts = Vec::new();
    let mut flags = PRIMARY_QUERY_FLAGS;

    let monitor = match api.monitor_from_window(window) {
        Ok(value) => {
            push_attempt(&mut attempts, flags, OP_MONITOR, PASS, 0, 0, 0);
            value
        }
        Err(error) => {
            push_attempt(&mut attempts, flags, OP_MONITOR, FAIL, error, 0, 0);
            return unavailable_collected(attempts, flags, R_MONITOR, error);
        }
    };
    let monitor_info = match api.monitor_info(monitor) {
        Ok(value) => {
            push_attempt(&mut attempts, flags, OP_MONITOR_INFO, PASS, 0, 0, 0);
            value
        }
        Err(error) => {
            push_attempt(&mut attempts, flags, OP_MONITOR_INFO, FAIL, error, 0, 0);
            return unavailable_collected(attempts, flags, R_MONITOR_INFO, error);
        }
    };
    let client_rect = match api.client_screen_rect(window) {
        Ok(value) if valid_rect(value) => {
            push_attempt(&mut attempts, flags, OP_CLIENT_RECT, PASS, 0, 0, 0);
            value
        }
        Ok(_) => {
            push_attempt(&mut attempts, flags, OP_CLIENT_RECT, FAIL, 0, 0, 0);
            return unavailable_collected(attempts, flags, R_CLIENT_RECT, 0);
        }
        Err(error) => {
            push_attempt(&mut attempts, flags, OP_CLIENT_RECT, FAIL, error, 0, 0);
            return unavailable_collected(attempts, flags, R_CLIENT_RECT, error);
        }
    };
    if !valid_rect(monitor_info.monitor_rect) || !valid_rect(monitor_info.work_rect) {
        push_attempt(&mut attempts, flags, OP_VALIDATE, FAIL, 0, 0, 0);
        return unavailable_collected(attempts, flags, R_MONITOR_INFO, 0);
    }
    let monitor_units = match fixed_utf16(&monitor_info.device_name) {
        Ok(units) if !units.is_empty() => units,
        _ => {
            push_attempt(&mut attempts, flags, OP_VALIDATE, FAIL, 0, 0, 0);
            return unavailable_collected(attempts, flags, R_MONITOR_INFO, 0);
        }
    };
    let monitor_name = Utf16Text::from_units(monitor_units)?;

    let mut queried = None;
    let mut fallback_used = false;
    let mut last_retry_error = ERROR_SUCCESS;
    for _ in 0..MAX_SIZE_QUERY_CYCLES {
        let sizes = api.buffer_sizes(flags);
        if sizes.error_code != ERROR_SUCCESS {
            let outcome = classify_failure(sizes.error_code, flags, fallback_used);
            push_attempt(
                &mut attempts,
                flags,
                OP_SIZE,
                outcome,
                sizes.error_code,
                sizes.path_count,
                sizes.mode_count,
            );
            if outcome == FALLBACK {
                fallback_used = true;
                flags = FALLBACK_QUERY_FLAGS;
                last_retry_error = sizes.error_code;
                continue;
            }
            if outcome == RETRY {
                last_retry_error = sizes.error_code;
                continue;
            }
            return unavailable_collected(attempts, flags, R_SIZE, sizes.error_code);
        }
        if sizes.path_count == 0
            || sizes.mode_count == 0
            || sizes.path_count > MAX_QUERY_PATHS
            || sizes.mode_count > MAX_QUERY_MODES
        {
            push_attempt(
                &mut attempts,
                flags,
                OP_SIZE,
                CAP_REFUSAL,
                0,
                sizes.path_count,
                sizes.mode_count,
            );
            let reason = if sizes.path_count > MAX_QUERY_PATHS || sizes.mode_count > MAX_QUERY_MODES
            {
                R_QUERY_CAP
            } else {
                R_QUERY_SHAPE
            };
            return unavailable_collected(attempts, flags, reason, 0);
        }
        push_attempt(
            &mut attempts,
            flags,
            OP_SIZE,
            PASS,
            0,
            sizes.path_count,
            sizes.mode_count,
        );

        let mut paths = bounded_default_vec::<DISPLAYCONFIG_PATH_INFO>(sizes.path_count as usize)?;
        let mut modes = bounded_default_vec::<DISPLAYCONFIG_MODE_INFO>(sizes.mode_count as usize)?;
        let query = api.query(flags, &mut paths, &mut modes);
        if query.error_code == ERROR_SUCCESS {
            let valid = query.path_count > 0
                && query.mode_count > 0
                && query.path_count <= sizes.path_count
                && query.mode_count <= sizes.mode_count
                && query.path_count <= MAX_QUERY_PATHS
                && query.mode_count <= MAX_QUERY_MODES;
            push_attempt(
                &mut attempts,
                flags,
                OP_QUERY,
                if valid { PASS } else { FAIL },
                0,
                query.path_count,
                query.mode_count,
            );
            if !valid {
                return unavailable_collected(attempts, flags, R_QUERY_SHAPE, 0);
            }
            paths.truncate(query.path_count as usize);
            modes.truncate(query.mode_count as usize);
            queried = Some((paths, modes));
            break;
        }
        let outcome = classify_failure(query.error_code, flags, fallback_used);
        push_attempt(
            &mut attempts,
            flags,
            OP_QUERY,
            outcome,
            query.error_code,
            query.path_count,
            query.mode_count,
        );
        if outcome == FALLBACK {
            fallback_used = true;
            flags = FALLBACK_QUERY_FLAGS;
            last_retry_error = query.error_code;
            continue;
        }
        if outcome == RETRY {
            last_retry_error = query.error_code;
            continue;
        }
        return unavailable_collected(attempts, flags, R_QUERY, query.error_code);
    }
    let Some((paths, modes)) = queried else {
        return unavailable_collected(attempts, flags, R_RETRY_EXHAUSTED, last_retry_error);
    };

    match build_available(
        api,
        &monitor_name,
        client_rect,
        monitor_info,
        &paths,
        &modes,
    ) {
        Ok(built) => {
            push_attempt(
                &mut attempts,
                flags,
                OP_SOURCE_NAMES,
                PASS,
                0,
                built.source_name_calls,
                0,
            );
            push_attempt(
                &mut attempts,
                flags,
                OP_TARGET_NAMES,
                if built.target_name_failures == 0 {
                    PASS
                } else {
                    PARTIAL
                },
                0,
                built.target_name_calls,
                built.target_name_failures,
            );
            push_attempt(
                &mut attempts,
                flags,
                OP_VALIDATE,
                PASS,
                0,
                paths.len() as u32,
                modes.len() as u32,
            );
            let topology = OutputTopologyObservation::new_available(
                acquisition(attempts, flags),
                built.source,
                built.routes,
            )?;
            Ok(CollectedActiveOutput {
                topology,
                target_keys: Some(built.target_keys),
            })
        }
        Err(CollectorError::Observed { reason, error }) => {
            push_attempt(
                &mut attempts,
                flags,
                OP_VALIDATE,
                FAIL,
                error,
                paths.len() as u32,
                modes.len() as u32,
            );
            unavailable_collected(attempts, flags, reason, error)
        }
        Err(CollectorError::Core(error)) => Err(error),
    }
}

fn classify_failure(error: u32, flags: u32, fallback_used: bool) -> u32 {
    if error == ERROR_INVALID_PARAMETER && flags == PRIMARY_QUERY_FLAGS && !fallback_used {
        FALLBACK
    } else if error == ERROR_INSUFFICIENT_BUFFER {
        RETRY
    } else {
        FAIL
    }
}

fn profile(flags: u32) -> u32 {
    if flags == PRIMARY_QUERY_FLAGS {
        PROFILE_VRR
    } else {
        PROFILE_VMODE
    }
}

fn acquisition(attempts: Vec<AcquisitionAttempt>, flags: u32) -> AcquisitionFacts {
    AcquisitionFacts {
        profile_code_raw: profile(flags),
        request_flags_raw: flags,
        attempts,
    }
}

fn unavailable(
    attempts: Vec<AcquisitionAttempt>,
    flags: u32,
    reason: u32,
    error: u32,
) -> Result<OutputTopologyObservation, OutputTopologyError> {
    OutputTopologyObservation::new_unavailable(acquisition(attempts, flags), reason, error)
}

fn unavailable_collected(
    attempts: Vec<AcquisitionAttempt>,
    flags: u32,
    reason: u32,
    error: u32,
) -> Result<CollectedActiveOutput, OutputTopologyError> {
    unavailable(attempts, flags, reason, error).map(|topology| CollectedActiveOutput {
        topology,
        target_keys: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn push_attempt(
    attempts: &mut Vec<AcquisitionAttempt>,
    flags: u32,
    operation: u32,
    outcome: u32,
    error: u32,
    paths: u32,
    modes: u32,
) {
    attempts.push(AcquisitionAttempt {
        operation_code_raw: operation,
        profile_code_raw: profile(flags),
        request_flags_raw: flags,
        retry_ordinal: attempts.len() as u32,
        outcome_code_raw: outcome,
        error_code: error,
        reported_path_capacity: paths,
        reported_mode_capacity: modes,
    });
}

fn bounded_default_vec<T: Default + Clone>(count: usize) -> Result<Vec<T>, OutputTopologyError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| OutputTopologyError::AllocationFailed { requested: count })?;
    values.resize(count, T::default());
    Ok(values)
}

#[derive(Debug)]
enum CollectorError {
    Observed { reason: u32, error: u32 },
    Core(OutputTopologyError),
}

impl CollectorError {
    fn observed(reason: u32) -> Self {
        Self::Observed { reason, error: 0 }
    }

    fn api(reason: u32, error: u32) -> Self {
        Self::Observed { reason, error }
    }
}

impl From<OutputTopologyError> for CollectorError {
    fn from(value: OutputTopologyError) -> Self {
        Self::Core(value)
    }
}

struct BuiltTopology {
    source: SourceFacts,
    routes: Vec<TargetRouteFacts>,
    target_keys: Vec<TargetKey>,
    source_name_calls: u32,
    target_name_calls: u32,
    target_name_failures: u32,
}

struct CanonicalRouteRow {
    target_key: TargetKey,
    route: TargetRouteFacts,
}

fn build_available<A: DisplayTopologyApi>(
    api: &mut A,
    monitor_name: &Utf16Text,
    client_rect: RectI32,
    monitor: RawMonitorInfo,
    paths: &[DISPLAYCONFIG_PATH_INFO],
    modes: &[DISPLAYCONFIG_MODE_INFO],
) -> Result<BuiltTopology, CollectorError> {
    for path in paths {
        if path.flags & DISPLAYCONFIG_PATH_ACTIVE == 0
            || !matches!(path.targetInfo.targetAvailable, 0 | 1)
        {
            return Err(CollectorError::observed(R_PATH_SHAPE));
        }
    }

    let mut source_keys = Vec::new();
    source_keys.try_reserve_exact(paths.len()).map_err(|_| {
        OutputTopologyError::AllocationFailed {
            requested: paths.len(),
        }
    })?;
    for path in paths {
        let key = source_key(path);
        if !source_keys.contains(&key) {
            source_keys.push(key);
        }
    }

    let mut matching_source = None;
    for key in &source_keys {
        let raw_name = api
            .source_name(*key)
            .map_err(|error| CollectorError::api(R_SOURCE_CALL, error))?;
        let units =
            fixed_utf16(&raw_name).map_err(|_| CollectorError::observed(R_SOURCE_STRING))?;
        if units.is_empty() {
            return Err(CollectorError::observed(R_SOURCE_STRING));
        }
        if units == monitor_name.units() && matching_source.replace(*key).is_some() {
            return Err(CollectorError::observed(R_SOURCE_AMBIGUOUS));
        }
    }
    let Some(selected_source) = matching_source else {
        return Err(CollectorError::observed(R_SOURCE_MISSING));
    };
    let selected: Vec<_> = paths
        .iter()
        .filter(|path| source_key(path) == selected_source)
        .collect();
    if selected.is_empty() {
        return Err(CollectorError::observed(R_SOURCE_MISSING));
    }
    if selected.len() > MAX_OUTPUT_TOPOLOGY_ROUTES {
        return Err(CollectorError::observed(R_ROUTE_CAP));
    }

    let source_mode = resolve_source_mode(selected[0], modes)?;
    let source_status = selected[0].sourceInfo.statusFlags;
    for path in selected.iter().skip(1) {
        if resolve_source_mode(path, modes)? != source_mode
            || path.sourceInfo.statusFlags != source_status
        {
            return Err(CollectorError::observed(R_SOURCE_CONFLICT));
        }
    }

    let source = SourceFacts {
        gdi_device_name: monitor_name.clone(),
        client_rect,
        monitor_rect: monitor.monitor_rect,
        work_rect: monitor.work_rect,
        monitor_flags_raw: monitor.flags_raw,
        mode: source_mode,
        status_flags_raw: source_status,
    };
    let mut rows = Vec::new();
    rows.try_reserve_exact(selected.len())
        .map_err(|_| OutputTopologyError::AllocationFailed {
            requested: selected.len(),
        })?;
    let mut target_name_failures = 0_u32;
    for path in &selected {
        let target_mode = resolve_target_mode(path, modes)?;
        let signal = target_mode.targetVideoSignalInfo;
        // SAFETY: videoStandard spans the initialized 32-bit union and remains an uninterpreted
        // raw value; the alternate bitfield is never accessed.
        let video_standard_raw = unsafe { signal.Anonymous.videoStandard };
        let target_key = target_key(path);
        let identity = match api.target_name(target_key) {
            Ok(packet) => match target_identity(packet)? {
                IdentityDecode::Available(value) => value,
                IdentityDecode::Unavailable(reason) => {
                    target_name_failures += 1;
                    TargetIdentityObservation::Unavailable {
                        reason_code_raw: reason,
                        error_code: 0,
                    }
                }
            },
            Err(error) => {
                target_name_failures += 1;
                TargetIdentityObservation::Unavailable {
                    reason_code_raw: I_CALL,
                    error_code: error,
                }
            }
        };
        rows.push(CanonicalRouteRow {
            target_key,
            route: TargetRouteFacts {
                identity,
                output_technology_raw: path.targetInfo.outputTechnology as u32,
                target_available: path.targetInfo.targetAvailable == 1,
                status_flags_raw: path.targetInfo.statusFlags,
                rotation_raw: path.targetInfo.rotation as u32,
                scaling_raw: path.targetInfo.scaling as u32,
                refresh_rate: rational(path.targetInfo.refreshRate),
                path_flags_raw: path.flags,
                signal: TargetSignalTiming {
                    pixel_rate: signal.pixelRate,
                    h_sync: rational(signal.hSyncFreq),
                    v_sync: rational(signal.vSyncFreq),
                    active_size: PixelSize {
                        width: signal.activeSize.cx,
                        height: signal.activeSize.cy,
                    },
                    total_size: PixelSize {
                        width: signal.totalSize.cx,
                        height: signal.totalSize.cy,
                    },
                    video_standard_raw,
                    scanline_ordering_raw: signal.scanLineOrdering as u32,
                },
                desktop_image: resolve_desktop_image(path, modes)?,
            },
        });
    }
    rows.sort_by(|left, right| left.route.canonical_cmp(&right.route));
    for index in 0..rows.len() {
        if rows[index + 1..]
            .iter()
            .any(|other| rows[index].route.stable_cmp(&other.route).is_eq())
        {
            // Two unnamed/materially identical targets cannot be distinguished without reviving a
            // transient numeric ID. Friendly annotation is deliberately excluded: it is not a
            // route identity. Refuse the observation rather than collapse the clone set.
            return Err(CollectorError::observed(R_PATH_SHAPE));
        }
    }
    let mut routes = Vec::new();
    routes
        .try_reserve_exact(rows.len())
        .map_err(|_| OutputTopologyError::AllocationFailed {
            requested: rows.len(),
        })?;
    let mut target_keys = Vec::new();
    target_keys.try_reserve_exact(rows.len()).map_err(|_| {
        OutputTopologyError::AllocationFailed {
            requested: rows.len(),
        }
    })?;
    for row in rows {
        routes.push(row.route);
        target_keys.push(row.target_key);
    }

    Ok(BuiltTopology {
        source,
        routes,
        target_keys,
        source_name_calls: source_keys.len() as u32,
        target_name_calls: selected.len() as u32,
        target_name_failures,
    })
}

fn source_key(path: &DISPLAYCONFIG_PATH_INFO) -> SourceKey {
    SourceKey {
        adapter: AdapterKey::from_luid(path.sourceInfo.adapterId),
        id: path.sourceInfo.id,
    }
}

fn target_key(path: &DISPLAYCONFIG_PATH_INFO) -> TargetKey {
    TargetKey {
        adapter: AdapterKey::from_luid(path.targetInfo.adapterId),
        id: path.targetInfo.id,
    }
}

fn resolve_source_mode(
    path: &DISPLAYCONFIG_PATH_INFO,
    modes: &[DISPLAYCONFIG_MODE_INFO],
) -> Result<SourceModeFacts, CollectorError> {
    // SAFETY: QueryDisplayConfig initializes the complete selector union. The path flag below
    // selects its documented layout before any index is interpreted.
    let word = unsafe { path.sourceInfo.Anonymous.modeInfoIdx };
    let index = if path.flags & DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE != 0 {
        let index = word >> 16;
        if index == DISPLAYCONFIG_PATH_SOURCE_MODE_IDX_INVALID {
            return Err(CollectorError::observed(R_MODE_REF));
        }
        index
    } else {
        if word == DISPLAYCONFIG_PATH_MODE_IDX_INVALID {
            return Err(CollectorError::observed(R_MODE_REF));
        }
        word
    };
    let mode = modes
        .get(index as usize)
        .ok_or_else(|| CollectorError::observed(R_MODE_REF))?;
    if mode.infoType != DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE {
        return Err(CollectorError::observed(R_MODE_KIND));
    }
    if AdapterKey::from_luid(mode.adapterId) != AdapterKey::from_luid(path.sourceInfo.adapterId)
        || mode.id != path.sourceInfo.id
    {
        return Err(CollectorError::observed(R_MODE_OWNER));
    }
    // SAFETY: the checked discriminator establishes the active union member.
    let source = unsafe { mode.Anonymous.sourceMode };
    if source.width == 0 || source.height == 0 {
        return Err(CollectorError::observed(R_MODE_REF));
    }
    Ok(SourceModeFacts {
        position_x: source.position.x,
        position_y: source.position.y,
        width: source.width,
        height: source.height,
        pixel_format_raw: source.pixelFormat as u32,
    })
}

fn resolve_target_mode(
    path: &DISPLAYCONFIG_PATH_INFO,
    modes: &[DISPLAYCONFIG_MODE_INFO],
) -> Result<windows_sys::Win32::Devices::Display::DISPLAYCONFIG_TARGET_MODE, CollectorError> {
    // SAFETY: see resolve_source_mode; this copies the initialized raw selector word.
    let word = unsafe { path.targetInfo.Anonymous.modeInfoIdx };
    let index = if path.flags & DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE != 0 {
        let index = word >> 16;
        if index == DISPLAYCONFIG_PATH_TARGET_MODE_IDX_INVALID {
            return Err(CollectorError::observed(R_MODE_REF));
        }
        index
    } else {
        if word == DISPLAYCONFIG_PATH_MODE_IDX_INVALID {
            return Err(CollectorError::observed(R_MODE_REF));
        }
        word
    };
    let mode = modes
        .get(index as usize)
        .ok_or_else(|| CollectorError::observed(R_MODE_REF))?;
    if mode.infoType != DISPLAYCONFIG_MODE_INFO_TYPE_TARGET {
        return Err(CollectorError::observed(R_MODE_KIND));
    }
    if AdapterKey::from_luid(mode.adapterId) != AdapterKey::from_luid(path.targetInfo.adapterId)
        || mode.id != path.targetInfo.id
    {
        return Err(CollectorError::observed(R_MODE_OWNER));
    }
    // SAFETY: the checked discriminator establishes the active union member.
    Ok(unsafe { mode.Anonymous.targetMode })
}

fn resolve_desktop_image(
    path: &DISPLAYCONFIG_PATH_INFO,
    modes: &[DISPLAYCONFIG_MODE_INFO],
) -> Result<Option<DesktopImageFacts>, CollectorError> {
    if path.flags & DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE == 0 {
        return Ok(None);
    }
    // SAFETY: QueryDisplayConfig initialized the complete selector union.
    let word = unsafe { path.targetInfo.Anonymous.modeInfoIdx };
    let index = word & 0xffff;
    if index == DISPLAYCONFIG_PATH_DESKTOP_IMAGE_IDX_INVALID {
        return Ok(None);
    }
    let mode = modes
        .get(index as usize)
        .ok_or_else(|| CollectorError::observed(R_MODE_REF))?;
    if mode.infoType != DISPLAYCONFIG_MODE_INFO_TYPE_DESKTOP_IMAGE {
        return Err(CollectorError::observed(R_MODE_KIND));
    }
    // Microsoft defines no desktop-image owner-ID invariant. The bounded index plus the checked
    // discriminator is the complete documented union guard.
    // SAFETY: discriminator establishes the active union member.
    let image = unsafe { mode.Anonymous.desktopImageInfo };
    let width =
        u32::try_from(image.PathSourceSize.x).map_err(|_| CollectorError::observed(R_MODE_REF))?;
    let height =
        u32::try_from(image.PathSourceSize.y).map_err(|_| CollectorError::observed(R_MODE_REF))?;
    Ok(Some(DesktopImageFacts {
        path_source_size: PixelSize { width, height },
        desktop_image_region: rectl(image.DesktopImageRegion),
        desktop_image_clip: rectl(image.DesktopImageClip),
    }))
}

enum IdentityDecode {
    Available(TargetIdentityObservation),
    Unavailable(u32),
}

fn target_identity(
    packet: DISPLAYCONFIG_TARGET_DEVICE_NAME,
) -> Result<IdentityDecode, CollectorError> {
    let path_units = match fixed_utf16(&packet.monitorDevicePath) {
        Ok(value) => value,
        Err(_) => return Ok(IdentityDecode::Unavailable(I_STRING)),
    };
    if path_units.is_empty() {
        return Ok(IdentityDecode::Unavailable(I_PATH));
    }
    let friendly_units = match fixed_utf16(&packet.monitorFriendlyDeviceName) {
        Ok(value) => value,
        Err(_) => return Ok(IdentityDecode::Unavailable(I_STRING)),
    };
    // SAFETY: value spans the complete initialized flags union and is retained uninterpreted.
    let name_flags_raw = unsafe { packet.flags.Anonymous.value };
    Ok(IdentityDecode::Available(
        TargetIdentityObservation::Available {
            device_path: Utf16Text::from_units(path_units)?,
            friendly_annotation: if friendly_units.is_empty() {
                None
            } else {
                Some(Utf16Text::from_units(friendly_units)?)
            },
            name_flags_raw,
            edid: (name_flags_raw & 0x4 != 0).then_some(EdidIdentity {
                manufacturer_id: packet.edidManufactureId,
                product_code: packet.edidProductCodeId,
            }),
            connector_instance: packet.connectorInstance,
            reported_output_technology_raw: packet.outputTechnology as u32,
        },
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FixedUtf16Error {
    MissingTerminator,
    NonzeroTail,
}

fn fixed_utf16(buffer: &[u16]) -> Result<&[u16], FixedUtf16Error> {
    let Some(boundary) = buffer.iter().position(|unit| *unit == 0) else {
        return Err(FixedUtf16Error::MissingTerminator);
    };
    if buffer[boundary..].iter().any(|unit| *unit != 0) {
        return Err(FixedUtf16Error::NonzeroTail);
    }
    Ok(&buffer[..boundary])
}

fn rational(value: windows_sys::Win32::Devices::Display::DISPLAYCONFIG_RATIONAL) -> RawRational {
    RawRational {
        numerator: value.Numerator,
        denominator: value.Denominator,
    }
}

fn rect(value: RECT) -> RectI32 {
    RectI32 {
        left: value.left,
        top: value.top,
        right: value.right,
        bottom: value.bottom,
    }
}

fn rectl(value: windows_sys::Win32::Foundation::RECTL) -> RectI32 {
    RectI32 {
        left: value.left,
        top: value.top,
        right: value.right,
        bottom: value.bottom,
    }
}

fn valid_rect(value: RectI32) -> bool {
    value.right > value.left && value.bottom > value.top
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use crate::output_color::{AdvancedColorRouteFacts, OutputColorFacts};
    use crate::output_topology::OutputTopologyFacts;
    use windows_sys::Win32::Devices::Display::{
        DISPLAYCONFIG_2DREGION, DISPLAYCONFIG_PIXELFORMAT_32BPP, DISPLAYCONFIG_RATIONAL,
        DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE, DISPLAYCONFIG_SOURCE_MODE,
        DISPLAYCONFIG_TARGET_MODE, DISPLAYCONFIG_VIDEO_SIGNAL_INFO,
    };
    use windows_sys::Win32::Foundation::POINTL;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Call {
        Monitor(usize),
        MonitorInfo(usize),
        ClientRect(usize),
        Size(u32),
        Query(u32, usize, usize),
        SourceName(SourceKey),
        TargetName(TargetKey),
        AdvancedColor(TargetKey),
    }

    struct FakeQuery {
        result: QueryCall,
        paths: Vec<DISPLAYCONFIG_PATH_INFO>,
        modes: Vec<DISPLAYCONFIG_MODE_INFO>,
    }

    struct FakeApi {
        monitor: Result<usize, u32>,
        monitor_info: Result<RawMonitorInfo, u32>,
        client_rect: Result<RectI32, u32>,
        sizes: VecDeque<SizeCall>,
        queries: VecDeque<FakeQuery>,
        source_names: VecDeque<(SourceKey, Result<[u16; 32], u32>)>,
        target_names: VecDeque<(TargetKey, Result<DISPLAYCONFIG_TARGET_DEVICE_NAME, u32>)>,
        advanced_colors: VecDeque<(TargetKey, Result<RawAdvancedColorInfo, u32>)>,
        calls: Vec<Call>,
    }

    impl DisplayTopologyApi for FakeApi {
        fn monitor_from_window(&mut self, window: usize) -> Result<usize, u32> {
            self.calls.push(Call::Monitor(window));
            self.monitor
        }

        fn monitor_info(&mut self, monitor: usize) -> Result<RawMonitorInfo, u32> {
            self.calls.push(Call::MonitorInfo(monitor));
            self.monitor_info
        }

        fn client_screen_rect(&mut self, window: usize) -> Result<RectI32, u32> {
            self.calls.push(Call::ClientRect(window));
            self.client_rect
        }

        fn buffer_sizes(&mut self, flags: u32) -> SizeCall {
            self.calls.push(Call::Size(flags));
            self.sizes.pop_front().expect("unexpected size call")
        }

        fn query(
            &mut self,
            flags: u32,
            paths: &mut [DISPLAYCONFIG_PATH_INFO],
            modes: &mut [DISPLAYCONFIG_MODE_INFO],
        ) -> QueryCall {
            self.calls
                .push(Call::Query(flags, paths.len(), modes.len()));
            let response = self.queries.pop_front().expect("unexpected query call");
            for (destination, source) in paths.iter_mut().zip(&response.paths) {
                *destination = *source;
            }
            for (destination, source) in modes.iter_mut().zip(&response.modes) {
                *destination = *source;
            }
            response.result
        }

        fn source_name(&mut self, key: SourceKey) -> Result<[u16; 32], u32> {
            self.calls.push(Call::SourceName(key));
            let (expected, result) = self
                .source_names
                .pop_front()
                .expect("unexpected source-name call");
            assert_eq!(key, expected);
            result
        }

        fn target_name(&mut self, key: TargetKey) -> Result<DISPLAYCONFIG_TARGET_DEVICE_NAME, u32> {
            self.calls.push(Call::TargetName(key));
            let (expected, result) = self
                .target_names
                .pop_front()
                .expect("unexpected target-name call");
            assert_eq!(key, expected);
            result
        }

        fn advanced_color(&mut self, key: TargetKey) -> Result<RawAdvancedColorInfo, u32> {
            self.calls.push(Call::AdvancedColor(key));
            let (expected, result) = self
                .advanced_colors
                .pop_front()
                .expect("unexpected advanced-color call");
            assert_eq!(key, expected);
            result
        }
    }

    fn fixed<const N: usize>(units: &[u16]) -> [u16; N] {
        assert!(units.len() < N);
        let mut output = [0; N];
        output[..units.len()].copy_from_slice(units);
        output
    }

    fn display_name() -> [u16; 32] {
        fixed(&"\\\\.\\DISPLAY1".encode_utf16().collect::<Vec<_>>())
    }

    fn target_packet(path: &str, friendly: &str) -> DISPLAYCONFIG_TARGET_DEVICE_NAME {
        let mut packet = DISPLAYCONFIG_TARGET_DEVICE_NAME {
            monitorDevicePath: fixed(&path.encode_utf16().collect::<Vec<_>>()),
            monitorFriendlyDeviceName: fixed(&friendly.encode_utf16().collect::<Vec<_>>()),
            edidManufactureId: 0x1234,
            edidProductCodeId: 0x5678,
            connectorInstance: 1,
            outputTechnology: 5,
            ..Default::default()
        };
        packet.flags.Anonymous.value = 0x5;
        packet
    }

    fn source_mode(adapter: AdapterKey, source_id: u32) -> DISPLAYCONFIG_MODE_INFO {
        let mut mode = DISPLAYCONFIG_MODE_INFO {
            infoType: DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE,
            id: source_id,
            adapterId: adapter.into_luid(),
            ..Default::default()
        };
        mode.Anonymous.sourceMode = DISPLAYCONFIG_SOURCE_MODE {
            width: 1920,
            height: 1080,
            pixelFormat: DISPLAYCONFIG_PIXELFORMAT_32BPP,
            position: POINTL { x: 0, y: 0 },
        };
        mode
    }

    fn target_mode(adapter: AdapterKey, target_id: u32) -> DISPLAYCONFIG_MODE_INFO {
        let mut signal = DISPLAYCONFIG_VIDEO_SIGNAL_INFO {
            pixelRate: 148_500_000,
            hSyncFreq: DISPLAYCONFIG_RATIONAL {
                Numerator: 67_500,
                Denominator: 1,
            },
            vSyncFreq: DISPLAYCONFIG_RATIONAL {
                Numerator: 60_000,
                Denominator: 1_001,
            },
            activeSize: DISPLAYCONFIG_2DREGION { cx: 1920, cy: 1080 },
            totalSize: DISPLAYCONFIG_2DREGION { cx: 2200, cy: 1125 },
            scanLineOrdering: DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE,
            ..Default::default()
        };
        signal.Anonymous.videoStandard = 7;
        let mut mode = DISPLAYCONFIG_MODE_INFO {
            infoType: DISPLAYCONFIG_MODE_INFO_TYPE_TARGET,
            id: target_id,
            adapterId: adapter.into_luid(),
            ..Default::default()
        };
        mode.Anonymous.targetMode = DISPLAYCONFIG_TARGET_MODE {
            targetVideoSignalInfo: signal,
        };
        mode
    }

    fn path(
        adapter: AdapterKey,
        source_id: u32,
        target_id: u32,
        source_index: u16,
        target_index: u16,
    ) -> DISPLAYCONFIG_PATH_INFO {
        let mut path = DISPLAYCONFIG_PATH_INFO {
            flags: DISPLAYCONFIG_PATH_ACTIVE | DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE,
            ..Default::default()
        };
        path.sourceInfo.adapterId = adapter.into_luid();
        path.sourceInfo.id = source_id;
        path.sourceInfo.statusFlags = 1;
        path.sourceInfo.Anonymous.modeInfoIdx = u32::from(source_index) << 16 | 7;
        path.targetInfo.adapterId = adapter.into_luid();
        path.targetInfo.id = target_id;
        path.targetInfo.Anonymous.modeInfoIdx =
            u32::from(target_index) << 16 | DISPLAYCONFIG_PATH_DESKTOP_IMAGE_IDX_INVALID;
        path.targetInfo.outputTechnology = 5;
        path.targetInfo.rotation = 1;
        path.targetInfo.scaling = 1;
        path.targetInfo.refreshRate = DISPLAYCONFIG_RATIONAL {
            Numerator: 60_000,
            Denominator: 1_001,
        };
        path.targetInfo.scanLineOrdering = DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE;
        path.targetInfo.targetAvailable = 1;
        path.targetInfo.statusFlags = 1;
        path
    }

    fn base_api(adapter: AdapterKey, source_id: u32, target_ids: &[u32]) -> FakeApi {
        let paths: Vec<_> = target_ids
            .iter()
            .enumerate()
            .map(|(offset, target)| path(adapter, source_id, *target, 0, (offset + 1) as u16))
            .collect();
        let mut modes = vec![source_mode(adapter, source_id)];
        modes.extend(
            target_ids
                .iter()
                .map(|target| target_mode(adapter, *target)),
        );
        let target_names = target_ids
            .iter()
            .enumerate()
            .map(|(offset, target)| {
                (
                    TargetKey {
                        adapter,
                        id: *target,
                    },
                    Ok(target_packet(
                        &format!("\\\\?\\DISPLAY#TARGET{offset}"),
                        &format!("Panel {offset}"),
                    )),
                )
            })
            .collect();
        let advanced_colors = target_ids
            .iter()
            .map(|target| {
                (
                    TargetKey {
                        adapter,
                        id: *target,
                    },
                    Ok(RawAdvancedColorInfo {
                        flags_raw: *target,
                        color_encoding_raw: target.wrapping_add(0x8000_0000),
                        bits_per_color_channel: 10,
                    }),
                )
            })
            .collect();
        FakeApi {
            monitor: Ok(44),
            monitor_info: Ok(RawMonitorInfo {
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
                flags_raw: 1,
                device_name: display_name(),
            }),
            client_rect: Ok(RectI32 {
                left: 100,
                top: 100,
                right: 901,
                bottom: 901,
            }),
            sizes: VecDeque::from([SizeCall {
                error_code: ERROR_SUCCESS,
                path_count: paths.len() as u32,
                mode_count: modes.len() as u32,
            }]),
            queries: VecDeque::from([FakeQuery {
                result: QueryCall {
                    error_code: ERROR_SUCCESS,
                    path_count: paths.len() as u32,
                    mode_count: modes.len() as u32,
                },
                paths,
                modes,
            }]),
            source_names: VecDeque::from([(
                SourceKey {
                    adapter,
                    id: source_id,
                },
                Ok(display_name()),
            )]),
            target_names,
            advanced_colors,
            calls: Vec::new(),
        }
    }

    fn available(
        observation: &OutputTopologyObservation,
    ) -> &crate::output_topology::AvailableTopologyFacts {
        observation
            .available()
            .expect("expected available topology")
    }

    #[test]
    fn primary_profile_retains_one_route_and_exact_call_order() {
        let adapter = AdapterKey { low: 1, high: 2 };
        let mut api = base_api(adapter, 3, &[4]);
        let observation = observe_active_output_with(&mut api, 99).unwrap();
        observation.verify().unwrap();
        assert_eq!(available(&observation).routes.len(), 1);
        assert_eq!(available(&observation).acquisition.request_flags_raw, 0x52);
        assert_eq!(
            api.calls,
            vec![
                Call::Monitor(99),
                Call::MonitorInfo(44),
                Call::ClientRect(99),
                Call::Size(PRIMARY_QUERY_FLAGS),
                Call::Query(PRIMARY_QUERY_FLAGS, 1, 2),
                Call::SourceName(SourceKey { adapter, id: 3 }),
                Call::TargetName(TargetKey { adapter, id: 4 }),
            ]
        );
    }

    #[test]
    fn paired_color_rows_follow_canonical_routes_not_query_order() {
        let adapter = AdapterKey { low: 10, high: -2 };
        let mut api = base_api(adapter, 20, &[30, 31]);
        api.queries[0].paths.reverse();
        // Target-name collection follows raw query order, but Advanced Color is deliberately
        // queried only after target rows have been sorted by the canonical route comparator.
        api.target_names.make_contiguous().reverse();

        let paired = observe_active_output_with_color_with(&mut api, 99).unwrap();
        paired.verify().unwrap();
        assert_eq!(paired.topology().available().unwrap().routes.len(), 2);
        assert_eq!(
            paired.color().routes(),
            Some(
                [
                    AdvancedColorRouteFacts::Available {
                        flags_raw: 30,
                        color_encoding_raw: 0x8000_001e,
                        bits_per_color_channel: 10,
                    },
                    AdvancedColorRouteFacts::Available {
                        flags_raw: 31,
                        color_encoding_raw: 0x8000_001f,
                        bits_per_color_channel: 10,
                    },
                ]
                .as_slice()
            )
        );
        let advanced_calls: Vec<_> = api
            .calls
            .iter()
            .filter_map(|call| match call {
                Call::AdvancedColor(key) => Some(*key),
                _ => None,
            })
            .collect();
        assert_eq!(
            advanced_calls,
            [TargetKey { adapter, id: 30 }, TargetKey { adapter, id: 31 },]
        );
    }

    #[test]
    fn paired_color_keeps_per_target_failure_and_high_raw_words() {
        let adapter = AdapterKey { low: 1, high: 2 };
        let mut api = base_api(adapter, 3, &[4, 5]);
        api.advanced_colors[0].1 = Ok(RawAdvancedColorInfo {
            flags_raw: 0x8000_000f,
            color_encoding_raw: 0xffff_fffe,
            bits_per_color_channel: u32::MAX,
        });
        api.advanced_colors[1].1 = Err(1168);

        let paired = observe_active_output_with_color_with(&mut api, 99).unwrap();
        paired.verify().unwrap();
        assert!(paired.topology().available().is_some());
        assert_eq!(
            paired.color().facts(),
            &OutputColorFacts::Available {
                routes: vec![
                    AdvancedColorRouteFacts::Available {
                        flags_raw: 0x8000_000f,
                        color_encoding_raw: 0xffff_fffe,
                        bits_per_color_channel: u32::MAX,
                    },
                    AdvancedColorRouteFacts::Unavailable { error_code: 1168 },
                ],
            }
        );
    }

    #[test]
    fn paired_unavailable_topology_skips_color_queries_and_retains_reason() {
        let adapter = AdapterKey { low: 1, high: 2 };
        let mut api = base_api(adapter, 3, &[4]);
        api.monitor = Err(6);

        let paired = observe_active_output_with_color_with(&mut api, 99).unwrap();
        paired.verify().unwrap();
        assert!(paired.topology().unavailable().is_some());
        assert_eq!(
            paired.color().facts(),
            &OutputColorFacts::Unavailable {
                reason_code_raw: C_TOPOLOGY_UNAVAILABLE,
                error_code: 0,
            }
        );
        assert!(
            !api.calls
                .iter()
                .any(|call| matches!(call, Call::AdvancedColor(_)))
        );
    }

    #[test]
    fn invalid_parameter_falls_back_once_to_0x12() {
        let adapter = AdapterKey { low: 1, high: 2 };
        let mut api = base_api(adapter, 3, &[4]);
        api.sizes.push_front(SizeCall {
            error_code: ERROR_INVALID_PARAMETER,
            path_count: 0,
            mode_count: 0,
        });
        let observation = observe_active_output_with(&mut api, 99).unwrap();
        assert_eq!(available(&observation).acquisition.request_flags_raw, 0x12);
        let size_flags: Vec<_> = api
            .calls
            .iter()
            .filter_map(|call| match call {
                Call::Size(flags) => Some(*flags),
                _ => None,
            })
            .collect();
        assert_eq!(size_flags, [0x52, 0x12]);
        assert!(api.calls.contains(&Call::Query(0x12, 1, 2)));
    }

    #[test]
    fn insufficient_buffer_repeats_size_query_but_other_errors_do_not() {
        let adapter = AdapterKey { low: 1, high: 2 };
        let mut api = base_api(adapter, 3, &[4]);
        let success = api.queries.pop_front().unwrap();
        api.queries.push_back(FakeQuery {
            result: QueryCall {
                error_code: ERROR_INSUFFICIENT_BUFFER,
                path_count: 1,
                mode_count: 2,
            },
            paths: Vec::new(),
            modes: Vec::new(),
        });
        api.queries.push_back(success);
        api.sizes.push_back(SizeCall {
            error_code: ERROR_SUCCESS,
            path_count: 1,
            mode_count: 2,
        });
        let observation = observe_active_output_with(&mut api, 99).unwrap();
        assert!(observation.available().is_some());
        assert_eq!(
            api.calls
                .iter()
                .filter(|call| matches!(call, Call::Query(..)))
                .count(),
            2
        );

        let mut terminal = base_api(adapter, 3, &[4]);
        terminal.sizes[0].error_code = 5;
        let observation = observe_active_output_with(&mut terminal, 99).unwrap();
        assert!(observation.unavailable().is_some());
        assert!(
            !terminal
                .calls
                .iter()
                .any(|call| matches!(call, Call::Query(..)))
        );
    }

    #[test]
    fn path_or_mode_cap_refuses_before_query_allocation() {
        let adapter = AdapterKey { low: 1, high: 2 };
        for (paths, modes) in [(1025, 1), (1, 3073)] {
            let mut api = base_api(adapter, 3, &[4]);
            api.sizes[0] = SizeCall {
                error_code: ERROR_SUCCESS,
                path_count: paths,
                mode_count: modes,
            };
            let observation = observe_active_output_with(&mut api, 99).unwrap();
            assert!(observation.unavailable().is_some());
            assert!(!api.calls.iter().any(|call| matches!(call, Call::Query(..))));
        }
    }

    #[test]
    fn clone_source_retains_every_target_route() {
        let adapter = AdapterKey { low: 10, high: -2 };
        let mut api = base_api(adapter, 20, &[30, 31]);
        let observation = observe_active_output_with(&mut api, 99).unwrap();
        assert_eq!(available(&observation).routes.len(), 2);
        assert_eq!(
            api.calls
                .iter()
                .filter(|call| matches!(call, Call::TargetName(_)))
                .count(),
            2
        );
    }

    #[test]
    fn friendly_annotations_cannot_distinguish_materially_duplicate_routes() {
        let adapter = AdapterKey { low: 10, high: -2 };
        let mut api = base_api(adapter, 20, &[30, 31]);
        api.target_names[0].1 = Ok(target_packet("\\\\?\\DISPLAY#SAME", "Panel A"));
        api.target_names[1].1 = Ok(target_packet("\\\\?\\DISPLAY#SAME", "Panel B"));

        let observation = observe_active_output_with(&mut api, 99).unwrap();
        assert_eq!(
            observation
                .unavailable()
                .expect("stable duplicate routes must refuse the available topology")
                .reason_code_raw,
            R_PATH_SHAPE
        );
    }

    #[test]
    fn transient_ids_and_mode_positions_do_not_change_observation() {
        let mut left = base_api(AdapterKey { low: 1, high: 2 }, 3, &[4, 5]);
        let mut right = base_api(
            AdapterKey {
                low: 900,
                high: -700,
            },
            800,
            &[700, 701],
        );
        // Reorder both path priority and the mode table while repairing only transient indices.
        right.queries[0].paths.reverse();
        right.target_names.make_contiguous().reverse();
        let old_modes = std::mem::take(&mut right.queries[0].modes);
        right.queries[0].modes = vec![old_modes[2], old_modes[0], old_modes[1]];
        for path in &mut right.queries[0].paths {
            let target_index = if path.targetInfo.id == 700 { 2 } else { 0 };
            path.sourceInfo.Anonymous.modeInfoIdx = 1_u32 << 16 | 91;
            path.targetInfo.Anonymous.modeInfoIdx =
                target_index << 16 | DISPLAYCONFIG_PATH_DESKTOP_IMAGE_IDX_INVALID;
        }
        let left_observation = observe_active_output_with(&mut left, 99).unwrap();
        let right_observation = observe_active_output_with(&mut right, 123).unwrap();
        assert_eq!(left_observation.tag(), right_observation.tag());
        assert!(left_observation.stable_topology_eq(&right_observation));
    }

    #[test]
    fn fallback_profile_invalid_parameter_is_terminal() {
        let adapter = AdapterKey { low: 1, high: 2 };
        let mut api = base_api(adapter, 3, &[4]);
        api.sizes = VecDeque::from([
            SizeCall {
                error_code: ERROR_INVALID_PARAMETER,
                path_count: 0,
                mode_count: 0,
            },
            SizeCall {
                error_code: ERROR_INVALID_PARAMETER,
                path_count: 0,
                mode_count: 0,
            },
        ]);
        let observation = observe_active_output_with(&mut api, 99).unwrap();
        assert!(observation.unavailable().is_some());
        let size_flags: Vec<_> = api
            .calls
            .iter()
            .filter_map(|call| match call {
                Call::Size(flags) => Some(*flags),
                _ => None,
            })
            .collect();
        assert_eq!(size_flags, [0x52, 0x12]);
    }

    #[test]
    fn successful_query_cannot_report_more_than_allocated() {
        let adapter = AdapterKey { low: 1, high: 2 };
        let mut api = base_api(adapter, 3, &[4]);
        api.queries[0].result.path_count = 2;
        let observation = observe_active_output_with(&mut api, 99).unwrap();
        assert!(observation.unavailable().is_some());
        assert!(
            !api.calls
                .iter()
                .any(|call| matches!(call, Call::SourceName(_)))
        );
    }

    #[test]
    fn wrong_mode_owner_and_noncanonical_bool_fail_closed() {
        let adapter = AdapterKey { low: 1, high: 2 };
        let mut wrong_owner = base_api(adapter, 3, &[4]);
        wrong_owner.queries[0].modes[1].id = 999;
        assert!(
            observe_active_output_with(&mut wrong_owner, 99)
                .unwrap()
                .unavailable()
                .is_some()
        );

        let mut bad_bool = base_api(adapter, 3, &[4]);
        bad_bool.queries[0].paths[0].targetInfo.targetAvailable = 2;
        assert!(
            observe_active_output_with(&mut bad_bool, 99)
                .unwrap()
                .unavailable()
                .is_some()
        );
    }

    #[test]
    fn duplicate_source_name_match_is_ambiguous_not_first_wins() {
        let adapter = AdapterKey { low: 1, high: 2 };
        let other_adapter = AdapterKey { low: 8, high: 9 };
        let mut api = base_api(adapter, 3, &[4]);
        let other_path = path(other_adapter, 10, 11, 2, 3);
        api.queries[0].paths.push(other_path);
        api.queries[0].modes.push(source_mode(other_adapter, 10));
        api.queries[0].modes.push(target_mode(other_adapter, 11));
        api.queries[0].result.path_count = 2;
        api.queries[0].result.mode_count = 4;
        api.sizes[0].path_count = 2;
        api.sizes[0].mode_count = 4;
        api.source_names.push_back((
            SourceKey {
                adapter: other_adapter,
                id: 10,
            },
            Ok(display_name()),
        ));
        let observation = observe_active_output_with(&mut api, 99).unwrap();
        assert!(observation.unavailable().is_some());
        assert!(
            !api.calls
                .iter()
                .any(|call| matches!(call, Call::TargetName(_)))
        );
    }

    #[test]
    fn target_identity_failure_is_explicit_but_topology_remains_available() {
        let adapter = AdapterKey { low: 1, high: 2 };
        let mut api = base_api(adapter, 3, &[4]);
        api.target_names[0].1 = Err(123);
        let observation = observe_active_output_with(&mut api, 99).unwrap();
        let route = &available(&observation).routes[0];
        assert_eq!(
            route.identity,
            TargetIdentityObservation::Unavailable {
                reason_code_raw: I_CALL,
                error_code: 123,
            }
        );
    }

    #[test]
    fn wrong_mode_discriminator_and_invalid_selector_fail_closed() {
        let adapter = AdapterKey { low: 1, high: 2 };
        let mut wrong_kind = base_api(adapter, 3, &[4]);
        wrong_kind.queries[0].modes[0].infoType = DISPLAYCONFIG_MODE_INFO_TYPE_TARGET;
        assert!(
            observe_active_output_with(&mut wrong_kind, 99)
                .unwrap()
                .unavailable()
                .is_some()
        );

        let mut invalid = base_api(adapter, 3, &[4]);
        invalid.queries[0].paths[0].sourceInfo.Anonymous.modeInfoIdx =
            DISPLAYCONFIG_PATH_SOURCE_MODE_IDX_INVALID << 16;
        assert!(
            observe_active_output_with(&mut invalid, 99)
                .unwrap()
                .unavailable()
                .is_some()
        );
    }

    #[test]
    fn malformed_successful_target_name_becomes_explicit_identity_failure() {
        let adapter = AdapterKey { low: 1, high: 2 };
        let mut api = base_api(adapter, 3, &[4]);
        let packet = api.target_names[0].1.as_mut().unwrap();
        packet.monitorDevicePath[2] = 0;
        packet.monitorDevicePath[3] = b'X' as u16;
        let observation = observe_active_output_with(&mut api, 99).unwrap();
        assert_eq!(
            available(&observation).routes[0].identity,
            TargetIdentityObservation::Unavailable {
                reason_code_raw: I_STRING,
                error_code: 0,
            }
        );
    }

    #[test]
    fn retry_budget_is_four_total_cycles() {
        let adapter = AdapterKey { low: 1, high: 2 };
        let mut api = base_api(adapter, 3, &[4]);
        api.sizes = VecDeque::from(
            [SizeCall {
                error_code: ERROR_INSUFFICIENT_BUFFER,
                path_count: 0,
                mode_count: 0,
            }; 4],
        );
        let observation = observe_active_output_with(&mut api, 99).unwrap();
        assert!(observation.unavailable().is_some());
        assert_eq!(
            api.calls
                .iter()
                .filter(|call| matches!(call, Call::Size(_)))
                .count(),
            4
        );
    }

    #[test]
    fn fixed_utf16_preserves_unpaired_surrogate_and_rejects_nonzero_tail() {
        let raw = [0xd800, b'X' as u16, 0, 0];
        assert_eq!(fixed_utf16(&raw).unwrap(), &[0xd800, b'X' as u16]);
        let bad = [b'A' as u16, 0, b'B' as u16, 0];
        assert_eq!(fixed_utf16(&bad), Err(FixedUtf16Error::NonzeroTail));
    }

    #[test]
    fn unavailable_is_a_canonical_output_not_a_collector_error() {
        let adapter = AdapterKey { low: 1, high: 2 };
        let mut api = base_api(adapter, 3, &[4]);
        api.monitor = Err(6);
        let observation = observe_active_output_with(&mut api, 99).unwrap();
        assert!(matches!(
            observation.facts(),
            OutputTopologyFacts::Unavailable(_)
        ));
        observation.verify().unwrap();
    }
}
