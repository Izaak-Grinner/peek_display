use bevy::prelude::*;
use bevy_egui::{
    EguiContexts, EguiTextureHandle,
    egui::{self, Align, Align2, Color32, CornerRadius, Layout, Margin, RichText, Ui, vec2},
};

use super::widgets::{self, Button, Icon, NumberField, color::*};
use crate::app_mode::{AppMode, EditViewInsets};
use crate::camera::inset::MainCameraInset;
use crate::config::AppConfig;
use crate::scene::editor::{EditState, Pending, SaveAsDialog, SaveStatus, Selection};
use crate::scene::import::{ImportKind, ImportState};
use crate::scene::layout::{ObjectKind, Placement, SceneLayout, SceneObject};
use crate::scene::library::{self, SceneLibrary};
use crate::scene::package::{PackageMessage, PackageState};
use crate::scene::world::{self, WorldInfo, WorldNodeInfo};

const PANEL_WIDTH: f32 = 330.0;
const MENU_BAR_HEIGHT: f32 = 28.0;

// 新規シーンを名前を付けて保存するときの名前の初期値
const NEW_SCENE_NAME: &str = "新しいシーン";

// 覗き込みカメラの小窓の大きさ。俯瞰ビューの幅に対する割合と上下限、高さの上限 (俯瞰ビューに対する割合)
const INSET_WIDTH_RATIO: f32 = 0.3;
const INSET_WIDTH_RANGE: (f32, f32) = (200.0, 480.0);
const INSET_MAX_HEIGHT_RATIO: f32 = 0.45;
const INSET_MARGIN: f32 = 12.0;

// 複製したオブジェクトをずらす量(メートル)
const DUPLICATE_OFFSET: Vec3 = Vec3::new(0.05, 0.0, 0.0);

// よく使うアスペクト比
const ASPECT_PRESETS: [(f32, f32); 3] = [(16.0, 9.0), (16.0, 10.0), (3.0, 2.0)];

// 編集モードの操作方法。メニューの「ヘルプ → 操作方法」にまとめて出す
const HELP_ROWS: [(&str, &str); 19] = [
    ("左クリック", "オブジェクトを選択(一覧か3Dビュー。壁や床で選択を外す)"),
    ("右ドラッグ / ホイール", "視点の回転 / ズーム"),
    ("ホイールを押しながらドラッグ", "視点の平行移動"),
    ("矢印キー", "移動(↑↓ = Z軸、←→ = X軸)"),
    ("PageUp / PageDown", "上下に移動"),
    ("R / F", "X軸まわりに回転"),
    ("Q / E", "Y軸まわりに回転"),
    ("Z / X", "Z軸まわりに回転"),
    ("+ / -", "拡大・縮小 (ライトは明るさ)"),
    ("Shift を押しながら", "ゆっくり動かす(速さ1/10)"),
    ("Ctrl+Z / Ctrl+Shift+Z", "元に戻す / やり直し (Ctrl+Y でもやり直し)"),
    ("Ctrl+D", "選択中のオブジェクトを複製"),
    ("Delete", "選択中のオブジェクトを削除(ワールドのものは非表示に切り替え)"),
    ("Ctrl+N", "新規シーン"),
    ("Ctrl+S / Ctrl+Shift+S", "保存 / 名前を付けて保存"),
    ("Esc", "ホームへ戻る / ダイアログを閉じる"),
    ("Enter", "ダイアログで決定"),
    ("数値欄", "ドラッグで増減、クリックで入力"),
    ("メニュー", "シーンを開く・ワールドの取り込み・インポート・エクスポートは「ファイル」から"),
];

// パネルの見た目だけに関わる状態 (保存しない)
#[derive(Default)]
pub struct PanelUi {
    screen_closed: bool,
    eye_closed: bool,
    help_open: bool,
}

enum Action {
    // 未保存の変更があれば確認してから行う
    Request(Pending),
    Save,
    SaveAs,
    ConfirmSaveAs,
    CancelSaveAs,
    // 未保存の確認ダイアログの選択
    SaveAndContinue,
    DiscardAndContinue,
    CancelDiscard,
    Undo,
    Redo,
    ExportScene,
    ImportModel,
    ReloadWorld,
    OpenScenesFolder,
    // ワールドのノードの表示・非表示を切り替える
    ToggleHidden(String),
    // ワールドのノードを Blender の値に戻す
    ResetWorldNode(String),
    // のぞき窓を Blender の peek_window の位置に戻す
    ResetWindow,
    // ライトを初期値に戻す
    ResetLight,
    // 保存してあるシーンの削除 (確認ダイアログを出す → 削除する / やめる)
    AskDeleteScene(String),
    DeleteScene(String),
    CancelDeleteScene,
    Duplicate(u64),
    AskDelete(u64),
    Delete(u64),
    CancelDelete,
}

