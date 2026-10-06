// アプリ内に保存しておくシーンの一覧。
// scenes/<名前>.ini に1シーンずつ保存し、最後に開いたシーンの名前を scenes/current.txt に覚えておく
use bevy::prelude::*;
use std::path::PathBuf;

use crate::scene::layout::SceneLayout;

const SCENES_DIR: &str = "scenes";
const CURRENT_FILE: &str = "current.txt";
// 以前の保存先。scenes/ が空なら、これを最初のシーンとして移す
const LEGACY_SCENE_FILE: &str = "scene.ini";
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
    // 起動時に、最後に開いたシーンを読み込む
    pub fn open() -> (Self, SceneLayout) {
        if let Err(e) = std::fs::create_dir_all(SCENES_DIR) {
            warn!("{} フォルダを作成できませんでした: {}", SCENES_DIR, e);
        }
        migrate_legacy_scene();

        let names = list();
        let remembered = std::fs::read_to_string(dir().join(CURRENT_FILE))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|name| names.contains(name));
        let name = remembered.or_else(|| names.first().cloned());

        let Some(name) = name else {
            // シーンが1つも無いときは、初期配置で作る
            let layout = SceneLayout::default();
            let mut library = Self::untitled(FIRST_SCENE_NAME);
            match library.save_as(FIRST_SCENE_NAME, &layout) {
                Ok(()) => info!("シーンが無かったので初期配置で {} を作成しました", FIRST_SCENE_NAME),
                Err(e) => warn!("{}", e),
            }
            return (library, layout);
        };

        match load(&name) {
            Ok(layout) => {
                let library = Self {
                    current: Some(name.clone()),
                    suggested_name: name,
                };
                library.remember_current();
                (library, layout)
            }
            Err(e) => {
                warn!("{}", e);
                (Self::untitled(&name), SceneLayout::default())
            }
        }
    }

    pub fn untitled(suggested_name: &str) -> Self {
        Self {
            current: None,
            suggested_name: suggested_name.to_string(),
        }
    }

    // 今のシーンとして開く
    pub fn open_scene(&mut self, name: &str) -> Result<SceneLayout, String> {
        let layout = load(name)?;
        self.current = Some(name.to_string());
        self.suggested_name = name.to_string();
        self.remember_current();
        Ok(layout)
    }

    // 今のシーンに上書き保存する (名前が無いときは Err)
    pub fn save(&self, layout: &SceneLayout) -> Result<(), String> {
        let name = self
            .current
            .as_deref()
            .ok_or_else(|| "シーンに名前が付いていません".to_string())?;
        write(name, layout)
    }

    // 名前を付けて保存し、そのシーンを今のシーンにする
    pub fn save_as(&mut self, name: &str, layout: &SceneLayout) -> Result<(), String> {
        let name = validate_name(name)?;
        write(&name, layout)?;
        self.current = Some(name.clone());
        self.suggested_name = name;
        self.remember_current();
        Ok(())
    }

    // 保存してあるシーンを削除する (ワールドやモデルのファイルはほかのシーンでも使えるので消さない)。
    // 今のシーンを削除したときは、名前の無いシーンになる
    pub fn delete(&mut self, name: &str) -> Result<(), String> {
        std::fs::remove_file(path(name))
            .map_err(|e| format!("シーン「{}」を削除できませんでした ({})", name, e))?;
        if self.current.as_deref() == Some(name) {
            self.current = None;
        }
        info!("シーン「{}」を削除しました", name);
        Ok(())
    }

    // 最後に開いていたシーン (current.txt) を開き直す。無ければ空のシーンにする
    pub fn reopen_last(&mut self) -> SceneLayout {
        let remembered = std::fs::read_to_string(dir().join(CURRENT_FILE))
            .ok()
            .map(|s| s.trim().to_string());
        if let Some(name) = remembered {
            match self.open_scene(&name) {
                Ok(layout) => return layout,
                Err(e) => warn!("{}", e),
            }
        }
        SceneLayout::empty()
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
            let is_ini = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ini"));
            if !is_ini || !path.is_file() {
                return None;
            }
            Some(path.file_stem()?.to_string_lossy().into_owned())
        })
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names
}

pub fn exists(name: &str) -> bool {
    validate_name(name).is_ok_and(|name| path(&name).is_file())
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

fn path(name: &str) -> PathBuf {
    dir().join(format!("{}.ini", name))
}

fn load(name: &str) -> Result<SceneLayout, String> {
    std::fs::read_to_string(path(name))
        .map(|text| SceneLayout::parse(&text))
        .map_err(|e| format!("シーン「{}」を読めませんでした ({})", name, e))
}

fn write(name: &str, layout: &SceneLayout) -> Result<(), String> {
    std::fs::create_dir_all(dir())
        .and_then(|_| std::fs::write(path(name), layout.to_ini()))
        .map_err(|e| format!("シーン「{}」に書き込めません ({})", name, e))
}

// 以前の scene.ini を scenes/default.ini に移す
fn migrate_legacy_scene() {
    let legacy = PathBuf::from(LEGACY_SCENE_FILE);
    if !legacy.is_file() || !list().is_empty() {
        return;
    }
    let dest = path(FIRST_SCENE_NAME);
    match std::fs::rename(&legacy, &dest) {
        Ok(()) => info!("{} を {} に移しました", LEGACY_SCENE_FILE, dest.display()),
        Err(e) => warn!("{} を {} に移せませんでした: {}", LEGACY_SCENE_FILE, dest.display(), e),
    }
}
