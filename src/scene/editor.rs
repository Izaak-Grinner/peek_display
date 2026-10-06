use bevy::prelude::*;

use crate::app_mode::AppMode;
use crate::config::AppConfig;
use crate::scene::layout::{Placement, SceneLayout};
use crate::scene::library::{self, SceneLibrary};
use crate::scene::scene::Placeable;
use crate::scene::world::{self, WorldInfo, WorldNode};
use crate::ui::UiInputState;

// キー操作の速さ (Shift を押している間は 1/10)
const MOVE_SPEED: f32 = 0.2; // m/s
const ROTATE_SPEED: f32 = 60.0; // 度/s
const SCALE_SPEED: f32 = 1.5; // 倍/s
const SLOW_FACTOR: f32 = 0.1;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Selection {
    Window,
    Object(u64),
    // ワールドの中のノード (Blender のオブジェクト名)
    WorldNode(String),
}

// 最後に保存したときの結果
#[derive(Clone, Debug)]
pub enum SaveStatus {
    // 保存した時刻 ("14:32")
    Saved(String),
    // 保存に失敗した理由
    Failed(String),
    // シーンを開けなかった理由
    OpenFailed(String),
}

// 未保存の変更を確認してから行う操作
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    // ホームへ戻る
    Exit,
    // 新規シーン
    New,
    // 保存してあるシーンを開く
    Open(String),
    // .zip のシーンをインポートする
    ImportScene,
    // ワールド (.glb) をインポートし、新しいシーンとして開く
    ImportWorld,
}

// 「名前を付けて保存」ダイアログの入力
#[derive(Clone, Debug)]
pub struct SaveAsDialog {
    pub name: String,
    pub error: Option<String>,
    // 保存できたら続けて行う操作
    pub then: Option<Pending>,
}

// 編集モードの状態。保存済みの内容を持っておき、未保存の変更があるか比べる
#[derive(Resource)]
pub struct EditState {
    pub selection: Option<Selection>,
    pub save_status: Option<SaveStatus>,
    pub import_error: Option<String>,
    // 未保存の変更があるときの確認ダイアログ (確認後に行う操作)
    pub confirm_discard: Option<Pending>,
    pub save_as: Option<SaveAsDialog>,
    // 削除の確認ダイアログを出しているオブジェクト
    pub confirm_delete: Option<u64>,
    saved_layout: SceneLayout,
    saved_config: AppConfig,
}

impl FromWorld for EditState {
    fn from_world(world: &mut World) -> Self {
        Self {
            selection: None,
            save_status: None,
            import_error: None,
            confirm_discard: None,
            save_as: None,
            confirm_delete: None,
            saved_layout: world.resource::<SceneLayout>().clone(),
            saved_config: world.resource::<AppConfig>().clone(),
        }
    }
}

impl EditState {
    // 名前の無いシーンは、保存するまで常に未保存として扱う
    pub fn is_dirty(&self, layout: &SceneLayout, config: &AppConfig, library: &SceneLibrary) -> bool {
        library.current.is_none() || !layout.same_content(&self.saved_layout) || *config != self.saved_config
    }

    // ダイアログが出ているか (出ている間は 3D 側のキー操作を止める)
    pub fn dialog_open(&self) -> bool {
        self.confirm_discard.is_some() || self.save_as.is_some() || self.confirm_delete.is_some()
    }

    // 今のシーンと config.ini を上書き保存する。名前が無いシーンは保存できない
    pub fn save(&mut self, layout: &SceneLayout, config: &AppConfig, library: &SceneLibrary) -> bool {
        let result = library.save(layout).and_then(|_| save_config(config));
        self.finish_save(result, layout, config)
    }

    // 名前を付けて保存する。名前が正しくないときはダイアログに理由を出す
    pub fn save_as(
        &mut self,
        name: &str,
        layout: &SceneLayout,
        config: &AppConfig,
        library: &mut SceneLibrary,
    ) -> bool {
        if let Err(e) = library::validate_name(name) {
            if let Some(dialog) = &mut self.save_as {
                dialog.error = Some(e);
            }
            return false;
        }
        let result = library.save_as(name, layout).and_then(|_| save_config(config));
        self.finish_save(result, layout, config)
    }

