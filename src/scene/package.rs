// シーンファイル (.peek)。中身は zip で、scene.ini と、シーンが使うワールド・モデルのファイルを入れる。
// このファイル1つを渡せば、ほかのPCでも同じシーンが開ける。
// ファイルはシーンの中の相対パスで入れる (例: world/room.glb、models/izayoi/izayoi_02.glb)
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use crate::scene::editor::EditState;
use crate::scene::import::referenced_uris;
use crate::scene::layout::{ObjectKind, SceneLayout};
use crate::scene::library::{self, SceneLibrary};
use crate::scene::package_store::PackageStore;

pub const PEEK_EXTENSION: &str = "peek";
// .peek の中のシーンの設定
const SCENE_ENTRY: &str = "scene.ini";

// シーンの中の相対パス (区切りは /) → 中身
pub type PackageFiles = BTreeMap<String, Vec<u8>>;

// ---------------------------------------------------------------- 読み書き

pub fn read_peek(path: &Path) -> Result<(SceneLayout, PackageFiles), String> {
    let file = std::fs::File::open(path)
        .map_err(|e| format!("{} を開けませんでした: {}", path.display(), e))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| format!("{} はシーンファイルとして読めませんでした: {}", path.display(), e))?;

    let mut scene_text = None;
    let mut files = PackageFiles::new();
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| format!("シーンファイルを読めませんでした: {}", e))?;
        if entry.is_dir() {
            continue;
        }
        // 外 (../ や絶対パス) を指すものは読まない
        let Some(relative) = entry.enclosed_name() else {
            warn!("不正なパスのため読みません: {}", entry.name());
            continue;
        };
        let rel = to_rel_path(&relative);
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{} を読めませんでした: {}", rel, e))?;
        if rel == SCENE_ENTRY {
            scene_text = Some(String::from_utf8_lossy(&bytes).into_owned());
        } else {
            files.insert(rel, bytes);
        }
    }
    let scene_text = scene_text.ok_or_else(|| format!("{} が入っていません。このアプリのシーンファイル (.peek) を選んでください", SCENE_ENTRY))?;
    Ok((SceneLayout::parse(&scene_text), files))
}

