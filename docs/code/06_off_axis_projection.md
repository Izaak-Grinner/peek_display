# 06 camera/off_axis_projection.rs　窓の見え方の計算 ★

**一言で**: 頭の位置から、メインカメラの位置と「見える範囲（視錐台）」を毎フレーム計算する。このアプリの心臓部。
**場所**: `src/camera/off_axis_projection.rs`（138行）
**関係するファイル**: [05 tracking](05_tracking.md)（HeadPose）、[04 display](04_display.md)（画面の実寸）、[10 layout](10_layout.md)（スクリーンの位置）、[07 main_camera](07_main_camera.md)

---

## 何をしているのか（考え方）

### 普通のゲームのカメラ

普通のカメラは、見える範囲が **左右・上下対称** のピラミッド型です。

```
        ＼    │    ／
          ＼  │  ／      ← 見える範囲は真ん中を軸に対称
            ＼│／
             目
```

### 窓のカメラ（off-axis = 軸ずれ）

窓の外を見るとき、見える範囲は **目と窓枠を結んだ線** で決まります。
目が窓の正面からずれると、見える範囲は **非対称** になります。

```
  ディスプレイ(窓)
 ├────────────┤        ← 画面の左端と右端は動かない
   ＼          ＼
     ＼          ＼    ← 目が右にずれると、見える範囲が左に傾く
       ＼          ＼
                  目
```

このファイルは、
1. カメラを **目の位置** に置き、
2. 見える範囲を **「目から画面の4隅を通る線」** で切り取る

ことで、画面が本物の窓のように見えるようにしています。これを **非対称視錐台（off-axis projection）** と言います。

---

## `OffAxisProjection`（13〜34行目）

```rust
pub struct OffAxisProjection {
    pub left: f32,    // 見える範囲の左端
    pub right: f32,   // 右端
    pub bottom: f32,  // 下端
    pub top: f32,     // 上端
    pub near: f32,    // 一番手前 (0.01m)
    pub far: f32,     // 一番奥 (10m)
}
```

left / right / bottom / top は「目から `near`（1cm）前に置いた小さな板の上で、見える範囲がどこからどこまでか」を表します。
普通のカメラなら `left = -right` ですが、このカメラは目の位置によって左右がずれます。

---

## Bevy に「このカメラはこう映す」と教える部分（36〜69行目）

`impl CameraProjection for OffAxisProjection` は、Bevy の標準のカメラの代わりに、自作の映し方を使うための決まりごとです。

### `get_clip_from_view()`（37〜54行目）

left〜far の6つの値から、**射影行列**（3D空間の点を画面上のどこに描くかを決める 4×4 の表）を作ります。

```rust
Mat4::from_cols(
    Vec4::new(2n/(r-l), 0,         0,          0),
    Vec4::new(0,        2n/(t-b),  0,          0),
    Vec4::new((r+l)/(r-l), (t+b)/(t-b), n/(f-n), -1),
    Vec4::new(0,        0,         f*n/(f-n),  0),
)
```

- 3列目の `(r+l)/(r-l)` と `(t+b)/(t-b)` が **非対称の分のずれ** です。普通のカメラ（l = -r）ならここは 0 になります
- 奥行きの部分（`n/(f-n)` など）は、Bevy の決まり（**reverse-Z**: 手前が 1、奥が 0）に合わせた式です。OpenGL の教科書の式とは違うので、教科書の式に書き換えると描画が壊れます

### そのほかの関数

| 関数 | 中身 | 意味 |
|---|---|---|
| `get_clip_from_view_for_sub` | 同じ行列を返す | 画面の一部だけ描く機能用。使っていない |
| `update` | 何もしない | ウィンドウの大きさが変わったとき用。窓の計算は自分でやるので不要 |
| `far` | `self.far` | 一番奥の距離 |
| `get_frustum_corners` | 全部 0 を返す | 主に影（カスケードシャドウ）の計算用。今は使っていないため仮の値 |

