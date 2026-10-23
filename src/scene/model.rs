// モデル (.glb / .gltf) の中身。
// ワールドの部品 (Model::node が Some) は、ファイルの中身を丸ごと読み込んでから、そのノード以外を消して表示する
// (頂点データやテクスチャは Bevy が共有するので、部品がいくつあってもファイルは1つ分しか読まない)。
// peekScreen という名前のカメラがあれば、スクリーンをその位置に合わせられる
use bevy::{prelude::*, scene::SceneInstanceReady};
use std::collections::HashMap;

use crate::scene::layout::{Placement, SceneLayout};
use crate::scene::scene::{DefaultLight, Placeable};

// スクリーンの位置と向きを決めるカメラの名前 (Blender のオブジェクト名)
pub const SCREEN_NODE: &str = "peekScreen";

// 読み込みが終わったモデルの中身
#[derive(Clone, Debug, Default)]
pub struct ModelInfo {
    // Blender での peekScreen カメラの位置と向き (ファイルの中の座標)
    pub screen: Option<Transform>,
    // ファイルの一番上の階層のノード (Blender で親を持たないオブジェクト。screen などのカメラは除く)。
    // ワールドとして取り込んだときは、これを部品に分ける
    pub nodes: Vec<NodeInfo>,
    // ワールドの部品なら、そのノードの Blender での Transform (「Blender の値に戻す」で使う)
    pub part_original: Option<Transform>,
    // 表示しているものにライトが入っているか
    pub has_lights: bool,
}

#[derive(Clone, Debug)]
pub struct NodeInfo {
    pub name: String,
    // Blender での Transform
    pub original: Transform,
}

impl ModelInfo {
    // ワールドの部品の Blender での配置 (大きさはノード側に残すので 1)
    pub fn part_placement(&self) -> Option<Placement> {
        self.part_original.as_ref().map(Placement::from_transform)
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

    // peekScreen カメラの入った最初のモデルの名前と、そのカメラのシーンでの位置と向き。
    // ワールドの部品は、どの部品も同じファイルの peekScreen カメラを持つので、部品の名前ではなくファイル名を返す。
    // 部品は Blender の座標のまま置いたものなので、部品を動かしても peekScreen カメラの位置は変えない
    pub fn screen_placement(&self, layout: &SceneLayout) -> Option<(String, Placement)> {
        layout.objects.iter().find_map(|o| {
            let info = self.get(o.id)?;
            let screen = info.screen?;
            let (name, screen) = match (info.part_original, o.model()) {
                (Some(_), Some(model)) => (file_stem(&model.path), screen),
                _ => (o.name.clone(), o.placement.to_transform() * screen),
            };
            Some((name, Placement::from_transform(&screen)))
        })
    }
}

// シーンの中の相対パスから、拡張子を除いたファイル名 ("models/room/room.glb" → "room")
fn file_stem(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(file).to_string()
}

// ワールドの部品として表示するノードの名前 (scene.rs でモデルを出すときに付ける)
#[derive(Component)]
pub struct ModelPart(pub String);

pub struct ModelPlugin;

impl Plugin for ModelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadedModels>().add_systems(
            Update,
            (prune_loaded_models.run_if(resource_changed::<SceneLayout>), update_default_light).chain(),
        );
    }
}

type LightFilter = Or<(With<PointLight>, With<SpotLight>, With<DirectionalLight>)>;