pub fn edit_panel(
    mut contexts: EguiContexts,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut layout: ResMut<SceneLayout>,
    mut config: ResMut<AppConfig>,
    mut state: ResMut<EditState>,
    mut library: ResMut<SceneLibrary>,
    mut import: ResMut<ImportState>,
    mut package: ResMut<PackageState>,
    mut world: ResMut<WorldInfo>,
    mut insets: ResMut<EditViewInsets>,
    mut next: ResMut<NextState<AppMode>>,
    mut panel: Local<PanelUi>,
    mut inset: ResMut<MainCameraInset>,
) -> Result {
    let inset_texture = contexts.add_image(EguiTextureHandle::Weak(inset.image.id()));
    let ctx = contexts.ctx_mut()?;

    // 値を書き換えたときだけ変更扱いにする (毎フレーム変更扱いにすると、シーンの同期が毎回走る)
    let before_layout = layout.clone();
    let before_config = config.clone();
    let dirty = state.is_dirty(&layout, &config, &library);
    // 文字を入力している間は、パネルのショートカットを効かせない
    let typing = ctx.wants_keyboard_input();
    let mut action = None;

    {
        let layout = layout.bypass_change_detection();
        let config = config.bypass_change_detection();
        let state = state.as_mut();
        let library = library.as_mut();

        // メニューバーは画面の上端いっぱいに出す (右のパネルより先に置く)
        let menu = egui::TopBottomPanel::top("edit_menu_bar")
            .exact_height(MENU_BAR_HEIGHT)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(BAR).inner_margin(Margin::symmetric(6, 0)))
            .show(ctx, |ui| {
                menu_bar(ui, layout, config, state, library, &import, &package, &mut inset, &mut panel, dirty, &mut action)
            });
        let menu_rect = menu.response.rect;
        ctx.layer_painter(egui::LayerId::background())
            .hline(menu_rect.x_range(), menu_rect.bottom() - 0.5, widgets::hairline(BAR_BORDER));

        let response = egui::SidePanel::right("edit_panel")
            .exact_width(PANEL_WIDTH)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(PANEL))
            .show(ctx, |ui| {
                ui.style_mut().interaction.selectable_labels = false;
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;

                egui::TopBottomPanel::top("edit_panel_header")
                    .frame(egui::Frame::NONE)
                    .show_separator_line(false)
                    .show_inside(ui, |ui| save_error(ui, state.save_status.as_ref()));

                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show_inside(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                scene_section(ui, layout, state, &import, &package, &world, &mut action);
                                ui.add_space(4.0);
                                widgets::section_header(ui, "プロパティ", None, |_| {});
                                properties(ui, layout, config, state, &world, &mut *panel, &mut action);
                            });
                    });
            });
        let rect = response.response.rect;
        ctx.layer_painter(egui::LayerId::background())
            .vline(rect.left() + 0.5, rect.y_range(), widgets::hairline(PANEL_EDGE));
        insets.right = rect.width();
        insets.top = menu_rect.height();

        viewport_overlay(ctx, &mut inset, inset_texture, config.screen_size(), &insets);
        help_window(ctx, &mut panel.help_open);

        if let Some(id) = state.confirm_delete {
            delete_dialog(ctx, layout, id, &mut action);
        } else if let Some(name) = &state.confirm_delete_scene {
            delete_scene_dialog(ctx, name, library.current.as_deref() == Some(name.as_str()), &mut action);
        } else if let Some(dialog) = &mut state.save_as {
            save_as_dialog(ctx, dialog, library.current.as_deref(), &mut action);
        } else if let Some(pending) = &state.confirm_discard {
            discard_dialog(ctx, pending, library.current.is_none(), &mut action);
        }

        // キー操作 (ダイアログが出ているときは Esc / Enter をダイアログに使う)
        if action.is_none() {
            let esc = keyboard.just_pressed(KeyCode::Escape);
            let enter = keyboard.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]);
            let ctrl = keyboard.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
            let shift = keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
            if let Some(id) = state.confirm_delete {
                if esc {
                    action = Some(Action::CancelDelete);
                } else if enter {
                    action = Some(Action::Delete(id));
                }
            } else if let Some(name) = &state.confirm_delete_scene {
                if esc {
                    action = Some(Action::CancelDeleteScene);
                } else if enter {
                    action = Some(Action::DeleteScene(name.clone()));
                }
            } else if state.save_as.is_some() {
                if esc {
                    action = Some(Action::CancelSaveAs);
                } else if enter {
                    action = Some(Action::ConfirmSaveAs);
                }
            } else if state.confirm_discard.is_some() {
                if esc {
                    action = Some(Action::CancelDiscard);
                } else if enter {
                    action = Some(Action::SaveAndContinue);
                }
            } else if panel.help_open && esc {
                panel.help_open = false;
            } else if !typing {
                if esc {
                    action = Some(Action::Request(Pending::Exit));
                } else if ctrl && keyboard.just_pressed(KeyCode::KeyS) {
                    action = Some(if shift { Action::SaveAs } else { Action::Save });
                } else if ctrl && keyboard.just_pressed(KeyCode::KeyZ) {
                    action = Some(if shift { Action::Redo } else { Action::Undo });
                } else if ctrl && keyboard.just_pressed(KeyCode::KeyY) {
                    action = Some(Action::Redo);
                } else if ctrl && keyboard.just_pressed(KeyCode::KeyN) {
                    action = Some(Action::Request(Pending::New));
                } else if let Some(Selection::Object(id)) = state.selection {
                    if ctrl && keyboard.just_pressed(KeyCode::KeyD) {
                        action = Some(Action::Duplicate(id));
                    } else if keyboard.just_pressed(KeyCode::Delete) {
                        action = Some(Action::AskDelete(id));
                    }
                } else if let Some(Selection::WorldNode(name)) = &state.selection {
                    if keyboard.just_pressed(KeyCode::Delete) {
                        action = Some(Action::ToggleHidden(name.clone()));
                    }
                }
            }
        }

        let mut files = FileContext {
            layout,
            config,
            state,
            library,
            package: &mut package,
            import: &mut import,
            next: &mut next,
        };
        match action {
            Some(Action::Request(pending)) => files.request(pending),
            Some(Action::Save) => files.save(None),
            Some(Action::SaveAs) => {
                let name = files.library.current.clone().unwrap_or_else(|| files.library.suggested_name.clone());
                files.state.save_as = Some(SaveAsDialog { name, error: None, then: None });
            }
            Some(Action::ConfirmSaveAs) => files.confirm_save_as(),
            Some(Action::CancelSaveAs) => files.state.save_as = None,
            Some(Action::SaveAndContinue) => {
                if let Some(pending) = files.state.confirm_discard.take() {
                    files.save(Some(pending));
                }
            }
            Some(Action::DiscardAndContinue) => {
                if let Some(pending) = files.state.confirm_discard.take() {
                    files.state.discard(files.layout, files.config, files.library);
                    files.run(pending);
                }
            }
            Some(Action::CancelDiscard) => files.state.confirm_discard = None,
            Some(Action::ExportScene) => files.package.start_export(files.layout),
            Some(Action::ImportModel) => {
                files.state.import_error = None;
                files.import.start(ImportKind::Model);
            }
            Some(Action::ReloadWorld) => world.reload_requested = true,
            Some(Action::OpenScenesFolder) => open_scenes_folder(),
            Some(Action::ToggleHidden(name)) => {
                let o = files.layout.world_override_mut(&name);
                o.hidden = !o.hidden;
                files.layout.prune_world_overrides();
            }
            Some(Action::ResetWorldNode(name)) => {
                files.layout.world_override_mut(&name).placement = None;
                files.layout.prune_world_overrides();
            }
            Some(Action::ResetLight) => files.layout.light = Default::default(),
            Some(Action::AskDeleteScene(name)) => files.state.confirm_delete_scene = Some(name),
            Some(Action::DeleteScene(name)) => files.delete_scene(&name),
            Some(Action::CancelDeleteScene) => files.state.confirm_delete_scene = None,
            Some(Action::ResetWindow) => {
                if let Some(window) = world.window.clone() {
                    files.layout.window = window;
                }
            }
            Some(Action::Duplicate(id)) => {
                if let Some(source) = layout_object(files.layout, id) {
                    let placement = Placement {
                        position: source.placement.position + DUPLICATE_OFFSET,
                        ..source.placement
                    };
                    let name = format!("{}_copy", source.name);
                    let new_id = files.layout.add_object(&name, source.kind, placement);
                    files.state.selection = Some(Selection::Object(new_id));
                }
            }
            Some(Action::AskDelete(id)) => files.state.confirm_delete = Some(id),
            Some(Action::Delete(id)) => {
                files.layout.objects.retain(|o| o.id != id);
                if files.state.selection == Some(Selection::Object(id)) {
                    files.state.selection = None;
                }
                files.state.confirm_delete = None;
            }
            Some(Action::CancelDelete) => files.state.confirm_delete = None,
            Some(Action::Undo) => files.state.undo(files.layout, files.config),
            Some(Action::Redo) => files.state.redo(files.layout, files.config),
            None => {}
        }
    }

    if *layout != before_layout {
        layout.set_changed();
    }
    if *config != before_config {
        config.set_changed();
    }
    Ok(())
}

fn layout_object(layout: &SceneLayout, id: u64) -> Option<SceneObject> {
    layout.object(id).cloned()
}

// ---------------------------------------------------------------- シーンのファイル操作

struct FileContext<'a> {
    layout: &'a mut SceneLayout,
    config: &'a mut AppConfig,
    state: &'a mut EditState,
    library: &'a mut SceneLibrary,
    package: &'a mut PackageState,
    import: &'a mut ImportState,
    next: &'a mut NextState<AppMode>,
}

