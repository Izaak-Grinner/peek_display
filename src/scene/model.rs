// モデル (.glb / .gltf) の中身。
// 一番上の階層のノード (Blender で親を持たないオブジェクト) は、アプリで動かしたり隠したりできる。
// 変更は Model::nodes に名前で記録する。screen という名前のカメラがあれば、スクリーンをその位置に合わせられる
use bevy::{prelude::*, scene::SceneInstanceReady};
use std::collections::{HashMap, HashSet};

use crate::scene::import::read_model_files;
use crate::scene::layout::{Model, Placement, SceneLayout};
use crate::scene::package_store::PackageStore;
use crate::scene::scene::{DefaultLight, Placeable, RoomPart, spawn_object};

// スクリーンの位置と向きを決めるカメラの名前 (Blender のオブジェクト名)
pub const SCREEN_NODE: &str = "screen";

// 読み込みが終わったモデルの中身
#[derive(Clone, Debug, Default)]
pub struct ModelInfo {
    // Blender での screen カメラの位置と向き (モデルの中の座標)
    pub screen: Option<Transform>,
    // アプリで編集できるノード
    pub nodes: Vec<NodeInfo>,
    pub has_lights: bool,
}

#[derive(Clone, Debug)]
pub struct NodeInfo {
    pub name: String,
    pub entity: Entity,
    // Blender での Transform
    pub original: Transform,
}

impl ModelInfo {
    pub fn node(&self, name: &str) -> Option<&NodeInfo> {
        self.nodes.iter().find(|n| n.name == name)
    }

    // 中のノードを一覧に出して選べるようにするか (1つだけならモデルを動かすのと同じなので出さない)
    pub fn lists_nodes(&self) -> bool {
        self.nodes.len() >= 2
    }
}

// 読み込みが終わったモデル (オブジェクトの id → 中身)
#[derive(Resource, Default)]
pub struct LoadedModels(HashMap<u64, ModelInfo>);

impl LoadedModels {
    pub fn get(&self, id: u64) -> Option<&ModelInfo> {
        self.0.get(&id)
    }

    // ライトの入ったモデルがシーンにあるか
    pub fn has_lights(&self, layout: &SceneLayout) -> bool {
        layout.objects.iter().any(|o| self.get(o.id).is_some_and(|info| info.has_lights))
    }

    // screen カメラの入った最初のモデルの名前と、そのカメラのシーンでの位置と向き
    pub fn screen_placement<'a>(&self, layout: &'a SceneLayout) -> Option<(&'a str, Placement)> {
        layout.objects.iter().find_map(|o| {
            let screen = self.get(o.id)?.screen?;
            Some((o.name.as_str(), Placement::from_transform(&(o.placement.to_transform() * screen))))
        })
    }
}

// アプリで編集できる、モデルの中のノード (Blender のオブジェクト名)
#[derive(Component)]
pub struct ModelNode(pub String);

// 「再読み込み」が押されたモデルのオブジェクトの id
#[derive(Resource, Default)]
pub struct ReloadRequest(pub Option<u64>);

pub struct ModelPlugin;

impl Plugin for ModelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadedModels>()
            .init_resource::<ReloadRequest>()
            .add_systems(
                Update,
                (
                    reload_model,
                    respawn_reloaded,
                    prune_loaded_models.run_if(resource_changed::<SceneLayout>),
                    apply_node_overrides.run_if(resource_changed::<SceneLayout>.or(resource_changed::<LoadedModels>)),
                    update_room_and_light,
                )
                    .chain(),
            );
    }
}

// ノードの今の配置。変更が無ければ Blender の値 (scale は倍率なので 1)
pub fn node_placement(model: &Model, info: &ModelInfo, name: &str) -> Option<Placement> {
    if let Some(placement) = model.node(name).and_then(|n| n.placement.clone()) {
        return Some(placement);
    }
    info.node(name).map(|n| Placement::from_transform(&n.original))
}

pub fn node_hidden(model: &Model, name: &str) -> bool {
    model.node(name).is_some_and(|n| n.hidden)
}

// モデルの読み込みが終わったら、中のノードを調べる (scene.rs でモデルを出すときに付ける)
pub fn on_model_ready(
    ready: On<SceneInstanceReady>,
    mut commands: Commands,
    mut models: ResMut<LoadedModels>,
    placeables: Query<(&Placeable, Option<&Name>)>,
    children: Query<&Children>,
    nodes: Query<(&Transform, Option<&Name>)>,
    mut cameras: Query<&mut Camera>,
    lights: Query<(), Or<(With<PointLight>, With<SpotLight>, With<DirectionalLight>)>>,
) {
    let Ok((placeable, model_name)) = placeables.get(ready.entity) else {
        return;
    };
    let mut found = Found::default();
    // モデル自体の Transform (シーンでの配置) は含めず、モデルの中の座標で調べる
    let roots = children.get(ready.entity).map(|list| list.to_vec()).unwrap_or_default();
    for root in roots {
        visit(root, Transform::IDENTITY, true, &children, &nodes, &cameras.as_readonly(), &lights, &mut found);
    }

    // glTF のカメラは使わない (シーンビューやメインカメラと取り合わないように)
    for entity in &found.cameras {
        if let Ok(mut camera) = cameras.get_mut(*entity) {
            camera.is_active = false;
        }
    }
    for node in &found.nodes {
        commands.entity(node.entity).insert(ModelNode(node.name.clone()));
    }
    info!(
        "{} を読み込みました (編集できるノード {} 個、ライト {}、{} カメラ {})",
        model_name.map(|n| n.as_str()).unwrap_or("モデル"),
        found.nodes.len(),
        if found.has_lights { "あり" } else { "なし" },
        SCREEN_NODE,
        if found.screen.is_some() { "あり" } else { "なし" },
    );
    models.0.insert(
        placeable.0,
        ModelInfo {
            screen: found.screen,
            nodes: found.nodes,
            has_lights: found.has_lights,
        },
    );
}

