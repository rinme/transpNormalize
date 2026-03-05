//! Core image processing logic: bounding-box detection and PNG cropping.
//!
//! This module is shared by the CLI (`main.rs`) and the GUI (`gui_main.rs`).

use image::{GenericImageView, RgbaImage};
use std::path::Path;

/// Bounding box with *exclusive* right and bottom coordinates.
pub struct BoundingBox {
    pub left: u32,
    pub top: u32,
    /// One past the last included column.
    pub right: u32,
    /// One past the last included row.
    pub bottom: u32,
}

/// Finds the minimal axis-aligned bounding box of all pixels whose alpha
/// component is strictly greater than `threshold`.
///
/// Alpha is accessed as `pixel[3]` from an `Rgba<u8>` image.
/// Returns `None` when every pixel is transparent (alpha ≤ threshold).
pub fn compute_bounding_box(img: &RgbaImage, threshold: u8) -> Option<BoundingBox> {
    let (width, height) = img.dimensions();
    let mut left = width;
    let mut top = height;
    let mut right = 0u32;
    let mut bottom = 0u32;

    for (x, y, pixel) in img.enumerate_pixels() {
        // pixel[3] is the alpha component of the RGBA pixel.
        if pixel[3] > threshold {
            if x < left {
                left = x;
            }
            if x + 1 > right {
                right = x + 1;
            }
            if y < top {
                top = y;
            }
            if y + 1 > bottom {
                bottom = y + 1;
            }
        }
    }

    if right == 0 || bottom == 0 {
        return None;
    }
    Some(BoundingBox { left, top, right, bottom })
}

/// Expands `bbox` by `padding` pixels on every side, clamped to the original
/// image dimensions so the result never exceeds the image borders.
pub fn expand_bounding_box(
    bbox: BoundingBox,
    padding: u32,
    img_width: u32,
    img_height: u32,
) -> BoundingBox {
    BoundingBox {
        left: bbox.left.saturating_sub(padding),
        top: bbox.top.saturating_sub(padding),
        right: (bbox.right + padding).min(img_width),
        bottom: (bbox.bottom + padding).min(img_height),
    }
}

/// Processes a single PNG image:
/// 1. Opens `input` and converts it to RGBA8.
/// 2. Computes the bounding box of visible pixels (alpha > `threshold`).
/// 3. Expands the bounding box by `padding` (clamped to image borders).
/// 4. Crops and saves the result as a PNG to `output`.
///
/// If no visible pixels are found, the original image is written unchanged.
/// Returns an error message string on failure.
pub fn process_image(
    input: &Path,
    output: &Path,
    threshold: u8,
    padding: u32,
) -> Result<(), String> {
    // Load and convert to RGBA8 so the alpha channel is always present.
    let dynamic_img = image::open(input)
        .map_err(|e| format!("Cannot open '{}': {}", input.display(), e))?;

    let img: RgbaImage = dynamic_img.to_rgba8();
    let (width, height) = img.dimensions();

    match compute_bounding_box(&img, threshold) {
        None => {
            // No visible pixels – write original image unchanged.
            img.save(output)
                .map_err(|e| format!("Cannot write '{}': {}", output.display(), e))?;
        }
        Some(bbox) => {
            // Expand the bounding box by the requested padding.
            let bbox = expand_bounding_box(bbox, padding, width, height);
            let crop_w = bbox.right - bbox.left;
            let crop_h = bbox.bottom - bbox.top;

            // `view` returns a sub-image without copying; `to_image` materialises it.
            let cropped: RgbaImage = dynamic_img
                .view(bbox.left, bbox.top, crop_w, crop_h)
                .to_image();

            // Save – alpha channel is preserved because we use RgbaImage throughout.
            cropped
                .save(output)
                .map_err(|e| format!("Cannot write '{}': {}", output.display(), e))?;
        }
    }

    Ok(())
}
