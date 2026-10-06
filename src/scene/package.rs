// 共有用のシーンの書き出し・読み込み。
// シーンファイルと、シーンが使うワールド・モデルのファイルを1つの .zip にまとめる。
// zip の中のファイルは assets フォルダと同じ相対パスで入れる (例: worlds/room/room.glb)
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Component, Path};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use crate::scene::editor::EditState;
use crate::scene::import::{MODEL_DIR, WORLD_DIR, assets_dir, copy_into_assets, referenced_uris};
use crate::scene::layout::{ObjectKind, SceneLayout};
use crate::scene::library::{self, SceneLibrary};

// zip の中のシーンファイル名
const SCENE_ENTRY: &str = "scene.ini";

enum PackageResult {
    Cancelled,
    Exported(String),
    Imported { layout: SceneLayout, file_name: String },
    Failed(String),
}

// 最後に行った書き出し・読み込みの結果 (パネルに表示する)
#[derive(Clone, Debug)]
pub enum PackageMessage {
    Exported(String),
    Imported(String),
    Failed(String),
}

// ファイル選択ダイアログと zip の読み書きは別スレッドで行い、結果をチャネルで受け取る
#[derive(Resource, Default)]
pub struct PackageState {
    receiver: Option<Mutex<Receiver<PackageResult>>>,
    pub message: Option<PackageMessage>,
}

impl PackageState {
    pub fn busy(&self) -> bool {
        self.receiver.is_some()
    }

    // 今のシーン (保存していない変更も含む) を書き出す
    pub fn start_export(&mut self, layout: &SceneLayout) {
        let layout = layout.clone();
        self.start(move || run_export(&layout));
    }

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
            info!("シーンを {} に書き出しました", file_name);
            Some(PackageMessage::Exported(file_name))
        }
        PackageResult::Imported { layout: loaded, file_name } => {
            // zip の名前でシーンとして保存して開く。保存できなければ名前の無いシーンとして開く
            let stem = Path::new(&file_name)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let name = library::unique_name(&stem);
            if let Err(e) = library.save_as(&name, &loaded) {
                warn!("{}", e);
                *library = SceneLibrary::untitled(&name);
            }
            layout.replace_with(loaded);
            state.scene_switched(&layout);
            info!("シーンを {} から読み込みました", file_name);
            Some(PackageMessage::Imported(name))
        }
        PackageResult::Failed(e) => {
            warn!("シーンの書き出し・読み込みに失敗しました: {}", e);
            Some(PackageMessage::Failed(e))
        }
    };
}

// ---------------------------------------------------------------- 書き出し

fn run_export(layout: &SceneLayout) -> PackageResult {
    let Some(dest) = rfd::FileDialog::new()
        .set_title("シーンを書き出す")
        .add_filter("シーン (.zip)", &["zip"])
        .set_file_name("scene.zip")
        .save_file()
    else {
        return PackageResult::Cancelled;
    };
    let file_name = display_name(&dest);
    match write_package(layout, &dest) {
        Ok(()) => PackageResult::Exported(file_name),
        Err(e) => {
            // 書きかけのファイルを残さない
            let _ = std::fs::remove_file(&dest);
            PackageResult::Failed(e)
        }
    }
}

