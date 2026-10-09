// アプリ内に保存しておくシーンの一覧。
// scenes/<名前>.peek に1シーンずつ保存し、最後に開いたシーンの名前を scenes/current.txt に覚えておく
use bevy::prelude::*;
use std::path::PathBuf;

use crate::scene::default_room;
use crate::scene::layout::SceneLayout;
use crate::scene::package::{self, SCENE_EXTENSION, PackageFiles};
use crate::scene::package_store::PackageStore;

const SCENES_DIR: &str = "scenes";
const CURRENT_FILE: &str = "current.txt";
// 最初に作るシーンの名前
const FIRST_SCENE_NAME: &str = "default";
// ファイル名に使えない文字
const INVALID_CHARS: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];
const MAX_NAME_CHARS: usize = 64;

#[derive(Resource, Clone, Debug)]
pub struct SceneLibrary {
    // 今編集しているシーンの名前。None は、まだ名前を付けて保存していないシーン
    pub current: Option<String>,
    // 名前を付けて保存するときの初期値 (インポートした zip の名前など)
    pub suggested_name: String,
}

impl SceneLibrary {
    // 起動時に、最後に開いたシーンを読み込む (シーンの中のファイルも返す)
    pub fn open() -> (Self, SceneLayout, PackageFiles) {
        if let Err(e) = std::fs::create_dir_all(SCENES_DIR) {
            warn!("{} フォルダを作成できませんでした: {}", SCENES_DIR, e);
        }

        let names = list();
        let remembered = std::fs::read_to_string(dir().join(CURRENT_FILE))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|name| names.contains(name));
        let name = remembered.or_else(|| names.first().cloned());

        let Some(name) = name else {
            // シーンが1つも無いときは、新しいシーンを作る
            let (layout, files) = default_room::new_scene();
            let mut library = Self::untitled(FIRST_SCENE_NAME);
            match write(FIRST_SCENE_NAME, &layout, &files) {
                Ok(()) => {
                    library.current = Some(FIRST_SCENE_NAME.to_string());
                    library.remember_current();
                    info!("シーンが無かったので新しいシーン {} を作成しました", FIRST_SCENE_NAME);
                }
                Err(e) => warn!("{}", e),
            }
            return (library, layout, files);
        };

        match load(&name) {
            Ok((layout, files)) => {
                let library = Self {
                    current: Some(name.clone()),
                    suggested_name: name,
                };
                library.remember_current();
                (library, layout, files)
            }
            Err(e) => {
                warn!("{}", e);
                (Self::untitled(&name), SceneLayout::empty(), PackageFiles::new())
            }
        }
    }

    pub fn untitled(suggested_name: &str) -> Self {
        Self {
            current: None,
            suggested_name: suggested_name.to_string(),
        }
    }

    // 今のシーンとして開く。シーンの中のファイルは store に入れる
    pub fn open_scene(&mut self, name: &str, store: &mut PackageStore) -> Result<SceneLayout, String> {
        let (layout, files) = load(name)?;
        store.replace(files);
        self.current = Some(name.to_string());
        self.suggested_name = name.to_string();
        self.remember_current();
        Ok(layout)
    }

    // 今のシーンに上書き保存する (名前が無いときは Err)
    pub fn save(&self, layout: &SceneLayout, store: &PackageStore) -> Result<(), String> {
        let name = self
            .current
            .as_deref()
            .ok_or_else(|| "シーンに名前が付いていません".to_string())?;
        write(name, layout, store.files())
    }

    // 名前を付けて保存し、そのシーンを今のシーンにする
    pub fn save_as(&mut self, name: &str, layout: &SceneLayout, store: &PackageStore) -> Result<(), String> {
        let name = validate_name(name)?;
        write(&name, layout, store.files())?;
        self.current = Some(name.clone());
        self.suggested_name = name;
        self.remember_current();
        Ok(())
    }

    // 保存してあるシーンを削除する。
    // 今のシーンを削除したときは、名前の無いシーンになる
    pub fn delete(&mut self, name: &str) -> Result<(), String> {
        std::fs::remove_file(scene_path(name))
            .map_err(|e| format!("シーン「{}」を削除できませんでした ({})", name, e))?;
        if self.current.as_deref() == Some(name) {
            self.current = None;
        }
        info!("シーン「{}」を削除しました", name);
        Ok(())
    }

    // 最後に開いていたシーン (current.txt) を開き直す。無ければ新しいシーンにする
    pub fn reopen_last(&mut self, store: &mut PackageStore) -> SceneLayout {
        let remembered = std::fs::read_to_string(dir().join(CURRENT_FILE))
            .ok()
            .map(|s| s.trim().to_string());
        if let Some(name) = remembered {
            match self.open_scene(&name, store) {
                Ok(layout) => return layout,
                Err(e) => warn!("{}", e),
            }
        }
        let (layout, files) = default_room::new_scene();
        store.replace(files);
        layout
    }

    fn remember_current(&self) {
        let Some(name) = &self.current else {
            return;
        };
        if let Err(e) = std::fs::write(dir().join(CURRENT_FILE), name) {
            warn!("{} に書き込めませんでした: {}", CURRENT_FILE, e);
        }
    }
}

