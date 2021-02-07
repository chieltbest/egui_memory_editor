use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use egui::{Align, Color32, CtxRef, FontDefinitions, Label, Layout, Pos2, Rect, TextStyle, Ui, Vec2, Window};
use num::Integer;

use crate::egui_utilities::*;
use crate::option_data::{BetweenFrameUiData, MemoryEditorOptions};

mod egui_utilities;
mod list_clipper;
pub mod option_data;

/// Reads a value present at the provided address in the object `T`.
///
/// # Arguments
///
/// - `&mut T`: the object on which the read should be performed.
/// - `usize`: The address of the read.
pub type ReadFunction<T> = fn(&mut T, usize) -> u8;
/// Writes the changes the user made to the `T` object.
///
/// # Arguments
///
/// - `&mut T`: the object whose state is to be updated.
/// - `usize`: The address of the intended write.
/// - `u8`: The value set by the user for the provided address.
pub type WriteFunction<T> = fn(&mut T, usize, u8);

pub struct MemoryEditor<T> {
    /// The name of the `egui` window, can be left blank.
    window_name: String,
    /// The function used for getting the values out of the provided type `T` and displaying them.
    read_function: ReadFunction<T>,
    /// The function used when attempts are made to change values within the GUI.
    write_function: Option<WriteFunction<T>>,
    /// The range of possible values to be displayed, the GUI will start at the lower bound and go up to the upper bound.
    ///
    /// Note this *currently* only supports a range that has a max of `2^24`, due to `ScrollArea` limitations.
    address_ranges: BTreeMap<String, Range<usize>>,
    /// When `true` will disallow any edits, ensuring the `write_function` will never be called.
    /// The latter therefore doesn't need to be set.
    read_only: bool,
    /// A collection of options relevant for the `MemoryEditor` window.
    /// Can optionally be serialized/deserialized with `serde`
    pub options: MemoryEditorOptions,
    /// Data for layout between frames, rather hacky.
    frame_data: BetweenFrameUiData,
}

impl<T> MemoryEditor<T> {
    pub fn new(read_function: ReadFunction<T>) -> Self {
        MemoryEditor {
            window_name: "Memory Editor".to_string(),
            read_function,
            write_function: None,
            address_ranges: BTreeMap::new(),
            read_only: false,
            options: Default::default(),
            frame_data: Default::default(),
        }
    }

    /// Create a window and render the memory editor contents within.
    ///
    /// If you want to make your own window/container to be used for the editor contents, you can use `draw_viewer_contents()`.
    pub fn window_ui(&mut self, ctx: &CtxRef, memory: &mut T) {
        let mut is_open = self.options.is_open;

        Window::new(self.window_name.clone())
            .open(&mut is_open)
            .scroll(false)
            .resizable(true)
            .show(ctx, |ui| {
                self.shrink_window_ui(ui);
                self.draw_viewer_contents(ui, memory);
            });

        self.options.is_open = is_open;
    }

    /// Draws the actual memory viewer/editor.
    ///
    /// Can be included in whatever container you want.
    ///
    /// Use `window_ui()` if you want to have a window with the contents instead.
    pub fn draw_viewer_contents(&mut self, ui: &mut Ui, memory: &mut T) {
        assert!(self.address_ranges.len() > 0, "At least one address range needs to be added to render the contents!");
        assert!(self.write_function.is_some() || self.read_only, "The write function needs to be set if not in read only mode!");

        self.draw_options_area(ui);

        ui.separator();

        let line_height = self.get_line_height(ui);

        let Self {
            options,
            read_function,
            address_ranges,
            frame_data,
            ..
        } = self;

        let MemoryEditorOptions {
            data_preview_options,
            show_ascii_sidebar,
            show_zero_colour,
            zero_colour,
            column_count,
            address_text_colour,
            selected_address_range,
            memory_editor_address_text_style,
            memory_editor_ascii_text_style,
            memory_editor_text_style,
            ..
        } = options;

        let address_space = address_ranges.get(selected_address_range).unwrap().clone();
        // This is janky, but can't think of a better way.
        let address_characters = format!("{:X}", address_space.end).chars().count();
        // Memory Editor Part.
        let max_lines = address_space.len().div_ceil(column_count);

        list_clipper::ClippedScrollArea::auto_sized(max_lines, line_height).show(ui, |ui, line_range| {
            // Memory values and addresses
            egui::Grid::new("mem_edit_grid")
                .striped(true)
                .spacing(Vec2::new(15.0, ui.style().spacing.item_spacing.y))
                .show(ui, |mut ui| {
                    ui.style_mut().spacing.item_spacing.x = 3.0;

                    for start_row in line_range.clone() {
                        let start_address = address_space.start + (start_row * *column_count);
                        ui.add(Label::new(format!("0x{:01$X}", start_address, address_characters)).text_color(*address_text_colour).text_style(*memory_editor_address_text_style));

                        // Render the memory values
                        for grid_column in 0..column_count.div_ceil(&8) {
                            ui.columns((*column_count - 8 * grid_column).min(8), |columns| {
                                let start_address = start_address + 8 * grid_column;
                                for (i, column) in columns.iter_mut().enumerate() {
                                    let memory_address = start_address + i;

                                    if !address_space.contains(&memory_address) {
                                        break;
                                    }

                                    let mem_val: u8 = read_function(memory, memory_address);

                                    let text_colour = if *show_zero_colour && mem_val == 0{
                                        *zero_colour
                                    } else {
                                        column.style().visuals.text_color()
                                    };

                                    column.add(Label::new(format!("{:02X}", mem_val)).text_color(text_colour).text_style(*memory_editor_text_style));
                                }
                            });
                        }

                        // Optional ASCII side
                        if *show_ascii_sidebar {
                            // Not pretty atm, needs a better method: TODO
                            ui.horizontal(|ui| {
                                ui.add(egui::Separator::new().vertical().spacing(3.0));
                                ui.style_mut().spacing.item_spacing.x = 0.0;
                                ui.columns(*column_count, |columns| {
                                    for (i, column) in columns.iter_mut().enumerate() {
                                        let memory_address = start_address + i;
                                        if !address_space.contains(&memory_address) {
                                            break;
                                        }

                                        let mem_val: u8 = read_function(memory, memory_address);
                                        let character = if mem_val < 32 || mem_val >= 128 { '.' } else { mem_val as char };
                                        column.add(egui::Label::new(character).text_style(*memory_editor_ascii_text_style));
                                    }
                                });
                            });
                        }

                        ui.end_row();
                    }
                });

            // After we've drawn the area we want to resize to we want to save this size for the next frame.
            frame_data.previous_frame_editor_width = ui.min_rect().width();
        });
    }

