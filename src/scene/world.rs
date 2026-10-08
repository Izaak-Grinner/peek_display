// Blender などで作ったワールド (.glb) の表示。
// ワールドの一番上の階層のノード (Blender で親を持たないオブジェクト) は、
// アプリで動かしたり隠したりできる。変更は SceneLayout::world_overrides に名前で記録する
use bevy::{prelude::*, scene::SceneInstanceReady};

use crate::scene::editor::EditState;
use crate::scene::layout::{Placement, SceneLayout};
use crate::scene::import::read_model_files;
use crate::scene::library::SceneLibrary;
use crate::scene::package_store::PackageStore;
use crate::scene::scene::{DefaultLight, RoomPart};

// のぞき窓の位置と向きを決めるノードの名前 (Blender のオブジェクト名)
pub const PEEK_WINDOW_NODE: &str = "peek_window";

// 表示中のワールド
#[derive(Resource, Default)]
pub struct WorldInfo {
    // 読み込んでいるワールドのパス (peek://<番号>/...)。シーンを開き直すと変わる
    path: Option<String>,
    root: Option<Entity>,
    scene: Option<Handle<Scene>>,
    // 読み込みが終わり、ノードを調べ終わったか
    pub ready: bool,
    // Blender での peek_window の位置と向き
    pub window: Option<Placement>,
    // アプリで編集できるノード
    pub nodes: Vec<WorldNodeInfo>,
    pub has_lights: bool,
    // 「ワールドを再読み込み」が押された
    pub reload_requested: bool,
}

#[derive(Clone, Debug)]
pub struct WorldNodeInfo {
    pub name: String,
    pub entity: Entity,
    // Blender での Transform
    pub original: Transform,
}

impl WorldInfo {
    pub fn node(&self, name: &str) -> Option<&WorldNodeInfo> {
        self.nodes.iter().find(|n| n.name == name)
    }
}

// アプリで編集できるワールドのノード (Blender のオブジェクト名)
#[derive(Component)]
pub struct WorldNode(pub String);

// ワールドを表示しているエンティティ
#[derive(Component)]
struct WorldRoot;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldInfo>().add_systems(
            Update,
            (
                reload_world,
                sync_world.run_if(resource_changed::<SceneLayout>.or(resource_changed::<PackageStore>)),
                respawn_modified_world,
                apply_window_from_world,
                apply_world_overrides.run_if(resource_changed::<SceneLayout>.or(resource_changed::<WorldInfo>)),
                update_room_and_light,
            )
                .chain(),
        );
    }
}

// ノードの今の配置。変更が無ければ Blender の値 (scale は倍率なので 1)
pub fn node_placement(layout: &SceneLayout, info: &WorldInfo, name: &str) -> Option<Placement> {
    if let Some(placement) = layout.world_override(name).and_then(|o| o.placement.clone()) {
        return Some(placement);
    }
    info.node(name).map(|n| Placement::from_transform(&n.original))
}

pub fn node_hidden(layout: &SceneLayout, name: &str) -> bool {
    layout.world_override(name).is_some_and(|o| o.hidden)
}

// SceneLayout のワールドが変わったら、表示し直す
fn sync_world(
    mut commands: Commands,
    layout: Res<SceneLayout>,
    asset_server: Res<AssetServer>,
    store: Res<PackageStore>,
    mut info: ResMut<WorldInfo>,
) {
    let path = layout.world.as_deref().map(|w| store.asset_path(w));
    if info.path == path {
        return;
    }
    if let Some(root) = info.root.take() {
        commands.entity(root).despawn();
    }
    *info = WorldInfo {
        path: path.clone(),
        ..default()
    };
    let Some(path) = path else {
        return;
    };
    let scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset(path));
    let root = commands
        .spawn((WorldRoot, SceneRoot(scene.clone()), Transform::default()))
        .observe(on_world_ready)
        .id();
    info.root = Some(root);
    info.scene = Some(scene);
}