---

## 毎フレームの計算

### `update_off_axis_projection`（79〜96行目）

```rust
mut query: Query<(&mut Transform, &mut Projection),
                 Or<(With<MainCamera>, With<MainCameraPreview>)>>,
```

**メインカメラ** と **編集モード右下の小窓のカメラ** の両方を探して、同じ計算（`apply_off_axis`）をします。
だから小窓には本番と同じ映像が出ます。

88〜93行目の `let ... else { continue; }` は、「自作の映し方（OffAxisProjection）を使っているカメラでなければ飛ばす」という確認です。

### `apply_off_axis`（98〜138行目）　★ここが計算の本体

#### ① 縮尺（108行目）

```rust
let k = display.view_scale(layout.reference);
```

画面がシーンを作ったときと違う大きさなら、縮尺 `k` を掛けます（[04 display](04_display.md) の `view_scale`）。
目の位置と画面の大きさの **両方** に同じ `k` を掛けるので、頭を動かしたときの見え方の自然さは保たれます。

#### ② 目の位置（110〜114行目）

```rust
let ex = pose.x * 0.01 * k;    // 左右 (cm → m)
let ey = pose.y * 0.01 * k;    // 上下
let ez = (config.viewer_base_distance - pose.z * 0.01)
            .max(config.viewer_min_distance) * k;   // 画面からの距離
```

- 座標は **スクリーンの中心が原点**、スクリーンの手前（見る人の側）が +Z です
- `ez`: センター合わせをした位置で `viewer_base_distance`（初期値 0.5m）。そこから OpenTrack の z の分だけ近づく・離れる
- `.max(viewer_min_distance)`: 画面に近づきすぎて 0 や負になると計算が壊れるので、最小距離で止める

#### ③ カメラを置く（118〜120行目）

```rust
let screen = Transform::from_translation(layout.screen.position)
    .with_rotation(layout.screen.rotation());
*transform = screen * Transform::from_xyz(ex, ey, ez);
```

「スクリーンの位置と向き」×「スクリーンから見た目の位置」で、3D空間の中の目の位置が決まります。
カメラの向きは **常にスクリーンの向きと同じ**（スクリーンに垂直）です。目が動いても、カメラは首を振りません。見える範囲のずれは ④ で表現します。

> 編集モードでスクリーンを動かす・回すと、カメラも一緒に動くのはこのためです。

#### ④ 見える範囲（122〜132行目）

```rust
let n = projection.near;
let screen_width  = display.size.x * k;
let screen_height = display.size.y * k;

projection.left   = n * (-screen_width  * 0.5 - ex) / ez;
projection.right  = n * ( screen_width  * 0.5 - ex) / ez;
projection.bottom = n * (-screen_height * 0.5 - ey) / ez;
projection.top    = n * ( screen_height * 0.5 - ey) / ez;
```

**相似な三角形** の計算です。横から見た図:

```
  画面の右端 ─────┐ (W/2, 0)
                  │＼
                  │  ＼
  near の板 ──────┼───＼── right  ← ここを求める
                  │     ＼
                  │       ＼
                  │         目 (ex, ez)
```

- 目から画面の右端まで: 横に `W/2 - ex`、奥に `ez`
- 目から near の板まで: 奥に `n`
- 同じ傾きなので: `right = n × (W/2 - ex) / ez`

左端・上端・下端も同じ考え方です。

---

## 変更するときは

| やりたいこと | どこ |
|---|---|
| 頭の動きに対する反応を大きく／小さく | ② の `pose.x * 0.01` などに倍率を掛ける（ただし実物とずれる） |
| 近づける限界を変える | config.ini の `viewer_min_distance` |
| 遠くのものが切れる | config.rs の `OFF_AXIS_FAR` |
| 計算の値を確認したい | 134〜137行目のコメントアウトされた `println!` を戻す |