impl FileContext<'_> {
    // 未保存の変更があれば確認ダイアログを出し、無ければすぐ行う
    fn request(&mut self, pending: Pending) {
        if self.state.is_dirty(self.layout, self.config, self.library) {
            self.state.confirm_discard = Some(pending);
        } else {
            self.run(pending);
        }
    }

    fn run(&mut self, pending: Pending) {
        match pending {
            Pending::Exit => self.next.set(AppMode::Home),
            Pending::New => {
                self.layout.replace_with(SceneLayout::empty());
                *self.library = SceneLibrary::untitled(NEW_SCENE_NAME);
                self.state.scene_switched(self.layout);
            }
            Pending::Open(name) => match self.library.open_scene(&name) {
                Ok(loaded) => {
                    self.layout.replace_with(loaded);
                    self.state.scene_switched(self.layout);
                }
                Err(e) => {
                    warn!("{}", e);
                    self.state.save_status = Some(SaveStatus::OpenFailed(e));
                }
            },
            Pending::ImportScene => self.package.start_import(),
            Pending::ImportWorld => {
                self.state.import_error = None;
                self.import.start(ImportKind::World);
            }
        }
    }

    // 保存してあるシーンを削除する。今開いているシーンなら、ほかのシーンを開く (無ければ新規)
    fn delete_scene(&mut self, name: &str) {
        self.state.confirm_delete_scene = None;
        let was_current = self.library.current.as_deref() == Some(name);
        if let Err(e) = self.library.delete(name) {
            warn!("{}", e);
            self.state.save_status = Some(SaveStatus::Failed(e));
            return;
        }
        if !was_current {
            return;
        }
        match library::list().first() {
            Some(next) => self.run(Pending::Open(next.clone())),
            None => self.run(Pending::New),
        }
    }

    // 上書き保存する。名前の無いシーンは「名前を付けて保存」にする。
    // then: 保存できたら続けて行う操作
    fn save(&mut self, then: Option<Pending>) {
        if self.library.current.is_none() {
            self.state.save_as = Some(SaveAsDialog {
                name: self.library.suggested_name.clone(),
                error: None,
                then,
            });
            return;
        }
        if self.state.save(self.layout, self.config, self.library) {
            if let Some(pending) = then {
                self.run(pending);
            }
        }
    }

    fn confirm_save_as(&mut self) {
        let Some(dialog) = &self.state.save_as else {
            return;
        };
        let name = dialog.name.clone();
        if self.state.save_as(&name, self.layout, self.config, self.library) {
            if let Some(pending) = self.state.save_as.take().and_then(|d| d.then) {
                self.run(pending);
            }
        } else if self.state.save_as.as_ref().is_some_and(|d| d.error.is_none()) {
            // 書き込みに失敗したときは、ダイアログを閉じてパネル上部に理由を出す
            self.state.save_as = None;
        }
    }
}

// scenes フォルダをエクスプローラーで開く
fn open_scenes_folder() {
    let dir = library::dir();
    let _ = std::fs::create_dir_all(&dir);
    let dir = dir.canonicalize().unwrap_or(dir);
    #[cfg(windows)]
    let result = std::process::Command::new("explorer").arg(&dir).spawn();
    #[cfg(not(windows))]
    let result = std::process::Command::new("xdg-open").arg(&dir).spawn();
    if let Err(e) = result {
        warn!("{} を開けませんでした: {}", dir.display(), e);
    }
}

// ---------------------------------------------------------------- メニューバー

#[allow(clippy::too_many_arguments)]
fn menu_bar(
    ui: &mut Ui,
    layout: &SceneLayout,
    config: &AppConfig,
    state: &EditState,
    library: &SceneLibrary,
    import: &ImportState,
    package: &PackageState,
    inset: &mut MainCameraInset,
    panel: &mut PanelUi,
    dirty: bool,
    action: &mut Option<Action>,
) {
    let mut set = |a: Action| *action = Some(a);
    egui::MenuBar::new().ui(ui, |ui| {
        ui.set_height(MENU_BAR_HEIGHT);
        ui.spacing_mut().item_spacing.x = 2.0;

        // ホームへ戻る (メニューを開かずにすぐ戻る。未保存の変更があれば確認する)
        if ui.add(egui::Button::new("ホーム").frame_when_inactive(false)).clicked() {
            set(Action::Request(Pending::Exit));
        }
        ui.separator();

        ui.menu_button("ファイル", |ui| {
            ui.set_min_width(220.0);
            if ui.button("新規シーン").clicked() {
                set(Action::Request(Pending::New));
            }
            ui.menu_button("開く", |ui| {
                ui.set_min_width(200.0);
                let names = library::list();
                if names.is_empty() {
                    ui.label(RichText::new("保存したシーンはありません").color(TEXT_HINT));
                }
                for name in names {
                    let is_current = library.current.as_deref() == Some(name.as_str());
                    let label = if is_current { format!("✔ {}", name) } else { format!("    {}", name) };
                    if ui.button(label).clicked() && !is_current {
                        set(Action::Request(Pending::Open(name)));
                    }
                }
                ui.separator();
                if ui.button("シーンのフォルダを開く").clicked() {
                    set(Action::OpenScenesFolder);
                }
            });
            ui.menu_button("シーンを削除", |ui| {
                ui.set_min_width(200.0);
                let names = library::list();
                if names.is_empty() {
                    ui.label(RichText::new("保存したシーンはありません").color(TEXT_HINT));
                }
                for name in names {
                    let is_current = library.current.as_deref() == Some(name.as_str());
                    let label = if is_current { format!("{} (開いているシーン)…", name) } else { format!("{}…", name) };
                    if ui.button(label).clicked() {
                        set(Action::AskDeleteScene(name));
                    }
                }
            });
            ui.separator();
            if ui.button("保存").clicked() {
                set(Action::Save);
            }
            if ui.button("名前を付けて保存…").clicked() {
                set(Action::SaveAs);
            }
            ui.separator();
            ui.menu_button("インポート", |ui| {
                ui.set_min_width(220.0);
                if ui.add_enabled(!import.busy(), egui::Button::new("ワールド (.glb)…")).clicked() {
                    set(Action::Request(Pending::ImportWorld));
                }
                if ui.add_enabled(!package.busy(), egui::Button::new("シーン (.zip)…")).clicked() {
                    set(Action::Request(Pending::ImportScene));
                }
                if ui.add_enabled(!import.busy(), egui::Button::new("モデル (.glb / .gltf)…")).clicked() {
                    set(Action::ImportModel);
                }
            });
            ui.menu_button("エクスポート", |ui| {
                ui.set_min_width(200.0);
                if ui.add_enabled(!package.busy(), egui::Button::new("シーン (.zip)…")).clicked() {
                    set(Action::ExportScene);
                }
            });
            if ui.add_enabled(layout.world.is_some(), egui::Button::new("ワールドを再読み込み")).clicked() {
                set(Action::ReloadWorld);
            }
        });

        ui.menu_button("編集", |ui| {
            ui.set_min_width(180.0);
            if ui.add_enabled(state.can_undo(layout, config), egui::Button::new("元に戻す")).clicked() {
                set(Action::Undo);
            }
            if ui.add_enabled(state.can_redo(), egui::Button::new("やり直し")).clicked() {
                set(Action::Redo);
            }
        });

        ui.menu_button("表示", |ui| {
            ui.set_min_width(180.0);
            ui.checkbox(&mut inset.visible, "覗き込みカメラの小窓");
        });

        ui.menu_button("ヘルプ", |ui| {
            ui.set_min_width(160.0);
            if ui.button("操作方法").clicked() {
                panel.help_open = true;
                ui.close();
            }
        });

        // 右端: 今のシーンの名前と保存の状態
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(6.0);
            save_indicator(ui, dirty, state.save_status.as_ref());
            ui.add_space(6.0);
            let name = library.current.as_deref().unwrap_or("名前なし");
            ui.label(RichText::new(name).size(12.5).color(TEXT_STRONG));
            ui.label(RichText::new("シーン").size(11.0).color(TEXT_MUTED));
        });
    });
}

fn kind_label(kind: &ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Model { .. } => "モデル",
        ObjectKind::Cube { .. } => "キューブ",
    }
}

fn kind_icon(kind: &ObjectKind) -> (Icon, Color32) {
    match kind {
        ObjectKind::Model { .. } => (Icon::Model, ICON_MODEL),
        ObjectKind::Cube { .. } => (Icon::Cube, ICON_CUBE),
    }
}

