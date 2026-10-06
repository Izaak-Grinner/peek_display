use bevy::prelude::*;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use crate::scene::editor::{EditState, SaveStatus, Selection};
use crate::scene::layout::{FLOOR_Y, ObjectKind, Placement, SceneLayout};
use crate::scene::library::{self, SceneLibrary};

// インポートしたモデルとワールドを置く場所 (assets フォルダからの相対パス)
pub(super) const MODEL_DIR: &str = "models/imported";
pub(super) const WORLD_DIR: &str = "worlds";

// 何を取り込むか
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportKind {
    // 今のシーンに足すモデル
    Model,
    // Blender などで作ったワールド。新しいシーンとして開く
    World,
}

// のぞき窓からどれだけ奥に置くか(メートル)
const IMPORT_DISTANCE: f32 = 0.4;

enum ImportResult {
    Cancelled,
    Imported { kind: ImportKind, asset_path: String, name: String },
    Failed(String),
}

// ファイル選択ダイアログとコピーは別スレッドで行い、結果をチャネルで受け取る
#[derive(Resource, Default)]
pub struct ImportState {
    receiver: Option<Mutex<Receiver<ImportResult>>>,
}

impl ImportState {
    pub fn busy(&self) -> bool {
        self.receiver.is_some()
    }

    pub fn start(&mut self, kind: ImportKind) {
        if self.busy() {
            return;
        }
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            let _ = sender.send(run_import(kind));
        });
        self.receiver = Some(Mutex::new(receiver));
    }
}

pub struct ImportPlugin;

impl Plugin for ImportPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ImportState>()
            .add_systems(Update, poll_import);
    }
}

fn poll_import(
    mut import: ResMut<ImportState>,
    mut layout: ResMut<SceneLayout>,
    mut state: ResMut<EditState>,
    mut library: ResMut<SceneLibrary>,
) {
    let Some(receiver) = &import.receiver else {
        return;
    };
    let result = match receiver.lock().unwrap().try_recv() {
        Ok(result) => result,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => ImportResult::Failed("インポート処理が中断されました".into()),
    };
    import.receiver = None;

    match result {
        ImportResult::Cancelled => {}
        ImportResult::Imported {
            kind: ImportKind::World,
            asset_path,
            name,
        } => {
            // ワールドだけの新しいシーンとして保存して開く。のぞき窓はワールドの peek_window に合わせる
            let name = library::unique_name(&name);
            let world = SceneLayout::with_world(&asset_path);
            match library.save_as(&name, &world) {
                Ok(()) => {
                    layout.replace_with(world);
                    state.scene_switched(&layout);
                    info!("{} をワールドとして取り込み、シーン「{}」を作りました", asset_path, name);
                }
                Err(e) => {
                    warn!("{}", e);
                    state.save_status = Some(SaveStatus::Failed(e));
                }
            }
        }
        ImportResult::Imported {
            kind: ImportKind::Model,
            asset_path,
            name,
        } => {
            // のぞき窓の正面・奥に、床に置く (窓が傾いていても、モデルはまっすぐ立てる)
            let window = &layout.window;
            let facing = Quat::from_rotation_y(window.yaw_deg.to_radians());
            let mut position = window.position + facing * Vec3::new(0.0, 0.0, -IMPORT_DISTANCE);
            position.y = FLOOR_Y;
            let placement = Placement {
                position,
                yaw_deg: window.yaw_deg,
                ..default()
            };
            let id = layout.add_object(&name, ObjectKind::Model { path: asset_path.clone() }, placement);
            if let Some(object) = layout.object_mut(id) {
                object.auto_fit = true;
            }
            state.selection = Some(Selection::Object(id));
            state.import_error = None;
            info!("{} をインポートしました", asset_path);
        }
        ImportResult::Failed(e) => {
            warn!("インポートに失敗しました: {}", e);
            state.import_error = Some(e);
        }
    }
}

