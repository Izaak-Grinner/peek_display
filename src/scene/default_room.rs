// 新しいシーンに最初から置く部屋 (チェッカー模様の床と壁、天井) のモデル。
// インポートしたモデルと同じように扱えるよう、glTF (.gltf + .bin) のファイルとして作る
use bevy::prelude::*;
use std::fmt::Write as _;

use crate::scene::layout::{FLOOR_Y, Model, ObjectKind, Placement, SceneLayout};
use crate::scene::package::PackageFiles;

// 部屋のオブジェクトの名前と、シーンの中での置き場所
const ROOM_NAME: &str = "部屋";
const ROOM_FOLDER: &str = "models/room";
const GLTF_NAME: &str = "room.gltf";
const BIN_NAME: &str = "room.bin";

// 部屋の大きさ (メートル)
const TILE_SIZE: f32 = 0.1;
const HALF_WIDTH_TILES: i32 = 6;
const DEPTH_TILES: i32 = 10;
const ROOM_HEIGHT: f32 = 0.8;
const WALL_THICKNESS: f32 = 0.02;
const FLOOR_THICKNESS: f32 = 0.002;

// 部屋のモデルの本体のパス (シーンの中の相対パス)
fn room_path() -> String {
    format!("{}/{}", ROOM_FOLDER, GLTF_NAME)
}

// 部屋のモデルのファイル (シーンの中の相対パス → 中身)
fn room_files() -> PackageFiles {
    let width = HALF_WIDTH_TILES as f32 * 2.0 * TILE_SIZE;
    let depth = DEPTH_TILES as f32 * TILE_SIZE;
    let wall_y = FLOOR_Y + ROOM_HEIGHT * 0.5;

    let light_gray = material("明るい床", Color::srgb(0.6, 0.6, 0.6));
    let dark_gray = material("暗い床", Color::srgb(0.3, 0.3, 0.3));
    let wall = material("壁", Color::srgb(0.4, 0.4, 0.45));
    let ceiling = material("天井", Color::srgb(0.5, 0.5, 0.55));

    // 床はタイルを色ごとに1つのメッシュにまとめる (画面から奥に向かって奥行きの目印になる)
    let mut light_tiles = Geometry::default();
    let mut dark_tiles = Geometry::default();
    for ix in -HALF_WIDTH_TILES..HALF_WIDTH_TILES {
        for iz in 0..DEPTH_TILES {
            let x = ix as f32 * TILE_SIZE + TILE_SIZE * 0.5;
            let z = -(iz as f32 * TILE_SIZE) - TILE_SIZE * 0.5 + depth * 0.5;
            let tiles = if (ix + iz).rem_euclid(2) == 0 { &mut light_tiles } else { &mut dark_tiles };
            tiles.add_box(Vec3::new(x, 0.0, z), Vec3::new(TILE_SIZE, FLOOR_THICKNESS, TILE_SIZE));
        }
    }

    let parts = [
        Part {
            name: "床",
            center: Vec3::new(0.0, FLOOR_Y, -depth * 0.5),
            primitives: vec![(light_tiles, 0), (dark_tiles, 1)],
        },
        Part {
            name: "奥の壁",
            center: Vec3::new(0.0, wall_y, -depth),
            primitives: vec![(Geometry::cuboid(Vec3::new(width, ROOM_HEIGHT, WALL_THICKNESS)), 2)],
        },
        Part {
            name: "左の壁",
            center: Vec3::new(-width * 0.5, wall_y, -depth * 0.5),
            primitives: vec![(Geometry::cuboid(Vec3::new(WALL_THICKNESS, ROOM_HEIGHT, depth)), 2)],
        },
        Part {
            name: "右の壁",
            center: Vec3::new(width * 0.5, wall_y, -depth * 0.5),
            primitives: vec![(Geometry::cuboid(Vec3::new(WALL_THICKNESS, ROOM_HEIGHT, depth)), 2)],
        },
        Part {
            name: "天井",
            center: Vec3::new(0.0, FLOOR_Y + ROOM_HEIGHT, -depth * 0.5),
            primitives: vec![(Geometry::cuboid(Vec3::new(width, WALL_THICKNESS, depth)), 3)],
        },
    ];
    let (gltf, bin) = write_gltf(&parts, &[light_gray, dark_gray, wall, ceiling]);

    let mut files = PackageFiles::new();
    files.insert(room_path(), gltf.into_bytes());
    files.insert(format!("{}/{}", ROOM_FOLDER, BIN_NAME), bin);
    files
}

// 一番上の階層のノード (アプリで個別に動かせる)。メッシュはノードの位置を中心に作る
struct Part {
    name: &'static str,
    center: Vec3,
    // メッシュと、使うマテリアルの番号
    primitives: Vec<(Geometry, usize)>,
}

#[derive(Default)]
struct Geometry {
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
    indices: Vec<u32>,
}

impl Geometry {
    fn cuboid(size: Vec3) -> Self {
        let mut geometry = Self::default();
        geometry.add_box(Vec3::ZERO, size);
        geometry
    }