// 3D ビュー左上の表示 (クリックは 3D 側に通す)
fn viewport_overlay(
    ctx: &egui::Context,
    inset: &mut ResMut<MainCameraInset>,
    texture: egui::TextureId,
    (screen_width, screen_height): (f32, f32),
    insets: &EditViewInsets,
) {
    // 俯瞰ビューの範囲 (メニューバーと右のパネルを除く)
    let screen = ctx.content_rect();
    let view = egui::Rect::from_min_max(
        screen.min + vec2(0.0, insets.top),
        egui::pos2(screen.right() - insets.right, screen.bottom()),
    );
    egui::Area::new(egui::Id::new("edit_viewport_overlay"))
        .fixed_pos(view.min + vec2(12.0, 12.0))
        .interactable(false)
        .order(egui::Order::Background)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                overlay_chip(ui, "俯瞰ビュー", 12.0, TEXT);
            });
        });

    if !inset.visible {
        return;
    }

    // 覗き込みカメラの小窓 (俯瞰ビューの右下)。映像は InsetCamera が画像に描いたもの
    let aspect = screen_width / screen_height;
    let mut width = (view.width() * INSET_WIDTH_RATIO)
        .clamp(INSET_WIDTH_RANGE.0, INSET_WIDTH_RANGE.1)
        .min(view.width() - INSET_MARGIN * 2.0);
    let max_height = view.height() * INSET_MAX_HEIGHT_RATIO;
    if width / aspect > max_height {
        width = max_height * aspect;
    }
    let size = vec2(width, width / aspect).max(egui::Vec2::splat(1.0));
    let image_size = (size * ctx.pixels_per_point()).round();
    let image_size = UVec2::new(image_size.x as u32, image_size.y as u32);
    if inset.size != image_size {
        inset.size = image_size;
    }

    egui::Area::new(egui::Id::new("main_camera_inset"))
        .fixed_pos(view.right_bottom() - size - egui::Vec2::splat(INSET_MARGIN))
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let painter = ui.painter();
            painter.image(
                texture,
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            painter.rect_stroke(
                rect.expand(1.0),
                CornerRadius::same(2),
                widgets::hairline(ICON_WINDOW.gamma_multiply(0.7)),
                egui::StrokeKind::Inside,
            );
            let galley = widgets::text(ui, "覗き込みカメラ", 11.0, TEXT);
            let chip = egui::Rect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(galley.size().x + 16.0, 22.0));
            painter.rect(
                chip,
                CornerRadius::same(4),
                Color32::from_rgba_unmultiplied(20, 20, 20, 217),
                widgets::hairline(Color32::from_gray(0x3a)),
                egui::StrokeKind::Inside,
            );
            painter.galley(chip.center() - galley.size() / 2.0, galley, TEXT);
        });
}

fn overlay_chip(ui: &mut Ui, label: &str, size: f32, color: Color32) {
    let galley = widgets::text(ui, label, size, color);
    let (rect, _) = ui.allocate_exact_size(vec2(galley.size().x + 20.0, 26.0), egui::Sense::hover());
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        Color32::from_rgba_unmultiplied(20, 20, 20, 217),
        widgets::hairline(Color32::from_gray(0x3a)),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, color);
}

// ---------------------------------------------------------------- ヘッダー・ステータス・フッター

// メニューバーの右端に出す保存の状態 (右から左に並べる)
fn save_indicator(ui: &mut Ui, dirty: bool, status: Option<&SaveStatus>) {
    ui.spacing_mut().item_spacing.x = 4.0;
    match status {
        Some(SaveStatus::Failed(_) | SaveStatus::OpenFailed(_)) => {
            let color = Color32::from_rgb(0xff, 0x8f, 0x8f);
            let label = if matches!(status, Some(SaveStatus::Failed(_))) { "保存失敗" } else { "開けませんでした" };
            ui.label(RichText::new(label).size(11.0).color(color));
            widgets::icon(ui, Icon::Warning, 12.0, color, 1.5);
        }
        _ if dirty => {
            ui.label(RichText::new("未保存").size(11.0).color(Color32::from_rgb(0xf0, 0xb0, 0x5a)));
            let (rect, _) = ui.allocate_exact_size(vec2(8.0, 8.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 3.5, WARN);
        }
        Some(SaveStatus::Saved(time)) => {
            let color = Color32::from_rgb(0x7f, 0xd1, 0x8a);
            ui.label(RichText::new(time).size(11.0).color(color))
                .on_hover_text(format!("{} に保存しました", time));
            widgets::icon(ui, Icon::Check, 12.0, color, 1.8);
        }
        None => {}
    }
}

// 保存やシーンを開くのに失敗したときだけ、理由をパネルの上に出す
fn save_error(ui: &mut Ui, status: Option<&SaveStatus>) {
    let (title, reason) = match status {
        Some(SaveStatus::Failed(reason)) => ("保存できませんでした", reason),
        Some(SaveStatus::OpenFailed(reason)) => ("シーンを開けませんでした", reason),
        _ => return,
    };
    let response = egui::Frame::new()
        .fill(Color32::from_rgb(0x2e, 0x1d, 0x1d))
        .inner_margin(Margin::symmetric(12, 7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 2.0);
                let color = Color32::from_rgb(0xff, 0x8f, 0x8f);
                widgets::icon(ui, Icon::Warning, 13.0, color, 1.5);
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).size(12.0).color(color));
                    widgets::wrapped_label(ui, reason, 11.0, Color32::from_rgb(0xe3, 0xb2, 0xb2));
                });
            });
        });
    let rect = response.response.rect;
    ui.painter()
        .hline(rect.x_range(), rect.bottom() - 0.5, widgets::hairline(BAR_BORDER));
}

// ---------------------------------------------------------------- シーン一覧

fn scene_section(
    ui: &mut Ui,
    layout: &SceneLayout,
    state: &mut EditState,
    import: &ImportState,
    package: &PackageState,
    world: &WorldInfo,
    action: &mut Option<Action>,
) {
    let count = (layout.objects.len() + world.nodes.len() + 2).to_string();
    widgets::section_header(ui, "シーン", Some(count), |_| {});
    package_status(ui, package);

    egui::Frame::new()
        .inner_margin(Margin { left: 10, right: 10, top: 8, bottom: 4 })
        .show(ui, |ui| {
            egui::Frame::new()
                .fill(WELL)
                .stroke(widgets::hairline(WELL_BORDER))
                .corner_radius(4)
                .inner_margin(4)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("scene_list")
                        .max_height(196.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| scene_list(ui, layout, state, import, world, action))
                });
        });
}

