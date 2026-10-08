// 開いているシーン (.peek) の中のファイル (ワールド・モデル) を、メモリ上に置いて Bevy から読み込ませる。
// 読み込むときのパスは peek://<番号>/<シーンの中の相対パス>。
// シーンを開き直すたびに番号を変え、同じ相対パスでも前に開いたシーンの読み込み済みデータと混ざらないようにする
use bevy::{
    asset::io::{
        AssetSourceBuilder,
        memory::{Dir, MemoryAssetReader},
    },
    prelude::*,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// アセットの読み込み元の名前 (peek://...)
const SOURCE: &str = "peek";

#[derive(Resource)]
pub struct PackageStore {
    dir: Dir,
    generation: u64,
    // シーンの中の相対パス (区切りは /) → 中身。保存のときはここから書く
    files: BTreeMap<String, Vec<u8>>,
}

impl PackageStore {
    // 読み込み元を登録する。DefaultPlugins (AssetPlugin) より前に呼ぶ
    pub fn register(app: &mut App) -> Self {
        let dir = Dir::new(PathBuf::new());
        let reader_dir = dir.clone();
        app.register_asset_source(
            SOURCE,
            AssetSourceBuilder::default().with_reader(move || {
                Box::new(MemoryAssetReader {
                    root: reader_dir.clone(),
                })
            }),
        );
        Self {
            dir,
            generation: 0,
            files: BTreeMap::new(),
        }
    }

    // シーンの中の相対パスを、Bevy で読み込むときのパスにする
    pub fn asset_path(&self, rel: &str) -> String {
        format!("{}://{}/{}", SOURCE, self.generation, rel)
    }

    pub fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }

    // 別のシーンの中身に入れ替える。番号を変えるので、読み込み直しになる
    pub fn replace(&mut self, files: BTreeMap<String, Vec<u8>>) {
        // 前のシーンのファイルはもう読まないので、メモリから外す
        for rel in self.files.keys() {
            self.dir.remove_asset(&self.memory_path(rel));
        }
        self.generation += 1;
        self.files = files;
        for (rel, bytes) in &self.files {
            self.dir.insert_asset(&self.memory_path(rel), bytes.clone());
        }
    }

    // ファイルを足す (同じパスなら置き換える)
    pub fn insert(&mut self, rel: String, bytes: Vec<u8>) {
        self.dir.insert_asset(&self.memory_path(&rel), bytes.clone());
        self.files.insert(rel, bytes);
    }

    // dir/<名前> の形で、まだ使われていないフォルダ名を返す (重なれば _2, _3 … を付ける)
    pub fn unique_folder(&self, dir: &str, name: &str) -> String {
        let used = |folder: &str| {
            let prefix = format!("{}/", folder);
            self.files.keys().any(|k| k.starts_with(&prefix))
        };
        let mut folder = format!("{}/{}", dir, name);
        let mut n = 2;
        while used(&folder) {
            folder = format!("{}/{}_{}", dir, name, n);
            n += 1;
        }
        folder
    }

    fn memory_path(&self, rel: &str) -> PathBuf {
        Path::new(&self.generation.to_string()).join(rel)
    }
}
