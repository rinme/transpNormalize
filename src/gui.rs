//! egui application for mass-processing PNG files.
//!
//! Features:
//! - Drag-and-drop multiple PNG files onto the window.
//! - "Add Files" button opens a native file picker for multi-selection.
//! - Configurable alpha threshold and padding via sliders.
//! - Choice between same-directory output (with `_cropped` suffix) or a custom
//!   output directory selected through a native folder picker.
//! - "Process All" runs cropping in a background thread so the UI stays responsive.
//! - Per-file status: Pending / Processing / Done / Error.

use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::processing;

// ─── File status ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum FileStatus {
    Pending,
    Processing,
    Done { output: PathBuf },
    Error(String),
}

// ─── App state ───────────────────────────────────────────────────────────────

pub struct App {
    files: Vec<(PathBuf, FileStatus)>,

    // Settings
    threshold: u8,
    padding: u32,
    use_custom_output_dir: bool,
    output_dir: PathBuf,

    // Background processing channel
    sender: mpsc::Sender<(usize, FileStatus)>,
    receiver: mpsc::Receiver<(usize, FileStatus)>,
    processing: bool,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            files: Vec::new(),
            threshold: 10,
            padding: 0,
            use_custom_output_dir: false,
            output_dir: PathBuf::from("."),
            sender,
            receiver,
            processing: false,
        }
    }

    /// Adds paths that are not already in the list.
    fn enqueue(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        for path in paths {
            if !self.files.iter().any(|(p, _)| p == &path) {
                self.files.push((path, FileStatus::Pending));
            }
        }
    }

    /// Derives the output path for an input file based on the current settings.
    fn output_for(&self, input: &Path) -> PathBuf {
        let output_dir = if self.use_custom_output_dir {
            Some(self.output_dir.as_path())
        } else {
            None
        };
        crate::processing::derive_output_path(input, output_dir)
    }

    /// Spawns a background thread to process all pending files.
    fn start_processing(&mut self, ctx: egui::Context) {
        if self.processing {
            return;
        }

        // Collect (index, input_path, output_path) for every pending file.
        let jobs: Vec<(usize, PathBuf, PathBuf)> = self
            .files
            .iter()
            .enumerate()
            .filter(|(_, (_, status))| *status == FileStatus::Pending)
            .map(|(i, (path, _))| (i, path.clone(), self.output_for(path)))
            .collect();

        if jobs.is_empty() {
            return;
        }

        // Mark all about-to-run files as Processing in the UI.
        for (i, _, _) in &jobs {
            self.files[*i].1 = FileStatus::Processing;
        }
        self.processing = true;

        let threshold = self.threshold;
        let padding = self.padding;
        let sender = self.sender.clone();

        // Process files one-by-one in a background thread.
        // After each file, send the result back and request a UI repaint.
        std::thread::spawn(move || {
            for (i, input, output) in jobs {
                let status =
                    match processing::process_image(&input, &output, threshold, padding) {
                        Ok(()) => FileStatus::Done { output },
                        Err(e) => FileStatus::Error(e),
                    };
                let _ = sender.send((i, status));
                ctx.request_repaint();
            }
        });
    }
}