// モデルの読み込みが終わったら、中身を調べる。ワールドの部品なら、そのノード以外を消す
// (scene.rs でモデルを出すときに付ける)
#[allow(clippy::too_many_arguments)]
pub fn on_model_ready(
    ready: On<SceneInstanceReady>,
    mut commands: Commands,
    mut models: ResMut<LoadedModels>,
    placeables: Query<(&Placeable, Option<&Name>, Option<&ModelPart>)>,
    children: Query<&Children>,
    nodes: Query<(&Transform, Option<&Name>)>,
    mut cameras: Query<&mut Camera>,
    lights: Query<(), LightFilter>,
    meshes: Query<&Mesh3d>,
    mut mesh_assets: ResMut<Assets<Mesh>>,
) {
    let Ok((placeable, model_name, part)) = placeables.get(ready.entity) else {
        return;
    };
    let mut found = Found::default();
    // モデル自体の Transform (シーンでの配置) は含めず、ファイルの中の座標で調べる
    let roots = children.get(ready.entity).map(|list| list.to_vec()).unwrap_or_default();
    for root in roots {
        visit(root, Transform::IDENTITY, None, &children, &nodes, &cameras.as_readonly(), &meshes, &mut found);
    }

    // 鏡に映したように反転した配置 (拡大率がマイナス) のメッシュは、面の巻き方向を反転したものに差し替える。
    // Bevy は両面表示の材質で裏表を巻き方向だけで決めるので、そのままだと見えている面を裏として描き、暗くなる。
    // ワールドの部品なら、残すノードの中のものだけ
    let kept = part.and_then(|ModelPart(node)| found.top.iter().find(|(name, ..)| name == node).map(|(_, e, _)| *e));
    let mut inverted: HashMap<AssetId<Mesh>, Option<Handle<Mesh>>> = HashMap::new();
    for (top, entity, mesh) in &found.mirrored {
        if part.is_some() && *top != kept {
            continue;
        }
        let handle = inverted
            .entry(mesh.id())
            .or_insert_with(|| {
                let mesh = mesh_assets.get(mesh)?.clone().with_inverted_winding().ok()?;
                Some(mesh_assets.add(mesh))
            })
            .clone();
        if let Some(handle) = handle {
            commands.entity(*entity).insert(Mesh3d(handle));
        }
    }
    if !inverted.is_empty() {
        info!(
            "{} の反転したメッシュ {} 個の巻き方向を直しました",
            model_name.map(|n| n.as_str()).unwrap_or("モデル"),
            inverted.values().filter(|h| h.is_some()).count()
        );
    }

    // glTF のカメラは使わない (シーンビューやメインカメラと取り合わないように)
    for entity in &found.cameras {
        if let Ok(mut camera) = cameras.get_mut(*entity) {
            camera.is_active = false;
        }
    }

    let has_light = |entity: Entity| {
        std::iter::once(entity)
            .chain(children.iter_descendants(entity))
            .any(|e| lights.contains(e))
    };
    let mut info = ModelInfo {
        screen: found.screen,
        nodes: found.top.iter().map(|(name, _, original)| NodeInfo { name: name.clone(), original: *original }).collect(),
        ..default()
    };

    if let Some(ModelPart(node)) = part {
        // そのノードだけを残す。位置と回転はオブジェクトの配置で持つので、ノードには大きさだけ残す
        for (name, entity, original) in &found.top {
            if name == node {
                commands.entity(*entity).insert(Transform::from_scale(original.scale));
                info.part_original = Some(*original);
                info.has_lights = has_light(*entity);
            } else {
                commands.entity(*entity).despawn();
            }
        }
        for entity in &found.other_top {
            commands.entity(*entity).despawn();
        }
        if info.part_original.is_none() {
            warn!("{} の中に {} が見つかりませんでした", model_name.map(|n| n.as_str()).unwrap_or("モデル"), node);
        }
    } else {
        info.has_lights = found.top.iter().any(|(_, entity, _)| has_light(*entity));
        info!(
            "{} を読み込みました (一番上のオブジェクト {} 個、ライト {}、{} カメラ {})",
            model_name.map(|n| n.as_str()).unwrap_or("モデル"),
            info.nodes.len(),
            if info.has_lights { "あり" } else { "なし" },
            SCREEN_NODE,
            if info.screen.is_some() { "あり" } else { "なし" },
        );
    }
    models.0.insert(placeable.0, info);
}

#[derive(Default)]
struct Found {
    screen: Option<Transform>,
    // 一番上の階層の名前の付いたノード (名前、エンティティ、Blender での Transform)
    top: Vec<(String, Entity, Transform)>,
    // 一番上の階層の、部品にしないノード (カメラ)
    other_top: Vec<Entity>,
    cameras: Vec<Entity>,
    // 反転した配置のメッシュ (含まれる一番上のノード、エンティティ、メッシュ)
    mirrored: Vec<(Option<Entity>, Entity, Handle<Mesh>)>,
}

// モデルの子孫をたどる。top: 含まれる一番上の名前の付いたノード (None ならまだ一番上の階層を探している)
#[allow(clippy::too_many_arguments)]
fn visit(
    entity: Entity,
    parent: Transform,
    top: Option<Entity>,
    children: &Query<&Children>,
    nodes: &Query<(&Transform, Option<&Name>)>,
    cameras: &Query<&Camera>,
    meshes: &Query<&Mesh3d>,
    found: &mut Found,
) {
    let (local, name) = nodes.get(entity).map(|(t, n)| (*t, n)).unwrap_or((Transform::IDENTITY, None));
    let global = parent * local;
    let is_camera = cameras.contains(entity);
    let child_list = children.get(entity).ok();
    let holds_camera = is_camera || child_list.is_some_and(|list| list.iter().any(|c| cameras.contains(c)));
    if is_camera {
        found.cameras.push(entity);
    }

    let mut top = top;
    if let Some(name) = name {
        if name.as_str() == SCREEN_NODE && holds_camera {
            found.screen = Some(global);
        }
        if top.is_none() {
            if holds_camera {
                found.other_top.push(entity);
            } else {
                found.top.push((name.to_string(), entity, local));
            }
            top = Some(entity);
        }
    }
    if let Ok(Mesh3d(mesh)) = meshes.get(entity)
        && global.compute_affine().matrix3.determinant() < 0.0
    {
        found.mirrored.push((top, entity, mesh.clone()));
    }
    if let Some(list) = child_list {
        for child in list {
            visit(*child, global, top, children, nodes, cameras, meshes, found);
        }
    }
}

// シーンから無くなったモデルの中身を忘れる
fn prune_loaded_models(layout: Res<SceneLayout>, mut models: ResMut<LoadedModels>) {
    if models.0.keys().any(|id| layout.object(*id).is_none()) {
        models.0.retain(|id, _| layout.object(*id).is_some());
    }
}

// ライトの入ったモデルがあるときは、アプリのライトを消す
fn update_default_light(
    layout: Res<SceneLayout>,
    models: Res<LoadedModels>,
    mut light: Query<&mut Visibility, With<DefaultLight>>,
) {
    let target = if models.has_lights(&layout) { Visibility::Hidden } else { Visibility::Inherited };
    for mut visibility in &mut light {
        if *visibility != target {
            *visibility = target;
        }
    }
}