// Blender で書き出し直したワールドを、元のファイルから読み直してシーンの中身を差し替える
fn reload_world(
    mut info: ResMut<WorldInfo>,
    layout: Res<SceneLayout>,
    mut store: ResMut<PackageStore>,
    asset_server: Res<AssetServer>,
) {
    if !info.reload_requested {
        return;
    }
    info.reload_requested = false;
    let (Some(path), Some(world), Some(source)) = (&info.path, &layout.world, &layout.world_source) else {
        return;
    };
    let model = match read_model_files(std::path::Path::new(source)) {
        Ok(model) => model,
        Err(e) => {
            warn!("ワールドを再読み込みできませんでした: {}", e);
            return;
        }
    };
    // シーンの中で、ワールドを置いているフォルダに入れ直す
    let folder = world.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    let (_, files) = model.placed_in(folder);
    for (rel, bytes) in files {
        store.insert(rel, bytes);
    }
    info!("ワールドを {} から再読み込みします", source);
    asset_server.reload(path.clone());
}

// 読み直しが終わったら、ワールドを出し直してノードを調べ直す
fn respawn_modified_world(
    mut commands: Commands,
    mut events: MessageReader<AssetEvent<Scene>>,
    mut info: ResMut<WorldInfo>,
) {
    let Some(scene) = info.scene.clone() else {
        events.clear();
        return;
    };
    let modified = events
        .read()
        .any(|e| matches!(e, AssetEvent::Modified { id } if *id == scene.id()));
    if !modified {
        return;
    }
    if let Some(root) = info.root.take() {
        commands.entity(root).despawn();
    }
    let root = commands
        .spawn((WorldRoot, SceneRoot(scene.clone()), Transform::default()))
        .observe(on_world_ready)
        .id();
    let path = info.path.clone();
    *info = WorldInfo {
        path,
        root: Some(root),
        scene: Some(scene),
        ..default()
    };
}

// ワールドの読み込みが終わったら、ノードを調べる
fn on_world_ready(
    ready: On<SceneInstanceReady>,
    mut commands: Commands,
    mut info: ResMut<WorldInfo>,
    children: Query<&Children>,
    nodes: Query<(&Transform, Option<&Name>)>,
    mut cameras: Query<&mut Camera>,
    lights: Query<(), Or<(With<PointLight>, With<SpotLight>, With<DirectionalLight>)>>,
) {
    if info.root != Some(ready.entity) {
        return;
    }
    let mut found = Found::default();
    visit(ready.entity, Transform::IDENTITY, true, &children, &nodes, &cameras.as_readonly(), &lights, &mut found);

    // glTF のカメラは使わない (俯瞰カメラや覗き込みカメラと取り合わないように)
    for entity in &found.cameras {
        if let Ok(mut camera) = cameras.get_mut(*entity) {
            camera.is_active = false;
        }
    }
    for node in &found.nodes {
        commands.entity(node.entity).insert(WorldNode(node.name.clone()));
    }
    if let Some(window) = &found.window {
        info!(
            "{}: 位置 {:.3?}、回転 X {:.1}° Y {:.1}° Z {:.1}°",
            PEEK_WINDOW_NODE, window.position, window.pitch_deg, window.yaw_deg, window.roll_deg
        );
    } else {
        warn!(
            "ワールドに {} が見つかりませんでした。のぞき窓は初期位置のままです",
            PEEK_WINDOW_NODE
        );
    }
    info!(
        "ワールドを読み込みました (編集できるノード {} 個、ライト {})",
        found.nodes.len(),
        if found.has_lights { "あり" } else { "なし" }
    );
    info.window = found.window;
    info.nodes = found.nodes;
    info.has_lights = found.has_lights;
    info.ready = true;
}

#[derive(Default)]
struct Found {
    window: Option<Placement>,
    nodes: Vec<WorldNodeInfo>,
    cameras: Vec<Entity>,
    has_lights: bool,
}