fn run_import(kind: ImportKind) -> ImportResult {
    let (title, dir) = match kind {
        ImportKind::Model => ("インポートするモデルを選択", MODEL_DIR),
        ImportKind::World => ("インポートするワールドを選択", WORLD_DIR),
    };
    let Some(source) = rfd::FileDialog::new()
        .set_title(title)
        .add_filter("glTF (.glb / .gltf)", &["glb", "gltf"])
        .pick_file()
    else {
        return ImportResult::Cancelled;
    };

    match copy_into_assets(&source, dir) {
        Ok((asset_path, name)) => ImportResult::Imported { kind, asset_path, name },
        Err(e) => ImportResult::Failed(e),
    }
}

// Bevy の AssetServer と同じ基準で assets フォルダを決める
// (BEVY_ASSET_ROOT → CARGO_MANIFEST_DIR → 実行ファイルの場所)
pub(super) fn assets_dir() -> PathBuf {
    let base = std::env::var("BEVY_ASSET_ROOT")
        .or_else(|_| std::env::var("CARGO_MANIFEST_DIR"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .unwrap_or_default()
        });
    base.join("assets")
}

// モデルを assets/<dir>/<名前>/ にコピーし、(アセットパス, 名前) を返す。
// すでに assets フォルダ内にあるファイルはコピーせずにそのまま使う
pub(super) fn copy_into_assets(source: &Path, dir: &str) -> Result<(String, String), String> {
    let assets = assets_dir();
    let name = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "model".to_string());
    let file_name = source
        .file_name()
        .ok_or_else(|| "ファイル名を取得できませんでした".to_string())?;

    if let (Ok(source_abs), Ok(assets_abs)) = (source.canonicalize(), assets.canonicalize()) {
        if let Ok(relative) = source_abs.strip_prefix(&assets_abs) {
            return Ok((to_asset_path(relative), name));
        }
    }

    let is_gltf = source
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("gltf"));

    // 同名のフォルダがあれば _2, _3 … を付ける
    let import_root = assets.join(dir);
    let mut folder = name.clone();
    let mut n = 2;
    while import_root.join(&folder).exists() {
        folder = format!("{}_{}", name, n);
        n += 1;
    }
    let dest_dir = import_root.join(&folder);
    std::fs::create_dir_all(&dest_dir)
        .map_err(|e| format!("{} を作成できませんでした: {}", dest_dir.display(), e))?;

    copy_file(source, &dest_dir.join(file_name))?;

    // .gltf は .bin やテクスチャを別ファイルで参照しているので、それも一緒にコピーする
    if is_gltf {
        let text = std::fs::read_to_string(source)
            .map_err(|e| format!("{} を読めませんでした: {}", source.display(), e))?;
        let source_dir = source.parent().unwrap_or(Path::new(""));
        for uri in referenced_uris(&text) {
            let relative = PathBuf::from(&uri);
            if relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
            {
                warn!("フォルダの外を参照しているためコピーしません: {}", uri);
                continue;
            }
            let dest = dest_dir.join(&relative);
            if let Some(parent) = dest.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            copy_file(&source_dir.join(&relative), &dest)?;
        }
    }

    let asset_path = format!("{}/{}/{}", dir, folder, file_name.to_string_lossy());
    Ok((asset_path, name))
}

fn copy_file(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("{} をコピーできませんでした: {}", from.display(), e))
}

fn to_asset_path(relative: &Path) -> String {
    relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

// .gltf の JSON から "uri": "..." の値を取り出す (埋め込みの data: URI は除く)
pub(super) fn referenced_uris(text: &str) -> Vec<String> {
    let mut uris = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("\"uri\"") {
        rest = &rest[i + 5..];
        let Some(value) = rest
            .trim_start()
            .strip_prefix(':')
            .map(str::trim_start)
            .and_then(|s| s.strip_prefix('"'))
        else {
            continue;
        };
        let Some(end) = value.find('"') else {
            break;
        };
        let uri = &value[..end];
        if !uri.starts_with("data:") {
            uris.push(percent_decode(uri));
        }
    }
    uris
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
