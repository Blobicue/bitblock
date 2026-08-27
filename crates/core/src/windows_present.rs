//! Checked Win32 submission of the canonical CPU pixel buffer.
//!
//! The pinned Softbuffer Win32 backend discards the Boolean result of its GDI `BitBlt` call.
//! This surface instead owns a plain Rust pixel buffer and submits it with
//! `SetDIBitsToDevice`, whose returned scanline count is retained and checked. Success here is
//! still only a checked GDI submission; it is not compositor, scanout, or physical-light proof.

use std::fmt;
use std::mem::size_of;

use windows_sys::Win32::Foundation::{GetLastError, HWND, SetLastError};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, GetDC, ReleaseDC, SetDIBitsToDevice,
};

use crate::protocol::{ContentTag, TagKind, hash_canonical};

const WINDOWS_PRESENT_POLICY_SCHEMA: &[u8] =
    b"bitblock/windows-present-policy/set-dibits-to-device/v1";
const WINDOWS_PRESENT_POLICY_PAYLOAD_LEN: u64 = 17;

/// Content identity for the exact checked Win32 submission policy.
pub fn windows_present_policy_tag() -> ContentTag {
    hash_canonical(
        TagKind::Policy,
        WINDOWS_PRESENT_POLICY_SCHEMA,
        WINDOWS_PRESENT_POLICY_PAYLOAD_LEN,
        |sink| {
            sink(&1_u16.to_be_bytes()); // policy revision
            sink(&[0]); // SetDIBitsToDevice into a window DC
            sink(&BI_RGB.to_be_bytes());
            sink(&32_u16.to_be_bytes());
            sink(&[1]); // negative DIB height: top-down rows
            sink(&DIB_RGB_COLORS.to_be_bytes());
            sink(&[1]); // require the complete requested scanline count
            sink(&[1]); // acquire and release the window DC per submission
            sink(&[1]); // retain ReleaseDC outcome separately from pixel acceptance
        },
    )
    .expect("Windows presentation policy encoding has a fixed length")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowsPresentFacts {
    pub policy: ContentTag,
    pub requested_scanlines: u32,
    pub returned_scanlines: i32,
    pub call_error_code: u32,
    pub release_result: i32,
    pub release_error_code: u32,
}

impl WindowsPresentFacts {
    pub const fn release_succeeded(self) -> bool {
        self.release_result != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsPresentError {
    Uninitialized,
    AcquireWindowDc {
        error_code: u32,
    },
    ScanlineCount {
        expected: u32,
        actual: i32,
        error_code: u32,
        release_result: i32,
        release_error_code: u32,
    },
}

impl fmt::Display for WindowsPresentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Uninitialized => formatter.write_str("pixel surface has not been sized"),
            Self::AcquireWindowDc { error_code } => {
                write!(formatter, "GetDC failed with Win32 error {error_code}")
            }
            Self::ScanlineCount {
                expected,
                actual,
                error_code,
                release_result,
                release_error_code,
            } => write!(
                formatter,
                "SetDIBitsToDevice returned {actual} of {expected} scanlines (Win32 error {error_code}); ReleaseDC returned {release_result} (Win32 error {release_error_code})"
            ),
        }
    }
}

impl std::error::Error for WindowsPresentError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsSurfaceError {
    NullWindowHandle,
    Uninitialized,
    DimensionOutOfRange { width: u32, height: u32 },
    PixelCountOverflow { width: u32, height: u32 },
    AllocationFailed { width: u32, height: u32 },
}

impl fmt::Display for WindowsSurfaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::NullWindowHandle => formatter.write_str("window handle is null"),
            Self::Uninitialized => formatter.write_str("pixel surface has not been sized"),
            Self::DimensionOutOfRange { width, height } => write!(
                formatter,
                "surface dimensions {width}x{height} exceed the signed Win32 DIB range"
            ),
            Self::PixelCountOverflow { width, height } => {
                write!(
                    formatter,
                    "surface pixel count overflows usize for {width}x{height}"
                )
            }
            Self::AllocationFailed { width, height } => {
                write!(
                    formatter,
                    "could not allocate surface pixels for {width}x{height}"
                )
            }
        }
    }
}

impl std::error::Error for WindowsSurfaceError {}

/// A top-down XRGB8888 buffer submitted directly through checked Win32 GDI.
pub struct WindowsPixelSurface {
    hwnd: HWND,
    width: u32,
    height: u32,
    pixels: Vec<u32>,
}

impl WindowsPixelSurface {
    /// Creates a surface for an HWND that the caller keeps alive for this surface's lifetime.
    ///
    /// # Safety
    ///
    /// `hwnd` must identify a live window owned by the calling event-loop thread whenever
    /// [`Self::present`] is called.
    pub unsafe fn new(hwnd: HWND) -> Result<Self, WindowsSurfaceError> {
        if hwnd.is_null() {
            return Err(WindowsSurfaceError::NullWindowHandle);
        }
        Ok(Self {
            hwnd,
            width: 0,
            height: 0,
            pixels: Vec::new(),
        })
    }

