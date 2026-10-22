//! Draws the tray indicator (layout letters on a colored rounded square) as
//! an `HICON`. What to draw is decided by `retype_core::indicator`.

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;

use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS, CreateBitmap,
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DIB_RGB_COLORS, DT_CENTER,
    DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, FF_DONTCARE,
    FW_BOLD, GdiFlush, HBITMAP, HDC, HGDIOBJ, OUT_DEFAULT_PRECIS, SelectObject, SetBkMode,
    SetTextColor, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, GetSystemMetrics, HICON, ICONINFO, SM_CXSMICON,
};
use windows::core::w;

use retype_core::indicator::{Indicator, Rgb, rounded_square_alpha};

/// `HGDI_ERROR`: `SelectObject`'s failure value for regions, `(HGDIOBJ)-1`.
const HGDI_ERROR: isize = -1;

/// An icon we created; destroyed on drop.
pub struct OwnedIcon(HICON);

impl OwnedIcon {
    pub fn handle(&self) -> HICON {
        self.0
    }
}

impl Drop for OwnedIcon {
    fn drop(&mut self) {
        // SAFETY: the icon was created by `render` and is destroyed once.
        if let Err(err) = unsafe { DestroyIcon(self.0) } {
            log::warn!("DestroyIcon: {err}");
        }
    }
}

/// Renders `indicator` at the small-icon size of the current DPI.
pub fn render(indicator: &Indicator) -> windows::core::Result<OwnedIcon> {
    // SAFETY: no preconditions.
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.clamp(16, 64);
    let pixels = usize::try_from(size * size).unwrap_or(0);

    // SAFETY: a memory DC compatible with the screen; deleted by the guard.
    let dc = unsafe { CreateCompatibleDC(None) };
    if dc.is_invalid() {
        return Err(windows::core::Error::from_thread());
    }
    let dc = Dc(dc);
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: u32::try_from(size_of::<BITMAPINFOHEADER>()).unwrap_or(0),
            biWidth: size,
            biHeight: -size, // top-down rows
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut c_void = ptr::null_mut();
    // SAFETY: `info` describes a 32-bit DIB; `bits` receives its pixel
    // memory, owned by the bitmap (freed with it by the guard).
    let color = Object(
        unsafe { CreateDIBSection(Some(dc.0), &info, DIB_RGB_COLORS, &mut bits, None, 0) }?.into(),
    );
    if bits.is_null() {
        return Err(windows::core::Error::from_thread());
    }
    let pixel_ptr = bits.cast::<[u8; 4]>();

    // Background. The slice ends before GDI draws into the same memory.
    {
        // SAFETY: the DIB holds `size`×`size` BGRA pixels at `bits` while the
        // bitmap lives; GDI is not drawing while this slice exists.
        let bgra = unsafe { std::slice::from_raw_parts_mut(pixel_ptr, pixels) };
        bgra.fill(bgra_of(indicator.background));
    }

    draw_label(&dc, color.0, size, indicator)?;

    // GDI clears alpha where it draws; set it from the rounded mask. Drawing
    // is finished (flushed in `draw_label`), so a fresh slice is sound.
    {
        // SAFETY: as above; GDI has flushed and no longer touches the pixels.
        let bgra = unsafe { std::slice::from_raw_parts_mut(pixel_ptr, pixels) };
        let radius = size as f32 / 5.0;
        for (pixel, alpha) in bgra
            .iter_mut()
            .zip(rounded_square_alpha(size as u32, radius))
        {
            pixel[3] = alpha;
        }
    }

    // SAFETY: a monochrome AND mask; ignored for 32-bit icons with alpha
    // but required by CreateIconIndirect. Zero-initialised by GDI.
    let mask = unsafe { CreateBitmap(size, size, 1, 1, None) };
    if mask.is_invalid() {
        return Err(windows::core::Error::from_thread());
    }
    let mask = Object(mask.into());
    let icon_info = ICONINFO {
        fIcon: true.into(),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: HBITMAP(mask.0.0),
        hbmColor: HBITMAP(color.0.0),
    };
    // SAFETY: both bitmaps are valid; the icon copies them, so the guards
    // may delete ours afterwards.
    unsafe { CreateIconIndirect(&icon_info) }.map(OwnedIcon)
}

/// Draws the label onto `bitmap` and flushes GDI, so the pixels may be read
/// afterwards. Leaves the DC as it found it.
fn draw_label(
    dc: &Dc,
    bitmap: HGDIOBJ,
    size: i32,
    indicator: &Indicator,
) -> windows::core::Result<()> {
    let scale = if indicator.label.chars().count() > 2 {
        0.46
    } else {
        0.62
    };
    // SAFETY: plain font creation from a static face name; deleted by the guard.
    let font = unsafe {
        CreateFontW(
            -((size as f32 * scale).round() as i32),
            0,
            0,
            0,
            FW_BOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            FF_DONTCARE.0.into(),
            w!("Segoe UI"),
        )
    };
    if font.is_invalid() {
        return Err(windows::core::Error::from_thread());
    }
    let font = Object(font.into());
    let old_bitmap = select(dc, bitmap)?;
    let drawn = select(dc, font.0).map(|old_font| {
        // SAFETY: `dc` is valid.
        unsafe { SetBkMode(dc.0, TRANSPARENT) };
        // SAFETY: `dc` is valid.
        unsafe { SetTextColor(dc.0, colorref_of(indicator.foreground)) };
        let mut text: Vec<u16> = indicator.label.encode_utf16().collect();
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: size,
            bottom: size,
        };
        // SAFETY: `text` and `rect` are valid for the call; DT_NOPREFIX keeps
        // `&` literal.
        let height = unsafe {
            DrawTextW(
                dc.0,
                &mut text,
                &mut rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            )
        };
        if height == 0 {
            log::warn!("tray indicator: DrawTextW drew nothing");
        }
        // SAFETY: finishes drawing before the pixels are read.
        if !unsafe { GdiFlush() }.as_bool() {
            log::warn!("tray indicator: GdiFlush failed");
        }
        // Put the previous font back before the guard deletes ours.
        let _ = select(dc, old_font);
    });
    // Put the previous bitmap back before the guard deletes ours.
    select(dc, old_bitmap)?;
    drawn
}

/// `SelectObject`, failing on a null or `HGDI_ERROR` result.
fn select(dc: &Dc, object: HGDIOBJ) -> windows::core::Result<HGDIOBJ> {
    // SAFETY: `dc` is a valid memory DC and `object` a live GDI object.
    let previous = unsafe { SelectObject(dc.0, object) };
    if previous.is_invalid() || previous.0 as isize == HGDI_ERROR {
        Err(windows::core::Error::from_thread())
    } else {
        Ok(previous)
    }
}

fn bgra_of(Rgb(r, g, b): Rgb) -> [u8; 4] {
    [b, g, r, 255]
}

fn colorref_of(Rgb(r, g, b): Rgb) -> COLORREF {
    COLORREF(u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16))
}

/// A memory DC, deleted on drop.
struct Dc(HDC);

impl Drop for Dc {
    fn drop(&mut self) {
        // SAFETY: created by CreateCompatibleDC and deleted once.
        if !unsafe { DeleteDC(self.0) }.as_bool() {
            log::warn!("DeleteDC failed");
        }
    }
}

/// A GDI object (bitmap, font), deleted on drop.
struct Object(HGDIOBJ);

impl Drop for Object {
    fn drop(&mut self) {
        // SAFETY: we created the object and it is not selected into a DC.
        if !unsafe { DeleteObject(self.0) }.as_bool() {
            log::warn!("DeleteObject failed");
        }
    }
}
