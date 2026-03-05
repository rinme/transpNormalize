//! remove_transparent_margins – CLI
//!
//! Removes transparent borders around visible content in PNG images.
//! Supports processing one or many files in a single invocation.
//!
//! Usage (single file):
//!   remove_transparent_margins input.png output_dir/ --threshold 10 --padding 2
//!
//! Usage (batch):
//!   remove_transparent_margins *.png --output-dir ./cropped/ --threshold 10

mod processing;

use clap::Parser;
use std::path::PathBuf;
use std::process;

/// Remove transparent margins from one or more PNG images.
#[derive(Parser, Debug)]
#[command(
    name = "remove_transparent_margins",
    about = "Removes transparent borders around visible content in PNG images",
    long_about = "Reads one or more PNGs with an alpha channel, finds the minimal bounding \
                  box of pixels whose alpha exceeds --threshold, optionally expands it by \
                  --padding pixels, and writes the cropped result(s).\n\n\
                  When --output-dir is not set, each output file is written next to its input \
                  with '_cropped' inserted before the extension (e.g. image.png → image_cropped.png)."
)]
struct Args {
    /// Input PNG file(s). Provide one or more paths for batch processing.
    #[arg(required = true, value_name = "INPUT")]
    inputs: Vec<PathBuf>,

    /// Directory for output files. Created automatically if it does not exist.
    /// When omitted, each output is placed next to its input with a '_cropped' suffix.
    #[arg(long, value_name = "DIR")]
    output_dir: Option<PathBuf>,

    /// Alpha threshold (0–255). Pixels with alpha > threshold are considered visible.
    #[arg(long, default_value_t = 10)]
    threshold: u8,

    /// Extra padding in pixels to add around the detected bounding box.
    #[arg(long, default_value_t = 0)]
    padding: u32,
}

/// Derives an output path for `input` given an optional `output_dir`.
/// - With `output_dir`: output_dir/<filename>
/// - Without: <stem>_cropped[.<ext>] next to the input file.
fn derive_output(input: &PathBuf, output_dir: &Option<PathBuf>) -> PathBuf {
    processing::derive_output_path(input, output_dir.as_deref())
}

fn run() -> Result<(), ()> {
    let args = Args::parse();

    // Create output directory if it was specified and does not yet exist.
    if let Some(ref dir) = args.output_dir {
        if !dir.exists() {
            std::fs::create_dir_all(dir).map_err(|e| {
                eprintln!("Error: cannot create output directory '{}': {}", dir.display(), e);
            })?;
        }
    }

    let mut had_error = false;
    for input in &args.inputs {
        let output = derive_output(input, &args.output_dir);
        match processing::process_image(input, &output, args.threshold, args.padding) {
            Ok(()) => println!("{} -> {}", input.display(), output.display()),
            Err(e) => {
                eprintln!("Error: {}", e);
                had_error = true;
            }
        }
    }

    if had_error { Err(()) } else { Ok(()) }
}

fn main() {
    if run().is_err() {
        process::exit(1);
    }
}

