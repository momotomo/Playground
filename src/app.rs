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
    show_help: bool,
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
                "描く準備ができました。ブラシや図形ツールを選んで、必要ならヘルプを開いてください。",
            ),
            document_name: storage.suggested_file_name().to_owned(),
            saved_snapshot: document,
            layer_name_draft: "レイヤー 1".to_owned(),
            layer_name_draft_for: Some(1),
            show_help: false,
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
            self.set_info(format!("{} に切り替えました。", tool.label()));
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
        let dirty_suffix = if self.is_dirty() {
            "未保存の変更あり"
        } else {
            "保存済み"
        };
        ui.label(RichText::new("ドキュメント").strong());
        ui.label(self.document_name.as_str());
        ui.small(dirty_suffix);
    }

    fn show_tools(&mut self, ui: &mut egui::Ui) {
        ui.heading("ツール");
        ui.label("ツールを選んで、現在のレイヤーに描くか編集します。");
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
        ui.label("色");
        let mut color = color32_from_rgba(self.brush_color);
        if ui.color_edit_button_srgba(&mut color).changed() {
            self.brush_color = rgba_from_color32(color);
            self.set_info("描画色を変更しました。");
        }

        ui.add_space(12.0);
        ui.label("線幅");
        if ui
            .add(egui::Slider::new(
                &mut self.brush_width,
                MIN_BRUSH_WIDTH..=MAX_BRUSH_WIDTH,
            ))
            .changed()
        {
            self.set_info(format!("線幅を {:.1}px に変更しました。", self.brush_width));
        }
        ui.label(format!("{:.1}px", self.brush_width));

        ui.separator();
        ui.label(RichText::new("現在のモード").strong());
        ui.small(tool_hint(self.active_tool));
        ui.small(self.canvas.selection_summary(self.document()));

        ui.separator();
        self.show_file_summary(ui);
        ui.add_space(8.0);
        ui.label(RichText::new("キャンバス").strong());
        ui.label(format!(
            "{:.0} x {:.0}px",
            self.document().canvas_size.width,
            self.document().canvas_size.height
        ));
        ui.label(format!(
            "要素数: 全体 {} / 現在のレイヤー {}",
            self.document().total_element_count(),
            self.document().element_count()
        ));
        ui.label(format!("ズーム: {}", self.canvas.zoom_label()));
        if let Some(active_layer) = self.document().active_layer() {
            ui.label(format!("現在のレイヤー: {}", active_layer.name));
        }
        ui.small("図形: 角ハンドルでサイズ変更、丸いハンドルで回転します");
        ui.small("ストローク: 単体 / 複数変形では簡易的な移動 / 拡大縮小 / 回転を使います");
        ui.small("複数選択: Shift+Click またはドラッグ選択");
        ui.small("複数編集: 移動、グループ化、サイズ変更 / 回転、整列、等間隔、重なり順変更");
        ui.small("パン: Space+Drag または中ボタンドラッグ");
        ui.small("表示リセット: Ctrl/Cmd + 0");
        ui.small("ヒント: Shift+Click で複数選択、ガイドはドラッグで移動、Ctrl/Cmd+Wheel でズーム");

        ui.separator();
        self.show_canvas_aids(ui);

        ui.separator();
        ui.label(RichText::new("保存と書き出し").strong());
        ui.small(self.storage.storage_strategy_summary());
        ui.small(self.storage.editable_format_label());
        ui.small(self.storage.planned_export_format());
        ui.small("上部バー: JSON保存 / JSONを開く / PNG書き出し / ヘルプ");
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

        ui.label(RichText::new("配置補助").strong());
        ui.small(format!(
            "ルーラー: {} · グリッド: {:.0}px · スマートガイド: {} · ガイド: {}本",
            if rulers_visible {
                "表示"
            } else {
                "非表示"
            },
            grid.spacing,
            if smart_guides_visible {
                "オン"
            } else {
                "オフ"
            },
            guides.len(),
        ));
        ui.small("移動中はグリッド / ガイド / スマートガイドに吸着できます。表示中のガイドはドラッグで動かせます。");

        ui.add_enabled_ui(!has_canvas_interaction, |ui| {
            ui.horizontal_wrapped(|ui| {
                let mut show_rulers = rulers_visible;
                if ui.checkbox(&mut show_rulers, "ルーラーを表示").changed() {
                    pending_action = Some(AidAction::ToggleRulersVisible);
                }

                let mut show_smart_guides = smart_guides_visible;
                if ui
                    .checkbox(&mut show_smart_guides, "スマートガイド")
                    .changed()
                {
                    pending_action = Some(AidAction::ToggleSmartGuidesVisible);
                }
            });

            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                let mut show_grid = grid.visible;
                if ui.checkbox(&mut show_grid, "グリッドを表示").changed() {
                    pending_action = Some(AidAction::ToggleGridVisible);
                }

                let mut snap_grid = grid.snap_enabled;
                if ui.checkbox(&mut snap_grid, "グリッドに吸着").changed() {
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
                ui.small("間隔プリセット:");
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
                if ui.checkbox(&mut show_guides, "ガイドを表示").changed() {
                    pending_action = Some(AidAction::ToggleGuidesVisible);
                }

                let mut snap_guides = guides_snap;
                if ui.checkbox(&mut snap_guides, "ガイドに吸着").changed() {
                    pending_action = Some(AidAction::ToggleGuidesSnap);
                }

                ui.small(format!("{}本", guides.len()));
            });

            ui.horizontal(|ui| {
                if ui.button("横ガイド追加").clicked() {
                    pending_action = Some(AidAction::AddGuide(GuideAxis::Horizontal));
                }
                if ui.button("縦ガイド追加").clicked() {
                    pending_action = Some(AidAction::AddGuide(GuideAxis::Vertical));
                }
            });
        });

        if has_canvas_interaction {
            ui.small("編集中はグリッドやガイドの設定を変更できません。");
        }

        if guides.is_empty() {
            ui.small(
                "ガイドはまだありません。追加すると、選択の中心かキャンバス中央に置かれます。",
            );
        } else {
            for (index, guide) in guides {
                ui.horizontal(|ui| {
                    ui.small(format!("{} {:.0}px", guide.axis.label(), guide.position));
                    if ui
                        .add_enabled(!has_canvas_interaction, egui::Button::new("削除"))
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

        ui.heading("レイヤー");
        ui.small("選択と描画の対象は、表示中かつロックされていない現在のレイヤーです。");
        ui.small(
            "移動 / 複製は、現在のレイヤーで選択した要素を表示中かつ編集可能なレイヤーへ送ります。",
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui.button("レイヤー追加").clicked() {
                pending_action = Some(LayerAction::Add);
            }

            if ui
                .add_enabled(layer_count > 1, egui::Button::new("レイヤー削除"))
                .clicked()
            {
                pending_action = Some(LayerAction::DeleteActive);
            }
        });

        if selection_count > 0 {
            ui.small(format!(
                "現在のレイヤーで {selection_count} 個選択中です。移動先のレイヤーで「ここへ移動」または「ここへ複製」を使えます。"
            ));
        } else {
            ui.small("現在のレイヤーで要素を選ぶと、別のレイヤーへ移動したり複製したりできます。");
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
                        .small_button(if visible { "非表示" } else { "表示" })
                        .clicked()
                    {
                        pending_action = Some(LayerAction::ToggleVisibility(layer_id));
                    }

                    if ui
                        .small_button(if locked {
                            "ロック解除"
                        } else {
                            "ロック"
                        })
                        .clicked()
                    {
                        pending_action = Some(LayerAction::ToggleLocked(layer_id));
                    }
                });

                ui.horizontal(|ui| {
                    if is_active {
                        ui.label(RichText::new("作業中").small().strong());
                    }
                    if !visible {
                        ui.label(RichText::new("非表示").small());
                    }
                    if locked {
                        ui.label(RichText::new("ロック中").small());
                    }
                    ui.small(format!("{element_count}個"));
                    if ui.small_button("上へ").clicked() {
                        pending_action = Some(LayerAction::MoveUp(layer_id));
                    }
                    if ui.small_button("下へ").clicked() {
                        pending_action = Some(LayerAction::MoveDown(layer_id));
                    }
                });

                if can_receive_selection {
                    ui.horizontal_wrapped(|ui| {
                        let can_drop_here = visible && !locked;
                        if ui
                            .add_enabled(can_drop_here, egui::Button::new("ここへ移動"))
                            .clicked()
                        {
                            pending_action = Some(LayerAction::MoveSelectionTo(layer_id));
                        }
                        if ui
                            .add_enabled(can_drop_here, egui::Button::new("ここへ複製"))
                            .clicked()
                        {
                            pending_action = Some(LayerAction::DuplicateSelectionTo(layer_id));
                        }
                    });
                }

                if index + 1 == total_layers {
                    ui.small("最前面");
                } else if index == 0 {
                    ui.small("最背面");
                }
            });
            ui.add_space(4.0);
        }

        ui.separator();
        ui.label(RichText::new("現在のレイヤー名").strong());
        let rename_response = ui.text_edit_singleline(&mut self.layer_name_draft);
        let rename_on_enter =
            rename_response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
        if ui.button("レイヤー名を変更").clicked() || rename_on_enter {
            pending_action = Some(LayerAction::RenameActive);
        }

        if let Some((_, active_name, visible, locked)) = active_layer_state {
            if !visible {
                ui.small(format!(
                    "{active_name} は非表示です。表示も書き出しもされません。"
                ));
            } else if locked {
                ui.small(format!(
                    "{active_name} はロック中です。表示はされますが、選択や編集はできません。"
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
                .add_enabled(can_undo, egui::Button::new("元に戻す"))
                .clicked()
            {
                self.perform_undo();
            }

            if ui
                .add_enabled(can_redo, egui::Button::new("やり直す"))
                .clicked()
            {
                self.perform_redo();
            }

            if ui
                .add_enabled(can_clear, egui::Button::new("クリア"))
                .clicked()
            {
                self.perform_clear();
            }

            ui.separator();

            if ui
                .add_enabled(can_file_io, egui::Button::new("JSON保存"))
                .clicked()
            {
                self.save_document(ctx);
            }

            if ui
                .add_enabled(can_file_io, egui::Button::new("JSONを開く"))
                .clicked()
            {
                self.load_document(ctx);
            }

            if ui
                .add_enabled(can_file_io, egui::Button::new("PNG書き出し"))
                .clicked()
            {
                self.export_png(ctx);
            }

            ui.separator();

            ui.add_enabled_ui(can_align, |ui| {
                ui.menu_button("整列", |ui| {
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
                .add_enabled(can_group, egui::Button::new("グループ化"))
                .clicked()
            {
                self.apply_group();
            }

            if ui
                .add_enabled(can_ungroup, egui::Button::new("グループ解除"))
                .clicked()
            {
                self.apply_ungroup();
            }

            ui.add_enabled_ui(can_distribute, |ui| {
                ui.menu_button("等間隔", |ui| {
                    for distribution in [DistributionKind::Horizontal, DistributionKind::Vertical] {
                        if ui.button(distribution.label()).clicked() {
                            self.apply_distribution(distribution);
                        }
                    }
                });
            });

            ui.add_enabled_ui(can_reorder, |ui| {
                ui.menu_button("重なり順", |ui| {
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
                .add_enabled(can_adjust_view, egui::Button::new("表示をリセット"))
                .clicked()
            {
                self.reset_view();
            }

            ui.separator();
            if ui
                .button(if self.show_help {
                    "ヘルプを閉じる"
                } else {
                    "ヘルプ"
                })
                .clicked()
            {
                self.show_help = !self.show_help;
            }

            ui.separator();
            ui.label(RichText::new("状態").small().strong());
            ui.label(self.status_message.rich_text());
        });
    }

    fn show_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_help {
            return;
        }

        egui::Window::new("かんたんヘルプ")
            .open(&mut self.show_help)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.label(RichText::new("最初に").strong());
                ui.small("描く: ブラシ、四角形、楕円、直線を選んでキャンバスをドラッグします。");
                ui.small("選択: 選択ツールで要素をクリックします。角ハンドルでサイズ変更、丸いハンドルで回転します。");
                ui.small("複数選択: Shift+Click またはドラッグ選択。移動、グループ化、整列、等間隔、重なり順変更ができます。");
                ui.small("パンとズーム: Space+Drag または中ボタンドラッグでパン。Ctrl/Cmd+Wheel か +/- でズーム、Ctrl/Cmd+0 で表示を戻します。");
                ui.small("ファイル: JSON保存 は再編集用、JSONを開く は復元、PNG書き出し は共有用画像です。");
                ui.small("レイヤー: 現在のレイヤーに描きます。非表示レイヤーは書き出しに含まれません。");
                #[cfg(target_arch = "wasm32")]
                ui.small("Web版: GitHub Pages では JSON保存 と PNG書き出し はダウンロード、JSONを開く はファイル選択になります。");

                ui.add_space(8.0);
                ui.label(RichText::new("ショートカット").strong());
                ui.small("元に戻す: Ctrl/Cmd+Z · やり直す: Ctrl/Cmd+Shift+Z または Ctrl/Cmd+Y");
                ui.small("JSON保存: Ctrl/Cmd+S · JSONを開く: Ctrl/Cmd+O · PNG書き出し: Ctrl/Cmd+Shift+E");
                ui.small("ツール: V 選択 · B ブラシ · R 四角形 · O 楕円 · L 直線 · E 消しゴム");
            });
    }

    fn perform_undo(&mut self) {
        if self.canvas.discard_active_interaction() {
            self.set_info("進行中の操作を取り消しました。");
        } else if self.history.undo() {
            self.canvas.clear_selection();
            self.sync_layer_name_draft();
            self.set_info("ひとつ前に戻しました。");
        }
    }

    fn perform_redo(&mut self) {
        if self.history.redo() {
            self.canvas.clear_selection();
            self.sync_layer_name_draft();
            self.set_info("やり直しました。");
        }
    }

    fn perform_clear(&mut self) {
        let discarded = self.canvas.discard_active_interaction();
        if self.history.clear() {
            self.canvas.clear_selection();
            self.sync_layer_name_draft();
            self.set_info("キャンバスをクリアしました。");
        } else if discarded {
            self.set_info("進行中の操作を取り消しました。");
        }
    }

    fn zoom_in(&mut self) {
        let canvas_size = self.document().canvas_size;
        if self.canvas.zoom_in(canvas_size) {
            self.set_info(format!(
                "ズームを {} にしました。",
                self.canvas.zoom_label()
            ));
        }
    }

    fn zoom_out(&mut self) {
        let canvas_size = self.document().canvas_size;
        if self.canvas.zoom_out(canvas_size) {
            self.set_info(format!(
                "ズームを {} にしました。",
                self.canvas.zoom_label()
            ));
        }
    }

    fn reset_view(&mut self) {
        let canvas_size = self.document().canvas_size;
        self.canvas.request_view_reset();
        let _ = self.canvas.reset_view(canvas_size);
        self.set_info("表示をキャンバス全体に戻しました。");
    }

    fn commit_element(&mut self, element: PaintElement) {
        let label = element.kind_label().to_owned();
        if self.history.commit_element(element) {
            self.canvas.clear_selection();
            self.set_info(format!("{label}を追加しました。"));
        }
    }

    fn apply_document_edit(&mut self, edit: CommittedDocumentEdit) {
        let selection_layer_id = edit.document.active_layer_id();
        if self.history.replace_document(edit.document) {
            self.canvas
                .set_selection_indices(selection_layer_id, edit.selection_indices);
            let message = match edit.mode {
                DocumentEditMode::Move => "選択中の要素を移動しました。",
                DocumentEditMode::Resize => {
                    if self.canvas.selection_count() > 1 {
                        "選択中の要素のサイズを変更しました。"
                    } else {
                        "選択中の図形のサイズを変更しました。"
                    }
                }
                DocumentEditMode::Rotate => {
                    if self.canvas.selection_count() > 1 {
                        "選択中の要素を回転しました。"
                    } else {
                        "選択中の図形を回転しました。"
                    }
                }
                DocumentEditMode::Guide => "ガイドを移動しました。",
                DocumentEditMode::Group => "選択中の要素をグループ化しました。",
                DocumentEditMode::Ungroup => "選択中のグループを解除しました。",
                DocumentEditMode::Align(alignment) => match alignment {
                    AlignmentKind::Left => "左揃えにしました。",
                    AlignmentKind::HorizontalCenter => "横中央揃えにしました。",
                    AlignmentKind::Right => "右揃えにしました。",
                    AlignmentKind::Top => "上揃えにしました。",
                    AlignmentKind::VerticalCenter => "縦中央揃えにしました。",
                    AlignmentKind::Bottom => "下揃えにしました。",
                },
                DocumentEditMode::Distribute(distribution) => match distribution {
                    DistributionKind::Horizontal => "横方向に等間隔配置しました。",
                    DistributionKind::Vertical => "縦方向に等間隔配置しました。",
                },
                DocumentEditMode::Reorder(command) => match command {
                    StackOrderCommand::BringToFront => "選択中の要素を最前面へ移動しました。",
                    StackOrderCommand::SendToBack => "選択中の要素を最背面へ移動しました。",
                    StackOrderCommand::BringForward => "選択中の要素を一つ前面へ移動しました。",
                    StackOrderCommand::SendBackward => "選択中の要素を一つ背面へ移動しました。",
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
                self.set_info(format!("作業レイヤーを {} に切り替えました。", layer.name));
            }
        }
    }

    fn add_layer(&mut self) {
        let document = self.document().clone();
        let (next, layer_id) = document.add_layer_document();
        let layer_name = next
            .layer(layer_id)
            .map(|layer| layer.name.clone())
            .unwrap_or_else(|| "新しいレイヤー".to_owned());
        self.apply_layer_document_change(next, format!("{layer_name} を追加しました。"));
    }

    fn delete_active_layer(&mut self) {
        let document = self.document().clone();
        if let Some((next, next_active)) = document.delete_active_layer_document() {
            let next_name = next
                .layer(next_active)
                .map(|layer| layer.name.clone())
                .unwrap_or_else(|| "残りのレイヤー".to_owned());
            self.apply_layer_document_change(
                next,
                format!("現在のレイヤーを削除しました。{next_name} が作業レイヤーです。"),
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
                format!(
                    "レイヤー名を {} に変更しました。",
                    self.layer_name_draft.trim()
                ),
            );
        }
    }

    fn toggle_layer_visibility(&mut self, layer_id: LayerId) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_layer_visibility_document(layer_id) {
            let message = next
                .layer(layer_id)
                .map(|layer| {
                    if layer.visible {
                        format!("{} を表示しました。", layer.name)
                    } else {
                        format!("{} を非表示にしました。", layer.name)
                    }
                })
                .unwrap_or_else(|| "レイヤーの表示状態を変更しました。".to_owned());
            self.apply_layer_document_change(next, message);
        }
    }

    fn toggle_layer_locked(&mut self, layer_id: LayerId) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_layer_locked_document(layer_id) {
            let message = next
                .layer(layer_id)
                .map(|layer| {
                    if layer.locked {
                        format!("{} をロックしました。", layer.name)
                    } else {
                        format!("{} のロックを解除しました。", layer.name)
                    }
                })
                .unwrap_or_else(|| "レイヤーのロック状態を変更しました。".to_owned());
            self.apply_layer_document_change(next, message);
        }
    }

    fn move_layer_up(&mut self, layer_id: LayerId) {
        let document = self.document().clone();
        if let Some(next) = document.moved_layer_up_document(layer_id) {
            self.apply_layer_document_change(next, "レイヤーを上へ移動しました。");
        }
    }

    fn move_layer_down(&mut self, layer_id: LayerId) {
        let document = self.document().clone();
        if let Some(next) = document.moved_layer_down_document(layer_id) {
            self.apply_layer_document_change(next, "レイヤーを下へ移動しました。");
        }
    }

    fn toggle_rulers_visibility(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_rulers_visibility_document() {
            let message = if next.rulers().visible {
                "ルーラーを表示しました。"
            } else {
                "ルーラーを非表示にしました。"
            };
            self.apply_document_configuration_change(next, message);
        }
    }

    fn toggle_grid_visibility(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_grid_visibility_document() {
            let message = if next.grid().visible {
                "グリッドを表示しました。"
            } else {
                "グリッドを非表示にしました。"
            };
            self.apply_document_configuration_change(next, message);
        }
    }

    fn toggle_grid_snap(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_grid_snap_document() {
            let message = if next.grid().snap_enabled {
                "グリッド吸着をオンにしました。"
            } else {
                "グリッド吸着をオフにしました。"
            };
            self.apply_document_configuration_change(next, message);
        }
    }

    fn set_grid_spacing(&mut self, spacing: f32) {
        let document = self.document().clone();
        if let Some(next) = document.set_grid_spacing_document(spacing) {
            let applied_spacing = next.grid().spacing;
            self.apply_document_configuration_change(
                next,
                format!("グリッド間隔を {:.0}px に設定しました。", applied_spacing),
            );
        }
    }

    fn toggle_smart_guides_visibility(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_smart_guides_visibility_document() {
            let message = if next.smart_guides().visible {
                "スマートガイドをオンにしました。"
            } else {
                "スマートガイドをオフにしました。"
            };
            self.apply_document_configuration_change(next, message);
        }
    }

    fn toggle_guides_visibility(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_guides_visibility_document() {
            let message = if next.guides().visible {
                "ガイドを表示しました。"
            } else {
                "ガイドを非表示にしました。"
            };
            self.apply_document_configuration_change(next, message);
        }
    }

    fn toggle_guides_snap(&mut self) {
        let document = self.document().clone();
        if let Some(next) = document.toggled_guides_snap_document() {
            let message = if next.guides().snap_enabled {
                "ガイド吸着をオンにしました。"
            } else {
                "ガイド吸着をオフにしました。"
            };
            self.apply_document_configuration_change(next, message);
        }
    }

    fn add_guide(&mut self, axis: GuideAxis) {
        let document = self.document().clone();
        let position = self.suggested_guide_position(axis);
        if let Some(next) = document.add_guide_document(axis, position) {
            self.apply_document_configuration_change(
                next,
                format!(
                    "{}ガイドを {:.0}px に追加しました。",
                    axis.label(),
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
                    "{}ガイド（{:.0}px）を削除しました。",
                    guide.axis.label(),
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
                "{} は表示中かつロック解除されていないと、要素を移動できません。",
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
                .unwrap_or_else(|| "移動先レイヤー".to_owned());
            let message = if moved_count == 1 {
                format!("選択中の要素を {destination_name} へ移動しました。")
            } else {
                format!("選択中の {moved_count} 個の要素を {destination_name} へ移動しました。")
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
                "{} は表示中かつロック解除されていないと、要素を複製できません。",
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
                .unwrap_or_else(|| "複製先レイヤー".to_owned());
            let message = if duplicated_count == 1 {
                format!("選択中の要素を {destination_name} へ複製しました。")
            } else {
                format!(
                    "選択中の {duplicated_count} 個の要素を {destination_name} へ複製しました。"
                )
            };
            self.apply_layer_selection_document_change(next, layer_id, next_selection, message);
        }
    }

    fn finish_save(&mut self, saved: SavedDocument) {
        self.document_name = saved.file_name;
        self.saved_snapshot = self.document().clone();
        self.set_info(format!(
            "再編集用の JSON を {} として保存しました。",
            self.document_name
        ));
    }

    fn finish_load(&mut self, loaded: LoadedDocument) {
        self.canvas.discard_active_interaction();
        self.canvas.clear_selection();
        self.canvas.request_view_reset();
        self.history.replace_document(loaded.document.clone());
        self.document_name = loaded.file_name;
        self.saved_snapshot = loaded.document;
        self.sync_layer_name_draft();
        self.set_info(format!("{} を開きました。", self.document_name));
    }

    fn finish_export(&mut self, exported: ExportedImage) {
        self.set_info(format!(
            "PNG を {} として書き出しました。",
            exported.file_name
        ));
    }

    fn storage_action_title(action: &'static str) -> &'static str {
        match action {
            "save" => "JSON保存",
            "load" => "JSONを開く",
            "export" => "PNG書き出し",
            _ => "ファイル操作",
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn storage_pending_message(action: &'static str) -> &'static str {
        match action {
            "save" => {
                "ブラウザの保存ダイアログを開きました。再編集用 JSON の保存先を選んでください。"
            }
            "load" => "ブラウザのファイル選択を開きました。.paint.json を選んでください。",
            "export" => "ブラウザで PNG ダウンロードを準備しています...",
            _ => "ブラウザのファイル操作を待っています...",
        }
    }

    fn storage_error_message(action: &'static str, error: &StorageError) -> String {
        let label = Self::storage_action_title(action);
        match error {
            StorageError::Cancelled => format!("{label}をキャンセルしました。"),
            StorageError::EmptyFile => {
                format!("{label}できませんでした。選択したファイルは空です。")
            }
            StorageError::UnsupportedFormat(_) => {
                format!(
                    "{label}できませんでした。このアプリの .paint.json ファイルを選んでください。"
                )
            }
            StorageError::UnsupportedVersion(_) => {
                format!(
                    "{label}できませんでした。このドキュメントのバージョンにはまだ対応していません。"
                )
            }
            StorageError::Deserialize(_) => {
                format!("{label}できませんでした。お絵かきドキュメントとして読み込めませんでした。")
            }
            StorageError::Serialize(_) => {
                format!("{label}できませんでした。ファイルの準備中に失敗しました。")
            }
            StorageError::Render(_) => {
                format!("{label}できませんでした。キャンバスの描画中に失敗しました。")
            }
            StorageError::Io(details) => format!("{label}できませんでした: {details}"),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn report_storage_result(&mut self, action: &'static str, result: Result<(), StorageError>) {
        if let Err(error) = result {
            match &error {
                StorageError::Cancelled => {
                    self.set_info(Self::storage_error_message(action, &error));
                }
                _ => {
                    self.set_error(Self::storage_error_message(action, &error));
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
            self.set_info(Self::storage_pending_message("save"));
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
            self.set_info(Self::storage_pending_message("load"));
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
            self.set_info(Self::storage_pending_message("export"));
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
            self.set_info("進行中の操作をキャンセルしました。");
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
                Err(error) => match &error {
                    StorageError::Cancelled => {
                        self.set_info(Self::storage_error_message(task.label, &error));
                    }
                    _ => {
                        self.set_error(Self::storage_error_message(task.label, &error));
                    }
                },
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

        self.show_help_window(ctx);
    }
}

fn tool_hint(tool: CanvasToolKind) -> &'static str {
    match tool {
        CanvasToolKind::Select => {
            "現在のレイヤー上でクリックすると選択できます。Shift+Click で追加 / 解除、空き領域のドラッグで矩形選択です。単体図形はサイズ変更 / 回転、複数選択は移動 / グループ変形 / 整列 / 重なり順変更ができます。"
        }
        CanvasToolKind::Brush => {
            "フリーハンドで線を描くツールです。現在のレイヤー上をドラッグして描きます。"
        }
        CanvasToolKind::Eraser => "キャンバス背景色でなぞるフリーハンド消しゴムです。",
        CanvasToolKind::Rectangle => {
            "現在のレイヤーで、始点の角から反対側の角までドラッグして四角形を作ります。"
        }
        CanvasToolKind::Ellipse => "現在のレイヤーで、外接する枠をドラッグして楕円を作ります。",
        CanvasToolKind::Line => "現在のレイヤーで、始点から終点までドラッグして直線を作ります。",
    }
}
