use bevy::prelude::*;
use std::fmt::Write as _;

use crate::config::parse_or_warn;

// 警告に出すファイルの種類
const SCENE_FILE_NAME: &str = "シーンファイル";

// 床の高さ。部屋の生成と、インポートしたモデルの初期配置で使う
pub const FLOOR_Y: f32 = -0.15;

// 位置・回転・拡大率。
// 回転は X/Y/Z 軸まわりの角度(度)で持ち、Y → X → Z の順にかける
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub position: Vec3,
    // X軸まわり
    pub pitch_deg: f32,
    // Y軸まわり
    pub yaw_deg: f32,
    // Z軸まわり
    pub roll_deg: f32,
    pub scale: f32,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            pitch_deg: 0.0,
            yaw_deg: 0.0,
            roll_deg: 0.0,
            scale: 1.0,
        }
    }
}

impl Placement {
    pub fn at(position: Vec3) -> Self {
        Self {
            position,
            ..default()
        }
    }

    pub fn rotation(&self) -> Quat {
        Quat::from_euler(
            EulerRot::YXZ,
            self.yaw_deg.to_radians(),
            self.pitch_deg.to_radians(),
            self.roll_deg.to_radians(),
        )
    }

    // Transform から作る (scale は 1。ワールドのノードでは倍率として使うため)
    pub fn from_transform(transform: &Transform) -> Self {
        let (yaw, pitch, roll) = transform.rotation.to_euler(EulerRot::YXZ);
        Self {
            position: transform.translation,
            pitch_deg: pitch.to_degrees(),
            yaw_deg: yaw.to_degrees(),
            roll_deg: roll.to_degrees(),
            scale: 1.0,
        }
    }