// シーンのエクスポート・インポートの結果を見出しの下に 1 行で出す (全文はマウスを乗せると出る)
fn package_status(ui: &mut Ui, package: &PackageState) {
    let (icon, color, fill, line, detail) = if package.busy() {
        (
            Icon::Spinner,
            Color32::from_rgb(0x7f, 0xa8, 0xec),
            Color32::from_rgb(0x1f, 0x26, 0x33),
            "ファイルを選択中…".to_string(),
            "シーンとモデルを .zip にまとめてエクスポート・インポートします".to_string(),
        )
    } else {
        match &package.message {
            Some(PackageMessage::Exported(file)) => (
                Icon::Check,
                Color32::from_rgb(0x8f, 0xd4, 0x9a),
                Color32::from_rgb(0x1d, 0x2b, 0x21),
                format!("{} にエクスポートしました", file),
                format!("{} にシーンとモデルをまとめました", file),
            ),
            Some(PackageMessage::Imported(file)) => (
                Icon::Check,
                Color32::from_rgb(0x8f, 0xd4, 0x9a),
                Color32::from_rgb(0x1d, 0x2b, 0x21),
                format!("シーン「{}」としてインポートしました", file),
                format!("zip の中身をシーン「{}」として保存し、開きました", file),
            ),
            Some(PackageMessage::Failed(e)) => (
                Icon::Warning,
                Color32::from_rgb(0xff, 0x8f, 0x8f),
                Color32::from_rgb(0x2e, 0x1d, 0x1d),
                format!("できませんでした: {}", e),
                e.clone(),
            ),
            None => return,
        }
    };
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::ZERO, fill);
    painter.hline(rect.x_range(), rect.bottom() - 0.5, widgets::hairline(SECTION_BORDER));
    let icon_rect = egui::Rect::from_center_size(egui::pos2(rect.left() + 16.0, rect.center().y), egui::Vec2::splat(11.0));
    widgets::draw_icon(painter, icon_rect, icon, color, 1.8);
    let x = rect.left() + 28.0;
    let galley = widgets::truncated_text(ui, &line, 11.0, color, rect.right() - 10.0 - x);
    ui.painter()
        .galley(egui::pos2(x, rect.center().y - galley.size().y / 2.0), galley, color);
    response.on_hover_text(detail);
}

fn scene_list(
    ui: &mut Ui,
    layout: &SceneLayout,
    state: &mut EditState,
    import: &ImportState,
    world: &WorldInfo,
    action: &mut Option<Action>,
) {
    widgets::group_label(ui, "覗き込みカメラ", 4.0);
    let selected = state.selection == Some(Selection::Window);
    if widgets::list_row(ui, selected, Icon::Monitor, ICON_WINDOW, "のぞき窓", "カメラ").clicked() {
        state.selection = Some(Selection::Window);
    }

    widgets::group_label(ui, "照明", 8.0);
    let selected = state.selection == Some(Selection::Light);
    if widgets::list_row(ui, selected, Icon::Sun, ICON_LIGHT, "ライト", "太陽光・環境光").clicked() {
        state.selection = Some(Selection::Light);
    }

    if let Some(path) = &layout.world {
        widgets::group_label(ui, "ワールド", 8.0);
        if !world.ready {
            let file = path.rsplit('/').next().unwrap_or(path);
            ui.label(RichText::new(format!("{} を読み込み中…", file)).size(11.0).color(TEXT_HINT));
        }
        for node in &world.nodes {
            let selected = state.selection.as_ref() == Some(&Selection::WorldNode(node.name.clone()));
            let tag = if world::node_hidden(layout, &node.name) { "非表示" } else { "ワールド" };
            if widgets::list_row(ui, selected, Icon::Model, ICON_MODEL, &node.name, tag).clicked() {
                state.selection = Some(Selection::WorldNode(node.name.clone()));
            }
        }
    }

    widgets::group_label(ui, "オブジェクト", 8.0);
    let busy = import.busy();
    let label = if busy { "ファイルを選択中…" } else { "モデルを追加…" };
    let add = widgets::add_row(ui, label, ".glb / .gltf", busy)
        .on_hover_text("のぞき窓の正面・床の上に、約30cmの大きさで置かれます");
    if add.clicked() {
        *action = Some(Action::ImportModel);
    }
    if let Some(error) = &state.import_error {
        ui.add_space(4.0);
        notice_card(
            ui,
            Icon::Warning,
            Color32::from_rgb(0xff, 0x8f, 0x8f),
            Color32::from_rgb(0x2e, 0x1d, 0x1d),
            Color32::from_rgb(0x6a, 0x3a, 0x3a),
            "インポートできませんでした",
            error,
        );
    }
    ui.add_space(2.0);
    for object in &layout.objects {
        let selected = state.selection == Some(Selection::Object(object.id));
        let (icon, icon_color) = kind_icon(&object.kind);
        if widgets::list_row(ui, selected, icon, icon_color, &object.name, kind_label(&object.kind)).clicked() {
            state.selection = Some(Selection::Object(object.id));
        }
    }
}

fn notice_card(
    ui: &mut Ui,
    icon: Icon,
    accent: Color32,
    fill: Color32,
    border: Color32,
    title: &str,
    detail: &str,
) {
    egui::Frame::new()
        .fill(fill)
        .stroke(widgets::hairline(border))
        .corner_radius(3)
        .inner_margin(Margin::symmetric(8, 7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 2.0);
                widgets::icon(ui, icon, 14.0, accent, 1.8);
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).size(12.0).color(Color32::from_rgb(0xcf, 0xdc, 0xf4)));
                    widgets::wrapped_label(ui, detail, 10.5, Color32::from_rgb(0x9a, 0xa6, 0xbb));
                });
            });
        });
    ui.add_space(4.0);
}

// ---------------------------------------------------------------- プロパティ

fn properties(
    ui: &mut Ui,
    layout: &mut SceneLayout,
    config: &mut AppConfig,
    state: &mut EditState,
    world: &WorldInfo,
    panel: &mut PanelUi,
    action: &mut Option<Action>,
) {
    egui::Frame::new().inner_margin(10).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 10.0;
        match state.selection.clone() {
            Some(Selection::Window) => window_properties(ui, layout, config, world, panel, action),
            Some(Selection::Light) => light_properties(ui, layout, world, action),
            Some(Selection::WorldNode(name)) => match world.node(&name) {
                Some(node) => world_node_properties(ui, layout, world, node, action),
                None => nothing_selected(ui, state),
            },
            Some(Selection::Object(id)) => match layout.object_mut(id) {
                Some(object) => object_properties(ui, object, action),
                None => nothing_selected(ui, state),
            },
            None => nothing_selected(ui, state),
        }
    });
}

fn nothing_selected(ui: &mut Ui, state: &mut EditState) {
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.add_space(18.0);
        widgets::icon(ui, Icon::Cursor, 28.0, Color32::from_gray(0x6b), 1.0);
        ui.label(RichText::new("何も選択されていません").size(13.0).color(Color32::from_gray(0xd0)));
        ui.add_space(8.0);
        let open = Button::new("のぞき窓・画面の設定を開く", widgets::BUTTON)
            .colored_icon(Icon::Monitor, ICON_WINDOW)
            .width(ui.available_width())
            .height(30.0)
            .show(ui);
        if open.clicked() {
            state.selection = Some(Selection::Window);
        }
        ui.add_space(14.0);
    });
}

