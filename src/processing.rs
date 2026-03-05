//! Core image processing logic: bounding-box detection and PNG cropping.
//!
//! This module is shared by the CLI (`main.rs`) and the GUI (`gui_main.rs`).

use image::{GenericImageView, RgbaImage};
use std::path::{Path, PathBuf};

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

/// Derives an output path for `input`:
///
/// - `output_dir = Some(dir)` → `dir/<filename>`
/// - `output_dir = None`      → `<stem>_cropped[.<ext>]` next to the input file.
///
/// When the input has no file extension the dot separator is omitted so that
/// `myfile` becomes `myfile_cropped` rather than `myfile_cropped.`.
pub fn derive_output_path(input: &Path, output_dir: Option<&Path>) -> PathBuf {
    match output_dir {
        Some(dir) => dir.join(input.file_name().unwrap_or_default()),
        None => {
            let stem = input.file_stem().unwrap_or_default().to_string_lossy();
            let ext = input.extension().unwrap_or_default().to_string_lossy();
            let filename = if ext.is_empty() {
                format!("{}_cropped", stem)
            } else {
                format!("{}_cropped.{}", stem, ext)
            };
            input.with_file_name(filename)
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use std::path::PathBuf;

    // ── helpers ──────────────────────────────────────────────────────────────

    /// Creates a solid `width × height` RGBA image where every pixel has the
    /// given alpha value and RGB components set to 128.
    fn solid_alpha(width: u32, height: u32, alpha: u8) -> RgbaImage {
        let mut img = RgbaImage::new(width, height);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([128, 128, 128, alpha]);
        }
        img
    }

    // ── compute_bounding_box ─────────────────────────────────────────────────

    #[test]
    fn bbox_all_transparent_returns_none() {
        let img = solid_alpha(10, 10, 0);
        assert!(compute_bounding_box(&img, 10).is_none());
    }

    #[test]
    fn bbox_threshold_excludes_equal_alpha() {
        // Every pixel has alpha == threshold; none should be "visible".
        let img = solid_alpha(5, 5, 10);
        assert!(compute_bounding_box(&img, 10).is_none());
    }

    #[test]
    fn bbox_single_visible_pixel() {
        let mut img = solid_alpha(10, 10, 0);
        img.put_pixel(3, 7, Rgba([255, 0, 0, 255]));
        let bbox = compute_bounding_box(&img, 10).expect("should find one pixel");
        assert_eq!(bbox.left, 3);
        assert_eq!(bbox.top, 7);
        assert_eq!(bbox.right, 4);   // exclusive
        assert_eq!(bbox.bottom, 8);  // exclusive
    }

    #[test]
    fn bbox_full_image_visible() {
        let img = solid_alpha(8, 6, 255);
        let bbox = compute_bounding_box(&img, 10).expect("all pixels visible");
        assert_eq!((bbox.left, bbox.top, bbox.right, bbox.bottom), (0, 0, 8, 6));
    }

    #[test]
    fn bbox_inner_rect() {
        // 10×10 image; only pixels in the rectangle [2,6)×[3,7) are visible.
        let mut img = solid_alpha(10, 10, 0);
        for y in 3..7 {
            for x in 2..6 {
                img.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
        let bbox = compute_bounding_box(&img, 0).unwrap();
        assert_eq!((bbox.left, bbox.top, bbox.right, bbox.bottom), (2, 3, 6, 7));
    }

    // ── expand_bounding_box ──────────────────────────────────────────────────

    #[test]
    fn expand_zero_padding_unchanged() {
        let bbox = BoundingBox { left: 2, top: 3, right: 6, bottom: 7 };
        let expanded = expand_bounding_box(bbox, 0, 10, 10);
        assert_eq!((expanded.left, expanded.top, expanded.right, expanded.bottom), (2, 3, 6, 7));
    }

    #[test]
    fn expand_clamps_to_image_borders() {
        // Padding of 100 should be clamped to image edges (10×10 image).
        let bbox = BoundingBox { left: 2, top: 2, right: 8, bottom: 8 };
        let expanded = expand_bounding_box(bbox, 100, 10, 10);
        assert_eq!((expanded.left, expanded.top), (0, 0));
        assert_eq!((expanded.right, expanded.bottom), (10, 10));
    }

    #[test]
    fn expand_fits_inside_image() {
        let bbox = BoundingBox { left: 3, top: 3, right: 7, bottom: 7 };
        let expanded = expand_bounding_box(bbox, 2, 10, 10);
        assert_eq!((expanded.left, expanded.top, expanded.right, expanded.bottom), (1, 1, 9, 9));
    }

    // ── derive_output_path ───────────────────────────────────────────────────

    #[test]
    fn derive_output_suffix_mode_with_extension() {
        let input = PathBuf::from("/some/dir/image.png");
        let out = derive_output_path(&input, None);
        assert_eq!(out, PathBuf::from("/some/dir/image_cropped.png"));
    }

    #[test]
    fn derive_output_suffix_mode_no_extension() {
        let input = PathBuf::from("/some/dir/myfile");
        let out = derive_output_path(&input, None);
        // Must NOT produce a trailing dot.
        assert_eq!(out, PathBuf::from("/some/dir/myfile_cropped"));
    }

    #[test]
    fn derive_output_custom_dir() {
        let input = PathBuf::from("/a/b/image.png");
        let dir = PathBuf::from("/out/dir");
        let out = derive_output_path(&input, Some(&dir));
        assert_eq!(out, PathBuf::from("/out/dir/image.png"));
    }
}