// シーンが参照しているファイルだけを書く。一時ファイルに書いてから置き換え、途中で失敗しても元のファイルを壊さない
pub fn write_peek(path: &Path, layout: &SceneLayout, files: &PackageFiles) -> Result<(), String> {
    let temp = path.with_extension(format!("{}.tmp", PEEK_EXTENSION));
    let result = write_zip(&temp, layout, files).and_then(|_| {
        std::fs::rename(&temp, path).map_err(|e| format!("{} に書き込めませんでした: {}", path.display(), e))
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn write_zip(path: &Path, layout: &SceneLayout, files: &PackageFiles) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = std::fs::File::create(path)
        .map_err(|e| format!("{} を作成できませんでした: {}", path.display(), e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let zip_err = |e: zip::result::ZipError| format!("シーンファイルを書き込めませんでした: {}", e);
    let io_err = |e: std::io::Error| format!("シーンファイルを書き込めませんでした: {}", e);

    zip.start_file(SCENE_ENTRY, options).map_err(zip_err)?;
    zip.write_all(layout.to_ini().as_bytes()).map_err(io_err)?;

    let mut entries = BTreeSet::new();
    for path in used_files(layout) {
        entries.extend(model_files(files, path)?);
    }
    for entry in entries {
        let bytes = files
            .get(&entry)
            .ok_or_else(|| format!("{} がシーンの中にありません", entry))?;
        zip.start_file(entry.as_str(), options).map_err(zip_err)?;
        zip.write_all(bytes).map_err(io_err)?;
    }
    zip.finish().map_err(zip_err)?;
    Ok(())
}

// シーンが使うワールドとモデルのパス
fn used_files(layout: &SceneLayout) -> impl Iterator<Item = &String> {
    layout.world.iter().chain(layout.objects.iter().filter_map(|o| match &o.kind {
        ObjectKind::Model { path } => Some(path),
        ObjectKind::Cube { .. } => None,
    }))
}

// モデルが使うファイルの一覧。.gltf は .bin やテクスチャを別ファイルで参照しているので、それも含める
fn model_files(files: &PackageFiles, path: &str) -> Result<Vec<String>, String> {
    let mut list = vec![path.to_string()];
    if !path.to_ascii_lowercase().ends_with(".gltf") {
        return Ok(list);
    }
    let bytes = files
        .get(path)
        .ok_or_else(|| format!("{} がシーンの中にありません", path))?;
    let text = String::from_utf8_lossy(bytes);
    let dir = path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    for uri in referenced_uris(&text) {
        if !is_inside(&uri) {
            warn!("フォルダの外を参照しているため含めません: {}", uri);
            continue;
        }
        let uri = uri.replace('\\', "/");
        list.push(if dir.is_empty() { uri } else { format!("{}/{}", dir, uri) });
    }
    Ok(list)
}

// 相対パスが、そのフォルダの中だけを指しているか
pub(super) fn is_inside(path: &str) -> bool {
    Path::new(path)
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

fn to_rel_path(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

// ---------------------------------------------------------------- エクスポート・インポート

enum PackageResult {
    Cancelled,
    Exported(String),
    // scenes フォルダにコピーしたシーンの名前
    Imported(String),
    Failed(String),
}

// 最後に行ったエクスポート・インポートの結果 (パネルに表示する)
#[derive(Clone, Debug)]
pub enum PackageMessage {
    Exported(String),
    Imported(String),
    Failed(String),
}

// ファイル選択ダイアログとファイルの読み書きは別スレッドで行い、結果をチャネルで受け取る
#[derive(Resource, Default)]
pub struct PackageState {
    receiver: Option<Mutex<Receiver<PackageResult>>>,
    pub message: Option<PackageMessage>,
}

impl PackageState {
    pub fn busy(&self) -> bool {
        self.receiver.is_some()
    }

    // 今のシーン (保存していない変更も含む) を、選んだ場所に書き出す
    pub fn start_export(&mut self, layout: &SceneLayout, files: &PackageFiles, name: &str) {
        let layout = layout.clone();
        let files = files.clone();
        let file_name = format!("{}.{}", name, PEEK_EXTENSION);
        self.start(move || run_export(&layout, &files, &file_name));
    }

    // 選んだ .peek を scenes フォルダにコピーする (開くのは poll_package)
    pub fn start_import(&mut self) {
        self.start(run_import);
    }

    fn start(&mut self, task: impl FnOnce() -> PackageResult + Send + 'static) {
        if self.busy() {
            return;
        }
        self.message = None;
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            let _ = sender.send(task());
        });
        self.receiver = Some(Mutex::new(receiver));
    }
}

pub struct PackagePlugin;

impl Plugin for PackagePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PackageState>()
            .add_systems(Update, poll_package);
    }
}

fn poll_package(
    mut package: ResMut<PackageState>,
    mut layout: ResMut<SceneLayout>,
    mut state: ResMut<EditState>,
    mut library: ResMut<SceneLibrary>,
    mut store: ResMut<PackageStore>,
) {
    let Some(receiver) = &package.receiver else {
        return;
    };
    let result = match receiver.lock().unwrap().try_recv() {
        Ok(result) => result,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => PackageResult::Failed("処理が中断されました".into()),
    };
    package.receiver = None;

    package.message = match result {
        PackageResult::Cancelled => None,
        PackageResult::Exported(file_name) => {
            info!("シーンを {} にエクスポートしました", file_name);
            Some(PackageMessage::Exported(file_name))
        }
        PackageResult::Imported(name) => match library.open_scene(&name, &mut store) {
            Ok(loaded) => {
                layout.replace_with(loaded);
                state.scene_switched(&layout);
                info!("シーン「{}」をインポートしました", name);
                Some(PackageMessage::Imported(name))
            }
            Err(e) => Some(PackageMessage::Failed(e)),
        },
        PackageResult::Failed(e) => {
            warn!("シーンのエクスポート・インポートに失敗しました: {}", e);
            Some(PackageMessage::Failed(e))
        }
    };
}

fn run_export(layout: &SceneLayout, files: &PackageFiles, file_name: &str) -> PackageResult {
    let Some(dest) = rfd::FileDialog::new()
        .set_title("シーンをエクスポート")
        .add_filter("シーン (.peek)", &[PEEK_EXTENSION])
        .set_file_name(file_name)
        .save_file()
    else {
        return PackageResult::Cancelled;
    };
    match write_peek(&dest, layout, files) {
        Ok(()) => PackageResult::Exported(display_name(&dest)),
        Err(e) => PackageResult::Failed(e),
    }
}

fn run_import() -> PackageResult {
    let Some(source) = rfd::FileDialog::new()
        .set_title("シーンをインポート")
        .add_filter("シーン (.peek)", &[PEEK_EXTENSION])
        .pick_file()
    else {
        return PackageResult::Cancelled;
    };
    // 読めるファイルか先に確かめてから、scenes フォルダにコピーする
    if let Err(e) = read_peek(&source) {
        return PackageResult::Failed(e);
    }
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = library::unique_name(&stem);
    let dest: PathBuf = library::scene_path(&name);
    match std::fs::copy(&source, &dest) {
        Ok(_) => PackageResult::Imported(name),
        Err(e) => PackageResult::Failed(format!("{} をコピーできませんでした: {}", source.display(), e)),
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::layout::Placement;

    // 書き出した .peek を読み直すと、同じシーンと同じファイルに戻る。参照していないファイルは書かない
    #[test]
    fn write_then_read_round_trip() {
        let mut layout = SceneLayout::default();
        layout.world = Some("world/room.gltf".to_string());
        layout.world_source = Some(r"C:\blender\room.gltf".to_string());
        layout.world_override_mut("Cube").hidden = true;
        layout.world_override_mut("Light").placement = Some(Placement {
            position: Vec3::new(0.1, 0.2, 0.3),
            yaw_deg: 30.0,
            scale: 2.0,
            ..default()
        });
        layout.reference = Some(Vec2::new(0.3446, 0.2154));
        layout.light.illuminance = 1234.0;
        layout.light.ambient = 20.0;
        layout.light.pitch_deg = -30.0;
        layout.light.yaw_deg = 45.0;

        let mut files = PackageFiles::new();
        files.insert("world/room.gltf".into(), br#"{"buffers":[{"uri":"room.bin"}]}"#.to_vec());
        files.insert("world/room.bin".into(), vec![1, 2, 3]);
        files.insert("models/izayoi/izayoi_02.glb".into(), vec![4, 5, 6]);
        files.insert("models/unused/unused.glb".into(), vec![7]);

        let dir = std::env::temp_dir().join(format!("peek_display_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scene.peek");
        write_peek(&path, &layout, &files).unwrap();
        let (loaded, loaded_files) = read_peek(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(layout.same_content(&loaded));
        files.remove("models/unused/unused.glb");
        assert_eq!(files, loaded_files);
    }
}