// ─── egui App trait ─────────────────────────────────────────────────────────

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ── 1. Collect results from the background thread ──────────────────
        while let Ok((i, status)) = self.receiver.try_recv() {
            if let Some(entry) = self.files.get_mut(i) {
                entry.1 = status;
            }
        }
        // Clear the "processing" flag when no file is still in flight.
        if self.processing && !self.files.iter().any(|(_, s)| *s == FileStatus::Processing) {
            self.processing = false;
        }

        // ── 2. Handle drag-and-drop ────────────────────────────────────────
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("png")).unwrap_or(false))
                .collect()
        });
        if !dropped.is_empty() {
            self.enqueue(dropped);
        }

        // Detect whether the user is currently hovering with files.
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());

        // ── 3. Settings panel (top) ────────────────────────────────────────
        egui::TopBottomPanel::top("settings_panel")
            .min_height(80.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Threshold:");
                    ui.add(
                        egui::Slider::new(&mut self.threshold, 0..=255)
                            .trailing_fill(true),
                    );
                    ui.separator();
                    ui.label("Padding:");
                    ui.add(egui::Slider::new(&mut self.padding, 0..=500).suffix(" px"));
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.radio_value(
                        &mut self.use_custom_output_dir,
                        false,
                        "Same directory (add _cropped suffix)",
                    );
                    ui.radio_value(
                        &mut self.use_custom_output_dir,
                        true,
                        "Custom output directory:",
                    );
                    if self.use_custom_output_dir {
                        ui.label(
                            egui::RichText::new(self.output_dir.display().to_string())
                                .monospace(),
                        );
                        if ui.button("Browse…").clicked() {
                            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                                self.output_dir = dir;
                            }
                        }
                    }
                });
                ui.add_space(4.0);
            });

        // ── 4. Action buttons (bottom) ────────────────────────────────────
        egui::TopBottomPanel::bottom("buttons_panel")
            .min_height(44.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    // Add Files button – opens a native multi-file picker.
                    if ui.button("➕  Add Files").clicked() {
                        if let Some(paths) = rfd::FileDialog::new()
                            .add_filter("PNG Images", &["png"])
                            .pick_files()
                        {
                            self.enqueue(paths);
                        }
                    }

                    if ui.button("✔ Clear Done").clicked() {
                        self.files.retain(|(_, s)| !matches!(s, FileStatus::Done { .. }));
                    }

                    let clear_all = ui
                        .add_enabled(!self.processing, egui::Button::new("🗑 Clear All"));
                    if clear_all.clicked() {
                        self.files.clear();
                    }

                    // "Process All" on the right-hand side.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let has_pending =
                            self.files.iter().any(|(_, s)| *s == FileStatus::Pending);
                        let btn = ui.add_enabled(
                            !self.processing && has_pending,
                            egui::Button::new("▶  Process All"),
                        );
                        if btn.clicked() {
                            self.start_processing(ctx.clone());
                        }
                        if self.processing {
                            ui.spinner();
                            ui.label("Processing…");
                        }
                    });
                });
            });

        // ── 5. Central panel: drop zone or file list ───────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.files.is_empty() {
                // Empty state / drop zone.
                let rect = ui.available_rect_before_wrap();
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new(if hovering {
                            "📂  Release to add files"
                        } else {
                            "Drop PNG files here\nor click ➕ Add Files"
                        })
                        .size(22.0)
                        .color(if hovering {
                            egui::Color32::LIGHT_BLUE
                        } else {
                            egui::Color32::from_gray(130)
                        }),
                    );
                });
                // Draw a dashed border when the user hovers with files.
                if hovering {
                    ui.painter().rect_stroke(
                        rect.shrink(8.0),
                        6.0,
                        egui::Stroke::new(2.0, egui::Color32::LIGHT_BLUE),
                        egui::StrokeKind::Middle,
                    );
                }
            } else {
                // File list.
                if hovering {
                    // Highlight the entire panel border while hovering.
                    let rect = ui.available_rect_before_wrap();
                    ui.painter().rect_stroke(
                        rect.shrink(4.0),
                        4.0,
                        egui::Stroke::new(2.0, egui::Color32::LIGHT_BLUE),
                        egui::StrokeKind::Middle,
                    );
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let mut remove_idx: Option<usize> = None;

                    for (i, (path, status)) in self.files.iter().enumerate() {
                        ui.horizontal(|ui| {
                            // File path (as much as fits).
                            ui.label(path.display().to_string());

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    // Remove button (disabled while processing).
                                    if !self.processing
                                        && ui.small_button("✕").clicked()
                                    {
                                        remove_idx = Some(i);
                                    }

                                    // Status badge.
                                    match status {
                                        FileStatus::Pending => {
                                            ui.label(
                                                egui::RichText::new("○ Pending")
                                                    .color(egui::Color32::GRAY),
                                            );
                                        }
                                        FileStatus::Processing => {
                                            ui.spinner();
                                            ui.label(
                                                egui::RichText::new("Processing…")
                                                    .color(egui::Color32::YELLOW),
                                            );
                                        }
                                        FileStatus::Done { output } => {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "✓  → {}",
                                                    output
                                                        .file_name()
                                                        .unwrap_or_default()
                                                        .to_string_lossy()
                                                ))
                                                .color(egui::Color32::GREEN),
                                            );
                                        }
                                        FileStatus::Error(msg) => {
                                            ui.label(
                                                egui::RichText::new(format!("✗  {}", msg))
                                                    .color(egui::Color32::RED),
                                            )
                                            .on_hover_text(msg);
                                        }
                                    }
                                },
                            );
                        });
                        ui.separator();
                    }

                    // Remove the file entry outside the borrow of self.files.
                    if let Some(i) = remove_idx {
                        self.files.remove(i);
                    }
                });
            }
        });
    }
}
