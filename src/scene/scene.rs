use bevy::{camera::primitives::Aabb, prelude::*};
use std::collections::HashSet;

use crate::scene::layout::{FLOOR_Y, ObjectKind, SceneLayout, SceneObject};

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (setup_light, setup_room))
            .add_systems(
                Update,
                (
                    sync_scene_objects.run_if(resource_changed::<SceneLayout>),
                    apply_light.run_if(resource_changed::<SceneLayout>),
                    auto_fit_imported,
                )
                    .chain(),
            );
    }
}

// アプリが作る部屋の壁・床・天井 (ワールドがあるときは隠す)
#[derive(Component)]
pub struct RoomPart;

// アプリが置くライト (ワールドにライトがあるときは消す)
#[derive(Component)]
pub struct DefaultLight;

// SceneLayout の objects[].id と対応するエンティティ
#[derive(Component)]
pub struct Placeable(pub u64);

// インポート直後のモデル。読み込みが終わったら大きさを自動調整する
#[derive(Component)]
struct AutoFit;

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
            &mut meshes,
            &mut materials,
            object,
        );
    }
}

fn spawn_object(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    object: &SceneObject,
) {
    let transform = object.placement.to_transform();
    let mut entity = match &object.kind {
        ObjectKind::Model { path } => commands.spawn((
            SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(path.clone()))),
            transform,
        )),
        ObjectKind::Cube { color, size } => commands.spawn((
            Mesh3d(meshes.add(Cuboid::from_length(*size))),
            MeshMaterial3d(materials.add(Color::srgb(color.x, color.y, color.z))),
            transform,
        )),
    };
    entity.insert((Placeable(object.id), Name::new(object.name.clone())));
    if object.auto_fit {
        entity.insert(AutoFit);
    }
}

// メッシュの Aabb とワールド座標 (大きさを求めるときに使う)
pub type MeshBoundsQuery<'w, 's> = Query<'w, 's, (Option<&'static Aabb>, &'static GlobalTransform), With<Mesh3d>>;

// entity とその子孫のメッシュ全体を囲む箱 (ワールド座標の min, max)。
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

// インポートしたモデルの読み込みが終わったら、最大辺が AUTO_FIT_SIZE になるよう縮尺を決め、
// 底面を床に合わせる。メッシュの Aabb は PostUpdate で計算されるので、揃うまで毎フレーム待つ
fn auto_fit_imported(
    mut commands: Commands,
    mut layout: ResMut<SceneLayout>,
    targets: Query<(Entity, &Placeable), With<AutoFit>>,
    children: Query<&Children>,
    meshes: MeshBoundsQuery,
) {
    for (entity, placeable) in &targets {
        let Some((min, max)) = mesh_bounds(entity, &children, &meshes) else {
            continue;
        };

        commands.entity(entity).remove::<AutoFit>();
        let Some(object) = layout.object_mut(placeable.0) else {
            continue;
        };
        object.auto_fit = false;

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

fn setup_room(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let light_gray = materials.add(Color::srgb(0.6, 0.6, 0.6));
    let dark_gray = materials.add(Color::srgb(0.3, 0.3, 0.3));

    let tile_size = 0.1;
    let half_width_tiles: i32 = 6;
    let depth_tiles: i32 = 10;
    let room_height = 0.8;

    let room_half_width = half_width_tiles as f32 * tile_size;
    let room_depth = depth_tiles as f32 * tile_size;
    let ceiling_y = FLOOR_Y + room_height;
    let wall_z = -room_depth;
    let center_z = -room_depth * 0.5;

    // 床(チェッカー模様、画面から奥に向かって奥行きの目印になる)
    for ix in -half_width_tiles..half_width_tiles {
        for iz in 0..depth_tiles {
            let x = ix as f32 * tile_size + tile_size * 0.5;
            let z = -(iz as f32 * tile_size) - tile_size * 0.5;
            let material = if (ix + iz).rem_euclid(2) == 0 {
                light_gray.clone()
            } else {
                dark_gray.clone()
            };

            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(tile_size, 0.002, tile_size))),
                MeshMaterial3d(material),
                Transform::from_xyz(x, FLOOR_Y, z),
                RoomPart,
            ));
        }
    }

    let wall_color = materials.add(Color::srgb(0.4, 0.4, 0.45));

    // 背面の壁(奥行きの終端を示す)
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(room_half_width * 2.0, room_height, 0.02))),
        MeshMaterial3d(wall_color.clone()),
        Transform::from_xyz(0.0, FLOOR_Y + room_height * 0.5, wall_z),
        RoomPart,
    ));

    // 左の壁
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(0.02, room_height, room_depth))),
        MeshMaterial3d(wall_color.clone()),
        Transform::from_xyz(-room_half_width, FLOOR_Y + room_height * 0.5, center_z),
        RoomPart,
    ));

    // 右の壁
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(0.02, room_height, room_depth))),
        MeshMaterial3d(wall_color),
        Transform::from_xyz(room_half_width, FLOOR_Y + room_height * 0.5, center_z),
        RoomPart,
    ));

    // 天井
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(room_half_width * 2.0, 0.02, room_depth))),
        MeshMaterial3d(materials.add(Color::srgb(0.5, 0.5, 0.55))),
        Transform::from_xyz(0.0, ceiling_y, center_z),
        RoomPart,
    ));
}