#[derive(Default)]
struct Found {
    screen: Option<Transform>,
    nodes: Vec<NodeInfo>,
    cameras: Vec<Entity>,
    has_lights: bool,
}

// モデルの子孫をたどる。top: まだ名前の付いたノードに入っていない (一番上の階層を探している)
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
    let child_list = children.get(entity).ok();
    let holds_camera = is_camera || child_list.is_some_and(|list| list.iter().any(|c| cameras.contains(c)));
    let holds_light = is_light || child_list.is_some_and(|list| list.iter().any(|c| lights.contains(c)));
    if is_camera {
        found.cameras.push(entity);
    }
    found.has_lights |= is_light;

    let mut top = top;
    if let Some(name) = name {
        if name.as_str() == SCREEN_NODE && holds_camera {
            found.screen = Some(global);
        } else if top && !holds_light && !holds_camera {
            found.nodes.push(NodeInfo {
                name: name.to_string(),
                entity,
                original: local,
            });
        }
        top = false;
    }
    if let Some(list) = child_list {
        for child in list {
            visit(*child, global, top, children, nodes, cameras, lights, found);
        }
    }
}

// Blender で書き出し直したモデルを、元のファイルから読み直してシーンの中身を差し替える
fn reload_model(
    mut request: ResMut<ReloadRequest>,
    layout: Res<SceneLayout>,
    mut store: ResMut<PackageStore>,
    asset_server: Res<AssetServer>,
) {
    let Some(id) = request.0.take() else {
        return;
    };
    let Some(model) = layout.object(id).and_then(|o| o.model()) else {
        return;
    };
    let Some(source) = &model.source else {
        return;
    };
    let files = match read_model_files(std::path::Path::new(source)) {
        Ok(files) => files,
        Err(e) => {
            warn!("モデルを再読み込みできませんでした: {}", e);
            return;
        }
    };
    // シーンの中で、モデルを置いているフォルダに入れ直す
    let folder = model.path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    let (_, files) = files.placed_in(folder);
    for (rel, bytes) in files {
        store.insert(rel, bytes);
    }
    info!("{} から再読み込みします", source);
    asset_server.reload(store.asset_path(&model.path));
}

// 読み直しが終わったら、そのモデルを出し直してノードを調べ直す (同じファイルを使う複製も)
#[allow(clippy::too_many_arguments)]
fn respawn_reloaded(
    mut commands: Commands,
    mut events: MessageReader<AssetEvent<Scene>>,
    layout: Res<SceneLayout>,
    asset_server: Res<AssetServer>,
    store: Res<PackageStore>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut models: ResMut<LoadedModels>,
    roots: Query<(Entity, &Placeable, &SceneRoot)>,
) {
    let modified: HashSet<_> = events
        .read()
        .filter_map(|e| match e {
            AssetEvent::Modified { id } => Some(*id),
            _ => None,
        })
        .collect();
    if modified.is_empty() {
        return;
    }
    for (entity, placeable, root) in &roots {
        if !modified.contains(&root.0.id()) {
            continue;
        }
        commands.entity(entity).despawn();
        models.0.remove(&placeable.0);
        if let Some(object) = layout.object(placeable.0) {
            spawn_object(&mut commands, &asset_server, &store, &mut meshes, &mut materials, object);
        }
    }
}

// シーンから無くなったモデルの中身を忘れる
fn prune_loaded_models(layout: Res<SceneLayout>, mut models: ResMut<LoadedModels>) {
    if models.0.keys().any(|id| layout.object(*id).is_none()) {
        models.0.retain(|id, _| layout.object(*id).is_some());
    }
}

// ノードの変更 (位置・回転・倍率・非表示) を反映する
fn apply_node_overrides(
    layout: Res<SceneLayout>,
    models: Res<LoadedModels>,
    mut nodes: Query<(&mut Transform, &mut Visibility), With<ModelNode>>,
) {
    for object in &layout.objects {
        let (Some(model), Some(info)) = (object.model(), models.get(object.id)) else {
            continue;
        };
        for node in &info.nodes {
            let Ok((mut transform, mut visibility)) = nodes.get_mut(node.entity) else {
                continue;
            };
            let target = match model.node(&node.name).and_then(|n| n.placement.as_ref()) {
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
            let target = if node_hidden(model, &node.name) { Visibility::Hidden } else { Visibility::Inherited };
            if *visibility != target {
                *visibility = target;
            }
        }
    }
}

// 部屋はシーンの設定に合わせて出す。ライトの入ったモデルがあるときは、アプリのライトを消す
fn update_room_and_light(
    layout: Res<SceneLayout>,
    models: Res<LoadedModels>,
    mut room: Query<&mut Visibility, (With<RoomPart>, Without<DefaultLight>)>,
    mut light: Query<&mut Visibility, (With<DefaultLight>, Without<RoomPart>)>,
) {
    let room_target = if layout.show_room { Visibility::Inherited } else { Visibility::Hidden };
    for mut visibility in &mut room {
        if *visibility != room_target {
            *visibility = room_target;
        }
    }
    let light_target = if models.has_lights(&layout) { Visibility::Hidden } else { Visibility::Inherited };
    for mut visibility in &mut light {
        if *visibility != light_target {
            *visibility = light_target;
        }
    }
}
