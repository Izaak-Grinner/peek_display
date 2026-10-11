# 09 camera/camera_preview.rs　編集モード右下の小窓

**一言で**: 編集モードの右下に出す「メインカメラと同じ映像」の小窓のためのカメラ。
**場所**: `src/camera/camera_preview.rs`（91行）
**関係するファイル**: [06 off_axis_projection](06_off_axis_projection.md)（同じ計算で動かす）、[23 edit_panel](23_edit_panel.md)（小窓を表示する）

---

## なぜメインカメラとは別のカメラなのか

コメント（10〜12行目）より:
> 画面に直接描くとシーンビューと描画先を共有して背景が混ざるので、画像に描いて UI で表示する。

```
小窓用カメラ ──描く──→ 画像（テクスチャ）──→ UI（egui）が右下に貼る
```

画面に直接描くのではなく、**いったん画像に描いて、その画像を UI の部品として表示** しています。

---

## 出てくるデータ

### `MainCameraPreview`（13〜14行目）

小窓用カメラの目印です。[06 off_axis_projection](06_off_axis_projection.md) は `MainCamera` と `MainCameraPreview` の両方に同じ計算をするので、小窓は本番と同じ見え方になります。

### `CameraPreview`（17〜23行目）

```rust
pub struct CameraPreview {
    pub visible: bool,         // 表示するか（メニューの「表示 → カメラプレビュー」）
    pub image: Handle<Image>,  // 描き込む画像
    pub size: UVec2,           // 画像の大きさ（画素）。UI 側が決める
}
```

---

## 関数ごとの説明

### `setup_preview_camera`（34〜59行目）

1. 16×10 の小さな画像を作る（大きさは後で UI に合わせて変わる）
2. カメラを作る。`target: RenderTarget::Image(...)` で **描き先を画像にする**
3. `order: -1` で、画面に描くほかのカメラより先に描く（UI が貼るときに描き終わっているように）

### `update_preview_camera`（62〜71行目）

**編集モード かつ 表示オン** のときだけカメラをオンにします。
オフのときは描かないので、余計な負荷がかかりません。

### `resize_preview_image`（73〜91行目）

UI 側（[23 edit_panel](23_edit_panel.md) の `viewport_overlay`）が `size` を書き換えたら、画像の大きさを変えます。
大きさは画面の縦横比に合わせて UI 側が計算しています。

`if !preview.is_changed() { return; }` で、変更があったフレームだけ処理します。
