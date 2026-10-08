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

    // Transform から作る (scale は 1。モデルの中のノードでは倍率として使うため)
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

// モデルの中のノードに対する、アプリでの変更。ノードは Blender のオブジェクト名で区別する
#[derive(Clone, Debug, PartialEq)]
pub struct NodeOverride {
    pub name: String,
    // 位置と回転は置き換え (モデルの中の座標)、scale は Blender での大きさに掛ける倍率。None なら Blender の値のまま
    pub placement: Option<Placement>,
    pub hidden: bool,
}

// .glb / .gltf のモデル
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    // シーンの中の相対パス
    pub path: String,
    // 元のファイルの場所 (「再読み込み」で読み直す。無いPCでは使えない)
    pub source: Option<String>,
    // 中のノード (Blender で親を持たないオブジェクト) への変更
    pub nodes: Vec<NodeOverride>,
}

impl Model {
    pub fn new(path: &str, source: Option<String>) -> Self {
        Self {
            path: path.to_string(),
            source,
            nodes: Vec::new(),
        }
    }

    pub fn node(&self, name: &str) -> Option<&NodeOverride> {
        self.nodes.iter().find(|n| n.name == name)
    }

    // ノードの変更を取り出す。無ければ作る
    pub fn node_mut(&mut self, name: &str) -> &mut NodeOverride {
        if let Some(i) = self.nodes.iter().position(|n| n.name == name) {
            return &mut self.nodes[i];
        }
        self.nodes.push(NodeOverride {
            name: name.to_string(),
            placement: None,
            hidden: false,
        });
        self.nodes.last_mut().unwrap()
    }

