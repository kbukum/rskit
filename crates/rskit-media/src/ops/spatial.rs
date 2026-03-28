//! Spatial operations for video/image processing.

use crate::spatial::Resolution;

/// Resize operation.
#[derive(Debug, Clone)]
pub struct ResizeOp {
    /// Target resolution.
    pub resolution: Resolution,
    /// How to handle aspect ratio.
    pub mode: ResizeMode,
}

/// How to handle aspect ratio when resizing.
#[derive(Debug, Clone, Copy)]
pub enum ResizeMode {
    /// Stretch to exact dimensions (may distort).
    Exact,
    /// Scale down to fit within bounds (preserves aspect ratio, may have empty space).
    Fit,
    /// Scale up to fill bounds then crop (preserves aspect ratio, no empty space).
    Fill,
    /// Scale to fit the given width, adjusting height proportionally.
    FitWidth,
    /// Scale to fit the given height, adjusting width proportionally.
    FitHeight,
}

/// A rectangular region for cropping.
#[derive(Debug, Clone)]
pub struct CropRegion {
    /// X offset from top-left.
    pub x: u32,
    /// Y offset from top-left.
    pub y: u32,
    /// Width of the crop region.
    pub width: u32,
    /// Height of the crop region.
    pub height: u32,
}

impl CropRegion {
    /// Create a new crop region.
    pub fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Self {
            x,
            y,
            width: w,
            height: h,
        }
    }

    /// Create a centered crop with the given aspect ratio.
    pub fn center_aspect(source_res: Resolution, aspect_w: u32, aspect_h: u32) -> Self {
        let source_ar = source_res.width as f64 / source_res.height as f64;
        let target_ar = aspect_w as f64 / aspect_h as f64;

        let (w, h) = if source_ar > target_ar {
            let h = source_res.height;
            let w = (h as f64 * target_ar) as u32;
            (w, h)
        } else {
            let w = source_res.width;
            let h = (w as f64 / target_ar) as u32;
            (w, h)
        };

        let x = (source_res.width - w) / 2;
        let y = (source_res.height - h) / 2;
        Self::new(x, y, w, h)
    }

    /// Create a centered crop of the given dimensions.
    pub fn center(source_res: Resolution, w: u32, h: u32) -> Self {
        let x = source_res.width.saturating_sub(w) / 2;
        let y = source_res.height.saturating_sub(h) / 2;
        Self::new(x, y, w, h)
    }
}

/// Rotation angle.
#[derive(Debug, Clone, Copy)]
pub enum Rotation {
    /// 90° clockwise.
    Degrees90,
    /// 180°.
    Degrees180,
    /// 270° clockwise (= 90° counter-clockwise).
    Degrees270,
    /// Arbitrary rotation in degrees.
    Arbitrary(f64),
}

/// Flip direction.
#[derive(Debug, Clone, Copy)]
pub enum FlipDirection {
    /// Mirror horizontally.
    Horizontal,
    /// Mirror vertically.
    Vertical,
    /// Mirror both axes.
    Both,
}

/// Pad the video/image to a target size with a background color.
#[derive(Debug, Clone)]
pub struct PadOp {
    /// Target width.
    pub width: u32,
    /// Target height.
    pub height: u32,
    /// Background color (e.g., "black", "#FF0000").
    pub color: String,
}