// シーンの照明 (アプリの太陽光と環境光)
fn light_properties(ui: &mut Ui, layout: &mut SceneLayout, world: &WorldInfo, action: &mut Option<Action>) {
    const LABEL: f32 = 72.0;
    identity(ui, Icon::Sun, ICON_LIGHT, |ui| {
        ui.label(RichText::new("ライト").size(13.0).color(TEXT_STRONG));
        ui.label(RichText::new("太陽光の明るさと向き、環境光").size(10.5).color(TEXT_MUTED));
    });

    // ワールドのライトを使っていて、アプリの太陽光は消えているか
    let sun_off = layout.world.is_some() && world.has_lights;
    let light = &mut layout.light;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 6.0);
        widgets::sub_header(ui, "太陽光");
        ui.horizontal(|ui| {
            widgets::fixed_label(ui, LABEL, 24.0, "明るさ", 12.0, TEXT_LABEL, Align::Min);
            NumberField::new(&mut light.illuminance, 50.0, 0)
                .unit("lx")
                .range(0.0..=200_000.0)
                .show(ui, ui.available_width())
                .on_hover_text("晴れた日中で 10000〜100000、曇りで 1000〜10000 くらい");
        });
        ui.horizontal(|ui| {
            let width = (ui.available_width() - LABEL - 4.0) / 2.0;
            widgets::fixed_label(ui, LABEL, 24.0, "向き", 12.0, TEXT_LABEL, Align::Min);
            NumberField::new(&mut light.pitch_deg, 1.0, 1)
                .prefix("傾き")
                .unit("°")
                .accent(AXIS_X)
                .show(ui, width)
                .on_hover_text("X軸まわりの角度。-90 で真上から、0 で水平に照らす");
            NumberField::new(&mut light.yaw_deg, 1.0, 1)
                .prefix("方角")
                .unit("°")
                .accent(AXIS_Y)
                .show(ui, width)
                .on_hover_text("Y軸まわりの角度。0 で手前から奥へ照らす");
        });
        if sun_off {
            widgets::wrapped_label(
                ui,
                "ワールドにライトがあるため、この太陽光は消えています（環境光は効きます）。",
                10.5,
                Color32::from_rgb(0xf0, 0xb0, 0x5a),
            );
        }

        widgets::sub_header(ui, "環境光");
        ui.horizontal(|ui| {
            widgets::fixed_label(ui, LABEL, 24.0, "明るさ", 12.0, TEXT_LABEL, Align::Min);
            NumberField::new(&mut light.ambient, 1.0, 0)
                .range(0.0..=5000.0)
                .show(ui, ui.available_width())
                .on_hover_text("影になる側が真っ黒にならないよう、全体をうっすら照らす光");
        });
    });

    let is_default = layout.light == Default::default();
    if Button::new("初期値に戻す", widgets::BUTTON)
        .width(ui.available_width())
        .enabled(!is_default)
        .show(ui)
        .clicked()
    {
        *action = Some(Action::ResetLight);
    }
}

// ワールドのノード。触っていなければ Blender の値を出し、値を変えたら変更として記録する
fn world_node_properties(
    ui: &mut Ui,
    layout: &mut SceneLayout,
    world: &WorldInfo,
    node: &WorldNodeInfo,
    action: &mut Option<Action>,
) {
    let hidden = world::node_hidden(layout, &node.name);
    let changed = layout.world_override(&node.name).is_some_and(|o| o.placement.is_some());
    identity(ui, Icon::Model, ICON_MODEL, |ui| {
        ui.label(RichText::new(&node.name).size(13.0).color(TEXT_STRONG));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            kind_chip(ui, "ワールド", ICON_MODEL);
            let note = match (changed, hidden) {
                (_, true) => "非表示",
                (true, false) => "アプリで変更",
                (false, false) => "Blender の値",
            };
            ui.label(RichText::new(note).size(10.5).color(TEXT_HINT));
        });
    });

    let Some(before) = world::node_placement(layout, world, &node.name) else {
        return;
    };
    let mut placement = before.clone();
    placement_editor(ui, &mut placement, true);
    if placement != before {
        layout.world_override_mut(&node.name).placement = Some(placement);
    }
    widgets::wrapped_label(ui, "大きさは Blender での大きさに掛ける倍率です。", 10.5, TEXT_HINT);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let width = (ui.available_width() - 6.0) / 2.0;
        let label = if hidden { "表示する" } else { "非表示にする" };
        if Button::new(label, widgets::BUTTON).width(width).show(ui).clicked() {
            *action = Some(Action::ToggleHidden(node.name.clone()));
        }
        if Button::new("Blender の値に戻す", widgets::BUTTON)
            .width(width)
            .enabled(changed)
            .show(ui)
            .clicked()
        {
            *action = Some(Action::ResetWorldNode(node.name.clone()));
        }
    });
}

fn object_properties(ui: &mut Ui, object: &mut SceneObject, action: &mut Option<Action>) {
    let (icon, icon_color) = kind_icon(&object.kind);
    let detail = match &object.kind {
        ObjectKind::Model { path } => Some(format!("assets/{}", path)),
        ObjectKind::Cube { .. } => None,
    };
    identity(ui, icon, icon_color, |ui| {
        name_edit(ui, &mut object.name, object.id);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            kind_chip(ui, kind_label(&object.kind), icon_color);
            if let Some(path) = &detail {
                let galley = widgets::truncated_text(ui, path, 10.5, TEXT_HINT, ui.available_width());
                ui.label(galley).on_hover_text(path);
            }
        });
    });

    placement_editor(ui, &mut object.placement, true);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let width = (ui.available_width() - 6.0) / 2.0;
        let duplicate = Button::new("複製", widgets::BUTTON)
            .icon(Icon::Copy)
            .width(width)
            .show(ui);
        if duplicate.clicked() {
            *action = Some(Action::Duplicate(object.id));
        }
        let delete = Button::new("削除…", widgets::BUTTON_DANGER)
            .icon(Icon::Trash)
            .width(width)
            .show(ui);
        if delete.clicked() {
            *action = Some(Action::AskDelete(object.id));
        }
    });
}

// 選択中のものの見出し: 左に大きめのアイコン、右に名前など
fn identity(ui: &mut Ui, icon: Icon, color: Color32, contents: impl FnOnce(&mut Ui)) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = vec2(10.0, 5.0);
        let (rect, _) = ui.allocate_exact_size(vec2(34.0, 34.0), egui::Sense::hover());
        ui.painter().rect(
            rect,
            CornerRadius::same(6),
            color.gamma_multiply(0.12),
            widgets::hairline(color.gamma_multiply(0.35)),
            egui::StrokeKind::Inside,
        );
        widgets::draw_icon(ui.painter(), egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(17.0)), icon, color, 1.3);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 5.0;
            contents(ui);
        });
    });
}

// 「モデル」などの種類の札
fn kind_chip(ui: &mut Ui, label: &str, color: Color32) {
    let galley = widgets::text(ui, label, 10.0, color);
    let (rect, _) = ui.allocate_exact_size(vec2(galley.size().x + 12.0, 16.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(3), color.gamma_multiply(0.15));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
}

fn name_edit(ui: &mut Ui, name: &mut String, id: u64) {
    ui.scope(|ui| {
        let visuals = ui.visuals_mut();
        visuals.extreme_bg_color = WELL;
        visuals.text_edit_bg_color = Some(WELL);
        visuals.selection.stroke = widgets::hairline(FOCUS);
        visuals.widgets.inactive.bg_stroke = widgets::hairline(Color32::from_gray(0x3c));
        visuals.widgets.hovered.bg_stroke = widgets::hairline(Color32::from_gray(0x50));
        for w in [&mut visuals.widgets.inactive, &mut visuals.widgets.hovered, &mut visuals.widgets.active] {
            w.corner_radius = CornerRadius::same(4);
            w.expansion = 0.0;
        }
        ui.add(
            egui::TextEdit::singleline(name)
                .id_salt(("object_name", id))
                .font(egui::FontId::proportional(12.5))
                .text_color(TEXT_STRONG)
                .margin(Margin::symmetric(8, 4))
                .desired_width(f32::INFINITY)
                .min_size(vec2(0.0, 26.0))
                .hint_text("名前"),
        );
    });
}

// 位置・回転 (・大きさ)。左にラベルの列、右に X / Y / Z の列を並べた表
fn placement_editor(ui: &mut Ui, placement: &mut Placement, scalable: bool) {
    const LABEL: f32 = 52.0;
    const GAP: f32 = 4.0;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(GAP, 6.0);
        widgets::sub_header(ui, "トランスフォーム");

        let column = (ui.available_width() - LABEL - GAP * 3.0) / 3.0;
        ui.horizontal(|ui| {
            ui.add_space(LABEL);
            for (axis, color) in [("X", AXIS_X), ("Y", AXIS_Y), ("Z", AXIS_Z)] {
                widgets::fixed_label(ui, column, 14.0, axis, 10.5, color, Align::Center);
            }
        });

        ui.horizontal(|ui| {
            row_label(ui, LABEL, "位置", "m");
            for (color, value) in [
                (AXIS_X, &mut placement.position.x),
                (AXIS_Y, &mut placement.position.y),
                (AXIS_Z, &mut placement.position.z),
            ] {
                NumberField::new(value, 0.005, 3).accent(color).show(ui, column);
            }
        });

        ui.horizontal(|ui| {
            row_label(ui, LABEL, "回転", "°");
            for (color, value, hint) in [
                (AXIS_X, &mut placement.pitch_deg, "X軸まわりの回転"),
                (AXIS_Y, &mut placement.yaw_deg, "Y軸まわりの回転"),
                (AXIS_Z, &mut placement.roll_deg, "Z軸まわりの回転"),
            ] {
                NumberField::new(value, 1.0, 1)
                    .accent(color)
                    .show(ui, column)
                    .on_hover_text(hint);
            }
        });

        if scalable {
            ui.horizontal(|ui| {
                row_label(ui, LABEL, "大きさ", "");
                NumberField::new(&mut placement.scale, 0.005, 4)
                    .prefix("×")
                    .range(0.00001..=1000.0)
                    .show(ui, ui.available_width());
            });
        }
    });
}