    fn finish_save(&mut self, result: Result<(), String>, layout: &SceneLayout, config: &AppConfig) -> bool {
        match result {
            Ok(()) => {
                self.saved_layout = layout.clone();
                self.saved_config = config.clone();
                self.save_status = Some(SaveStatus::Saved(local_time_hhmm()));
                info!("シーンと config.ini を保存しました");
                true
            }
            Err(e) => {
                warn!("保存できませんでした: {}", e);
                self.save_status = Some(SaveStatus::Failed(e));
                false
            }
        }
    }

    // シーンファイルに書き込んだ内容を、保存済みの状態とする (選択などはそのまま)
    pub fn mark_layout_saved(&mut self, layout: &SceneLayout) {
        self.saved_layout = layout.clone();
    }

    // 別のシーンに切り替えたとき。その内容を保存済みの状態とする
    pub fn scene_switched(&mut self, layout: &SceneLayout) {
        self.saved_layout = layout.clone();
        self.selection = None;
        self.save_status = None;
        self.confirm_delete = None;
    }

    // 保存していない変更を捨てて、保存してある内容に戻す。
    // 名前の無いシーンは捨てて、最後に保存したシーンを開き直す
    pub fn discard(&mut self, layout: &mut SceneLayout, config: &mut AppConfig, library: &mut SceneLibrary) {
        if library.current.is_some() {
            layout.replace_with(self.saved_layout.clone());
        } else {
            layout.replace_with(library.reopen_last());
        }
        *config = self.saved_config.clone();
        self.scene_switched(layout);
    }
}

fn save_config(config: &AppConfig) -> Result<(), String> {
    config
        .save()
        .map_err(|e| format!("config.ini に書き込めません ({})", e))
}

// 今の時刻 (ローカル時刻の "時:分")
#[cfg(windows)]
fn local_time_hhmm() -> String {
    use windows_sys::Win32::{Foundation::SYSTEMTIME, System::SystemInformation::GetLocalTime};
    let mut time: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut time) };
    format!("{}:{:02}", time.wHour, time.wMinute)
}

#[cfg(not(windows))]
fn local_time_hhmm() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}:{:02} UTC", secs / 3600 % 24, secs / 60 % 60)
}

pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EditState>()
            .add_observer(select_on_click)
            .add_systems(
                Update,
                (keyboard_edit, draw_selection).run_if(in_state(AppMode::Edit)),
            );
    }
}

// 俯瞰画面でオブジェクトをクリックして選択する。
// glb の子メッシュに当たった場合も、親をたどって Placeable (ワールドのノードなら WorldNode) を探す
fn select_on_click(
    mut click: On<Pointer<Click>>,
    mode: Res<State<AppMode>>,
    ui_input: Res<UiInputState>,
    parents: Query<&ChildOf>,
    placeables: Query<&Placeable>,
    world_nodes: Query<&WorldNode>,
    mut state: ResMut<EditState>,
) {
    if *mode.get() != AppMode::Edit
        || click.event.button != PointerButton::Primary
        || ui_input.pointer
    {
        return;
    }

    let mut entity = click.entity;
    loop {
        if let Ok(placeable) = placeables.get(entity) {
            state.selection = Some(Selection::Object(placeable.0));
            click.propagate(false);
            return;
        }
        if let Ok(node) = world_nodes.get(entity) {
            state.selection = Some(Selection::WorldNode(node.0.clone()));
            click.propagate(false);
            return;
        }
        match parents.get(entity) {
            Ok(child_of) => entity = child_of.parent(),
            Err(_) => break,
        }
    }
    // 部屋など、配置対象でないものをクリックしたら選択を外す
    state.selection = None;
}