// ワールドの子孫をたどる。top: まだ名前の付いたノードに入っていない (一番上の階層を探している)
#[allow(clippy::too_many_arguments)]
fn visit(
    entity: Entity,
    parent: Transform,
    top: bool,
    children: &Query<&Children>,
    nodes: &Query<(&Transform, Option<&Name>)>,
    cameras: &Query<&Camera>,
    lights: &Query<(), Or<(With<PointLight>, With<SpotLight>, With<DirectionalLight>)>>,
    found: &mut Found,
) {
    let (local, name) = nodes.get(entity).map(|(t, n)| (*t, n)).unwrap_or((Transform::IDENTITY, None));
    let global = parent * local;
    let is_camera = cameras.contains(entity);
    let is_light = lights.contains(entity);
    // glTF のライトはノードの子エンティティとして作られるので、子も見る
    let holds_light_or_camera = is_light
        || is_camera
        || children
            .get(entity)
            .is_ok_and(|list| list.iter().any(|c| lights.contains(c) || cameras.contains(c)));
    if is_camera {
        found.cameras.push(entity);
    }
    found.has_lights |= is_light;

    let mut top = top;
    if let Some(name) = name {
        if name.as_str() == PEEK_WINDOW_NODE {
            found.window = Some(Placement::from_transform(&global));
        } else if top && !holds_light_or_camera {
            found.nodes.push(WorldNodeInfo {
                name: name.to_string(),
                entity,
                original: local,
            });
        }
        top = false;
    }
    if let Ok(list) = children.get(entity) {
        for child in list {
            visit(*child, global, top, children, nodes, cameras, lights, found);
        }
    }
}

// インポートしたばかりのワールドは、のぞき窓を peek_window に合わせて保存する
fn apply_window_from_world(
    mut layout: ResMut<SceneLayout>,
    info: Res<WorldInfo>,
    library: Res<SceneLibrary>,
    store: Res<PackageStore>,
    mut state: ResMut<EditState>,
) {
    if !layout.window_from_world || !info.ready {
        return;
    }
    layout.window_from_world = false;
    let Some(window) = info.window.clone() else {
        return;
    };
    layout.window = window;
    if library.current.is_some() {
        match library.save(&layout, &store) {
            Ok(()) => state.mark_layout_saved(&layout),
            Err(e) => warn!("{}", e),
        }
    }
}

// ノードの変更 (位置・回転・倍率・非表示) を反映する
fn apply_world_overrides(
    layout: Res<SceneLayout>,
    info: Res<WorldInfo>,
    mut nodes: Query<(&mut Transform, &mut Visibility), With<WorldNode>>,
) {
    if !info.ready {
        return;
    }
    for node in &info.nodes {
        let Ok((mut transform, mut visibility)) = nodes.get_mut(node.entity) else {
            continue;
        };
        let target = match layout.world_override(&node.name).and_then(|o| o.placement.as_ref()) {
            Some(p) => Transform {
                translation: p.position,
                rotation: p.rotation(),
                scale: node.original.scale * p.scale,
            },
            None => node.original,
        };
        if *transform != target {
            *transform = target;
        }
        let target = if node_hidden(&layout, &node.name) { Visibility::Hidden } else { Visibility::Inherited };
        if *visibility != target {
            *visibility = target;
        }
    }
}

// ワールドがあるときは、アプリの部屋を隠す。ワールドにライトがあるときは、アプリのライトも消す
fn update_room_and_light(
    layout: Res<SceneLayout>,
    info: Res<WorldInfo>,
    mut room: Query<&mut Visibility, (With<RoomPart>, Without<DefaultLight>)>,
    mut light: Query<&mut Visibility, (With<DefaultLight>, Without<RoomPart>)>,
) {
    let room_target = if layout.world.is_some() { Visibility::Hidden } else { Visibility::Inherited };
    for mut visibility in &mut room {
        if *visibility != room_target {
            *visibility = room_target;
        }
    }
    let light_target = if layout.world.is_some() && info.has_lights {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };
    for mut visibility in &mut light {
        if *visibility != light_target {
            *visibility = light_target;
        }
    }
}