pub fn dir() -> PathBuf {
    PathBuf::from(SCENES_DIR)
}

// 保存してあるシーンの名前 (名前順)
pub fn list() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir()) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let is_scene = path.extension().is_some_and(|e| e.eq_ignore_ascii_case(SCENE_EXTENSION));
            if !is_scene || !path.is_file() {
                return None;
            }
            Some(path.file_stem()?.to_string_lossy().into_owned())
        })
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names
}

pub fn exists(name: &str) -> bool {
    validate_name(name).is_ok_and(|name| scene_path(&name).is_file())
}

// 名前を確かめ、前後の空白を除いたものを返す
pub fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("名前を入力してください".into());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!("名前は{}文字までにしてください", MAX_NAME_CHARS));
    }
    if let Some(c) = name.chars().find(|c| INVALID_CHARS.contains(c) || c.is_control()) {
        return Err(format!("名前に「{}」は使えません", c));
    }
    if name.ends_with('.') || name.starts_with('.') {
        return Err("名前の最初と最後に「.」は使えません".into());
    }
    Ok(name.to_string())
}

// 保存してあるシーンと重ならない名前にする (使えない文字は _ にし、重なれば _2, _3 … を付ける)
pub fn unique_name(base: &str) -> String {
    let cleaned: String = base
        .trim()
        .chars()
        .map(|c| if INVALID_CHARS.contains(&c) || c.is_control() { '_' } else { c })
        .take(MAX_NAME_CHARS - 4)
        .collect();
    let cleaned = cleaned.trim_matches('.').trim().to_string();
    let base = if cleaned.is_empty() { "scene".to_string() } else { cleaned };
    let mut name = base.clone();
    let mut n = 2;
    while exists(&name) {
        name = format!("{}_{}", base, n);
        n += 1;
    }
    name
}

// シーンファイルの場所
pub fn scene_path(name: &str) -> PathBuf {
    dir().join(format!("{}.{}", name, SCENE_EXTENSION))
}

fn load(name: &str) -> Result<(SceneLayout, PackageFiles), String> {
    package::read_scene_file(&scene_path(name)).map_err(|e| format!("シーン「{}」を読めませんでした ({})", name, e))
}

fn write(name: &str, layout: &SceneLayout, files: &PackageFiles) -> Result<(), String> {
    std::fs::create_dir_all(dir())
        .map_err(|e| e.to_string())
        .and_then(|_| package::write_scene_file(&scene_path(name), layout, files))
        .map_err(|e| format!("シーン「{}」に書き込めません ({})", name, e))
}
