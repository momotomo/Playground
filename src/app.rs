use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText};
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

use crate::canvas::{
    CanvasController, CanvasToolKind, CommittedDocumentEdit, DocumentEditMode, ToolSettings,
    color32_from_rgba, rgba_from_color32,
};
use crate::model::{
    AlignmentKind, DistributionKind, DocumentHistory, GuideAxis, LayerId, PaintDocument,
    PaintElement, RgbaColor, StackOrderCommand,
};
use crate::storage::{ExportedImage, LoadedDocument, SavedDocument, StorageError, StorageFacade};

const MIN_BRUSH_WIDTH: f32 = 1.0;
const MAX_BRUSH_WIDTH: f32 = 48.0;
const GRID_SPACING_PRESETS: [f32; 6] = [16.0, 24.0, 32.0, 48.0, 64.0, 96.0];
const GRID_SPACING_STEP: f32 = 8.0;

fn shortcut_undo() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Z)
}

fn shortcut_redo() -> KeyboardShortcut {
    KeyboardShortcut::new(
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
        Key::Z,
    )
}

fn shortcut_redo_alt() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Y)
}

fn shortcut_save() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::S)
}

fn shortcut_load() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::O)
}

fn shortcut_export_png() -> KeyboardShortcut {
    KeyboardShortcut::new(
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
        Key::E,
    )
}

fn shortcut_group() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::G)
}

fn shortcut_ungroup() -> KeyboardShortcut {
    KeyboardShortcut::new(
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
        Key::G,
    )
}

fn shortcut_zoom_in() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Plus)
}

fn shortcut_zoom_in_alt() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Equals)
}

fn shortcut_zoom_out() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Minus)
}

fn shortcut_reset_view() -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Num0)
}

#[derive(Clone)]
struct StatusMessage {
    kind: StatusKind,
    text: String,
}

#[derive(Clone, Copy)]
enum StatusKind {
    Info,
    Error,
}

impl StatusMessage {
    fn info(text: impl Into<String>) -> Self {
        Self {
            kind: StatusKind::Info,
            text: text.into(),
        }
    }

    fn error(text: impl Into<String>) -> Self {
        Self {
            kind: StatusKind::Error,
            text: text.into(),
        }
    }

    fn rich_text(&self) -> RichText {
        let color = match self.kind {
            StatusKind::Info => egui::Color32::from_gray(70),
            StatusKind::Error => egui::Color32::from_rgb(160, 40, 40),
        };

        RichText::new(&self.text).small().color(color)
    }
}

#[cfg(target_arch = "wasm32")]
struct PendingWebStorageTask {
    label: &'static str,
    slot: Rc<RefCell<Option<Result<WebStorageResult, StorageError>>>>,
}

#[cfg(target_arch = "wasm32")]
enum WebStorageResult {
    Saved(SavedDocument),
    Loaded(LoadedDocument),
    Exported(ExportedImage),
}

pub struct PaintApp {
    history: DocumentHistory,
    canvas: CanvasController,
    storage: StorageFacade,
    active_tool: CanvasToolKind,
    brush_color: RgbaColor,
    brush_width: f32,
    status_message: StatusMessage,
    document_name: String,
    saved_snapshot: PaintDocument,
    layer_name_draft: String,
    layer_name_draft_for: Option<LayerId>,
    #[cfg(target_arch = "wasm32")]
    pending_web_task: Option<PendingWebStorageTask>,
}

impl Default for PaintApp {
    fn default() -> Self {
        let storage = StorageFacade::new();
        let document = PaintDocument::default();
        Self {
            history: DocumentHistory::new(document.clone()),
            canvas: CanvasController::default(),
            storage,
            active_tool: CanvasToolKind::Brush,
            brush_color: RgbaColor::charcoal(),
            brush_width: 6.0,
            status_message: StatusMessage::info(
                "Ready. Shift+Click or drag a marquee to multi-select, then group, transform, or arrange elements.",
            ),
            document_name: storage.suggested_file_name().to_owned(),
            saved_snapshot: document,
            layer_name_draft: "Layer 1".to_owned(),
            layer_name_draft_for: Some(1),
            #[cfg(target_arch = "wasm32")]
            pending_web_task: None,
        }
    }
}

