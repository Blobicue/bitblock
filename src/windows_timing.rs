//! Raw Win32 composition-timing observations around one checked pixel submission.
//!
//! This collector is research-feature-only. Observation failures are retained in the returned
//! facts and never rewrite a successful checked presentation as a failure. Conversely, a failed
//! checked presentation returns the presenter's error and never produces an admissible envelope.

use std::mem::size_of;
use std::ptr::{addr_of, addr_of_mut, null_mut};

use windows_sys::Win32::Foundation::{GetLastError, SetLastError};
use windows_sys::Win32::Graphics::Dwm::{
    DWM_TIMING_INFO, DwmFlush, DwmGetCompositionTimingInfo, UNSIGNED_RATIO,
};
use windows_sys::Win32::Graphics::Gdi::GdiFlush;
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

use crate::frame_v2::FrameReceiptV2;
use crate::presentation::{
    CompositionTimingEnvelope, DwmFlushCall, DwmRatio, DwmTimingCall, DwmTimingSnapshot,
    GdiFlushCall, PresentationProtocolError, PresenterEvidence, QpcBracket, QpcFrequency, QpcRead,
};
use crate::windows_present::WindowsPresentFacts;

const _: () = assert!(size_of::<DWM_TIMING_INFO>() == 292);
const _: () = assert!(size_of::<UNSIGNED_RATIO>() == 8);

/// One complete raw observation whose exact composite receipt can be bound after submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowsCompositionTimingObservation {
    pub presenter: PresenterEvidence,
    pub qpc_frequency: QpcFrequency,
    pub pre: DwmTimingCall,
    pub presenter_bracket: QpcBracket,
    pub gdi_flush: GdiFlushCall,
    pub dwm_flush: DwmFlushCall,
    pub post: DwmTimingCall,
}

impl WindowsCompositionTimingObservation {
    /// Binds these host facts to the exact committed composite receipt.
    pub fn bind(
        self,
        receipt: FrameReceiptV2,
    ) -> Result<CompositionTimingEnvelope, PresentationProtocolError> {
        CompositionTimingEnvelope::new(
            receipt,
            self.presenter,
            self.qpc_frequency,
            self.pre,
            self.presenter_bracket,
            self.gdi_flush,
            self.dwm_flush,
            self.post,
        )
    }
}

/// Calls the checked presenter once and surrounds it with raw session-level timing observations.
///
/// QPC, DWM timing, `GdiFlush`, and `DwmFlush` failures remain data. Only `present` returning an
/// error prevents a successful observation from being returned.
pub fn observe_checked_present<E>(
    present: impl FnOnce() -> Result<WindowsPresentFacts, E>,
) -> Result<WindowsCompositionTimingObservation, E> {
    collect_with(&mut Win32TimingApi, present)
}

trait TimingApi {
    fn qpc_frequency(&mut self) -> QpcFrequency;
    fn qpc(&mut self) -> QpcRead;
    fn dwm_timing(&mut self) -> (i32, Option<DwmTimingSnapshot>);
    fn gdi_flush(&mut self) -> bool;
    fn dwm_flush(&mut self) -> i32;
}

fn collect_with<A: TimingApi, E>(
    api: &mut A,
    present: impl FnOnce() -> Result<WindowsPresentFacts, E>,
) -> Result<WindowsCompositionTimingObservation, E> {
    let qpc_frequency = api.qpc_frequency();
    let pre = observe_dwm_call(api);

    let presenter_before = api.qpc();
    let presented = present();
    let presenter_after = api.qpc();
    let presenter = presented?;

    // Every post-presentation boundary is called even if a preceding observation boundary failed.
    let gdi_before = api.qpc();
    let gdi_succeeded = api.gdi_flush();
    let gdi_after = api.qpc();
    let dwm_before = api.qpc();
    let dwm_hresult = api.dwm_flush();
    let dwm_after = api.qpc();
    let post = observe_dwm_call(api);

    Ok(WindowsCompositionTimingObservation {
        presenter: PresenterEvidence {
            policy: presenter.policy,
            requested_scanlines: presenter.requested_scanlines,
            returned_scanlines: presenter.returned_scanlines,
            call_error_code: presenter.call_error_code,
            release_result: presenter.release_result,
            release_error_code: presenter.release_error_code,
        },
        qpc_frequency,
        pre,
        presenter_bracket: QpcBracket {
            before: presenter_before,
            after: presenter_after,
        },
        gdi_flush: GdiFlushCall {
            bracket: QpcBracket {
                before: gdi_before,
                after: gdi_after,
            },
            succeeded: gdi_succeeded,
        },
        dwm_flush: DwmFlushCall {
            bracket: QpcBracket {
                before: dwm_before,
                after: dwm_after,
            },
            hresult: dwm_hresult,
        },
        post,
    })
}

