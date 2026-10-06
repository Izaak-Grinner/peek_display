use bevy::prelude::*;

use crate::app_mode::AppMode;
use crate::config::AppConfig;
use crate::scene::layout::{Placement, SceneLayout};
use crate::scene::library::{self, SceneLibrary};
use crate::scene::highlight::SELECTION_COLOR;
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

// 元に戻せる回数
const HISTORY_LIMIT: usize = 100;

// 元に戻す・やり直しで記録する状態
#[derive(Clone)]
struct Snapshot {
    layout: SceneLayout,
    config: AppConfig,
}

impl Snapshot {
    fn new(layout: &SceneLayout, config: &AppConfig) -> Self {
        Self {
            layout: layout.clone(),
            config: config.clone(),
        }
    }

    fn same_as(&self, layout: &SceneLayout, config: &AppConfig) -> bool {
        self.layout.same_content(layout) && self.config == *config
    }
}

// 元に戻す・やり直しの履歴。
// ドラッグやキーを押し続けての移動は、手を離したときに1回分としてまとめる
#[derive(Default)]
struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    // 最後に確定した状態。今の状態と違えば、確定していない変更がある
    settled: Option<Snapshot>,
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
    history: History,
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
            history: History::default(),
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

    // 別のシーンに切り替えたとき。その内容を保存済みの状態とし、元に戻す履歴は消す
    pub fn scene_switched(&mut self, layout: &SceneLayout) {
        self.saved_layout = layout.clone();
        self.selection = None;
        self.save_status = None;
        self.confirm_delete = None;
        self.history = History::default();
    }

    // 変更を履歴に記録する。idle: マウスもキーも押していない (ドラッグなどが終わった)
    fn track_history(&mut self, layout: &SceneLayout, config: &AppConfig, idle: bool) {
        // 読み込み直後の自動調整は、取り込みと一緒に1回分にする
        let adjusting = layout.window_from_world || layout.objects.iter().any(|o| o.auto_fit);
        let Some(settled) = &self.history.settled else {
            self.history.settled = Some(Snapshot::new(layout, config));
            return;
        };
        if !idle || adjusting || settled.same_as(layout, config) {
            return;
        }
        self.commit(layout, config);
    }

    // 確定していない変更を、1回分として履歴に積む
    fn commit(&mut self, layout: &SceneLayout, config: &AppConfig) {
        let Some(settled) = self.history.settled.replace(Snapshot::new(layout, config)) else {
            return;
        };
        if settled.same_as(layout, config) {
            return;
        }
        self.history.undo.push(settled);
        if self.history.undo.len() > HISTORY_LIMIT {
            self.history.undo.remove(0);
        }
        self.history.redo.clear();
    }

    pub fn can_undo(&self, layout: &SceneLayout, config: &AppConfig) -> bool {
        !self.history.undo.is_empty()
            || self.history.settled.as_ref().is_some_and(|s| !s.same_as(layout, config))
    }

    pub fn can_redo(&self) -> bool {
        !self.history.redo.is_empty()
    }

    pub fn undo(&mut self, layout: &mut SceneLayout, config: &mut AppConfig) {
        self.commit(layout, config);
        let Some(previous) = self.history.undo.pop() else {
            return;
        };
        self.history.redo.push(Snapshot::new(layout, config));
        self.apply(previous, layout, config);
    }

    pub fn redo(&mut self, layout: &mut SceneLayout, config: &mut AppConfig) {
        self.commit(layout, config);
        let Some(next) = self.history.redo.pop() else {
            return;
        };
        self.history.undo.push(Snapshot::new(layout, config));
        self.apply(next, layout, config);
    }

    fn apply(&mut self, snapshot: Snapshot, layout: &mut SceneLayout, config: &mut AppConfig) {
        layout.restore(&snapshot.layout);
        *config = snapshot.config.clone();
        self.history.settled = Some(snapshot);
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
                (keyboard_edit, track_history, draw_selection)
                    .chain()
                    .run_if(in_state(AppMode::Edit)),
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


// 変更を元に戻す履歴に記録する
fn track_history(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    ui_input: Res<UiInputState>,
    layout: Res<SceneLayout>,
    config: Res<AppConfig>,
    mut state: ResMut<EditState>,
) {
    // 文字を入力している間は、入力が終わってからまとめて記録する
    let idle = keys.get_pressed().next().is_none() && mouse.get_pressed().next().is_none() && !ui_input.keyboard;
    state.bypass_change_detection().track_history(&layout, &config, idle);
}

// 選択中の対象に目印を描く (ギズモは俯瞰カメラにしか映らない)
fn draw_selection(
    mut gizmos: Gizmos,
    state: Res<EditState>,
    world: Res<WorldInfo>,
    globals: Query<&GlobalTransform>,
    placeables: Query<(Entity, &Placeable)>,
) {
    let color = LinearRgba::from(SELECTION_COLOR);
    // オブジェクトの縁取りは highlight.rs。ここでは回転・拡大の中心 (原点) に小さな点を描く
    let origin = |gizmos: &mut Gizmos, entity: Entity| {
        if let Ok(global) = globals.get(entity) {
            gizmos.sphere(global.translation(), 0.006, color);
        }
    };
    match &state.selection {
        // のぞき窓の縁取りは preview.rs (枠・視線・目の位置をまとめて縁取る)
        Some(Selection::Window) => {}
        Some(Selection::Object(id)) => {
            if let Some((entity, _)) = placeables.iter().find(|(_, p)| p.0 == *id) {
                origin(&mut gizmos, entity);
            }
        }
        Some(Selection::WorldNode(name)) => {
            if let Some(node) = world.node(name) {
                origin(&mut gizmos, node.entity);
            }
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (EditState, SceneLayout, AppConfig) {
        let mut world = World::new();
        world.insert_resource(SceneLayout::default());
        world.insert_resource(AppConfig::default());
        let state = EditState::from_world(&mut world);
        let layout = world.resource::<SceneLayout>().clone();
        let config = world.resource::<AppConfig>().clone();
        (state, layout, config)
    }

    // ドラッグ中の変更は手を離したときに1回分になり、元に戻す・やり直しで行き来できる
    #[test]
    fn undo_redo_groups_changes_until_idle() {
        let (mut state, mut layout, mut config) = setup();
        state.track_history(&layout, &config, true);
        let start = layout.window.position;

        // 押している間に2回動かす → まだ確定しない
        layout.window.position.x += 0.1;
        state.track_history(&layout, &config, false);
        layout.window.position.x += 0.1;
        state.track_history(&layout, &config, false);
        // 離したら1回分として確定
        state.track_history(&layout, &config, true);
        let moved = layout.window.position;

        // 画面サイズの変更も1回分
        config.screen_diagonal_inches += 1.0;
        state.track_history(&layout, &config, true);

        state.undo(&mut layout, &mut config);
        assert_eq!(config.screen_diagonal_inches, AppConfig::default().screen_diagonal_inches);
        assert_eq!(layout.window.position, moved);
        state.undo(&mut layout, &mut config);
        assert_eq!(layout.window.position, start);
        assert!(!state.can_undo(&layout, &config));

        state.redo(&mut layout, &mut config);
        assert_eq!(layout.window.position, moved);
        assert!(state.can_redo());

        // 新しく変更したら、やり直しの履歴は消える
        layout.window.position.y += 0.2;
        state.track_history(&layout, &config, true);
        assert!(!state.can_redo());
    }

    // 削除したオブジェクトは同じ id で戻る
    #[test]
    fn undo_restores_deleted_object_with_same_id() {
        let (mut state, mut layout, mut config) = setup();
        state.track_history(&layout, &config, true);
        let id = layout.objects[0].id;
        layout.objects.retain(|o| o.id != id);
        state.track_history(&layout, &config, true);
        state.undo(&mut layout, &mut config);
        assert!(layout.object(id).is_some());
    }

    // 確定していない変更 (押している最中) も元に戻せる
    #[test]
    fn undo_commits_pending_change_first() {
        let (mut state, mut layout, mut config) = setup();
        state.track_history(&layout, &config, true);
        let start = layout.window.position;
        layout.window.position.z -= 0.3;
        state.track_history(&layout, &config, false);
        assert!(state.can_undo(&layout, &config));
        state.undo(&mut layout, &mut config);
        assert_eq!(layout.window.position, start);
    }
}