// 表の左の列のラベル。単位は小さく添える
fn row_label(ui: &mut Ui, width: f32, label: &str, unit: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 24.0), egui::Sense::hover());
    let painter = ui.painter();
    let label = painter.layout_no_wrap(label.to_owned(), egui::FontId::proportional(12.0), TEXT_LABEL);
    let label_width = label.size().x;
    painter.galley(egui::pos2(rect.left(), rect.center().y - label.size().y / 2.0), label, TEXT_LABEL);
    if !unit.is_empty() {
        let unit = painter.layout_no_wrap(unit.to_owned(), egui::FontId::proportional(10.5), TEXT_FAINT);
        painter.galley(
            egui::pos2(rect.left() + label_width + 4.0, rect.center().y - unit.size().y / 2.0),
            unit,
            TEXT_FAINT,
        );
    }
}

fn window_properties(
    ui: &mut Ui,
    layout: &mut SceneLayout,
    config: &mut AppConfig,
    world: &WorldInfo,
    panel: &mut PanelUi,
    action: &mut Option<Action>,
) {
    const LABEL: f32 = 72.0;
    const GAP: f32 = 4.0;

    identity(ui, Icon::Monitor, ICON_WINDOW, |ui| {
        ui.label(RichText::new("のぞき窓").size(13.0).color(TEXT_STRONG));
        ui.label(RichText::new("実際のディスプレイの位置と大きさ").size(10.5).color(TEXT_MUTED));
    });

    placement_editor(ui, &mut layout.window, false);
    if layout.world.is_some() {
        let can_reset = world.window.is_some();
        let reset = Button::new("Blender の位置に戻す", widgets::BUTTON)
            .width(ui.available_width())
            .enabled(can_reset)
            .show(ui);
        if reset.clicked() {
            *action = Some(Action::ResetWindow);
        }
        if !can_reset && world.ready {
            widgets::wrapped_label(
                ui,
                &format!("ワールドに {} がありません。Blender で同じ名前のカメラを置くと、その位置が使えます。", world::PEEK_WINDOW_NODE),
                10.5,
                TEXT_HINT,
            );
        }
    }

    // 画面サイズ
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(GAP, 6.0);
        let mut open = !panel.screen_closed;
        let summary = format!(
            "{:.1}″ · {}:{}",
            config.screen_diagonal_inches,
            format_ratio(config.screen_aspect_width),
            format_ratio(config.screen_aspect_height)
        );
        widgets::collapsing_header(ui, &mut open, "画面サイズ", &summary);
        panel.screen_closed = !open;
        if !open {
            return;
        }

        ui.horizontal(|ui| {
            widgets::fixed_label(ui, LABEL, 24.0, "対角", 12.0, TEXT_LABEL, Align::Min);
            NumberField::new(&mut config.screen_diagonal_inches, 0.1, 1)
                .unit("インチ")
                .range(1.0..=200.0)
                .show(ui, ui.available_width());
        });
        ui.horizontal(|ui| {
            let width = (ui.available_width() - LABEL - 10.0 - GAP * 3.0) / 2.0;
            widgets::fixed_label(ui, LABEL, 24.0, "アスペクト比", 12.0, TEXT_LABEL, Align::Min);
            NumberField::new(&mut config.screen_aspect_width, 0.1, 1)
                .prefix("横")
                .range(0.1..=100.0)
                .show(ui, width);
            widgets::fixed_label(ui, 10.0, 24.0, ":", 12.0, TEXT_MUTED, Align::Center);
            NumberField::new(&mut config.screen_aspect_height, 0.1, 1)
                .prefix("縦")
                .range(0.1..=100.0)
                .show(ui, width);
        });
        ui.horizontal(|ui| {
            let width = (ui.available_width() - LABEL - GAP * 3.0) / 3.0;
            widgets::fixed_label(ui, LABEL, 22.0, "よく使う比", 11.0, TEXT_MUTED, Align::Min);
            let current = config.screen_aspect_width / config.screen_aspect_height;
            for (w, h) in ASPECT_PRESETS {
                let on = (current - w / h).abs() < 1e-3;
                let style = if on { widgets::BUTTON_CHIP_ON } else { widgets::BUTTON_CHIP };
                let label = format!("{}:{}", w, h);
                let clicked = Button::new(&label, style)
                    .width(width)
                    .height(22.0)
                    .font_size(11.0)
                    .radius(3)
                    .show(ui)
                    .clicked();
                if clicked {
                    config.screen_aspect_width = w;
                    config.screen_aspect_height = h;
                }
            }
        });
        let (width, height) = config.screen_size();
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), egui::Sense::hover());
        let painter = ui.painter();
        painter.rect(rect, CornerRadius::same(4), WELL, widgets::hairline(WELL_BORDER), egui::StrokeKind::Inside);
        let inner = rect.shrink2(vec2(8.0, 0.0));
        painter.text(inner.left_center(), Align2::LEFT_CENTER, "実寸", egui::FontId::proportional(11.0), TEXT_MUTED);
        painter.text(
            inner.right_center(),
            Align2::RIGHT_CENTER,
            format!("幅 {:.3} m × 高さ {:.3} m", width, height),
            egui::FontId::proportional(12.0),
            Color32::from_gray(0xdc),
        );
    });

    // 目と画面の距離
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(GAP, 6.0);
        let mut open = !panel.eye_closed;
        let summary = format!("{:.3} m", config.viewer_base_distance);
        widgets::collapsing_header(ui, &mut open, "目と画面の距離", &summary);
        panel.eye_closed = !open;
        if !open {
            return;
        }

        ui.horizontal(|ui| {
            let width = (ui.available_width() - LABEL * 2.0 - GAP * 3.0) / 2.0;
            widgets::fixed_label(ui, LABEL, 24.0, "基準距離", 12.0, TEXT_LABEL, Align::Min);
            NumberField::new(&mut config.viewer_base_distance, 0.005, 3)
                .unit("m")
                .range(0.01..=5.0)
                .show(ui, width)
                .on_hover_text("opentrack でセンターを合わせた姿勢のときの、目から画面までの距離");
            widgets::fixed_label(ui, LABEL, 24.0, "最小距離", 12.0, TEXT_LABEL, Align::Max);
            NumberField::new(&mut config.viewer_min_distance, 0.005, 3)
                .unit("m")
                .range(0.001..=5.0)
                .show(ui, width);
        });
    });
}

// 16.0 → "16"、16.5 → "16.5"
fn format_ratio(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{:.1}", value)
    }
}

// ---------------------------------------------------------------- 操作方法