    fn draw_options_area(&mut self, ui: &mut Ui) {
        let Self {
            options,
            address_ranges,
            ..
        } = self;

        let MemoryEditorOptions {
            data_preview_options,
            show_ascii_sidebar,
            show_zero_colour,
            zero_colour,
            column_count,
            address_text_colour,
            memory_editor_text_style,
            combo_box_enabled,
            selected_address_range: combo_box_value_selected,
            ..
        } = options;

        egui::CollapsingHeader::new("Options")
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new("options_grid").show(ui, |ui| {
                    // Memory region selection
                    if *combo_box_enabled {
                        egui::combo_box_with_label(ui, "Memory Region", combo_box_value_selected.clone(), |ui| {
                            address_ranges.iter().for_each(|(range_name, _)| {
                                ui.selectable_value(combo_box_value_selected, range_name.clone(), range_name);
                            });
                        });
                    }

                    // Column dragger
                    let mut columns = *column_count as u8;
                    ui.add(egui::DragValue::u8(&mut columns).range(1.0..=64.0).prefix("Columns: ").speed(0.5));
                    *column_count = columns as usize;

                    ui.end_row();

                    // Checkboxes
                    ui.checkbox(show_ascii_sidebar, "Show ASCII")
                        .on_hover_text(format!("{} the ASCII representation view", if *show_ascii_sidebar { "Disable" } else { "Enable" }));
                    ui.checkbox(show_zero_colour, "Custom zero colour")
                        .on_hover_text("If enabled '0' will be coloured differently");
                });
            });
    }

    /// Return the line height for the current provided `Ui` and selected `TextStyle`s
    fn get_line_height(&self, ui: &mut Ui) -> f32 {
        let address_size = Label::new("##invisible").text_style(self.options.memory_editor_address_text_style).layout(ui).size.y;
        let body_size = Label::new("##invisible").text_style(self.options.memory_editor_text_style).layout(ui).size.y;
        let ascii_size = Label::new("##invisible").text_style(self.options.memory_editor_ascii_text_style).layout(ui).size.y;
        address_size.max(body_size).max(ascii_size) + ui.style().spacing.item_spacing.y
    }

    /// Shrink the window to the previous frame's memory viewer's width.
    /// This essentially allows us to only have height resize, and have width grow/shrink as appropriate.
    fn shrink_window_ui(&self, ui: &mut Ui) {
        ui.set_max_width(ui.min_rect().width().max(self.frame_data.previous_frame_editor_width));
    }

    /// Set the window title, only relevant if using the `window_ui()` call.
    pub fn with_window_title(mut self, title: impl Into<String>) -> Self {
        self.window_name = title.into();
        self
    }

    /// Set the function used to write to the provided object `T`.
    ///
    /// This function is only necessary if `read_only` is `false`.
    pub fn with_write_function(mut self, write_function: WriteFunction<T>) -> Self {
        self.write_function = Some(write_function);
        self
    }

    /// Add an address range to the range list.
    /// Multiple address ranges can be added, and will be displayed in the UI by a drop-down box if more than 1
    /// range was added.
    ///
    /// The first range that is added will be displayed by default when launching the UI.
    ///
    /// The UI will query your set `read_function` with the values within this `Range`
    pub fn with_address_range(mut self, range_name: impl Into<String>, address_range: Range<usize>) -> Self {
        self.address_ranges.insert(range_name.into(), address_range);
        self.options.combo_box_enabled = self.address_ranges.len() > 1;
        if let Some((name, _)) = self.address_ranges.iter().next() {
            self.options.selected_address_range = name.clone();
        }
        self
    }

    /// If set to `true` the UI will not allow any manual memory edits, and thus the `write_function` will never be called
    /// (and therefore doesn't need to be set).
    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Set the memory options, useful if you use the `persistence` feature.
    pub fn with_options(mut self, options: MemoryEditorOptions) -> Self {
        self.options = options;
        self
    }
}