fn keyboard_edit(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    ui_input: Res<UiInputState>,
    state: Res<EditState>,
    world: Res<WorldInfo>,
    mut layout: ResMut<SceneLayout>,
) {
    // Ctrl+S などのショートカットで動かないようにする
    if ui_input.keyboard
        || state.dialog_open()
        || keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
    {
        return;
    }
    let Some(selection) = &state.selection else {
        return;
    };
    let axis = |plus: &[KeyCode], minus: &[KeyCode]| -> f32 {
        let pressed = |codes: &[KeyCode]| codes.iter().any(|k| keys.pressed(*k));
        pressed(plus) as i32 as f32 - pressed(minus) as i32 as f32
    };

    let forward = axis(&[KeyCode::ArrowUp], &[KeyCode::ArrowDown]);
    let right = axis(&[KeyCode::ArrowRight], &[KeyCode::ArrowLeft]);
    let up = axis(&[KeyCode::PageUp], &[KeyCode::PageDown]);
    let turn_x = axis(&[KeyCode::KeyR], &[KeyCode::KeyF]);
    let turn_y = axis(&[KeyCode::KeyQ], &[KeyCode::KeyE]);
    // C はトラッキング用に予約されているので使わない
    let turn_z = axis(&[KeyCode::KeyZ], &[KeyCode::KeyX]);
    let grow = axis(
        &[KeyCode::Equal, KeyCode::NumpadAdd],
        &[KeyCode::Minus, KeyCode::NumpadSubtract],
    );
    if [forward, right, up, turn_x, turn_y, turn_z, grow].iter().all(|v| *v == 0.0) {
        return; // 何も押していないときは SceneLayout を変更扱いにしない
    }

    let slow = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let dt = time.delta_secs() * if slow { SLOW_FACTOR } else { 1.0 };

    // ワールド座標の軸に沿って動かす (↑ = -Z、→ = +X)
    let delta = Vec3::new(right, up, -forward) * MOVE_SPEED * dt;

    let apply = |placement: &mut Placement, scalable: bool| {
        placement.position += delta;
        placement.pitch_deg += turn_x * ROTATE_SPEED * dt;
        placement.yaw_deg += turn_y * ROTATE_SPEED * dt;
        placement.roll_deg += turn_z * ROTATE_SPEED * dt;
        if scalable {
            placement.scale = (placement.scale * SCALE_SPEED.powf(grow * dt)).max(1e-5);
        }
    };

    match selection {
        Selection::Window => apply(&mut layout.window, false),
        Selection::Object(id) => {
            if let Some(object) = layout.object_mut(*id) {
                apply(&mut object.placement, true);
            }
        }
        Selection::WorldNode(name) => {
            // 初めて動かしたときに、Blender の値から変更として記録する
            if let Some(mut placement) = world::node_placement(&layout, &world, name) {
                apply(&mut placement, true);
                layout.world_override_mut(name).placement = Some(placement);
            }
        }
    }
}


// 選択中の対象に目印を描く (ギズモは俯瞰カメラにしか映らない)
fn draw_selection(
    mut gizmos: Gizmos,
    state: Res<EditState>,
    layout: Res<SceneLayout>,
    config: Res<AppConfig>,
    world: Res<WorldInfo>,
    globals: Query<&GlobalTransform>,
) {
    let highlight = LinearRgba::new(1.0, 0.9, 0.0, 1.0);
    match &state.selection {
        Some(Selection::Window) => {
            let (width, height) = config.screen_size();
            gizmos.rect(
                Isometry3d::new(layout.window.position, layout.window.rotation()),
                Vec2::new(width, height) * 1.05,
                highlight,
            );
        }
        Some(Selection::Object(id)) => {
            if let Some(object) = layout.object(*id) {
                let placement = &object.placement;
                gizmos.axes(
                    Transform::from_translation(placement.position)
                        .with_rotation(placement.rotation()),
                    0.1,
                );
                gizmos.sphere(placement.position, 0.01, highlight);
            }
        }
        Some(Selection::WorldNode(name)) => {
            let global = world.node(name).and_then(|n| globals.get(n.entity).ok());
            if let Some(global) = global {
                let (_, rotation, translation) = global.to_scale_rotation_translation();
                gizmos.axes(Transform::from_translation(translation).with_rotation(rotation), 0.1);
                gizmos.sphere(translation, 0.01, highlight);
            }
        }
        None => {}
    }
}