fn help_window(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new("操作方法")
        .open(open)
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.style_mut().interaction.selectable_labels = false;
            for (key, description) in HELP_ROWS {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.allocate_ui_with_layout(vec2(140.0, 0.0), Layout::top_down(Align::Min), |ui| {
                        ui.set_width(140.0);
                        ui.label(RichText::new(key).size(12.0).color(Color32::from_gray(0xe0)));
                    });
                    widgets::wrapped_label(ui, description, 12.0, TEXT_DIM);
                });
                ui.add_space(5.0);
            }
            widgets::wrapped_label(
                ui,
                "文字を入力している間はキー操作が効きません。C と F12 はトラッキング用に予約されています。",
                11.0,
                TEXT_HINT,
            );
        });
}

// ---------------------------------------------------------------- ダイアログ

fn dialog(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    icon: Icon,
    icon_color: Color32,
    title: &str,
    body: &str,
    buttons: impl FnOnce(&mut Ui),
) {
    dialog_frame(ctx, id, width, icon, icon_color, title, |ui| {
        widgets::wrapped_label(ui, body, 13.0, TEXT_SUB);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            buttons(ui);
        });
    });
}

fn dialog_frame(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    icon: Icon,
    icon_color: Color32,
    title: &str,
    contents: impl FnOnce(&mut Ui),
) {
    let frame = egui::Frame::new()
        .fill(Color32::from_gray(0x2b))
        .stroke(widgets::hairline(Color32::from_gray(0x47)))
        .corner_radius(8)
        .inner_margin(20)
        .shadow(egui::Shadow {
            offset: [0, 16],
            blur: 48,
            spread: 0,
            color: Color32::from_black_alpha(140),
        });
    egui::Modal::new(egui::Id::new(id))
        .frame(frame)
        .backdrop_color(Color32::from_black_alpha(148))
        .show(ctx, |ui| {
            ui.style_mut().interaction.selectable_labels = false;
            ui.set_width(width - 40.0);
            ui.spacing_mut().item_spacing.y = 14.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                widgets::icon(ui, icon, 18.0, icon_color, 1.4);
                ui.label(RichText::new(title).size(15.0).color(TEXT_STRONG));
            });
            contents(ui);
        });
}

fn dialog_button(ui: &mut Ui, label: &str, style: widgets::ButtonStyle) -> bool {
    Button::new(label, style).height(32.0).padding(12.0).show(ui).clicked()
}

// 未保存の変更があるときに、続ける操作の前に出す確認
fn discard_dialog(ctx: &egui::Context, pending: &Pending, untitled: bool, action: &mut Option<Action>) {
    let (body, verb) = match pending {
        Pending::Exit => ("このままホームに戻ると、最後に保存してからの変更は失われます。", "戻る"),
        Pending::New => ("新しいシーンを作ると、最後に保存してからの変更は失われます。", "新規作成"),
        Pending::Open(_) => ("別のシーンを開くと、最後に保存してからの変更は失われます。", "開く"),
        Pending::ImportScene => ("シーンをインポートすると、最後に保存してからの変更は失われます。", "インポート"),
        Pending::ImportWorld => ("ワールドをインポートすると新しいシーンになり、最後に保存してからの変更は失われます。", "インポート"),
    };
    let body = if untitled {
        format!("{} このシーンはまだ名前を付けて保存していません。", body)
    } else {
        body.to_string()
    };
    let save_label = format!("保存して{}", verb);
    let discard_label = format!("保存せずに{}", verb);
    dialog(ctx, "confirm_discard", 460.0, Icon::Warning, WARN, "保存していない変更があります", &body, |ui| {
        if dialog_button(ui, &discard_label, widgets::BUTTON_DANGER_GHOST) {
            *action = Some(Action::DiscardAndContinue);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if dialog_button(ui, &save_label, widgets::BUTTON_PRIMARY) {
                *action = Some(Action::SaveAndContinue);
            }
            if dialog_button(ui, "キャンセル", widgets::BUTTON_SECONDARY) {
                *action = Some(Action::CancelDiscard);
            }
        });
    });
}

fn save_as_dialog(ctx: &egui::Context, dialog: &mut SaveAsDialog, current: Option<&str>, action: &mut Option<Action>) {
    let overwrite = library::validate_name(&dialog.name)
        .is_ok_and(|name| Some(name.as_str()) != current && library::exists(&name));
    dialog_frame(ctx, "save_as", 420.0, Icon::Check, ICON_WINDOW, "名前を付けて保存", |ui| {
        let edit = ui.add(
            egui::TextEdit::singleline(&mut dialog.name)
                .desired_width(f32::INFINITY)
                .font(egui::FontId::proportional(14.0)),
        );
        if !edit.has_focus() && !edit.lost_focus() {
            edit.request_focus();
        }
        if edit.changed() {
            dialog.error = None;
        }
        if let Some(error) = &dialog.error {
            widgets::wrapped_label(ui, error, 12.0, Color32::from_rgb(0xff, 0x8f, 0x8f));
        } else if overwrite {
            widgets::wrapped_label(ui, "同じ名前のシーンがあります。保存すると上書きされます。", 12.0, Color32::from_rgb(0xf0, 0xb0, 0x5a));
        } else {
            widgets::wrapped_label(ui, "scenes フォルダに保存されます。", 12.0, TEXT_HINT);
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = if overwrite { "上書き保存" } else { "保存" };
                if dialog_button(ui, label, widgets::BUTTON_PRIMARY) {
                    *action = Some(Action::ConfirmSaveAs);
                }
                if dialog_button(ui, "キャンセル", widgets::BUTTON_SECONDARY) {
                    *action = Some(Action::CancelSaveAs);
                }
            });
        });
    });
}

fn delete_scene_dialog(ctx: &egui::Context, name: &str, is_current: bool, action: &mut Option<Action>) {
    let title = format!("シーン「{}」を削除しますか?", name);
    let mut body = "シーンファイルを削除します。元に戻せません。ワールドやモデルのファイルは消えません。".to_string();
    if is_current {
        body.push_str("今開いているシーンなので、削除したあとは別のシーンを開きます。");
    }
    dialog(ctx, "confirm_delete_scene", 440.0, Icon::Trash, DANGER, &title, &body, |ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if dialog_button(ui, "削除する", widgets::BUTTON_DANGER_SOLID) {
                *action = Some(Action::DeleteScene(name.to_string()));
            }
            if dialog_button(ui, "キャンセル", widgets::BUTTON_SECONDARY) {
                *action = Some(Action::CancelDeleteScene);
            }
        });
    });
}

fn delete_dialog(ctx: &egui::Context, layout: &SceneLayout, id: u64, action: &mut Option<Action>) {
    let Some(object) = layout.object(id) else {
        *action = Some(Action::CancelDelete);
        return;
    };
    let title = format!("「{}」を削除しますか?", object.name);
    let body = match &object.kind {
        ObjectKind::Model { path } => {
            let file = std::path::Path::new(path)
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            format!("シーンから取り除きます。元のモデルファイル({})は消えません。", file)
        }
        ObjectKind::Cube { .. } => "シーンから取り除きます。".to_string(),
    };
    dialog(ctx, "confirm_delete", 420.0, Icon::Trash, DANGER, &title, &body, |ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if Button::new("削除する", widgets::BUTTON_DANGER_SOLID)
                .height(32.0)
                .padding(12.0)
                .show(ui)
                .clicked()
            {
                *action = Some(Action::Delete(id));
            }
            if Button::new("キャンセル", widgets::BUTTON_SECONDARY)
                .height(32.0)
                .padding(12.0)
                .show(ui)
                .clicked()
            {
                *action = Some(Action::CancelDelete);
            }
        });
    });
}