    // 直方体を足す。面ごとに頂点を分け、法線を外向きにする
    fn add_box(&mut self, center: Vec3, size: Vec3) {
        let half = size * 0.5;
        for normal in [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z] {
            // 面の上で直交する2方向 (u × v = normal になるように取る)
            let u = if normal.y.abs() > 0.5 { Vec3::X } else { Vec3::Y.cross(normal) };
            let v = normal.cross(u);
            let base = self.positions.len() as u32;
            for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let corner = normal + u * su + v * sv;
                self.positions.push(center + corner * half);
                self.normals.push(normal);
            }
            self.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
}

// glTF のマテリアル (JSON)。baseColorFactor はリニアの色で書く
fn material(name: &str, color: Color) -> String {
    let c = color.to_linear();
    format!(
        r#"{{"name":"{}","pbrMetallicRoughness":{{"baseColorFactor":[{},{},{},1],"metallicFactor":0,"roughnessFactor":0.5}}}}"#,
        name, c.red, c.green, c.blue
    )
}

// .gltf (JSON) と .bin (頂点データ) を作る
fn write_gltf(parts: &[Part], materials: &[String]) -> (String, Vec<u8>) {
    let mut bin = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let mut meshes = Vec::new();

    // データを .bin に足し、bufferView と accessor を作って accessor の番号を返す
    let mut push = |bytes: Vec<u8>, target: u32, accessor: String| -> usize {
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        views.push(format!(
            r#"{{"buffer":0,"byteOffset":{},"byteLength":{},"target":{}}}"#,
            bin.len(),
            bytes.len(),
            target
        ));
        bin.extend(bytes);
        accessors.push(accessor.replace("VIEW", &(views.len() - 1).to_string()));
        accessors.len() - 1
    };

    for part in parts {
        let mut primitives = Vec::new();
        for (geometry, material) in &part.primitives {
            let (min, max) = geometry.positions.iter().fold(
                (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
                |(min, max), p| (min.min(*p), max.max(*p)),
            );
            let count = geometry.positions.len();
            let position = push(
                vec3_bytes(&geometry.positions),
                34962,
                format!(
                    r#"{{"bufferView":VIEW,"componentType":5126,"count":{},"type":"VEC3","min":[{},{},{}],"max":[{},{},{}]}}"#,
                    count, min.x, min.y, min.z, max.x, max.y, max.z
                ),
            );
            let normal = push(
                vec3_bytes(&geometry.normals),
                34962,
                format!(r#"{{"bufferView":VIEW,"componentType":5126,"count":{},"type":"VEC3"}}"#, count),
            );
            let indices = push(
                geometry.indices.iter().flat_map(|i| i.to_le_bytes()).collect(),
                34963,
                format!(
                    r#"{{"bufferView":VIEW,"componentType":5125,"count":{},"type":"SCALAR"}}"#,
                    geometry.indices.len()
                ),
            );
            primitives.push(format!(
                r#"{{"attributes":{{"POSITION":{},"NORMAL":{}}},"indices":{},"material":{}}}"#,
                position, normal, indices, material
            ));
        }
        meshes.push(format!(r#"{{"name":"{}","primitives":[{}]}}"#, part.name, primitives.join(",")));
    }

    let nodes: Vec<String> = parts
        .iter()
        .enumerate()
        .map(|(i, part)| {
            format!(
                r#"{{"name":"{}","mesh":{},"translation":[{},{},{}]}}"#,
                part.name, i, part.center.x, part.center.y, part.center.z
            )
        })
        .collect();
    let node_indices: Vec<String> = (0..parts.len()).map(|i| i.to_string()).collect();

    let mut json = String::new();
    let _ = write!(
        json,
        r#"{{"asset":{{"version":"2.0","generator":"peek_display"}},"scene":0,"scenes":[{{"nodes":[{}]}}],"nodes":[{}],"meshes":[{}],"materials":[{}],"accessors":[{}],"bufferViews":[{}],"buffers":[{{"uri":"{}","byteLength":{}}}]}}"#,
        node_indices.join(","),
        nodes.join(","),
        meshes.join(","),
        materials.join(","),
        accessors.join(","),
        views.join(","),
        BIN_NAME,
        bin.len()
    );
    (json, bin)
}

fn vec3_bytes(values: &[Vec3]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|v| [v.x, v.y, v.z])
        .flat_map(f32::to_le_bytes)
        .collect()
}

// 新規作成したシーン。最初のオブジェクトとして部屋のモデルを置いておく (シーンのレイアウトと、中のファイル)
pub fn new_scene() -> (SceneLayout, PackageFiles) {
    let mut layout = SceneLayout::empty();
    layout.add_object(ROOM_NAME, ObjectKind::Model(Model::new(&room_path())), Placement::default());
    (layout, room_files())
}

#[cfg(test)]
mod tests {
    use super::*;

    // .gltf が参照する .bin が揃っていて、大きさが合っている
    #[test]
    fn room_files_are_consistent() {
        let (layout, files) = new_scene();
        let path = layout.objects[0].model().unwrap().path.clone();
        let gltf = String::from_utf8(files[&path].clone()).unwrap();
        let bin = &files[&format!("{}/{}", ROOM_FOLDER, BIN_NAME)];
        assert!(gltf.contains(&format!(r#""uri":"{}","byteLength":{}"#, BIN_NAME, bin.len())));
        assert_eq!(bin.len() % 4, 0);
        for name in ["床", "奥の壁", "左の壁", "右の壁", "天井"] {
            assert!(gltf.contains(&format!(r#""name":"{}","mesh""#, name)));
        }
    }
}
