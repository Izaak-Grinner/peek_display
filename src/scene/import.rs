// モデル・ワールドの取り込み。ファイルを読み込み、開いているシーン (.peek) の中に入れる
// (assets フォルダにはコピーしない。シーンを保存すると .peek に入る)
use bevy::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use crate::config::DisplayInfo;
use crate::scene::editor::{EditState, SaveStatus, Selection};
use crate::scene::layout::{FLOOR_Y, ObjectKind, Placement, SceneLayout};
use crate::scene::library::{self, SceneLibrary};
use crate::scene::package::{PackageFiles, is_inside};
use crate::scene::package_store::PackageStore;

// シーンの中で、追加したモデルとワールドを置くフォルダ
const MODEL_DIR: &str = "models";
const WORLD_DIR: &str = "world";

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

// 読み込んだモデルのファイル。files のパスは、本体のファイルがあるフォルダからの相対パス
pub struct ModelFiles {
    pub main: String,
    pub files: Vec<(String, Vec<u8>)>,
}

impl ModelFiles {
    // folder の下に置いたときのファイル一覧と、本体のパス
    pub fn placed_in(self, folder: &str) -> (String, PackageFiles) {
        let main = format!("{}/{}", folder, self.main);
        let files = self
            .files
            .into_iter()
            .map(|(rel, bytes)| (format!("{}/{}", folder, rel), bytes))
            .collect();
        (main, files)
    }
}

enum ImportResult {
    Cancelled,
    Imported {
        kind: ImportKind,
        source: PathBuf,
        model: ModelFiles,
        name: String,
    },
    Failed(String),
}

// ファイル選択ダイアログと読み込みは別スレッドで行い、結果をチャネルで受け取る
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
    mut store: ResMut<PackageStore>,
    display: Res<DisplayInfo>,
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
            source,
            model,
            name,
        } => {
            // ワールドだけの新しいシーンとして保存して開く。のぞき窓はワールドの peek_window に合わせる
            let name = library::unique_name(&name);
            let (world_path, files) = model.placed_in(WORLD_DIR);
            let mut world = SceneLayout::with_world(&world_path, &source.to_string_lossy());
            world.reference = Some(display.size);
            store.replace(files);
            match library.save_as(&name, &world, &store) {
                Ok(()) => {
                    layout.replace_with(world);
                    state.scene_switched(&layout);
                    info!("{} をワールドとして取り込み、シーン「{}」を作りました", source.display(), name);
                }
                Err(e) => {
                    warn!("{}", e);
                    state.save_status = Some(SaveStatus::Failed(e));
                }
            }
        }
        ImportResult::Imported {
            kind: ImportKind::Model,
            source,
            model,
            name,
        } => {
            // シーンの中の models/<名前>/ に入れる
            let folder = store.unique_folder(MODEL_DIR, &name);
            let (path, files) = model.placed_in(&folder);
            for (rel, bytes) in files {
                store.insert(rel, bytes);
            }
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
            let id = layout.add_object(&name, ObjectKind::Model { path }, placement);
            if let Some(object) = layout.object_mut(id) {
                object.auto_fit = true;
            }
            state.selection = Some(Selection::Object(id));
            state.import_error = None;
            info!("{} をインポートしました", source.display());
        }
        ImportResult::Failed(e) => {
            warn!("インポートに失敗しました: {}", e);
            state.import_error = Some(e);
        }
    }
}

fn run_import(kind: ImportKind) -> ImportResult {
    let title = match kind {
        ImportKind::Model => "インポートするモデルを選択",
        ImportKind::World => "インポートするワールドを選択",
    };
    let Some(source) = rfd::FileDialog::new()
        .set_title(title)
        .add_filter("glTF (.glb / .gltf)", &["glb", "gltf"])
        .pick_file()
    else {
        return ImportResult::Cancelled;
    };
    let name = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "model".to_string());
    match read_model_files(&source) {
        Ok(model) => ImportResult::Imported { kind, source, model, name },
        Err(e) => ImportResult::Failed(e),
    }
}

// モデルのファイルを読み込む。.gltf は .bin やテクスチャを別ファイルで参照しているので、それも読む
pub fn read_model_files(source: &Path) -> Result<ModelFiles, String> {
    let main = source
        .file_name()
        .ok_or_else(|| "ファイル名を取得できませんでした".to_string())?
        .to_string_lossy()
        .into_owned();
    let read = |path: &Path| std::fs::read(path).map_err(|e| format!("{} を読めませんでした: {}", path.display(), e));
    let bytes = read(source)?;
    let mut files = Vec::new();

    let is_gltf = source.extension().is_some_and(|e| e.eq_ignore_ascii_case("gltf"));
    if is_gltf {
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let source_dir = source.parent().unwrap_or(Path::new(""));
        for uri in referenced_uris(&text) {
            if !is_inside(&uri) {
                warn!("フォルダの外を参照しているため読み込みません: {}", uri);
                continue;
            }
            let uri = uri.replace('\\', "/");
            files.push((uri.clone(), read(&source_dir.join(&uri))?));
        }
    }
    files.insert(0, (main.clone(), bytes));
    Ok(ModelFiles { main, files })
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