    // 何も変えていないノードの記録を消す
    pub fn prune_nodes(&mut self) {
        self.nodes.retain(|n| n.placement.is_some() || n.hidden);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ObjectKind {
    Model(Model),
    Cube { color: Vec3, size: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneObject {
    // 実行中だけ使う識別子 (保存しない)
    pub id: u64,
    pub name: String,
    pub kind: ObjectKind,
    pub placement: Placement,
    // インポートした直後。読み込みが終わったら置き方を決める (scene.rs。保存しない)
    pub place_on_load: bool,
}

impl SceneObject {
    pub fn model(&self) -> Option<&Model> {
        match &self.kind {
            ObjectKind::Model(model) => Some(model),
            ObjectKind::Cube { .. } => None,
        }
    }

    pub fn model_mut(&mut self) -> Option<&mut Model> {
        match &mut self.kind {
            ObjectKind::Model(model) => Some(model),
            ObjectKind::Cube { .. } => None,
        }
    }
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
    // 手前の上から、奥へ斜め下に照らす (スクリーンから見える面が明るくなる)
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

// スクリーン (実際のディスプレイ) とオブジェクトの配置。scenes/<名前>.peek に保存する (library.rs)
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct SceneLayout {
    // スクリーンの中心と向き (scale は使わない)
    pub screen: Placement,
    // シーンを作ったときの画面の幅と高さ (m)。ほかの大きさの画面では、この範囲が全部入るように縮尺を変える
    pub reference: Option<Vec2>,
    pub light: LightSettings,
    // アプリが作る部屋 (床と壁) を表示するか
    pub show_room: bool,
    pub objects: Vec<SceneObject>,
    next_id: u64,
}

impl Default for SceneLayout {
    // テスト用の、モデルとキューブを置いたシーン
    fn default() -> Self {
        let mut layout = Self::empty();
        layout.add_object(
            "izayoi",
            ObjectKind::Model(Model::new("models/izayoi/izayoi_02.glb", None)),
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
    // スクリーンと部屋だけの空のシーン
    pub fn empty() -> Self {
        Self {
            screen: Placement::default(),
            reference: None,
            light: LightSettings::default(),
            show_room: true,
            objects: Vec::new(),
            next_id: 0,
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
            place_on_load: false,
        });
        id
    }

    pub fn object_mut(&mut self, id: u64) -> Option<&mut SceneObject> {
        self.objects.iter_mut().find(|o| o.id == id)
    }

    pub fn object(&self, id: u64) -> Option<&SceneObject> {
        self.objects.iter().find(|o| o.id == id)
    }

    // 保存される内容(id 以外)が同じか
    pub fn same_content(&self, other: &Self) -> bool {
        self.screen == other.screen
            && self.reference == other.reference
            && self.light == other.light
            && self.show_room == other.show_room
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
        self.screen = other.screen;
        self.reference = other.reference;
        self.light = other.light;
        self.show_room = other.show_room;
        self.objects = other.objects;
    }

    // シーンファイル形式の文字列を読む
    pub fn parse(text: &str) -> Self {
        enum Section {
            Screen,
            Reference,
            Light,
            Room,
            Object(u64),
            // 直前の [model] の中のノード (オブジェクトの id、nodes の位置)
            Node(u64, usize),
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
                    "screen" => Section::Screen,
                    "light" => Section::Light,
                    "reference" => Section::Reference,
                    "room" => Section::Room,
                    "model" => Section::Object(layout.add_object(
                        "model",
                        ObjectKind::Model(Model::new("", None)),
                        Placement::default(),
                    )),
                    "node" => {
                        let parent = match &section {
                            Section::Object(id) | Section::Node(id, _) => Some(*id),
                            _ => None,
                        };
                        let model = parent.and_then(|id| layout.object_mut(id)).and_then(|o| o.model_mut());
                        match (parent, model) {
                            (Some(id), Some(model)) => {
                                model.nodes.push(NodeOverride {
                                    name: String::new(),
                                    placement: None,
                                    hidden: false,
                                });
                                Section::Node(id, model.nodes.len() - 1)
                            }
                            _ => {
                                warn!("{}: [model] の後ろにない [node] を無視しました", SCENE_FILE_NAME);
                                Section::Unknown
                            }
                        }
                    }
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
                Section::Screen => {
                    if !parse_placement(&mut layout.screen, key, value, false) {
                        warn!("{}: 不明な設定項目を無視しました: {}", SCENE_FILE_NAME, key);
                    }
                }
                Section::Reference => {
                    let size = layout.reference.get_or_insert(Vec2::ZERO);
                    match key {
                        "width" => size.x = parse_or_warn(SCENE_FILE_NAME, key, value, size.x),
                        "height" => size.y = parse_or_warn(SCENE_FILE_NAME, key, value, size.y),
                        _ => warn!("{}: 不明な設定項目を無視しました: {}", SCENE_FILE_NAME, key),
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
                Section::Room => match key {
                    "visible" => layout.show_room = parse_or_warn(SCENE_FILE_NAME, key, value, layout.show_room),
                    _ => warn!("{}: 不明な設定項目を無視しました: {}", SCENE_FILE_NAME, key),
                },
                Section::Object(id) => {
                    let object = layout.objects.iter_mut().find(|o| o.id == *id).unwrap();
                    match (key, &mut object.kind) {
                        ("name", _) => object.name = value.to_string(),
                        ("path", ObjectKind::Model(model)) => model.path = value.to_string(),
                        ("source", ObjectKind::Model(model)) => {
                            model.source = Some(value.to_string()).filter(|s| !s.is_empty())
                        }
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
                Section::Node(id, i) => {
                    let Some(model) = layout.object_mut(*id).and_then(|o| o.model_mut()) else {
                        continue;
                    };
                    let node = &mut model.nodes[*i];
                    match key {
                        "name" => node.name = value.to_string(),
                        "hidden" => node.hidden = parse_or_warn(SCENE_FILE_NAME, key, value, node.hidden),
                        _ => {
                            let placement = node.placement.get_or_insert_with(Placement::default);
                            if !parse_placement(placement, key, value, true) {
                                warn!("{}: 不明な設定項目を無視しました: {}", SCENE_FILE_NAME, key);
                            }
                        }
                    }
                }
            }
        }

        layout.objects.retain(|o| match &o.kind {
            ObjectKind::Model(model) if model.path.is_empty() => {
                warn!("{}: path の無い [model] を無視しました", SCENE_FILE_NAME);
                false
            }
            _ => true,
        });
        for model in layout.objects.iter_mut().filter_map(|o| o.model_mut()) {
            model.nodes.retain(|n| {
                if n.name.is_empty() {
                    warn!("{}: name の無い [node] を無視しました", SCENE_FILE_NAME);
                }
                !n.name.is_empty()
            });
        }
        layout
    }

    // シーンファイル形式の文字列にする
    pub fn to_ini(&self) -> String {
        let mut text = String::new();
        let _ = writeln!(text, "# peek_display シーンファイル");
        let _ = writeln!(text, "# アプリの編集モードで保存すると、この内容は丸ごと書き直されます");
        let _ = writeln!(text, "# 長さはメートル。回転は度で、pitch = X軸まわり、yaw = Y軸まわり、roll = Z軸まわり");

        let _ = writeln!(text);
        let _ = writeln!(text, "# スクリーン = 実際のディスプレイの中心と向き");
        let _ = writeln!(text, "[screen]");
        write_placement(&mut text, &self.screen, false);

        if let Some(size) = self.reference {
            let _ = writeln!(text);
            let _ = writeln!(text, "# シーンを作ったときの画面の幅と高さ(m)。ほかの大きさの画面では、この範囲が全部入るように縮尺を変える");
            let _ = writeln!(text, "[reference]");
            let _ = writeln!(text, "width={}", size.x);
            let _ = writeln!(text, "height={}", size.y);
        }

        let _ = writeln!(text);
        let _ = writeln!(text, "# 照明。illuminance = 太陽光の明るさ(ルクス)、ambient = 環境光の明るさ、pitch/yaw = 太陽光の向き(度)");
        let _ = writeln!(text, "[light]");
        let _ = writeln!(text, "illuminance={}", self.light.illuminance);
        let _ = writeln!(text, "ambient={}", self.light.ambient);
        let _ = writeln!(text, "pitch={}", self.light.pitch_deg);
        let _ = writeln!(text, "yaw={}", self.light.yaw_deg);

        let _ = writeln!(text);
        let _ = writeln!(text, "# アプリが作る部屋 (床と壁)");
        let _ = writeln!(text, "[room]");
        let _ = writeln!(text, "visible={}", self.show_room);

        for object in &self.objects {
            let _ = writeln!(text);
            match &object.kind {
                ObjectKind::Model(model) => {
                    let _ = writeln!(text, "# path = シーンの中の相対パス、source = 元のファイルの場所");
                    let _ = writeln!(text, "[model]");
                    let _ = writeln!(text, "name={}", object.name);
                    let _ = writeln!(text, "path={}", model.path);
                    if let Some(source) = &model.source {
                        let _ = writeln!(text, "source={}", source);
                    }
                }
                ObjectKind::Cube { color, size } => {
                    let _ = writeln!(text, "[cube]");
                    let _ = writeln!(text, "name={}", object.name);
                    let _ = writeln!(text, "color={}", format_vec3(*color));
                    let _ = writeln!(text, "size={}", size);
                }
            }
            write_placement(&mut text, &object.placement, true);

            for node in object.model().map(|m| m.nodes.as_slice()).unwrap_or_default() {
                let _ = writeln!(text);
                let _ = writeln!(text, "# 直前のモデルの中のノードの変更 (scale は Blender での大きさに掛ける倍率)");
                let _ = writeln!(text, "[node]");
                let _ = writeln!(text, "name={}", node.name);
                if node.hidden {
                    let _ = writeln!(text, "hidden=true");
                }
                if let Some(placement) = &node.placement {
                    write_placement(&mut text, placement, true);
                }
            }
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
