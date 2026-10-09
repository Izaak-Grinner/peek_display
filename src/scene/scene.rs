use bevy::{camera::primitives::Aabb, prelude::*};
use std::collections::HashSet;

use crate::scene::layout::{FLOOR_Y, Model, ObjectKind, OnLoad, Placement, SceneLayout, SceneObject};
use crate::scene::model::{self, LoadedModels, ModelInfo, ModelPart};
use crate::scene::package_store::PackageStore;

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_light)
            .add_systems(
                Update,
                (
                    sync_scene_objects.run_if(resource_changed::<SceneLayout>),
                    apply_light.run_if(resource_changed::<SceneLayout>),
                    place_imported,
                )
                    .chain(),
            );
    }
}

// アプリが置くライト (ライトの入ったモデルがあるときは消す)
#[derive(Component)]
pub struct DefaultLight;

// SceneLayout の objects[].id と対応するエンティティ
#[derive(Component)]
pub struct Placeable(pub u64);

// インポート直後のモデル。読み込みが終わったらすること
#[derive(Component)]
struct PlaceOnLoad(OnLoad);

// インポートしたモデルの最大辺をこの長さ(メートル)に合わせる
const AUTO_FIT_SIZE: f32 = 0.3;

fn setup_light(mut commands: Commands) {
    commands.spawn((
        DefaultLight,
        DirectionalLight::default(),
        Transform::from_rotation(Quat::from_rotation_x(-1.0)),
    ));
}

// シーンの照明の設定を、アプリの太陽光と環境光に反映する
fn apply_light(
    layout: Res<SceneLayout>,
    mut lights: Query<(&mut DirectionalLight, &mut Transform), With<DefaultLight>>,
    mut ambient: ResMut<AmbientLight>,
) {
    let settings = &layout.light;
    for (mut light, mut transform) in &mut lights {
        if light.illuminance != settings.illuminance {
            light.illuminance = settings.illuminance;
        }
        let rotation = settings.rotation();
        if transform.rotation != rotation {
            transform.rotation = rotation;
        }
    }
    if ambient.brightness != settings.ambient {
        ambient.brightness = settings.ambient;
    }
}

// SceneLayout の内容に合わせて、オブジェクトの追加・削除・移動を反映する
fn sync_scene_objects(
    mut commands: Commands,
    layout: Res<SceneLayout>,
    asset_server: Res<AssetServer>,
    store: Res<PackageStore>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut placeables: Query<(Entity, &Placeable, &mut Transform)>,
) {
    let mut existing = HashSet::new();

    for (entity, placeable, mut transform) in &mut placeables {
        match layout.object(placeable.0) {
            Some(object) => {
                existing.insert(object.id);
                let target = object.placement.to_transform();
                if *transform != target {
                    *transform = target;
                }
            }
            None => commands.entity(entity).despawn(),
        }
    }

    for object in layout.objects.iter().filter(|o| !existing.contains(&o.id)) {
        spawn_object(
            &mut commands,
            &asset_server,
            &store,
            &mut meshes,
            &mut materials,
            object,
        );
    }
}

fn spawn_object(
    commands: &mut Commands,
    asset_server: &AssetServer,
    store: &PackageStore,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    object: &SceneObject,
) {
    let transform = object.placement.to_transform();
    let mut entity = match &object.kind {
        ObjectKind::Model(model) => {
            // シーンの中のファイルから読み込む (scene://<番号>/<path>)
            let mut entity = commands.spawn((
                SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(store.asset_path(&model.path)))),
                transform,
            ));
            entity.observe(model::on_model_ready);
            if let Some(node) = &model.node {
                entity.insert(ModelPart(node.clone()));
            }
            entity
        }
        ObjectKind::Cube { color, size } => commands.spawn((
            Mesh3d(meshes.add(Cuboid::from_length(*size))),
            MeshMaterial3d(materials.add(Color::srgb(color.x, color.y, color.z))),
            transform,
        )),
    };
    entity.insert((Placeable(object.id), Name::new(object.name.clone())));
    if let Some(on_load) = object.on_load {
        entity.insert(PlaceOnLoad(on_load));
    }
}

// メッシュの Aabb とシーンでの座標 (大きさを求めるときに使う)
pub type MeshBoundsQuery<'w, 's> = Query<'w, 's, (Option<&'static Aabb>, &'static GlobalTransform), With<Mesh3d>>;