    pub fn to_transform(&self) -> Transform {
        Transform::from_translation(self.position)
            .with_rotation(self.rotation())
            .with_scale(Vec3::splat(self.scale))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ObjectKind {
    // assets フォルダからの相対パス (.glb / .gltf)
    Model { path: String },
    Cube { color: Vec3, size: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneObject {
    // 実行中だけ使う識別子 (保存しない)
    pub id: u64,
    pub name: String,
    pub kind: ObjectKind,
    pub placement: Placement,
    // 読み込み完了後に大きさを自動調整する (インポート直後のみ。保存しない)
    pub auto_fit: bool,
}

// シーンの照明。アプリが置く太陽光 (平行光源) と、全体をうっすら照らす環境光
#[derive(Clone, Debug, PartialEq)]
pub struct LightSettings {
    // 太陽光の明るさ (ルクス)
    pub illuminance: f32,
    // 環境光の明るさ
    pub ambient: f32,
    // 太陽光の向き。X軸まわり (傾き) と Y軸まわり (方角) の角度 (度)
    pub pitch_deg: f32,
    pub yaw_deg: f32,
}

impl Default for LightSettings {
    // 手前の上から、奥へ斜め下に照らす (のぞき窓から見える面が明るくなる)
    fn default() -> Self {
        Self {
            illuminance: 10_000.0,
            ambient: 80.0,
            pitch_deg: -57.3,
            yaw_deg: 0.0,
        }
    }
}

impl LightSettings {
    pub fn rotation(&self) -> Quat {
        Quat::from_euler(EulerRot::YXZ, self.yaw_deg.to_radians(), self.pitch_deg.to_radians(), 0.0)
    }

    // 光が進む向き
    pub fn direction(&self) -> Vec3 {
        self.rotation() * Vec3::NEG_Z
    }
}

// ワールド(.glb)の中のノードに対する、アプリでの変更。ノードは Blender のオブジェクト名で区別する
#[derive(Clone, Debug, PartialEq)]
pub struct WorldOverride {
    pub node: String,
    // 位置と回転は置き換え、scale は Blender での大きさに掛ける倍率。None なら Blender の値のまま
    pub placement: Option<Placement>,
    pub hidden: bool,
}

// のぞき窓(ディスプレイ面)とオブジェクトの配置。scenes/<名前>.ini に保存する (library.rs)
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct SceneLayout {
    // Blender などで作ったワールド (assets フォルダからの相対パス)
    pub world: Option<String>,
    // のぞき窓の中心と向き (scale は使わない)
    pub window: Placement,
    pub light: LightSettings,
    pub world_overrides: Vec<WorldOverride>,
    pub objects: Vec<SceneObject>,
    // ワールドの読み込みが終わったら、のぞき窓を peek_window の位置に合わせる (保存しない)
    pub window_from_world: bool,
    next_id: u64,
}

impl Default for SceneLayout {
    // シーンが1つも無いときの初期配置
    fn default() -> Self {
        let mut layout = Self::empty();
        layout.add_object(
            "izayoi",
            ObjectKind::Model {
                path: "models/izayoi/izayoi_02.glb".to_string(),
            },
            Placement {
                position: Vec3::new(0.0, FLOOR_Y, -0.4),
                scale: 0.15,
                ..default()
            },
        );
        for (name, color, position) in [
            ("red", Vec3::new(1.0, 0.0, 0.0), Vec3::new(-0.05, 0.0, -0.1)),
            ("green", Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, -0.3)),
            ("blue", Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.05, 0.0, -0.5)),
        ] {
            layout.add_object(
                name,
                ObjectKind::Cube { color, size: 0.05 },
                Placement::at(position),
            );
        }
        layout
    }
}

impl SceneLayout {
    // のぞき窓だけの空のシーン
    pub fn empty() -> Self {
        Self {
            world: None,
            window: Placement::default(),
            light: LightSettings::default(),
            world_overrides: Vec::new(),
            objects: Vec::new(),
            window_from_world: false,
            next_id: 0,
        }
    }

    // ワールドだけのシーン。のぞき窓はワールドの peek_window に合わせる
    pub fn with_world(path: &str) -> Self {
        Self {
            world: Some(path.to_string()),
            window_from_world: true,
            ..Self::empty()
        }
    }

    pub fn add_object(&mut self, name: &str, kind: ObjectKind, placement: Placement) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.objects.push(SceneObject {
            id,
            name: name.to_string(),
            kind,
            placement,
            auto_fit: false,
        });
        id
    }

    pub fn object_mut(&mut self, id: u64) -> Option<&mut SceneObject> {
        self.objects.iter_mut().find(|o| o.id == id)
    }

    pub fn object(&self, id: u64) -> Option<&SceneObject> {
        self.objects.iter().find(|o| o.id == id)
    }

    pub fn world_override(&self, node: &str) -> Option<&WorldOverride> {
        self.world_overrides.iter().find(|o| o.node == node)
    }

    // ノードの変更を取り出す。無ければ作る
    pub fn world_override_mut(&mut self, node: &str) -> &mut WorldOverride {
        if let Some(i) = self.world_overrides.iter().position(|o| o.node == node) {
            return &mut self.world_overrides[i];
        }
        self.world_overrides.push(WorldOverride {
            node: node.to_string(),
            placement: None,
            hidden: false,
        });
        self.world_overrides.last_mut().unwrap()
    }

    // 何も変えていないノードの記録を消す
    pub fn prune_world_overrides(&mut self) {
        self.world_overrides.retain(|o| o.placement.is_some() || o.hidden);
    }

    // 保存される内容(id 以外)が同じか
    pub fn same_content(&self, other: &Self) -> bool {
        self.world == other.world
            && self.window == other.window
            && self.light == other.light
            && self.world_overrides == other.world_overrides
            && self.objects.len() == other.objects.len()
            && self.objects.iter().zip(&other.objects).all(|(a, b)| {
                a.name == b.name && a.kind == b.kind && a.placement == b.placement
            })
    }

    // 元に戻す・やり直しで、同じシーンの以前の状態に戻す。
    // id はそのまま使い、残っているオブジェクトのエンティティを使い回す
    pub fn restore(&mut self, snapshot: &Self) {
        let next_id = self.next_id.max(snapshot.next_id);
        *self = snapshot.clone();
        self.next_id = next_id;
    }

    // 別のシーンに入れ替える。id は今の続きから振り直し、
    // 既存のエンティティが別のオブジェクトと取り違えられないようにする
    pub fn replace_with(&mut self, mut other: Self) {
        for object in &mut other.objects {
            object.id = self.next_id;
            self.next_id += 1;
        }
        self.world = other.world;
        self.window = other.window;
        self.light = other.light;
        self.world_overrides = other.world_overrides;
        self.objects = other.objects;
        self.window_from_world = other.window_from_world;
    }

    // シーンファイル形式の文字列を読む
    pub fn parse(text: &str) -> Self {
        enum Section {
            World,
            Window,
            Light,
            WorldNode(usize),
            Object(u64),
            Unknown,
        }
        let mut layout = Self::empty();
        let mut section = Section::Unknown;

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                section = match name.trim() {
                    "world" => Section::World,
                    "window" => Section::Window,
                    "light" => Section::Light,
                    "world_node" => {
                        layout.world_overrides.push(WorldOverride {
                            node: String::new(),
                            placement: None,
                            hidden: false,
                        });
                        Section::WorldNode(layout.world_overrides.len() - 1)
                    }
                    "model" => Section::Object(layout.add_object(
                        "model",
                        ObjectKind::Model {
                            path: String::new(),
                        },
                        Placement::default(),
                    )),
                    "cube" => Section::Object(layout.add_object(
                        "cube",
                        ObjectKind::Cube {
                            color: Vec3::ONE,
                            size: 0.05,
                        },
                        Placement::default(),
                    )),
                    other => {
                        warn!("{}: 不明なセクションを無視しました: [{}]", SCENE_FILE_NAME, other);
                        Section::Unknown
                    }
                };
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();

            match &section {
                Section::Unknown => {}
                Section::World => match key {
                    "path" => layout.world = Some(value.to_string()).filter(|p| !p.is_empty()),
                    _ => warn!("{}: 不明な設定項目を無視しました: {}", SCENE_FILE_NAME, key),
                },
                Section::Window => {
                    if !parse_placement(&mut layout.window, key, value, false) {
                        warn!("{}: 不明な設定項目を無視しました: {}", SCENE_FILE_NAME, key);
                    }
                }
                Section::Light => {
                    let light = &mut layout.light;
                    match key {
                        "illuminance" => light.illuminance = parse_or_warn(SCENE_FILE_NAME, key, value, light.illuminance),
                        "ambient" => light.ambient = parse_or_warn(SCENE_FILE_NAME, key, value, light.ambient),
                        "pitch" => light.pitch_deg = parse_or_warn(SCENE_FILE_NAME, key, value, light.pitch_deg),
                        "yaw" => light.yaw_deg = parse_or_warn(SCENE_FILE_NAME, key, value, light.yaw_deg),
                        _ => warn!("{}: 不明な設定項目を無視しました: {}", SCENE_FILE_NAME, key),
                    }
                }
                Section::WorldNode(i) => {
                    let o = &mut layout.world_overrides[*i];
                    match key {
                        "name" => o.node = value.to_string(),
                        "hidden" => o.hidden = parse_or_warn(SCENE_FILE_NAME, key, value, o.hidden),
                        _ => {
                            let placement = o.placement.get_or_insert_with(Placement::default);
                            if !parse_placement(placement, key, value, true) {
                                warn!("{}: 不明な設定項目を無視しました: {}", SCENE_FILE_NAME, key);
                            }
                        }
                    }
                }
                Section::Object(id) => {
                    let object = layout.objects.iter_mut().find(|o| o.id == *id).unwrap();
                    match (key, &mut object.kind) {
                        ("name", _) => object.name = value.to_string(),
                        ("path", ObjectKind::Model { path }) => *path = value.to_string(),
                        ("color", ObjectKind::Cube { color, .. }) => *color = parse_vec3(key, value, *color),
                        ("size", ObjectKind::Cube { size, .. }) => {
                            *size = parse_or_warn(SCENE_FILE_NAME, key, value, *size)
                        }
                        _ => {
                            if !parse_placement(&mut object.placement, key, value, true) {
                                warn!("{}: 不明な設定項目を無視しました: {}", SCENE_FILE_NAME, key);
                            }
                        }
                    }
                }
            }
        }

        layout.objects.retain(|o| match &o.kind {
            ObjectKind::Model { path } if path.is_empty() => {
                warn!("{}: path の無い [model] を無視しました", SCENE_FILE_NAME);
                false
            }
            _ => true,
        });
        layout.world_overrides.retain(|o| {
            if o.node.is_empty() {
                warn!("{}: name の無い [world_node] を無視しました", SCENE_FILE_NAME);
            }
            !o.node.is_empty()
        });
        layout
    }