impl PaintApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        Self::default()
    }

    fn document(&self) -> &PaintDocument {
        self.history.current()
    }

    fn is_dirty(&self) -> bool {
        &self.saved_snapshot != self.document()
    }

    fn set_info(&mut self, text: impl Into<String>) {
        self.status_message = StatusMessage::info(text);
    }

    fn set_error(&mut self, text: impl Into<String>) {
        self.status_message = StatusMessage::error(text);
    }

    fn set_active_tool(&mut self, tool: CanvasToolKind, announce: bool) {
        self.active_tool = tool;
        if announce {
            self.set_info(format!("Switched to {}.", tool.label()));
        }
    }

    fn tool_settings(&self) -> ToolSettings {
        ToolSettings {
            tool: self.active_tool,
            color: self.brush_color,
            width: self.brush_width,
        }
    }

    fn sync_layer_name_draft(&mut self) {
        let Some((layer_id, layer_name)) = self
            .document()
            .active_layer()
            .map(|layer| (layer.id, layer.name.clone()))
        else {
            self.layer_name_draft_for = None;
            self.layer_name_draft.clear();
            return;
        };

        if self.layer_name_draft_for != Some(layer_id) {
            self.layer_name_draft_for = Some(layer_id);
            self.layer_name_draft = layer_name;
        }
    }

    fn show_file_summary(&self, ui: &mut egui::Ui) {
        let dirty_suffix = if self.is_dirty() { " *" } else { "" };
        ui.label(RichText::new("File").strong());
        ui.label(format!("{}{}", self.document_name, dirty_suffix));
    }

    fn show_tools(&mut self, ui: &mut egui::Ui) {
        ui.heading("Tools");
        ui.label("Select existing elements to edit, or pick a drawing tool and drag.");
        ui.add_space(8.0);

        for tool in [
            CanvasToolKind::Select,
            CanvasToolKind::Brush,
            CanvasToolKind::Rectangle,
            CanvasToolKind::Ellipse,
            CanvasToolKind::Line,
            CanvasToolKind::Eraser,
        ] {
            ui.selectable_value(&mut self.active_tool, tool, tool.label());
        }

        ui.add_space(12.0);
        ui.label("Color");
        let mut color = color32_from_rgba(self.brush_color);
        if ui.color_edit_button_srgba(&mut color).changed() {
            self.brush_color = rgba_from_color32(color);
            self.set_info("Drawing color updated.");
        }

        ui.add_space(12.0);
        ui.label("Stroke Width");
        if ui
            .add(egui::Slider::new(
                &mut self.brush_width,
                MIN_BRUSH_WIDTH..=MAX_BRUSH_WIDTH,
            ))
            .changed()
        {
            self.set_info(format!("Stroke width set to {:.1}px.", self.brush_width));
        }
        ui.label(format!("{:.1}px", self.brush_width));

        ui.separator();
        ui.label(RichText::new("Current Mode").strong());
        ui.small(tool_hint(self.active_tool));
        ui.small(self.canvas.selection_summary(self.document()));

        ui.separator();
        self.show_file_summary(ui);
        ui.add_space(8.0);
        ui.label(RichText::new("Canvas").strong());
        ui.label(format!(
            "{:.0} x {:.0}px",
            self.document().canvas_size.width,
            self.document().canvas_size.height
        ));
        ui.label(format!(
            "Elements: {} total / {} active layer",
            self.document().total_element_count(),
            self.document().element_count()
        ));
        ui.label(format!("Zoom: {}", self.canvas.zoom_label()));
        if let Some(active_layer) = self.document().active_layer() {
            ui.label(format!("Active Layer: {}", active_layer.name));
        }
        ui.small("Shapes: corner handles resize, round handle rotates");
        ui.small("Strokes: single and multi transforms use simple move / scale / rotate");
        ui.small("Multi-select: Shift + Click or drag a marquee");
        ui.small(
            "Multi-edit: move, group, resize / rotate, align, distribute, and change stack order",
        );
        ui.small("Pan: Space + Drag or Middle Drag");
        ui.small("Reset view: Ctrl/Cmd + 0");
        ui.small("Tips: Shift+Click multi-select, drag guides, Ctrl/Cmd+Wheel zoom");

        ui.separator();
        self.show_canvas_aids(ui);

        ui.separator();
        ui.label(RichText::new("Storage").strong());
        ui.small(self.storage.storage_strategy_summary());
        ui.small(self.storage.editable_format_label());
        ui.small(self.storage.planned_export_format());
        ui.small("Shortcuts: Undo, Redo, Save, Load, Export PNG, Group, Ungroup");
    }

    fn show_canvas_aids(&mut self, ui: &mut egui::Ui) {
        #[derive(Clone, Copy)]
        enum AidAction {
            ToggleRulersVisible,
            ToggleGridVisible,
            ToggleGridSnap,
            SetGridSpacing(f32),
            ToggleSmartGuidesVisible,
            ToggleGuidesVisible,
            ToggleGuidesSnap,
            AddGuide(GuideAxis),
            RemoveGuide(usize),
        }

        let has_canvas_interaction = self.canvas.has_active_interaction();
        let grid = self.document().grid();
        let rulers_visible = self.document().rulers().visible;
        let smart_guides_visible = self.document().smart_guides().visible;
        let guides_visible = self.document().guides().visible;
        let guides_snap = self.document().guides().snap_enabled;
        let guides: Vec<_> = self
            .document()
            .guides()
            .lines
            .iter()
            .copied()
            .enumerate()
            .collect();
        let mut pending_action = None;

        ui.label(RichText::new("Layout Aids").strong());
        ui.small(format!(
            "Rulers {} · Grid {:.0}px · Smart Guides {} · {} guide{}.",
            if rulers_visible { "on" } else { "off" },
            grid.spacing,
            if smart_guides_visible { "on" } else { "off" },
            guides.len(),
            if guides.len() == 1 { "" } else { "s" }
        ));
        ui.small("Move snapping supports grid, guides, and smart guides. Drag visible guides to reposition them.");

        ui.add_enabled_ui(!has_canvas_interaction, |ui| {
            ui.horizontal_wrapped(|ui| {
                let mut show_rulers = rulers_visible;
                if ui.checkbox(&mut show_rulers, "Show Rulers").changed() {
                    pending_action = Some(AidAction::ToggleRulersVisible);
                }

                let mut show_smart_guides = smart_guides_visible;
                if ui
                    .checkbox(&mut show_smart_guides, "Smart Guides")
                    .changed()
                {
                    pending_action = Some(AidAction::ToggleSmartGuidesVisible);
                }
            });

            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                let mut show_grid = grid.visible;
                if ui.checkbox(&mut show_grid, "Show Grid").changed() {
                    pending_action = Some(AidAction::ToggleGridVisible);
                }

                let mut snap_grid = grid.snap_enabled;
                if ui.checkbox(&mut snap_grid, "Snap to Grid").changed() {
                    pending_action = Some(AidAction::ToggleGridSnap);
                }

                ui.label(RichText::new(format!("{:.0}px", grid.spacing)).monospace());
                if ui.small_button("-").clicked() {
                    pending_action = Some(AidAction::SetGridSpacing(
                        (grid.spacing - GRID_SPACING_STEP).max(GRID_SPACING_STEP),
                    ));
                }
                if ui.small_button("+").clicked() {
                    pending_action =
                        Some(AidAction::SetGridSpacing(grid.spacing + GRID_SPACING_STEP));
                }
            });

            ui.horizontal_wrapped(|ui| {
                ui.small("Spacing presets:");
                for preset in GRID_SPACING_PRESETS {
                    let is_current = (grid.spacing - preset).abs() < 0.1;
                    if ui
                        .selectable_label(is_current, format!("{preset:.0}px"))
                        .clicked()
                    {
                        pending_action = Some(AidAction::SetGridSpacing(preset));
                    }
                }
            });

            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                let mut show_guides = guides_visible;
                if ui.checkbox(&mut show_guides, "Show Guides").changed() {
                    pending_action = Some(AidAction::ToggleGuidesVisible);
                }

                let mut snap_guides = guides_snap;
                if ui.checkbox(&mut snap_guides, "Snap to Guides").changed() {
                    pending_action = Some(AidAction::ToggleGuidesSnap);
                }

                ui.small(format!(
                    "{} guide{}",
                    guides.len(),
                    if guides.len() == 1 { "" } else { "s" }
                ));
            });

            ui.horizontal(|ui| {
                if ui.button("Add H Guide").clicked() {
                    pending_action = Some(AidAction::AddGuide(GuideAxis::Horizontal));
                }
                if ui.button("Add V Guide").clicked() {
                    pending_action = Some(AidAction::AddGuide(GuideAxis::Vertical));
                }
            });
        });

        if has_canvas_interaction {
            ui.small("Finish the current edit before changing grid or guide settings.");
        }

        if guides.is_empty() {
            ui.small("No guides yet. New guides use the selection center or canvas center.");
        } else {
            for (index, guide) in guides {
                ui.horizontal(|ui| {
                    ui.small(format!(
                        "{} {:.0}px",
                        match guide.axis {
                            GuideAxis::Horizontal => "H",
                            GuideAxis::Vertical => "V",
                        },
                        guide.position
                    ));
                    if ui
                        .add_enabled(!has_canvas_interaction, egui::Button::new("Remove"))
                        .clicked()
                    {
                        pending_action = Some(AidAction::RemoveGuide(index));
                    }
                });
            }
        }

        if let Some(action) = pending_action {
            match action {
                AidAction::ToggleRulersVisible => self.toggle_rulers_visibility(),
                AidAction::ToggleGridVisible => self.toggle_grid_visibility(),
                AidAction::ToggleGridSnap => self.toggle_grid_snap(),
                AidAction::SetGridSpacing(spacing) => self.set_grid_spacing(spacing),
                AidAction::ToggleSmartGuidesVisible => self.toggle_smart_guides_visibility(),
                AidAction::ToggleGuidesVisible => self.toggle_guides_visibility(),
                AidAction::ToggleGuidesSnap => self.toggle_guides_snap(),
                AidAction::AddGuide(axis) => self.add_guide(axis),
                AidAction::RemoveGuide(index) => self.remove_guide(index),
            }
        }
    }

    fn show_layers(&mut self, ui: &mut egui::Ui) {
        #[derive(Clone, Copy)]
        enum LayerAction {
            Add,
            DeleteActive,
            RenameActive,
            SetActive(LayerId),
            ToggleVisibility(LayerId),
            ToggleLocked(LayerId),
            MoveUp(LayerId),
            MoveDown(LayerId),
            MoveSelectionTo(LayerId),
            DuplicateSelectionTo(LayerId),
        }

        self.sync_layer_name_draft();

        let layer_count = self.document().layer_count();
        let active_layer_id = self.document().active_layer_id();
        let selection_count = self.canvas.selection_count();
        let has_canvas_interaction = self.canvas.has_active_interaction();
        let layers: Vec<_> = self
            .document()
            .layers()
            .iter()
            .enumerate()
            .map(|(index, layer)| {
                (
                    index,
                    layer.id,
                    layer.name.clone(),
                    layer.visible,
                    layer.locked,
                    layer.elements.len(),
                )
            })
            .collect();
        let active_layer_state = self
            .document()
            .active_layer()
            .map(|layer| (layer.id, layer.name.clone(), layer.visible, layer.locked));
        let mut pending_action = None;

        ui.heading("Layers");
        ui.small("Selection and drawing target the active visible, unlocked layer.");
        ui.small("Move / Duplicate sends the current active-layer selection to a visible, unlocked destination.");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui.button("Add Layer").clicked() {
                pending_action = Some(LayerAction::Add);
            }

            if ui
                .add_enabled(layer_count > 1, egui::Button::new("Delete Layer"))
                .clicked()
            {
                pending_action = Some(LayerAction::DeleteActive);
            }
        });

        if selection_count > 0 {
            ui.small(format!(
                "{selection_count} selected on the active layer. Use Move Here / Duplicate Here on a destination layer."
            ));
        } else {
            ui.small(
                "Select elements on the active layer to move or duplicate them into another layer.",
            );
        }

        ui.separator();

        let total_layers = layers.len();
        for (index, layer_id, name, visible, locked, element_count) in layers.into_iter().rev() {
            let is_active = layer_id == active_layer_id;
            let can_receive_selection =
                selection_count > 0 && !has_canvas_interaction && layer_id != active_layer_id;
            let active_fill = ui.visuals().selection.bg_fill.linear_multiply(0.14);
            let frame = egui::Frame::group(ui.style())
                .fill(if is_active {
                    active_fill
                } else {
                    ui.visuals().faint_bg_color
                })
                .stroke(if is_active {
                    ui.visuals().selection.stroke
                } else {
                    ui.visuals().widgets.noninteractive.bg_stroke
                });

            frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(
                            is_active,
                            RichText::new(name.as_str()).strong().size(if is_active {
                                15.0
                            } else {
                                14.0
                            }),
                        )
                        .clicked()
                    {
                        pending_action = Some(LayerAction::SetActive(layer_id));
                    }

                    if ui
                        .small_button(if visible { "Hide" } else { "Show" })
                        .clicked()
                    {
                        pending_action = Some(LayerAction::ToggleVisibility(layer_id));
                    }

                    if ui
                        .small_button(if locked { "Unlock" } else { "Lock" })
                        .clicked()
                    {
                        pending_action = Some(LayerAction::ToggleLocked(layer_id));
                    }
                });

                ui.horizontal(|ui| {
                    if is_active {
                        ui.label(RichText::new("ACTIVE").small().strong());
                    }
                    if !visible {
                        ui.label(RichText::new("HIDDEN").small());
                    }
                    if locked {
                        ui.label(RichText::new("LOCKED").small());
                    }
                    ui.small(format!("{element_count} elements"));
                    if ui.small_button("Up").clicked() {
                        pending_action = Some(LayerAction::MoveUp(layer_id));
                    }
                    if ui.small_button("Down").clicked() {
                        pending_action = Some(LayerAction::MoveDown(layer_id));
                    }
                });

                if can_receive_selection {
                    ui.horizontal_wrapped(|ui| {
                        let can_drop_here = visible && !locked;
                        if ui
                            .add_enabled(can_drop_here, egui::Button::new("Move Here"))
                            .clicked()
                        {
                            pending_action = Some(LayerAction::MoveSelectionTo(layer_id));
                        }
                        if ui
                            .add_enabled(can_drop_here, egui::Button::new("Duplicate Here"))
                            .clicked()
                        {
                            pending_action = Some(LayerAction::DuplicateSelectionTo(layer_id));
                        }
                    });
                }

                if index + 1 == total_layers {
                    ui.small("Topmost");
                } else if index == 0 {
                    ui.small("Bottom");
                }
            });
            ui.add_space(4.0);
        }

        ui.separator();
        ui.label(RichText::new("Rename Active").strong());
        let rename_response = ui.text_edit_singleline(&mut self.layer_name_draft);
        let rename_on_enter =
            rename_response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
        if ui.button("Rename Layer").clicked() || rename_on_enter {
            pending_action = Some(LayerAction::RenameActive);
        }

        if let Some((_, active_name, visible, locked)) = active_layer_state {
            if !visible {
                ui.small(format!(
                    "{active_name} is hidden. It will not render or export."
                ));
            } else if locked {
                ui.small(format!(
                    "{active_name} is locked. It renders but cannot be selected or edited."
                ));
            }
        }

        if let Some(action) = pending_action {
            match action {
                LayerAction::Add => self.add_layer(),
                LayerAction::DeleteActive => self.delete_active_layer(),
                LayerAction::RenameActive => self.rename_active_layer(),
                LayerAction::SetActive(layer_id) => self.set_active_layer(layer_id),
                LayerAction::ToggleVisibility(layer_id) => self.toggle_layer_visibility(layer_id),
                LayerAction::ToggleLocked(layer_id) => self.toggle_layer_locked(layer_id),
                LayerAction::MoveUp(layer_id) => self.move_layer_up(layer_id),
                LayerAction::MoveDown(layer_id) => self.move_layer_down(layer_id),
                LayerAction::MoveSelectionTo(layer_id) => self.move_selection_to_layer(layer_id),
                LayerAction::DuplicateSelectionTo(layer_id) => {
                    self.duplicate_selection_to_layer(layer_id);
                }
            }
        }
    }

    fn show_actions(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let has_canvas_interaction = self.canvas.has_active_interaction();
        let selection_count = self.canvas.selection_count();
        let can_undo = has_canvas_interaction || self.history.can_undo();
        let can_redo = !has_canvas_interaction && self.history.can_redo();
        let can_clear = has_canvas_interaction || self.document().has_elements();
        let can_file_io = !has_canvas_interaction && !self.has_pending_storage_task();
        let can_adjust_view = !has_canvas_interaction;
        let can_reorder = !has_canvas_interaction && selection_count >= 1;
        let can_align = !has_canvas_interaction && selection_count >= 2;
        let can_group = !has_canvas_interaction && selection_count >= 2;
        let can_ungroup =
            !has_canvas_interaction && self.canvas.selection_contains_group(self.document());
        let can_distribute = !has_canvas_interaction && selection_count >= 3;

        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(can_undo, egui::Button::new("Undo"))
                .clicked()
            {
                self.perform_undo();
            }

            if ui
                .add_enabled(can_redo, egui::Button::new("Redo"))
                .clicked()
            {
                self.perform_redo();
            }

            if ui
                .add_enabled(can_clear, egui::Button::new("Clear"))
                .clicked()
            {
                self.perform_clear();
            }

            ui.separator();

            if ui
                .add_enabled(can_file_io, egui::Button::new("Save"))
                .clicked()
            {
                self.save_document(ctx);
            }

            if ui
                .add_enabled(can_file_io, egui::Button::new("Load"))
                .clicked()
            {
                self.load_document(ctx);
            }

            if ui
                .add_enabled(can_file_io, egui::Button::new("Export PNG"))
                .clicked()
            {
                self.export_png(ctx);
            }

            ui.separator();

            ui.add_enabled_ui(can_align, |ui| {
                ui.menu_button("Align", |ui| {
                    for alignment in [
                        AlignmentKind::Left,
                        AlignmentKind::HorizontalCenter,
                        AlignmentKind::Right,
                        AlignmentKind::Top,
                        AlignmentKind::VerticalCenter,
                        AlignmentKind::Bottom,
                    ] {
                        if ui.button(alignment.label()).clicked() {
                            self.apply_alignment(alignment);
                        }
                    }
                });
            });

            if ui
                .add_enabled(can_group, egui::Button::new("Group"))
                .clicked()
            {
                self.apply_group();
            }

            if ui
                .add_enabled(can_ungroup, egui::Button::new("Ungroup"))
                .clicked()
            {
                self.apply_ungroup();
            }

            ui.add_enabled_ui(can_distribute, |ui| {
                ui.menu_button("Distribute", |ui| {
                    for distribution in [DistributionKind::Horizontal, DistributionKind::Vertical] {
                        if ui.button(distribution.label()).clicked() {
                            self.apply_distribution(distribution);
                        }
                    }
                });
            });

            ui.add_enabled_ui(can_reorder, |ui| {
                ui.menu_button("Order", |ui| {
                    for command in [
                        StackOrderCommand::BringToFront,
                        StackOrderCommand::BringForward,
                        StackOrderCommand::SendBackward,
                        StackOrderCommand::SendToBack,
                    ] {
                        if ui.button(command.label()).clicked() {
                            self.apply_stack_order(command);
                        }
                    }
                });
            });

            ui.separator();

            if ui
                .add_enabled(can_adjust_view, egui::Button::new("-"))
                .clicked()
            {
                self.zoom_out();
            }

            ui.label(RichText::new(self.canvas.zoom_label()).monospace());

            if ui
                .add_enabled(can_adjust_view, egui::Button::new("+"))
                .clicked()
            {
                self.zoom_in();
            }

            if ui
                .add_enabled(can_adjust_view, egui::Button::new("Reset View"))
                .clicked()
            {
                self.reset_view();
            }

            ui.separator();
            ui.label(self.status_message.rich_text());
        });
    }

    fn perform_undo(&mut self) {
        if self.canvas.discard_active_interaction() {
            self.set_info("Discarded the in-progress edit.");
        } else if self.history.undo() {
            self.canvas.clear_selection();
            self.sync_layer_name_draft();
            self.set_info("Undid the last change.");
        }
    }

    fn perform_redo(&mut self) {
        if self.history.redo() {
            self.canvas.clear_selection();
            self.sync_layer_name_draft();
            self.set_info("Redid the last undone change.");
        }
    }

    fn perform_clear(&mut self) {
        let discarded = self.canvas.discard_active_interaction();
        if self.history.clear() {
            self.canvas.clear_selection();
            self.sync_layer_name_draft();
            self.set_info("Cleared the canvas.");
        } else if discarded {
            self.set_info("Discarded the in-progress edit.");
        }
    }

    fn zoom_in(&mut self) {
        let canvas_size = self.document().canvas_size;
        if self.canvas.zoom_in(canvas_size) {
            self.set_info(format!("Zoomed in to {}.", self.canvas.zoom_label()));
        }
    }

    fn zoom_out(&mut self) {
        let canvas_size = self.document().canvas_size;
        if self.canvas.zoom_out(canvas_size) {
            self.set_info(format!("Zoomed out to {}.", self.canvas.zoom_label()));
        }
    }

    fn reset_view(&mut self) {
        let canvas_size = self.document().canvas_size;
        self.canvas.request_view_reset();
        let _ = self.canvas.reset_view(canvas_size);
        self.set_info("Reset the view to fit the canvas.");
    }

    fn commit_element(&mut self, element: PaintElement) {
        let label = element.kind_label().to_owned();
        if self.history.commit_element(element) {
            self.canvas.clear_selection();
            self.set_info(format!("Added {label}."));
        }
    }

    fn apply_document_edit(&mut self, edit: CommittedDocumentEdit) {
        let selection_layer_id = edit.document.active_layer_id();
        if self.history.replace_document(edit.document) {
            self.canvas
                .set_selection_indices(selection_layer_id, edit.selection_indices);
            let message = match edit.mode {
                DocumentEditMode::Move => {
                    if self.canvas.selection_count() > 1 {
                        "Moved the selected elements."
                    } else {
                        "Moved the selected element."
                    }
                }
                DocumentEditMode::Resize => {
                    if self.canvas.selection_count() > 1 {
                        "Resized the selected elements."
                    } else {
                        "Resized the selected shape."
                    }
                }
                DocumentEditMode::Rotate => {
                    if self.canvas.selection_count() > 1 {
                        "Rotated the selected elements."
                    } else {
                        "Rotated the selected shape."
                    }
                }
                DocumentEditMode::Guide => "Moved the guide.",
                DocumentEditMode::Group => "Grouped the selected elements.",
                DocumentEditMode::Ungroup => "Ungrouped the selected elements.",
                DocumentEditMode::Align(alignment) => match alignment {
                    AlignmentKind::Left => "Aligned the selection to the left edge.",
                    AlignmentKind::HorizontalCenter => {
                        "Aligned the selection to the horizontal center."
                    }
                    AlignmentKind::Right => "Aligned the selection to the right edge.",
                    AlignmentKind::Top => "Aligned the selection to the top edge.",
                    AlignmentKind::VerticalCenter => {
                        "Aligned the selection to the vertical center."
                    }
                    AlignmentKind::Bottom => "Aligned the selection to the bottom edge.",
                },
                DocumentEditMode::Distribute(distribution) => match distribution {
                    DistributionKind::Horizontal => {
                        "Distributed the selection evenly across the horizontal axis."
                    }
                    DistributionKind::Vertical => {
                        "Distributed the selection evenly across the vertical axis."
                    }
                },
                DocumentEditMode::Reorder(command) => match command {
                    StackOrderCommand::BringToFront => "Moved the selection to the front.",
                    StackOrderCommand::SendToBack => "Moved the selection to the back.",
                    StackOrderCommand::BringForward => "Moved the selection forward.",
                    StackOrderCommand::SendBackward => "Moved the selection backward.",
                },
            };
            self.set_info(message);
            self.sync_layer_name_draft();
        }
    }

    fn apply_alignment(&mut self, alignment: AlignmentKind) {
        let document = self.document().clone();
        if let Some(edit) = self.canvas.apply_alignment(&document, alignment) {
            self.apply_document_edit(edit);
        }
    }

    fn apply_group(&mut self) {
        let document = self.document().clone();
        if let Some(edit) = self.canvas.apply_group(&document) {
            self.apply_document_edit(edit);
        }
    }

    fn apply_ungroup(&mut self) {
        let document = self.document().clone();
        if let Some(edit) = self.canvas.apply_ungroup(&document) {
            self.apply_document_edit(edit);
        }
    }

    fn apply_distribution(&mut self, distribution: DistributionKind) {
        let document = self.document().clone();
        if let Some(edit) = self.canvas.apply_distribution(&document, distribution) {
            self.apply_document_edit(edit);
        }
    }

    fn apply_stack_order(&mut self, command: StackOrderCommand) {
        let document = self.document().clone();
        if let Some(edit) = self.canvas.apply_stack_order(&document, command) {
            self.apply_document_edit(edit);
        }
    }

    fn apply_layer_document_change(&mut self, document: PaintDocument, message: impl Into<String>) {
        if self.history.replace_document(document) {
            self.canvas.discard_active_interaction();
            self.canvas.clear_selection();
            self.sync_layer_name_draft();
            self.set_info(message);
        }
    }

    fn apply_document_configuration_change(
        &mut self,
        document: PaintDocument,
        message: impl Into<String>,
    ) {
        if self.history.replace_document(document) {
            self.canvas.discard_active_interaction();
            self.sync_layer_name_draft();
            self.set_info(message);
        }
    }

    fn apply_layer_selection_document_change(
        &mut self,
        document: PaintDocument,
        selection_layer_id: LayerId,
        selection_indices: Vec<usize>,
        message: impl Into<String>,
    ) {
        if self.history.replace_document(document) {
            self.canvas.discard_active_interaction();
            self.canvas
                .set_selection_indices(selection_layer_id, selection_indices);
            self.sync_layer_name_draft();
            self.set_info(message);
        }
    }

    fn set_active_layer(&mut self, layer_id: LayerId) {
        if self.history.set_active_layer(layer_id) {
            self.canvas.discard_active_interaction();
            self.canvas.clear_selection();
            self.sync_layer_name_draft();
            if let Some(layer) = self.document().active_layer() {
                self.set_info(format!("Switched to {}.", layer.name));
            }
        }
    }

    fn add_layer(&mut self) {
        let document = self.document().clone();
        let (next, layer_id) = document.add_layer_document();
        let layer_name = next
            .layer(layer_id)
            .map(|layer| layer.name.clone())
            .unwrap_or_else(|| "New Layer".to_owned());
        self.apply_layer_document_change(next, format!("Added {layer_name}."));
    }

    fn delete_active_layer(&mut self) {
        let document = self.document().clone();
        if let Some((next, next_active)) = document.delete_active_layer_document() {
            let next_name = next
                .layer(next_active)
                .map(|layer| layer.name.clone())
                .unwrap_or_else(|| "remaining layer".to_owned());
            self.apply_layer_document_change(
                next,
                format!("Deleted the active layer. {next_name} is now active."),
            );
        }
    }

    fn rename_active_layer(&mut self) {
        let Some(active_layer) = self.document().active_layer() else {
            return;
        };
        let document = self.document().clone();
        if let Some(next) = document.renamed_layer_document(active_layer.id, &self.layer_name_draft)
        {
            self.apply_layer_document_change(
                next,
                format!("Renamed the layer to {}.", self.layer_name_draft.trim()),
            );
        }
    }

    fn toggle_layer_visibility(&mut self, layer_id: LayerId) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_layer_visibility_document(layer_id) {
            let state = next
                .layer(layer_id)
                .map(|layer| if layer.visible { "visible" } else { "hidden" })
                .unwrap_or("updated");
            self.apply_layer_document_change(next, format!("Set the layer to {state}."));
        }
    }

    fn toggle_layer_locked(&mut self, layer_id: LayerId) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_layer_locked_document(layer_id) {
            let state = next
                .layer(layer_id)
                .map(|layer| if layer.locked { "locked" } else { "unlocked" })
                .unwrap_or("updated");
            self.apply_layer_document_change(next, format!("Set the layer to {state}."));
        }
    }

    fn move_layer_up(&mut self, layer_id: LayerId) {
        let document = self.document().clone();
        if let Some(next) = document.moved_layer_up_document(layer_id) {
            self.apply_layer_document_change(next, "Moved the layer up.");
        }
    }

    fn move_layer_down(&mut self, layer_id: LayerId) {
        let document = self.document().clone();
        if let Some(next) = document.moved_layer_down_document(layer_id) {
            self.apply_layer_document_change(next, "Moved the layer down.");
        }
    }

    fn toggle_rulers_visibility(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_rulers_visibility_document() {
            let state = if next.rulers().visible {
                "visible"
            } else {
                "hidden"
            };
            self.apply_document_configuration_change(next, format!("Set rulers to {state}."));
        }
    }

    fn toggle_grid_visibility(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_grid_visibility_document() {
            let state = if next.grid().visible {
                "visible"
            } else {
                "hidden"
            };
            self.apply_document_configuration_change(next, format!("Set the grid to {state}."));
        }
    }

    fn toggle_grid_snap(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_grid_snap_document() {
            let state = if next.grid().snap_enabled {
                "enabled"
            } else {
                "disabled"
            };
            self.apply_document_configuration_change(next, format!("Grid snap {state}."));
        }
    }

    fn set_grid_spacing(&mut self, spacing: f32) {
        let document = self.document().clone();
        if let Some(next) = document.set_grid_spacing_document(spacing) {
            let applied_spacing = next.grid().spacing;
            self.apply_document_configuration_change(
                next,
                format!("Set grid spacing to {:.0}px.", applied_spacing),
            );
        }
    }

    fn toggle_smart_guides_visibility(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_smart_guides_visibility_document() {
            let state = if next.smart_guides().visible {
                "enabled"
            } else {
                "disabled"
            };
            self.apply_document_configuration_change(next, format!("Smart guides {state}."));
        }
    }

    fn toggle_guides_visibility(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_guides_visibility_document() {
            let state = if next.guides().visible {
                "visible"
            } else {
                "hidden"
            };
            self.apply_document_configuration_change(next, format!("Set guides to {state}."));
        }
    }

    fn toggle_guides_snap(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_guides_snap_document() {
            let state = if next.guides().snap_enabled {
                "enabled"
            } else {
                "disabled"
            };
            self.apply_document_configuration_change(next, format!("Guide snap {state}."));
        }
    }

    fn add_guide(&mut self, axis: GuideAxis) {
        let document = self.document().clone();
        let position = self.suggested_guide_position(axis);
        if let Some(next) = document.add_guide_document(axis, position) {
            self.apply_document_configuration_change(
                next,
                format!(
                    "Added {} guide at {:.0}px.",
                    axis.label().to_lowercase(),
                    position
                ),
            );
        }
    }

    fn remove_guide(&mut self, index: usize) {
        let document = self.document().clone();
        let Some(guide) = document.guides().lines.get(index).copied() else {
            return;
        };
        if let Some(next) = document.remove_guide_document(index) {
            self.apply_document_configuration_change(
                next,
                format!(
                    "Removed {} guide at {:.0}px.",
                    guide.axis.label().to_lowercase(),
                    guide.position
                ),
            );
        }
    }

    fn suggested_guide_position(&self, axis: GuideAxis) -> f32 {
        let selection_indices = self.canvas.selection_indices();
        let selection_center = self
            .document()
            .selection_bounds(selection_indices)
            .map(|bounds| bounds.center());
        match axis {
            GuideAxis::Horizontal => selection_center
                .map(|center| center.y)
                .unwrap_or(self.document().canvas_size.height * 0.5),
            GuideAxis::Vertical => selection_center
                .map(|center| center.x)
                .unwrap_or(self.document().canvas_size.width * 0.5),
        }
    }

    fn move_selection_to_layer(&mut self, layer_id: LayerId) {
        let document = self.document().clone();
        if self.canvas.selection_layer_id() != Some(document.active_layer_id()) {
            return;
        }
        let selection_indices = self.canvas.selection_indices().to_vec();
        if selection_indices.is_empty() {
            return;
        }

        let Some(destination) = document.layer(layer_id) else {
            return;
        };
        if !destination.is_editable() {
            self.set_error(format!(
                "{} must be visible and unlocked before it can receive moved elements.",
                destination.name
            ));
            return;
        }

        if let Some((next, next_selection)) =
            document.moved_selection_to_layer_document(&selection_indices, layer_id)
        {
            let moved_count = next_selection.len();
            let destination_name = next
                .layer(layer_id)
                .map(|layer| layer.name.clone())
                .unwrap_or_else(|| "destination layer".to_owned());
            let message = if moved_count == 1 {
                format!("Moved the selected element to {destination_name}.")
            } else {
                format!("Moved {moved_count} selected elements to {destination_name}.")
            };
            self.apply_layer_selection_document_change(next, layer_id, next_selection, message);
        }
    }

    fn duplicate_selection_to_layer(&mut self, layer_id: LayerId) {
        let document = self.document().clone();
        if self.canvas.selection_layer_id() != Some(document.active_layer_id()) {
            return;
        }
        let selection_indices = self.canvas.selection_indices().to_vec();
        if selection_indices.is_empty() {
            return;
        }

        let Some(destination) = document.layer(layer_id) else {
            return;
        };
        if !destination.is_editable() {
            self.set_error(format!(
                "{} must be visible and unlocked before it can receive duplicated elements.",
                destination.name
            ));
            return;
        }

        if let Some((next, next_selection)) =
            document.duplicated_selection_to_layer_document(&selection_indices, layer_id)
        {
            let duplicated_count = next_selection.len();
            let destination_name = next
                .layer(layer_id)
                .map(|layer| layer.name.clone())
                .unwrap_or_else(|| "destination layer".to_owned());
            let message = if duplicated_count == 1 {
                format!("Duplicated the selected element into {destination_name}.")
            } else {
                format!("Duplicated {duplicated_count} selected elements into {destination_name}.")
            };
            self.apply_layer_selection_document_change(next, layer_id, next_selection, message);
        }
    }

    fn finish_save(&mut self, saved: SavedDocument) {
        self.document_name = saved.file_name;
        self.saved_snapshot = self.document().clone();
        self.set_info(format!("Saved {}.", self.document_name));
    }

    fn finish_load(&mut self, loaded: LoadedDocument) {
        self.canvas.discard_active_interaction();
        self.canvas.clear_selection();
        self.canvas.request_view_reset();
        self.history.replace_document(loaded.document.clone());
        self.document_name = loaded.file_name;
        self.saved_snapshot = loaded.document;
        self.sync_layer_name_draft();
        self.set_info(format!("Loaded {}.", self.document_name));
    }

    fn finish_export(&mut self, exported: ExportedImage) {
        self.set_info(format!("Exported {}.", exported.file_name));
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn report_storage_result(&mut self, action: &'static str, result: Result<(), StorageError>) {
        if let Err(error) = result {
            match error {
                StorageError::Cancelled => {
                    self.set_info(format!("{} cancelled.", capitalize(action)));
                }
                other => {
                    self.set_error(format!("{} failed: {other}", capitalize(action)));
                }
            }
        }
    }

    fn save_document(&mut self, _ctx: &egui::Context) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let result = self
                .storage
                .save_document_via_dialog(self.document(), &self.document_name)
                .map(|saved| self.finish_save(saved));
            self.report_storage_result("save", result);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let slot = Rc::new(RefCell::new(None));
            let task_slot = slot.clone();
            let storage = self.storage;
            let document = self.document().clone();
            let suggested_name = self.document_name.clone();
            let ctx = _ctx.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let result = storage
                    .save_document_via_dialog(&document, &suggested_name)
                    .await
                    .map(WebStorageResult::Saved);
                *task_slot.borrow_mut() = Some(result);
                ctx.request_repaint();
            });

            self.pending_web_task = Some(PendingWebStorageTask {
                label: "save",
                slot,
            });
            self.set_info("Waiting for the browser save flow...");
        }
    }

    fn load_document(&mut self, _ctx: &egui::Context) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let result = self
                .storage
                .load_document_via_dialog()
                .map(|loaded| self.finish_load(loaded));
            self.report_storage_result("load", result);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let slot = Rc::new(RefCell::new(None));
            let task_slot = slot.clone();
            let storage = self.storage;
            let ctx = _ctx.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let result = storage
                    .load_document_via_dialog()
                    .await
                    .map(WebStorageResult::Loaded);
                *task_slot.borrow_mut() = Some(result);
                ctx.request_repaint();
            });

            self.pending_web_task = Some(PendingWebStorageTask {
                label: "load",
                slot,
            });
            self.set_info("Waiting for the browser file picker...");
        }
    }

    fn export_png(&mut self, _ctx: &egui::Context) {
        let suggested_name = self.storage.suggested_png_file_name(&self.document_name);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let result = self
                .storage
                .export_png_via_dialog(self.document(), &suggested_name)
                .map(|exported| self.finish_export(exported));
            self.report_storage_result("export", result);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let slot = Rc::new(RefCell::new(None));
            let task_slot = slot.clone();
            let storage = self.storage;
            let document = self.document().clone();
            let ctx = _ctx.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let result = storage
                    .export_png_via_dialog(&document, &suggested_name)
                    .await
                    .map(WebStorageResult::Exported);
                *task_slot.borrow_mut() = Some(result);
                ctx.request_repaint();
            });

            self.pending_web_task = Some(PendingWebStorageTask {
                label: "export",
                slot,
            });
            self.set_info("Preparing the PNG download...");
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let has_canvas_interaction = self.canvas.has_active_interaction();

        let redo_pressed = ctx.input_mut(|input| input.consume_shortcut(&shortcut_redo()))
            || ctx.input_mut(|input| input.consume_shortcut(&shortcut_redo_alt()));
        if redo_pressed && !has_canvas_interaction && self.history.can_redo() {
            self.perform_redo();
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_undo()))
            && (has_canvas_interaction || self.history.can_undo())
        {
            self.perform_undo();
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_save()))
            && !has_canvas_interaction
            && !self.has_pending_storage_task()
        {
            self.save_document(ctx);
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_load()))
            && !has_canvas_interaction
            && !self.has_pending_storage_task()
        {
            self.load_document(ctx);
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_export_png()))
            && !has_canvas_interaction
            && !self.has_pending_storage_task()
        {
            self.export_png(ctx);
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_group()))
            && !has_canvas_interaction
            && self.canvas.selection_count() >= 2
        {
            self.apply_group();
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_ungroup()))
            && !has_canvas_interaction
            && self.canvas.selection_contains_group(self.document())
        {
            self.apply_ungroup();
        }

        let zoom_in_pressed = ctx.input_mut(|input| input.consume_shortcut(&shortcut_zoom_in()))
            || ctx.input_mut(|input| input.consume_shortcut(&shortcut_zoom_in_alt()));
        if zoom_in_pressed && !has_canvas_interaction {
            self.zoom_in();
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_zoom_out()))
            && !has_canvas_interaction
        {
            self.zoom_out();
        }

        if ctx.input_mut(|input| input.consume_shortcut(&shortcut_reset_view()))
            && !has_canvas_interaction
        {
            self.reset_view();
        }

        if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
            && self.canvas.discard_active_interaction()
        {
            self.set_info("Cancelled the in-progress edit.");
        }

        if !has_canvas_interaction {
            self.handle_tool_shortcuts(ctx);
        }
    }

    fn handle_tool_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::V)) {
            self.set_active_tool(CanvasToolKind::Select, true);
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::B)) {
            self.set_active_tool(CanvasToolKind::Brush, true);
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::R)) {
            self.set_active_tool(CanvasToolKind::Rectangle, true);
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::O)) {
            self.set_active_tool(CanvasToolKind::Ellipse, true);
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::L)) {
            self.set_active_tool(CanvasToolKind::Line, true);
        } else if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::E)) {
            self.set_active_tool(CanvasToolKind::Eraser, true);
        }
    }

    fn has_pending_storage_task(&self) -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            self.pending_web_task.is_some()
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            false
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn poll_web_storage_task(&mut self) {
        let Some(task) = self.pending_web_task.take() else {
            return;
        };

        let maybe_result = task.slot.borrow_mut().take();
        if let Some(result) = maybe_result {
            match result {
                Ok(WebStorageResult::Saved(saved)) => self.finish_save(saved),
                Ok(WebStorageResult::Loaded(loaded)) => self.finish_load(loaded),
                Ok(WebStorageResult::Exported(exported)) => self.finish_export(exported),
                Err(StorageError::Cancelled) => {
                    self.set_info(format!("{} cancelled.", capitalize(task.label)));
                }
                Err(error) => {
                    self.set_error(format!("{} failed: {error}", capitalize(task.label)));
                }
            }
        } else {
            self.pending_web_task = Some(task);
        }
    }
}

