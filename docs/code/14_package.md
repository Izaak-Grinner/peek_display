# 14 scene/package.rs　.peek ファイルの読み書き

**一言で**: シーンファイル（.peek = zip）を読む・書く。シーンのエクスポート・インポートも担当する。
**場所**: `src/scene/package.rs`（338行）
**関係するファイル**: [10 layout](10_layout.md)（scene.ini の中身）、[15 library](15_library.md)（scenes フォルダへの保存）、[13 package_store](13_package_store.md)

---

## .peek ファイルの中身

```
default.peek (zip)
├ scene.ini                     … SceneLayout を文字にしたもの
├ models/izayoi/izayoi_02.glb   … モデル本体
├ models/room/room.gltf         … モデル本体（JSON）
└ models/room/room.bin          … room.gltf が参照する頂点データ
```

---

## 読み書き

### `read_scene_file(path)`（26〜59行目）

```
1. ファイルを zip として開く
2. 中のファイルを1つずつ読む
   ├ フォルダ → 飛ばす
   ├ 外を指すパス（../ や絶対パス）→ 警告して飛ばす（安全のため）
   ├ scene.ini → 文字列として取っておく
   └ それ以外 → files（パス → 中身）に入れる
3. scene.ini が無ければエラー（「このアプリのシーンファイルを選んでください」）
4. (SceneLayout::parse(scene.ini), files) を返す
```

> `enclosed_name()` で外を指すパスを弾いているのは、悪意のある zip が `../../Windows/...` のような場所に書き込むのを防ぐ定番の対策です（このアプリは展開しないので、念のため）。

### `write_scene_file(path, layout, files)`（61〜71行目）

```rust
let temp = path.with_extension("peek.tmp");
write_zip(&temp, ...)          // まず一時ファイルに書く
  .and_then(|_| rename(&temp, path))  // 成功したら置き換える
// 失敗したら一時ファイルを消す
```

**途中で失敗しても元のファイルが壊れない** ための書き方です（書いている途中で落ちると、半分だけのファイルが残るのを防ぐ）。

### `write_zip`（73〜101行目）

1. `scene.ini` を書く（`layout.to_ini()`）
2. シーンで **使っているモデルのファイルだけ** を書く

`used_files`（104〜106行目）で使っているパスを集め、`model_files`（109〜128行目）で各モデルが必要とするファイルを足します。
`BTreeSet` に入れるので、同じファイルを複数の部品が使っていても1回だけ書かれます。

> **使っていないファイルは書かれない**: モデルを削除して保存すると、そのファイルも .peek から消えます。

### `model_files(files, path)`（109〜128行目）

- .glb → 1ファイルで完結しているので、そのまま
- .gltf → JSON の中の `"uri"` を読んで、参照している .bin やテクスチャ画像も含める（[17 import](17_import.md) の `referenced_uris`）

### `is_inside(path)`（131〜135行目）

相対パスが、そのフォルダの中だけを指しているか（`..` などを含まないか）を確かめます。

---

## エクスポート・インポート

「ファイル → エクスポート/インポート → シーン (.peek)」の処理です。

### 別スレッドで動かす理由

ファイル選択ダイアログ（`rfd::FileDialog`）は、ユーザーが選ぶまで処理が止まります。
メインの処理で開くと、その間アプリの画面が固まってしまうので、**別スレッドで開き、結果をチャネル（スレッド間の受け渡し口）で受け取ります**。

```
メイン                          別スレッド
start_export / start_import
  └ thread::spawn ─────────→   ダイアログを開く → 書き出す / コピーする
                                 └ sender.send(結果)
poll_package（毎フレーム）
  └ receiver.try_recv() ←────── 結果
     ├ まだ → 何もしない
     └ 届いた → 結果に応じて処理
```

### 型

| 型 | 意味 |
|---|---|
| `PackageResult`（146〜152行目） | 別スレッドからの結果（キャンセル／エクスポートした／インポートした／失敗） |
| `PackageMessage`（155〜160行目） | パネルに表示する結果 |
| `PackageState`（163〜198行目） | 受け取り口と、最後の結果（リソース） |

`busy()` は処理中か。処理中はメニューのボタンが押せなくなります。

### `start_export(layout, files, name)`

今のシーン（**保存していない変更も含む**）をコピーして、別スレッドで `run_export` を動かします。

### `run_export`（248〜261行目）

保存先を選ぶダイアログを出し、選ばれた場所に `write_scene_file` で書きます。

### `run_import`（263〜285行目）

```
1. .peek を選ぶダイアログを出す
2. 読めるファイルか確かめる（壊れていたら失敗）
3. 名前が重ならないようにする（unique_name。同じ名前があれば _2 を付ける）
4. scenes フォルダにコピーする
```

### `poll_package`（209〜246行目）

毎フレーム結果を確認し、インポートだった場合はそのシーンを開きます。

---

## テスト（288〜338行目）

`write_then_read_round_trip`: シーンを書いて読み直すと同じ内容に戻ること、使っていないファイル（`unused.glb`）は書かれないことを確認しています。