fn observe_dwm_call(api: &mut impl TimingApi) -> DwmTimingCall {
    let before = api.qpc();
    let (hresult, snapshot) = api.dwm_timing();
    let after = api.qpc();
    DwmTimingCall {
        bracket: QpcBracket { before, after },
        hresult,
        snapshot,
    }
}

struct Win32TimingApi;

impl TimingApi for Win32TimingApi {
    fn qpc_frequency(&mut self) -> QpcFrequency {
        let mut value = 0_i64;
        // SAFETY: `value` is a live writable LARGE_INTEGER-compatible i64 for this call.
        unsafe {
            SetLastError(0);
            if QueryPerformanceFrequency(&mut value) != 0 && value > 0 {
                QpcFrequency {
                    succeeded: true,
                    counts_per_second: value,
                    error_code: 0,
                }
            } else {
                QpcFrequency {
                    succeeded: false,
                    counts_per_second: 0,
                    error_code: GetLastError(),
                }
            }
        }
    }

    fn qpc(&mut self) -> QpcRead {
        let mut value = 0_i64;
        // SAFETY: `value` is a live writable LARGE_INTEGER-compatible i64 for this call.
        unsafe {
            SetLastError(0);
            if QueryPerformanceCounter(&mut value) != 0 {
                QpcRead {
                    succeeded: true,
                    value,
                    error_code: 0,
                }
            } else {
                QpcRead {
                    succeeded: false,
                    value: 0,
                    error_code: GetLastError(),
                }
            }
        }
    }

    fn dwm_timing(&mut self) -> (i32, Option<DwmTimingSnapshot>) {
        let mut raw = DWM_TIMING_INFO::default();
        let raw_ptr = addr_of_mut!(raw);
        // SAFETY: `raw_ptr` points to a live zero-initialized packed ABI struct. `cbSize` is
        // written without creating an unaligned reference, and a null HWND is required on modern
        // Windows for the session-scoped timing query.
        let hresult = unsafe {
            addr_of_mut!((*raw_ptr).cbSize).write_unaligned(size_of::<DWM_TIMING_INFO>() as u32);
            DwmGetCompositionTimingInfo(null_mut(), raw_ptr)
        };
        if hresult == 0 {
            // SAFETY: exact S_OK means the API initialized the declared DWM_TIMING_INFO fields.
            (hresult, Some(unsafe { read_dwm_snapshot(raw_ptr) }))
        } else {
            (hresult, None)
        }
    }

    fn gdi_flush(&mut self) -> bool {
        // SAFETY: GdiFlush takes no pointers; its BOOL is retained without inventing an error lane.
        unsafe { GdiFlush() != 0 }
    }

    fn dwm_flush(&mut self) -> i32 {
        // SAFETY: DwmFlush takes no pointers; the raw HRESULT is retained.
        unsafe { DwmFlush() }
    }
}

unsafe fn read_ratio(raw: *const UNSIGNED_RATIO) -> DwmRatio {
    // SAFETY: the caller supplies a pointer into an initialized packed DWM_TIMING_INFO. Reading
    // the packed ratio and its fields unaligned avoids ever forming an unaligned reference.
    let ratio = unsafe { raw.read_unaligned() };
    let ratio_ptr = addr_of!(ratio);
    DwmRatio {
        numerator: unsafe { addr_of!((*ratio_ptr).uiNumerator).read_unaligned() },
        denominator: unsafe { addr_of!((*ratio_ptr).uiDenominator).read_unaligned() },
    }
}