fn write_package(layout: &SceneLayout, dest: &Path) -> Result<(), String> {
    let assets = assets_dir();
    let file = std::fs::File::create(dest)
        .map_err(|e| format!("{} を作成できませんでした: {}", dest.display(), e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let zip_err = |e: zip::result::ZipError| format!("zip を書き込めませんでした: {}", e);

    zip.start_file(SCENE_ENTRY, options).map_err(zip_err)?;
    zip.write_all(layout.to_ini().as_bytes())
        .map_err(|e| format!("zip を書き込めませんでした: {}", e))?;

    let mut written = HashSet::new();
    for path in used_files(layout) {
        for entry in model_files(&assets, path)? {
            if !written.insert(entry.clone()) {
                continue;
            }
            let bytes = std::fs::read(assets.join(&entry))
                .map_err(|e| format!("{} を読めませんでした: {}", entry, e))?;
            zip.start_file(entry.as_str(), options).map_err(zip_err)?;
            zip.write_all(&bytes)
                .map_err(|e| format!("zip を書き込めませんでした: {}", e))?;
        }
    }
    zip.finish().map_err(zip_err)?;
    Ok(())
}

// シーンが使うワールドとモデルのアセットパス
fn used_files(layout: &SceneLayout) -> impl Iterator<Item = &String> {
    layout.world.iter().chain(layout.objects.iter().filter_map(|o| match &o.kind {
        ObjectKind::Model { path } => Some(path),
        ObjectKind::Cube { .. } => None,
    }))
}

// モデルが使うファイルの一覧 (base からの相対パス)。
// .gltf は .bin やテクスチャを別ファイルで参照しているので、それも含める
fn model_files(base: &Path, path: &str) -> Result<Vec<String>, String> {
    let mut files = vec![path.to_string()];
    if !path.to_ascii_lowercase().ends_with(".gltf") {
        return Ok(files);
    }
    let text = std::fs::read_to_string(base.join(path))
        .map_err(|e| format!("{} を読めませんでした: {}", path, e))?;
    let dir = path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    for uri in referenced_uris(&text) {
        if Path::new(&uri)
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            warn!("フォルダの外を参照しているため含めません: {}", uri);
            continue;
        }
        let uri = uri.replace('\\', "/");
        files.push(if dir.is_empty() { uri } else { format!("{}/{}", dir, uri) });
    }
    Ok(files)
}

// ---------------------------------------------------------------- 読み込み

fn run_import() -> PackageResult {
    let Some(source) = rfd::FileDialog::new()
        .set_title("シーンを読み込む")
        .add_filter("シーン (.zip)", &["zip"])
        .pick_file()
    else {
        return PackageResult::Cancelled;
    };

    // 一時フォルダに展開してから、モデルを assets フォルダへコピーする
    let temp = std::env::temp_dir().join(format!(
        "peek_display_scene_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let result = read_package(&source, &temp);
    let _ = std::fs::remove_dir_all(&temp);
    match result {
        Ok(layout) => PackageResult::Imported {
            layout,
            file_name: display_name(&source),
        },
        Err(e) => PackageResult::Failed(e),
    }
}

fn read_package(source: &Path, temp: &Path) -> Result<SceneLayout, String> {
    let file = std::fs::File::open(source)
        .map_err(|e| format!("{} を開けませんでした: {}", source.display(), e))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| format!("zip として読めませんでした: {}", e))?;

    let mut scene_text = None;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| format!("zip を読めませんでした: {}", e))?;
        if entry.is_dir() {
            continue;
        }
        // zip の外 (../ や絶対パス) を指すものは展開しない
        let Some(relative) = entry.enclosed_name() else {
            warn!("不正なパスのため展開しません: {}", entry.name());
            continue;
        };
        if relative == Path::new(SCENE_ENTRY) {
            let mut text = String::new();
            entry
                .read_to_string(&mut text)
                .map_err(|e| format!("{} を読めませんでした: {}", SCENE_ENTRY, e))?;
            scene_text = Some(text);
            continue;
        }
        let dest = temp.join(&relative);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("{} を作成できませんでした: {}", parent.display(), e))?;
        }
        let mut out = std::fs::File::create(&dest)
            .map_err(|e| format!("{} を作成できませんでした: {}", dest.display(), e))?;
        std::io::copy(&mut entry, &mut out)
            .map_err(|e| format!("{} を展開できませんでした: {}", relative.display(), e))?;
    }
    let scene_text = scene_text
        .ok_or_else(|| format!("{} が入っていません。このアプリで書き出したシーンの zip を選んでください", SCENE_ENTRY))?;

    let mut layout = SceneLayout::parse(&scene_text);
    let assets = assets_dir();
    if let Some(world) = &mut layout.world {
        *world = place_model(&assets, temp, world, WORLD_DIR)?;
    }
    // 同じモデルを使うオブジェクトが複数あっても、コピーは1回にする
    let mut resolved: HashMap<String, String> = HashMap::new();
    for object in &mut layout.objects {
        let ObjectKind::Model { path } = &mut object.kind else {
            continue;
        };
        if let Some(new_path) = resolved.get(path.as_str()) {
            *path = new_path.clone();
            continue;
        }
        let new_path = place_model(&assets, temp, path, MODEL_DIR)?;
        resolved.insert(path.clone(), new_path.clone());
        *path = new_path;
    }
    Ok(layout)
}

// 展開したモデル (またはワールド) を assets/<dir>/ に置き、使うアセットパスを返す。
// 同じパスに同じ中身のファイルがすでにあれば、コピーせずにそれを使う
fn place_model(assets: &Path, temp: &Path, path: &str, dir: &str) -> Result<String, String> {
    let packed = temp.join(path);
    let existing = assets.join(path);
    if !packed.is_file() {
        // zip に入っていなくても、このPCに同じモデルがあればそれを使う
        if existing.is_file() {
            return Ok(path.to_string());
        }
        return Err(format!("モデル {} が zip に入っていません", path));
    }
    if same_files(temp, assets, path) {
        return Ok(path.to_string());
    }
    copy_into_assets(&packed, dir).map(|(asset_path, _)| asset_path)
}

// モデルとその参照ファイルが、a と b で同じ中身か
fn same_files(a: &Path, b: &Path, path: &str) -> bool {
    let Ok(files) = model_files(a, path) else {
        return false;
    };
    files.iter().all(|f| match (std::fs::read(a.join(f)), std::fs::read(b.join(f))) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    })
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

    // 書き出した zip を読み込むと、同じシーンに戻る (モデルは assets にあるものを使い、コピーしない)
    #[test]
    fn export_then_import_round_trip() {
        let mut layout = SceneLayout::default();
        // ワールドとノードの変更も含める (ワールドには assets にある glb を使う)
        layout.world = Some("models/test/ke1.glb".to_string());
        layout.world_override_mut("Cube").hidden = true;
        layout.world_override_mut("Light").placement = Some(Placement {
            position: Vec3::new(0.1, 0.2, 0.3),
            yaw_deg: 30.0,
            scale: 2.0,
            ..default()
        });
        let dir = std::env::temp_dir().join(format!("peek_display_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("scene.zip");
        write_package(&layout, &zip_path).unwrap();

        let loaded = read_package(&zip_path, &dir.join("extract")).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(layout.same_content(&loaded));
    }
}