    /// Replaces the backing allocation only after the requested shape has been validated and
    /// fully reserved. A failed resize leaves the preceding surface intact.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), WindowsSurfaceError> {
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err(WindowsSurfaceError::DimensionOutOfRange { width, height });
        }
        let pixel_count = usize::try_from(u64::from(width) * u64::from(height))
            .map_err(|_| WindowsSurfaceError::PixelCountOverflow { width, height })?;
        if self.width == width && self.height == height && self.pixels.len() == pixel_count {
            return Ok(());
        }

        let mut replacement = Vec::new();
        replacement
            .try_reserve_exact(pixel_count)
            .map_err(|_| WindowsSurfaceError::AllocationFailed { width, height })?;
        replacement.resize(pixel_count, 0);
        self.width = width;
        self.height = height;
        self.pixels = replacement;
        Ok(())
    }

    pub fn pixels_mut(&mut self) -> Result<&mut [u32], WindowsSurfaceError> {
        if self.width == 0 || self.height == 0 {
            return Err(WindowsSurfaceError::Uninitialized);
        }
        Ok(&mut self.pixels)
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Submits all rows and accepts the occurrence only when GDI reports the full row count.
    /// A failed `ReleaseDC` is retained in otherwise-successful facts because the pixels may
    /// already have been accepted; cleanup failure must not be rewritten as non-submission.
    pub fn present(&self) -> Result<WindowsPresentFacts, WindowsPresentError> {
        if self.width == 0 || self.height == 0 {
            return Err(WindowsPresentError::Uninitialized);
        }
        debug_assert_eq!(
            self.pixels.len() as u64,
            u64::from(self.width) * u64::from(self.height)
        );

        // SAFETY: the winit window outlives this synchronous call and the event-loop thread owns
        // the HWND. The pixel slice and BITMAPINFO remain live until SetDIBitsToDevice returns.
        unsafe {
            SetLastError(0);
            let dc = GetDC(self.hwnd);
            if dc.is_null() {
                return Err(WindowsPresentError::AcquireWindowDc {
                    error_code: GetLastError(),
                });
            }

            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: self.width as i32,
                    biHeight: -(self.height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    biSizeImage: 0,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                },
                ..BITMAPINFO::default()
            };

            SetLastError(0);
            let returned_scanlines = SetDIBitsToDevice(
                dc,
                0,
                0,
                self.width,
                self.height,
                0,
                0,
                0,
                self.height,
                self.pixels.as_ptr().cast(),
                &info,
                DIB_RGB_COLORS,
            );
            let call_error_code = GetLastError();
            SetLastError(0);
            let release_result = ReleaseDC(self.hwnd, dc);
            let release_error_code = GetLastError();
            assess_present_result(
                self.height,
                returned_scanlines,
                call_error_code,
                release_result,
                release_error_code,
            )
        }
    }
}

fn assess_present_result(
    expected: u32,
    actual: i32,
    error_code: u32,
    release_result: i32,
    release_error_code: u32,
) -> Result<WindowsPresentFacts, WindowsPresentError> {
    let facts = WindowsPresentFacts {
        policy: windows_present_policy_tag(),
        requested_scanlines: expected,
        returned_scanlines: actual,
        call_error_code: error_code,
        release_result,
        release_error_code,
    };
    if actual == expected as i32 {
        Ok(facts)
    } else {
        Err(WindowsPresentError::ScanlineCount {
            expected,
            actual,
            error_code,
            release_result,
            release_error_code,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_scanline_count_is_the_only_accepted_submission_shape() {
        let complete = assess_present_result(801, 801, 0, 1, 0).unwrap();
        assert_eq!(complete.requested_scanlines, 801);
        assert_eq!(complete.returned_scanlines, 801);
        assert!(complete.release_succeeded());

        assert!(matches!(
            assess_present_result(801, 0, 5, 1, 0),
            Err(WindowsPresentError::ScanlineCount {
                expected: 801,
                actual: 0,
                error_code: 5,
                ..
            })
        ));
        assert!(matches!(
            assess_present_result(801, 800, 0, 1, 0),
            Err(WindowsPresentError::ScanlineCount {
                expected: 801,
                actual: 800,
                ..
            })
        ));
        assert!(matches!(
            assess_present_result(801, 802, 0, 1, 0),
            Err(WindowsPresentError::ScanlineCount {
                expected: 801,
                actual: 802,
                ..
            })
        ));
    }

    #[test]
    fn policy_matches_the_independent_python_known_answer() {
        assert_eq!(
            windows_present_policy_tag().to_string(),
            "#bb.policy.sha256.opgpybj32tw72pep2xusit4lzzgsiozs5f5reppq67iiejueetvq"
        );
    }

    #[test]
    fn release_failure_does_not_rewrite_an_accepted_pixel_submission() {
        let facts = assess_present_result(3, 3, 0, 0, 6).unwrap();
        assert!(!facts.release_succeeded());
        assert_eq!(facts.release_error_code, 6);
    }

    #[test]
    fn resize_is_exact_and_failure_preserves_the_previous_buffer() {
        // SAFETY: this test never invokes an HWND API; it exercises only owned-buffer sizing.
        let mut surface = unsafe { WindowsPixelSurface::new(1_usize as HWND) }.unwrap();
        surface.resize(3, 2).unwrap();
        surface
            .pixels_mut()
            .unwrap()
            .copy_from_slice(&[1, 2, 3, 4, 5, 6]);
        assert_eq!(surface.width(), 3);
        assert_eq!(surface.height(), 2);

        assert_eq!(
            surface.resize(i32::MAX as u32 + 1, 1),
            Err(WindowsSurfaceError::DimensionOutOfRange {
                width: i32::MAX as u32 + 1,
                height: 1,
            })
        );
        assert_eq!(surface.pixels_mut().unwrap(), &[1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn null_window_handle_is_rejected() {
        // SAFETY: null is deliberately passed to verify the constructor guard.
        assert!(matches!(
            unsafe { WindowsPixelSurface::new(std::ptr::null_mut()) },
            Err(WindowsSurfaceError::NullWindowHandle)
        ));
    }

    #[test]
    fn unsized_surface_refuses_before_calling_any_window_api() {
        // SAFETY: present returns at the shape guard before dereferencing this sentinel HWND.
        let surface = unsafe { WindowsPixelSurface::new(1_usize as HWND) }.unwrap();
        assert_eq!(surface.present(), Err(WindowsPresentError::Uninitialized));
    }
}