// entity とその子孫のメッシュ全体を囲む箱 (シーンでの座標の min, max)。
// まだシーンが展開されていない、または Aabb が計算されていないときは None
pub fn mesh_bounds(
    entity: Entity,
    children: &Query<&Children>,
    meshes: &MeshBoundsQuery,
) -> Option<(Vec3, Vec3)> {
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    let mut mesh_count = 0;

    for target in std::iter::once(entity).chain(children.iter_descendants(entity)) {
        let Ok((aabb, global)) = meshes.get(target) else {
            continue;
        };
        mesh_count += 1;
        let aabb = aabb?;
        let (center, half) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
        for corner in [
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(-1.0, -1.0, 1.0),
            Vec3::new(-1.0, 1.0, -1.0),
            Vec3::new(-1.0, 1.0, 1.0),
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::new(1.0, -1.0, 1.0),
            Vec3::new(1.0, 1.0, -1.0),
            Vec3::new(1.0, 1.0, 1.0),
        ] {
            let p = global.transform_point(center + half * corner);
            min = min.min(p);
            max = max.max(p);
        }
    }
    (mesh_count > 0).then_some((min, max))
}

// インポートしたモデルの読み込みが終わったら、置き方を決める。
// ワールドとして取り込んだものは、部品に分ける (split_world)。
// モデルは、peekScreen カメラが入っていれば (Blender で作った部屋など)、Blender の位置と大きさのまま置き、スクリーンをそのカメラに合わせる。
// 入っていなければ、最大辺が AUTO_FIT_SIZE になるよう縮尺を決め、底面を床に合わせる。
// メッシュの Aabb は PostUpdate で計算されるので、揃うまで毎フレーム待つ
fn place_imported(
    mut commands: Commands,
    mut layout: ResMut<SceneLayout>,
    models: Res<LoadedModels>,
    targets: Query<(Entity, &Placeable, &PlaceOnLoad)>,
    children: Query<&Children>,
    meshes: MeshBoundsQuery,
) {
    for (entity, placeable, on_load) in &targets {
        let Some(info) = models.get(placeable.0) else {
            continue;
        };
        if on_load.0 == OnLoad::Split {
            commands.entity(entity).remove::<PlaceOnLoad>();
            split_world(layout.as_mut(), placeable.0, info);
            continue;
        }
        let bounds = mesh_bounds(entity, &children, &meshes);
        if info.screen.is_none() && bounds.is_none() {
            continue;
        }

        commands.entity(entity).remove::<PlaceOnLoad>();
        let layout = layout.as_mut();
        if let Some(screen) = info.screen {
            layout.screen = Placement::from_transform(&screen);
        }
        let Some(object) = layout.objects.iter_mut().find(|o| o.id == placeable.0) else {
            continue;
        };
        object.on_load = None;

        if info.screen.is_some() {
            object.placement = Placement::default();
            info!(
                "{} に {} カメラがあるので、Blender の位置と大きさのまま置き、スクリーンを合わせました",
                object.name,
                model::SCREEN_NODE
            );
            continue;
        }

        let Some((min, max)) = bounds else {
            continue;
        };
        let largest = (max - min).max_element();
        if largest <= f32::EPSILON {
            continue;
        }
        // 原点まわりに拡大縮小したあとの床からの高さを求め、底面を床に合わせる
        let k = AUTO_FIT_SIZE / largest;
        let origin = object.placement.position;
        let new_min_y = origin.y + (min.y - origin.y) * k;
        object.placement.scale *= k;
        object.placement.position.y += FLOOR_Y - new_min_y;
        info!(
            "{} の大きさを自動調整しました (scale={:.4})",
            object.name, object.placement.scale
        );
    }
}

// ワールドとして取り込んだモデルを、一番上のノード (Blender で親を持たないオブジェクト) ごとの部品に分ける。
// 部品は Blender の位置と角度のまま置く。peekScreen カメラがあれば、スクリーンをその位置に合わせる
fn split_world(layout: &mut SceneLayout, id: u64, info: &ModelInfo) {
    let Some(index) = layout.objects.iter().position(|o| o.id == id) else {
        return;
    };
    let Some(path) = layout.objects[index].model().map(|m| m.path.clone()) else {
        return;
    };
    if let Some(screen) = info.screen {
        layout.screen = Placement::from_transform(&screen);
    }
    if info.nodes.is_empty() {
        // 分けられるノードが無ければ、丸ごと1つのオブジェクトのままにする
        layout.objects[index].on_load = None;
        return;
    }
    let world = layout.objects.remove(index);
    for node in &info.nodes {
        layout.add_object(
            &node.name,
            ObjectKind::Model(Model::part(&path, &node.name)),
            Placement::from_transform(&node.original),
        );
    }
    info!(
        "{} を {} 個のオブジェクトに分けました{}",
        world.name,
        info.nodes.len(),
        if info.screen.is_some() { "。スクリーンを peekScreen カメラに合わせました" } else { "" }
    );
}
