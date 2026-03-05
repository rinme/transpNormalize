//! remove_transparent_margins
//!
//! Removes transparent borders around visible content in PNG images.
//!
//! Usage:
//!   remove_transparent_margins <input> <output> [--threshold <u8>] [--padding <u32>]

use clap::Parser;
use image::{GenericImageView, ImageError, RgbaImage};
use std::path::PathBuf;
use std::process;

/// Remove transparent margins from a PNG image.
#[derive(Parser, Debug)]
#[command(
    name = "remove_transparent_margins",
    about = "Removes transparent borders around visible content in PNG images",
    long_about = "Reads a PNG with an alpha channel, finds the minimal bounding box of \
                  pixels whose alpha exceeds --threshold, optionally expands it by \
                  --padding pixels, and writes the cropped result."
)]
struct Args {
    /// Path to the input PNG file.
    input: PathBuf,

    /// Path for the output PNG file.
    output: PathBuf,

    /// Alpha threshold (0–255). Pixels with alpha > threshold are considered visible.
    #[arg(long, default_value_t = 10)]
    threshold: u8,

    /// Extra padding in pixels to add around the detected bounding box.
    #[arg(long, default_value_t = 0)]
    padding: u32,
}

/// Represents a bounding box as (left, top, right, bottom) pixel coordinates
/// where right and bottom are *exclusive* (one past the last included pixel).
struct BoundingBox {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

/// Computes the minimal bounding box of all pixels whose alpha channel value
/// is strictly greater than `threshold`.
///
/// The `image` crate exposes pixels via `pixels()` which iterates in row-major
/// order. Each pixel is an `Rgba<u8>` whose last component ([3]) is the alpha value.
///
/// Returns `None` if no visible pixel was found.
fn compute_bounding_box(img: &RgbaImage, threshold: u8) -> Option<BoundingBox> {
    let (width, height) = img.dimensions();

    let mut left = width;
    let mut top = height;
    let mut right = 0u32;
    let mut bottom = 0u32;

    // Iterate over every pixel to find the extremes of the visible region.
    for (x, y, pixel) in img.enumerate_pixels() {
        // pixel[3] is the alpha component of the Rgba pixel.
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
        // No visible pixel found.
        return None;
    }

    Some(BoundingBox { left, top, right, bottom })
}

/// Applies `padding` to the bounding box, clamping to the image dimensions so the
/// result never exceeds the original image boundaries.
fn expand_bounding_box(bbox: BoundingBox, padding: u32, img_width: u32, img_height: u32) -> BoundingBox {
    BoundingBox {
        left: bbox.left.saturating_sub(padding),
        top: bbox.top.saturating_sub(padding),
        right: (bbox.right + padding).min(img_width),
        bottom: (bbox.bottom + padding).min(img_height),
    }
}

fn run() -> Result<(), ImageError> {
    let args = Args::parse();

    // Load and decode the input file as RGBA8.
    // `image::open` returns a DynamicImage which we convert so that the alpha
    // channel is always present regardless of the original PNG colour type.
    let dynamic_img = image::open(&args.input).map_err(|e| {
        eprintln!(
            "Error: cannot open input file '{}': {}",
            args.input.display(),
            e
        );
        e
    })?;

    // Convert to RgbaImage so every pixel has an explicit alpha channel.
    let img: RgbaImage = dynamic_img.to_rgba8();
    let (width, height) = img.dimensions();

    // Compute the bounding box of visible content based on the alpha threshold.
    let bbox = match compute_bounding_box(&img, args.threshold) {
        Some(b) => b,
        None => {
            // No visible pixels — output the original image unchanged.
            eprintln!(
                "Warning: no visible pixels found (all alpha ≤ {}). Writing original image.",
                args.threshold
            );
            img.save(&args.output).map_err(|e| {
                eprintln!(
                    "Error: cannot write output file '{}': {}",
                    args.output.display(),
                    e
                );
                e
            })?;
            return Ok(());
        }
    };

    // Expand the bounding box by the requested padding, clamped to image borders.
    let bbox = expand_bounding_box(bbox, args.padding, width, height);

    // Crop the image to the (possibly padded) bounding box.
    // `view` returns a sub-image without copying pixel data; `to_image` materialises it.
    let crop_width = bbox.right - bbox.left;
    let crop_height = bbox.bottom - bbox.top;
    let cropped: RgbaImage = dynamic_img
        .view(bbox.left, bbox.top, crop_width, crop_height)
        .to_image();

    // Save the cropped image as PNG.
    // The alpha channel is preserved because we operate on RgbaImage throughout.
    cropped.save(&args.output).map_err(|e| {
        eprintln!(
            "Error: cannot write output file '{}': {}",
            args.output.display(),
            e
        );
        e
    })?;

    Ok(())
}

fn main() {
    if run().is_err() {
        process::exit(1);
    }
}