    // シーンファイル形式の文字列にする
    pub fn to_ini(&self) -> String {
        let mut text = String::new();
        let _ = writeln!(text, "# peek_display シーンファイル");
        let _ = writeln!(text, "# アプリの編集モードで保存すると、この内容は丸ごと書き直されます");
        let _ = writeln!(text, "# 長さはメートル。回転は度で、pitch = X軸まわり、yaw = Y軸まわり、roll = Z軸まわり");

        if let Some(world) = &self.world {
            let _ = writeln!(text);
            let _ = writeln!(text, "# Blender などで作ったワールド (assets フォルダからの相対パス)");
            let _ = writeln!(text, "[world]");
            let _ = writeln!(text, "path={}", world);
        }

        let _ = writeln!(text);
        let _ = writeln!(text, "# のぞき窓 = ディスプレイ面の中心と向き");
        let _ = writeln!(text, "[window]");
        write_placement(&mut text, &self.window, false);

        let _ = writeln!(text);
        let _ = writeln!(text, "# 照明。illuminance = 太陽光の明るさ(ルクス)、ambient = 環境光の明るさ、pitch/yaw = 太陽光の向き(度)");
        let _ = writeln!(text, "[light]");
        let _ = writeln!(text, "illuminance={}", self.light.illuminance);
        let _ = writeln!(text, "ambient={}", self.light.ambient);
        let _ = writeln!(text, "pitch={}", self.light.pitch_deg);
        let _ = writeln!(text, "yaw={}", self.light.yaw_deg);

        for o in &self.world_overrides {
            let _ = writeln!(text);
            let _ = writeln!(text, "# ワールドの中のノードの変更 (scale は Blender での大きさに掛ける倍率)");
            let _ = writeln!(text, "[world_node]");
            let _ = writeln!(text, "name={}", o.node);
            if o.hidden {
                let _ = writeln!(text, "hidden=true");
            }
            if let Some(placement) = &o.placement {
                write_placement(&mut text, placement, true);
            }
        }

        for object in &self.objects {
            let _ = writeln!(text);
            match &object.kind {
                ObjectKind::Model { path } => {
                    let _ = writeln!(text, "[model]");
                    let _ = writeln!(text, "name={}", object.name);
                    let _ = writeln!(text, "path={}", path);
                }
                ObjectKind::Cube { color, size } => {
                    let _ = writeln!(text, "[cube]");
                    let _ = writeln!(text, "name={}", object.name);
                    let _ = writeln!(text, "color={}", format_vec3(*color));
                    let _ = writeln!(text, "size={}", size);
                }
            }
            write_placement(&mut text, &object.placement, true);
        }
        text
    }
}