impl eframe::App for PaintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(target_arch = "wasm32")]
        self.poll_web_storage_task();

        self.canvas.sync_with_document(self.history.current());
        self.handle_shortcuts(ctx);

        egui::SidePanel::left("tools_panel")
            .resizable(false)
            .default_width(220.0)
            .min_width(220.0)
            .show(ctx, |ui| self.show_tools(ui));

        egui::SidePanel::right("layers_panel")
            .resizable(false)
            .default_width(240.0)
            .min_width(240.0)
            .show(ctx, |ui| self.show_layers(ui));

        egui::TopBottomPanel::top("actions_panel")
            .resizable(false)
            .show(ctx, |ui| self.show_actions(ui, ctx));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(4.0);
            let tool_settings = self.tool_settings();
            let output = self.canvas.show(ui, self.history.current(), tool_settings);

            if output.needs_repaint {
                ctx.request_repaint();
            }

            if let Some(edit) = output.committed_edit {
                self.apply_document_edit(edit);
            }

            if let Some(element) = output.committed_element {
                self.commit_element(element);
            }
        });
    }
}

fn tool_hint(tool: CanvasToolKind) -> &'static str {
    match tool {
        CanvasToolKind::Select => {
            "Click to select on the active layer. Shift+Click adds or removes. Drag empty space for marquee select. Single shapes resize/rotate; multi-select can move, group-resize, rotate, align, and reorder."
        }
        CanvasToolKind::Brush => {
            "Freehand drawing tool. Drag to draw a stroke on the active layer."
        }
        CanvasToolKind::Eraser => {
            "Freehand eraser that paints with the canvas background on the active layer."
        }
        CanvasToolKind::Rectangle => {
            "Drag from one corner to the opposite corner on the active layer."
        }
        CanvasToolKind::Ellipse => {
            "Drag a bounding box to create an ellipse outline on the active layer."
        }
        CanvasToolKind::Line => "Drag from a start point to an end point on the active layer.",
    }
}

fn capitalize(action: &str) -> String {
    let mut chars = action.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };

    first.to_uppercase().chain(chars).collect()
}