unsafe fn read_dwm_snapshot(raw: *const DWM_TIMING_INFO) -> DwmTimingSnapshot {
    macro_rules! read_field {
        ($field:ident) => {{
            // SAFETY: `raw` points to a successfully initialized packed DWM_TIMING_INFO, and the
            // projected field is copied with an unaligned raw-pointer read.
            unsafe { addr_of!((*raw).$field).read_unaligned() }
        }};
    }

    DwmTimingSnapshot {
        // SAFETY: both ratio pointers project initialized fields of `raw`.
        rate_refresh: unsafe { read_ratio(addr_of!((*raw).rateRefresh)) },
        qpc_refresh_period: read_field!(qpcRefreshPeriod),
        // SAFETY: both ratio pointers project initialized fields of `raw`.
        rate_compose: unsafe { read_ratio(addr_of!((*raw).rateCompose)) },
        qpc_vblank: read_field!(qpcVBlank),
        c_refresh: read_field!(cRefresh),
        c_dx_refresh: read_field!(cDXRefresh),
        qpc_compose: read_field!(qpcCompose),
        c_frame: read_field!(cFrame),
        c_dx_present: read_field!(cDXPresent),
        c_refresh_frame: read_field!(cRefreshFrame),
        c_frame_submitted: read_field!(cFrameSubmitted),
        c_dx_present_submitted: read_field!(cDXPresentSubmitted),
        c_frame_confirmed: read_field!(cFrameConfirmed),
        c_dx_present_confirmed: read_field!(cDXPresentConfirmed),
        c_refresh_confirmed: read_field!(cRefreshConfirmed),
        c_dx_refresh_confirmed: read_field!(cDXRefreshConfirmed),
        c_frames_late: read_field!(cFramesLate),
        c_frames_outstanding: read_field!(cFramesOutstanding),
        c_frame_displayed: read_field!(cFrameDisplayed),
        qpc_frame_displayed: read_field!(qpcFrameDisplayed),
        c_refresh_frame_displayed: read_field!(cRefreshFrameDisplayed),
        c_frame_complete: read_field!(cFrameComplete),
        qpc_frame_complete: read_field!(qpcFrameComplete),
        c_frame_pending: read_field!(cFramePending),
        qpc_frame_pending: read_field!(qpcFramePending),
        c_frames_displayed: read_field!(cFramesDisplayed),
        c_frames_complete: read_field!(cFramesComplete),
        c_frames_pending: read_field!(cFramesPending),
        c_frames_available: read_field!(cFramesAvailable),
        c_frames_dropped: read_field!(cFramesDropped),
        c_frames_missed: read_field!(cFramesMissed),
        c_refresh_next_displayed: read_field!(cRefreshNextDisplayed),
        c_refresh_next_presented: read_field!(cRefreshNextPresented),
        c_refreshes_displayed: read_field!(cRefreshesDisplayed),
        c_refreshes_presented: read_field!(cRefreshesPresented),
        c_refresh_started: read_field!(cRefreshStarted),
        c_pixels_received: read_field!(cPixelsReceived),
        c_pixels_drawn: read_field!(cPixelsDrawn),
        c_buffers_empty: read_field!(cBuffersEmpty),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_v2::{FrameReceiptV2, FrameStream};
    use crate::presentation::{CompositionProgress, FrameAttribution};
    use crate::protocol::{ContentTag, TagKind};
    use crate::windows_present::windows_present_policy_tag;

    fn tag(kind: TagKind, value: u8) -> ContentTag {
        ContentTag::from_digest(kind, [value; 32])
    }

    fn receipt() -> FrameReceiptV2 {
        FrameReceiptV2::new(
            None,
            FrameStream::Composite,
            tag(TagKind::TickOutcome, 1),
            tag(TagKind::FrameInput, 2),
            tag(TagKind::Frame, 3),
        )
        .unwrap()
    }

    fn present_facts() -> WindowsPresentFacts {
        WindowsPresentFacts {
            policy: windows_present_policy_tag(),
            requested_scanlines: 720,
            returned_scanlines: 720,
            call_error_code: 0,
            release_result: 1,
            release_error_code: 0,
        }
    }

    fn snapshot(value: u64) -> DwmTimingSnapshot {
        DwmTimingSnapshot {
            rate_refresh: DwmRatio {
                numerator: 60,
                denominator: 1,
            },
            qpc_refresh_period: 166_667,
            rate_compose: DwmRatio {
                numerator: 60,
                denominator: 1,
            },
            qpc_vblank: value,
            c_refresh: value,
            c_dx_refresh: value as u32,
            qpc_compose: value,
            c_frame: value,
            c_dx_present: value as u32,
            c_refresh_frame: value,
            c_frame_submitted: value,
            c_dx_present_submitted: value as u32,
            c_frame_confirmed: value,
            c_dx_present_confirmed: value as u32,
            c_refresh_confirmed: value,
            c_dx_refresh_confirmed: value as u32,
            c_frame_displayed: value,
            qpc_frame_displayed: value,
            c_refresh_frame_displayed: value,
            c_frame_complete: value,
            qpc_frame_complete: value,
            c_frame_pending: value,
            qpc_frame_pending: value,
            c_refresh_next_displayed: value,
            c_refresh_next_presented: value,
            ..DwmTimingSnapshot::default()
        }
    }

    struct ScriptedApi {
        events: Vec<&'static str>,
        qpc_values: [QpcRead; 10],
        qpc_index: usize,
        frequency: QpcFrequency,
        dwm_values: [(i32, Option<DwmTimingSnapshot>); 2],
        dwm_index: usize,
        gdi_succeeded: bool,
        dwm_hresult: i32,
    }

    impl ScriptedApi {
        fn complete() -> Self {
            Self {
                events: Vec::new(),
                qpc_values: std::array::from_fn(|index| QpcRead {
                    succeeded: true,
                    value: index as i64 + 1,
                    error_code: 0,
                }),
                qpc_index: 0,
                frequency: QpcFrequency {
                    succeeded: true,
                    counts_per_second: 10_000_000,
                    error_code: 0,
                },
                dwm_values: [(0, Some(snapshot(100))), (0, Some(snapshot(101)))],
                dwm_index: 0,
                gdi_succeeded: true,
                dwm_hresult: 0,
            }
        }
    }

    impl TimingApi for ScriptedApi {
        fn qpc_frequency(&mut self) -> QpcFrequency {
            self.events.push("qpf");
            self.frequency
        }

        fn qpc(&mut self) -> QpcRead {
            self.events.push("qpc");
            let value = self.qpc_values[self.qpc_index];
            self.qpc_index += 1;
            value
        }

        fn dwm_timing(&mut self) -> (i32, Option<DwmTimingSnapshot>) {
            self.events.push("dwm-timing");
            let value = self.dwm_values[self.dwm_index];
            self.dwm_index += 1;
            value
        }

        fn gdi_flush(&mut self) -> bool {
            self.events.push("gdi-flush");
            self.gdi_succeeded
        }

        fn dwm_flush(&mut self) -> i32 {
            self.events.push("dwm-flush");
            self.dwm_hresult
        }
    }

    #[test]
    fn complete_call_order_binds_without_promoting_frame_attribution() {
        let mut api = ScriptedApi::complete();
        let observation = collect_with(&mut api, || Ok::<_, ()>(present_facts())).unwrap();
        let envelope = observation.bind(receipt()).unwrap();

        assert_eq!(envelope.progress(), CompositionProgress::ComparableAdvanced);
        assert_eq!(envelope.frame_attribution(), FrameAttribution::Unknown);
        assert_eq!(api.qpc_index, 10);
        assert_eq!(api.dwm_index, 2);
        assert_eq!(
            api.events,
            [
                "qpf",
                "qpc",
                "dwm-timing",
                "qpc",
                "qpc",
                "qpc",
                "qpc",
                "gdi-flush",
                "qpc",
                "qpc",
                "dwm-flush",
                "qpc",
                "qpc",
                "dwm-timing",
                "qpc",
            ]
        );
    }

    #[test]
    fn observation_failures_do_not_short_circuit_later_boundaries() {
        let mut api = ScriptedApi::complete();
        api.frequency = QpcFrequency {
            succeeded: false,
            counts_per_second: 0,
            error_code: 5,
        };
        api.dwm_values = [(-1, None), (-2, None)];
        api.gdi_succeeded = false;
        api.dwm_hresult = -3;
        api.qpc_values[2] = QpcRead {
            succeeded: false,
            value: 0,
            error_code: 6,
        };

        let observation = collect_with(&mut api, || Ok::<_, ()>(present_facts())).unwrap();
        let envelope = observation.bind(receipt()).unwrap();

        assert_eq!(api.qpc_index, 10);
        assert_eq!(api.dwm_index, 2);
        assert_eq!(envelope.progress(), CompositionProgress::Unavailable);
        assert_eq!(envelope.frame_attribution(), FrameAttribution::Unknown);
    }

    #[test]
    fn checked_presenter_failure_prevents_an_observation() {
        let mut api = ScriptedApi::complete();
        let result = collect_with(&mut api, || Err::<WindowsPresentFacts, _>("present failed"));

        assert_eq!(result, Err("present failed"));
        assert_eq!(api.qpc_index, 4);
        assert_eq!(api.dwm_index, 1);
        assert!(!api.events.contains(&"gdi-flush"));
        assert!(!api.events.contains(&"dwm-flush"));
    }
}
