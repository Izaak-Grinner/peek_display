# 17 scene/import.rs　モデル・ワールドの取り込み

**一言で**: 「ファイル → インポート → モデル / ワールド」で選んだ .glb / .gltf を読み込み、今のシーンに足す。
**場所**: `src/scene/import.rs`（240行）
**関係するファイル**: [11 scene](11_scene.md)（読み込み後の置き方）、[13 package_store](13_package_store.md)（ファイルの置き場）、[23 edit_panel](23_edit_panel.md)（メニュー）

---

## 2種類の取り込み

`ImportKind`（17〜23行目）

| 種類 | 何に使う | 置き方 |
|---|---|---|
| `Model` | ひとかたまりのモデル（ねこ、キャラクター） | スクリーンの正面 40cm 奥、床の上。読み込み後に約30cmに縮める |
| `World` | Blender で作った部屋・背景一式 | Blender の位置のまま。読み込み後に部品ごとに分ける |

---

## 流れ

```
メニューで選ぶ
  └ ImportState::start(kind)
       └ 別スレッドで run_import
            ├ ファイル選択ダイアログ
            └ read_model_files … ファイルを読む（.gltf なら参照している .bin なども）
                 └ 結果をチャネルで送る
poll_import（毎フレーム）
  └ 結果を受け取ったら
       ├ ファイルをシーンの models/<名前>/ に入れる（PackageStore）
       ├ SceneLayout にオブジェクトを足す（on_load = Fit か Split）
       └ モデルなら選択状態にする
            ↓
[11 scene.rs] がモデルを表示 → 読み込みが終わったら place_imported が置き方を決める
```

ファイル選択中にアプリが固まらないよう、別スレッドで動かしています（[14 package](14_package.md) と同じ作り）。

> **ファイルはシーンの中に入るだけ**: assets フォルダにはコピーされません。保存すると .peek に入ります。保存しないで閉じると取り込んだモデルは消えます。

---

## 関数ごとの説明

### `ModelFiles`（29〜45行目）

読み込んだファイルの一覧（本体のファイル名と、パス→中身）。
`placed_in(folder)` で、パスの頭に `models/<名前>/` を付けます。

### `poll_import`（90〜146行目）

結果を受け取ったときの処理（117〜131行目）:

**モデル**の置き場所:
```rust
let facing = Quat::from_rotation_y(screen.yaw_deg.to_radians());   // スクリーンの方角だけ使う
let mut position = screen.position + facing * Vec3::new(0.0, 0.0, -IMPORT_DISTANCE);  // 正面の奥 40cm
position.y = FLOOR_Y;                                                 // 高さは床
```
スクリーンが傾いていても、モデルはまっすぐ立てます（方角 yaw だけ使い、傾き pitch は使わない）。

**ワールド**: 位置は原点（Blender のまま）。部品に分けると id が変わるので、選択状態にはしません。

### `run_import(kind)`（148〜168行目）

ダイアログで .glb / .gltf を選ばせ、ファイル名（拡張子なし）をオブジェクトの名前にします。

### `read_model_files(source)`（171〜196行目）

- .glb → そのファイルだけ
- .gltf → JSON の中の `"uri"` が指すファイル（.bin、テクスチャ画像）も読む。フォルダの外を指すものは読まない

### `referenced_uris(text)`（199〜221行目）

.gltf の JSON から `"uri": "..."` の値を全部取り出します。

- JSON を本格的に解析するのではなく、`"uri"` という文字を探して、その後の `"..."` を取り出す簡易的な方法です
- `data:` で始まるもの（ファイルの中に埋め込まれたデータ）は除きます
- `percent_decode`（223〜240行目）で `%20` などを元の文字に戻します（スペースや日本語のファイル名に対応）

---

## 定数

| 定数 | 値 | 意味 |
|---|---|---|
| `MODEL_DIR` | `"models"` | シーンの中でモデルを置くフォルダ |
| `IMPORT_DISTANCE` | 0.4 | モデルをスクリーンから何m奥に置くか |