// 位置・回転・大きさの項目を読む。該当する項目でなければ false
fn parse_placement(placement: &mut Placement, key: &str, value: &str, with_scale: bool) -> bool {
    match key {
        "position" => placement.position = parse_vec3(key, value, placement.position),
        "pitch" => placement.pitch_deg = parse_or_warn(SCENE_FILE_NAME, key, value, placement.pitch_deg),
        "yaw" => placement.yaw_deg = parse_or_warn(SCENE_FILE_NAME, key, value, placement.yaw_deg),
        "roll" => placement.roll_deg = parse_or_warn(SCENE_FILE_NAME, key, value, placement.roll_deg),
        "scale" if with_scale => placement.scale = parse_or_warn(SCENE_FILE_NAME, key, value, placement.scale),
        _ => return false,
    }
    true
}

fn write_placement(text: &mut String, placement: &Placement, with_scale: bool) {
    let _ = writeln!(text, "position={}", format_vec3(placement.position));
    let _ = writeln!(text, "pitch={}", placement.pitch_deg);
    let _ = writeln!(text, "yaw={}", placement.yaw_deg);
    let _ = writeln!(text, "roll={}", placement.roll_deg);
    if with_scale {
        let _ = writeln!(text, "scale={}", placement.scale);
    }
}

fn format_vec3(v: Vec3) -> String {
    format!("{},{},{}", v.x, v.y, v.z)
}

fn parse_vec3(key: &str, value: &str, fallback: Vec3) -> Vec3 {
    let parts: Vec<_> = value.split(',').map(|s| s.trim().parse::<f32>()).collect();
    match parts.as_slice() {
        [Ok(x), Ok(y), Ok(z)] => Vec3::new(*x, *y, *z),
        _ => {
            warn!(
                "{}: {} の値 '{}' を読めませんでした (x,y,z の形で書いてください)。{} を使います",
                SCENE_FILE_NAME,
                key,
                value,
                format_vec3(fallback)
            );
            fallback
        }
    }
}
