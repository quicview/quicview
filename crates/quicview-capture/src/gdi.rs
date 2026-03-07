use std::mem;

use windows_sys::Win32::Foundation::BOOL;
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetDC,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

use quicview_protocol::{DisplayId, DisplayInfo, PixelFormat, Resolution};

use crate::error::CaptureError;
use crate::source::{CaptureSource, FrameBuffer};

/// Windows GDI-based screen capture using `BitBlt`.
///
/// Captures the primary display at native resolution. Suitable for
/// moderate frame rates (~30 fps). For higher performance, consider
/// DXGI Desktop Duplication in a future iteration.
pub struct GdiCaptureSource {
    active: bool,
    target_display: Option<DisplayId>,
    resolution: Resolution,
    frame_count: u64,
}

impl GdiCaptureSource {
    pub fn new() -> Self {
        Self {
            active: false,
            target_display: None,
            resolution: Resolution::new(0, 0),
            frame_count: 0,
        }
    }
}

impl Default for GdiCaptureSource {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureSource for GdiCaptureSource {
    fn enumerate_displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        // SAFETY: GetSystemMetrics is safe to call with valid metric IDs.
        let (width, height) = unsafe {
            (
                GetSystemMetrics(SM_CXSCREEN),
                GetSystemMetrics(SM_CYSCREEN),
            )
        };

        if width <= 0 || height <= 0 {
            return Err(CaptureError::CaptureFailed(
                "failed to get screen dimensions".into(),
            ));
        }

        Ok(vec![DisplayInfo {
            id: DisplayId::PRIMARY,
            name: "Primary Display".into(),
            resolution: Resolution::new(width as u32, height as u32),
            refresh_hz: 60,
            is_virtual: false,
        }])
    }

    fn start(&mut self, display_id: DisplayId) -> Result<(), CaptureError> {
        let displays = self.enumerate_displays()?;
        let target = displays
            .iter()
            .find(|d| d.id == display_id)
            .ok_or(CaptureError::DisplayNotFound(display_id.0))?;

        self.target_display = Some(display_id);
        self.resolution = target.resolution;
        self.active = true;
        Ok(())
    }

    fn grab(&mut self) -> Result<Option<FrameBuffer>, CaptureError> {
        if !self.active {
            return Err(CaptureError::CaptureFailed("not started".into()));
        }

        let w = self.resolution.width as i32;
        let h = self.resolution.height as i32;

        // SAFETY: All GDI handles are properly acquired and released.
        // Resources follow a strict acquire → use → release order.
        unsafe {
            let hdc_screen = GetDC(0);
            if hdc_screen == 0 {
                return Err(CaptureError::CaptureFailed("GetDC failed".into()));
            }

            let hdc_mem = CreateCompatibleDC(hdc_screen);
            if hdc_mem == 0 {
                ReleaseDC(0, hdc_screen);
                return Err(CaptureError::CaptureFailed(
                    "CreateCompatibleDC failed".into(),
                ));
            }

            let hbm = CreateCompatibleBitmap(hdc_screen, w, h);
            if hbm == 0 {
                DeleteDC(hdc_mem);
                ReleaseDC(0, hdc_screen);
                return Err(CaptureError::CaptureFailed(
                    "CreateCompatibleBitmap failed".into(),
                ));
            }

            let old_obj = SelectObject(hdc_mem, hbm);

            let ok: BOOL = BitBlt(hdc_mem, 0, 0, w, h, hdc_screen, 0, 0, SRCCOPY);

            if ok == 0 {
                SelectObject(hdc_mem, old_obj);
                DeleteObject(hbm);
                DeleteDC(hdc_mem);
                ReleaseDC(0, hdc_screen);
                return Err(CaptureError::CaptureFailed("BitBlt failed".into()));
            }

            let mut bmi: BITMAPINFO = mem::zeroed();
            bmi.bmiHeader.biSize = mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = w;
            bmi.bmiHeader.biHeight = -h; // top-down
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB;

            let buf_size = (self.resolution.width * self.resolution.height * 4) as usize;
            let mut buffer = vec![0u8; buf_size];

            let lines = GetDIBits(
                hdc_mem,
                hbm,
                0,
                self.resolution.height,
                buffer.as_mut_ptr().cast(),
                &mut bmi,
                DIB_RGB_COLORS,
            );

            SelectObject(hdc_mem, old_obj);
            DeleteObject(hbm);
            DeleteDC(hdc_mem);
            ReleaseDC(0, hdc_screen);

            if lines == 0 {
                return Err(CaptureError::CaptureFailed("GetDIBits failed".into()));
            }

            self.frame_count += 1;

            Ok(Some(FrameBuffer {
                data: buffer,
                pixel_format: PixelFormat::Bgra8,
                resolution: self.resolution,
                timestamp_us: self.frame_count * 16_667,
            }))
        }
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        self.active = false;
        self.target_display = None;
        Ok(())
    }
}
