# 01 main.rs　起動処理

**一言で**: アプリの入口。必要なデータを用意し、全機能（プラグイン）を登録して動かす。
**場所**: `src/main.rs`（56行）

---

## このファイルの役割

`main()` はプログラムが始まると最初に呼ばれる関数です。やっていることは次の3つだけです。

1. 最初に必要なデータ（設定・シーン）を読み込む
2. 全機能（プラグイン）を登録する
3. `app.run()` でアプリを動かし始める（ここから先は Bevy が毎フレーム各システムを呼ぶ）

---

## コードの流れ

### ① モジュールの宣言（1〜8行目）

```rust
mod app_mode;
mod camera;
mod config;
mod scene;
mod tracking;
mod ui;
```

「`src/` の中にこのファイル・フォルダがある」と宣言しています。ここに書かれていないファイルは使われません。

### ② シーンの読み込み（13〜15行目）

```rust
let mut store = scene::package_store::PackageStore::register(&mut app);
let (library, layout, files) = scene::library::SceneLibrary::open();
store.replace(files);
```

| 行 | 何をしているか | 詳しくは |
|---|---|---|
| `PackageStore::register` | 「`scene://...` というパスでモデルを読めるようにする」仕組みを Bevy に登録する | [13 package_store](13_package_store.md) |
| `SceneLibrary::open()` | `scenes/current.txt` に書かれたシーン（無ければ最初のシーン）を開く。シーンが1つも無ければ新しく作る | [15 library](15_library.md) |
| `store.replace(files)` | シーンに入っていたモデルファイルの中身を、読み込めるようにメモリに置く | [13 package_store](13_package_store.md) |

> **なぜ DefaultPlugins より前なのか**: コメントにある通り、モデルの読み込み元（`scene://`）は、
> Bevy の標準機能（`DefaultPlugins` の中の AssetPlugin）が始まる前に登録しないと使えないためです。

### ③ リソースの登録（16〜19行目）

```rust
app.insert_resource(config::AppConfig::load())   // config.ini を読む
    .insert_resource(store)                      // モデルファイルの置き場
    .insert_resource(library)                    // 今開いているシーンの名前
    .insert_resource(layout)                     // シーンの中身（配置）
```

ここで入れたものが、ほかのファイルで `Res<AppConfig>` などとして使われます。

### ④ プラグインの登録（20〜36行目）

```rust
.add_plugins((DefaultPlugins, MeshPickingPlugin))
```

- `DefaultPlugins` … Bevy の標準機能一式（ウィンドウ、描画、入力、ファイル読み込みなど）
- `MeshPickingPlugin` … 3Dのモノをマウスでクリックできるようにする（編集モードの選択に使う）

続いて、このアプリ独自のプラグインを登録しています。

| プラグイン | ファイル | 役割 |
|---|---|---|
| `AppModePlugin` | app_mode.rs | モードの切り替え |
| `DisplayPlugin` | config/display.rs | 画面の実寸の計算 |
| `UiPlugin` | ui/mod.rs | 画面のUI |
| `ScenePlugin` | scene/scene.rs | シーンを3D空間に反映 |
| `EditorPlugin` | scene/editor.rs | 編集操作 |
| `ImportPlugin` | scene/import.rs | モデルの取り込み |
| `PackagePlugin` | scene/package.rs | .peek のエクスポート・インポート |
| `ModelPlugin` | scene/model.rs | モデルの中身の調査 |
| `HighlightPlugin` | scene/highlight.rs | 選択の輪郭線 |
| `MainCameraPlugin` | camera/main_camera.rs | 本番用カメラ |
| `CameraPreviewPlugin` | camera/camera_preview.rs | 編集モードの小窓 |
| `SceneViewPlugin` | camera/scene_view.rs | 外から見るカメラ |
| `OffAxisPlugin` | camera/off_axis_projection.rs | 窓の見え方の計算 |
| `TrackingPlugin` | tracking/tracking.rs | OpenTrack と頭の位置 |

### ⑤ デバッグ用スクリーンショット（37〜44行目、46〜56行目）

```rust
if let Ok(path) = std::env::var("PEEK_SHOT") { ... }
```

環境変数 `PEEK_SHOT` が設定されているときだけ動く、開発用の機能です。

- 起動から8秒後に、画面を `PEEK_SHOT` のパスに PNG で保存する
- 16秒後にアプリを終了する

`done: Local<u8>` は「0: まだ撮っていない / 1: 撮った」を覚えておく変数です。
人の手を使わずに画面を確認したいとき（AIに作業させるときなど）に使われていたと思われます。

---

## 変更するときは

| やりたいこと | どうする |
|---|---|
| 新しい機能（プラグイン）を足す | 新しいファイルに `Plugin` を作り、`add_plugins((...))` の中に追加する |
| 起動時に読み込むものを変える | ②③ のあたり |
