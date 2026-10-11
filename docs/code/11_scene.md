# 11 scene/scene.rs　データを3D空間に反映する ★

**一言で**: `SceneLayout`（データ）が変わったら、3D空間のモノを足す・消す・動かす。取り込んだモデルの大きさや置き方を決める。
**場所**: `src/scene/scene.rs`（277行）
**関係するファイル**: [10 layout](10_layout.md)（データ）、[12 model](12_model.md)（モデルの中身）、[13 package_store](13_package_store.md)（ファイルの読み込み元）

---

## このファイルの役割

```
SceneLayout（データ）              3D空間（Bevy のエンティティ）
  objects[0] id=5 部屋     ──→     モノ  [Placeable(5)] + 部屋のモデル
  objects[1] id=6 izayoi   ──→     モノ  [Placeable(6)] + izayoi のモデル
  light                    ──→     太陽光 [DefaultLight]
```

データとモノは **`Placeable(id)` というシール** で対応付けています。

---

## 登録（10〜23行目）

```rust
app.add_systems(Startup, setup_light)
   .add_systems(Update, (
        sync_scene_objects.run_if(resource_changed::<SceneLayout>),
        apply_light.run_if(resource_changed::<SceneLayout>),
        place_imported,
   ).chain());
```

`resource_changed::<SceneLayout>` … **SceneLayout が変わったフレームだけ** 動きます。何も変えていなければ処理しないので軽くなります。

---

## 出てくるデータ

| 型 | 種類 | 意味 |
|---|---|---|
| `DefaultLight` | シール | アプリが置く太陽光の目印 |
| `Placeable(u64)` | シール | 「このモノは SceneLayout の id 番のオブジェクト」 |
| `PlaceOnLoad(OnLoad)` | シール | 取り込んだ直後で、読み込みが終わったら置き方を決めるモノ |
| `AUTO_FIT_SIZE` | 定数 0.3 | 取り込んだモデルの一番長い辺を 30cm に合わせる |

---

## 関数ごとの説明

### `setup_light`（40〜46行目）

起動時に太陽光（平行光源 `DirectionalLight`）を1つ置きます。

### `apply_light`（49〜67行目）

`SceneLayout.light` の値を、太陽光の明るさ・向きと環境光の明るさに反映します。値が違うときだけ書き込みます。

### `sync_scene_objects`（70〜104行目）　★中心

```
① 今あるモノ（Placeable が付いたもの）を全部見る
   ├ SceneLayout にその id がある → 位置・回転・大きさを合わせる、「ある」と覚える
   └ SceneLayout にその id が無い → モノを消す（despawn）
② SceneLayout のオブジェクトのうち、まだモノが無いもの → 作る（spawn_object）
```

これで、オブジェクトの追加・削除・移動・元に戻す、が全部この1つの関数で反映されます。

### `spawn_object`（106〜138行目）

**モデルの場合**:

```rust
SceneRoot(asset_server.load(
    GltfAssetLabel::Scene(0).from_asset(store.asset_path(&model.path))
))
```

- `store.asset_path(...)` で `scene://3/models/room/room.gltf` のようなパスにして読み込む（[13 package_store](13_package_store.md)）
- `GltfAssetLabel::Scene(0)` … .gltf ファイルの中の最初のシーンを表示する
- 読み込みは裏で進むので、この時点ではまだ表示されない
- `entity.observe(model::on_model_ready)` … 読み込みが終わったら [12 model](12_model.md) の `on_model_ready` を呼ぶ
- ワールドの部品なら `ModelPart(部品名)` のシールを貼る（後で部品以外を消すため）

**箱の場合**: 立方体のメッシュと色のマテリアルを作って置きます。

最後に共通で `Placeable(id)` と名前を付け、取り込み直後なら `PlaceOnLoad` も付けます。

### `mesh_bounds`（145〜177行目）　モデルの大きさを測る

モノとその子孫（モデルの中の部品）すべてのメッシュを囲む箱（最小の点と最大の点）を求めます。

- `Aabb` は Bevy が計算する「メッシュを囲む箱」です。読み込み直後はまだ無いので、そのときは `None` を返します
- 箱の8つの角を3D空間の座標に変換して、全部を囲む範囲を出しています

### `place_imported`（184〜244行目）　取り込んだモデルの置き方を決める

毎フレーム、`PlaceOnLoad` が付いたモノを見ます。

```
├ まだモデルの読み込みが終わっていない → 次のフレームへ
├ OnLoad::Split（ワールド）→ split_world で部品に分ける
└ OnLoad::Fit（モデル）
    ├ 大きさがまだ測れない → 次のフレームへ
    ├ peekScreen カメラが入っている
    │   → Blender の位置・大きさのまま置き、スクリーンをそのカメラに合わせる
    └ 入っていない
        → 一番長い辺が 30cm になるよう縮尺を決め、底面を床（FLOOR_Y）に合わせる
```

縮尺と床合わせの計算（234〜238行目）:

```rust
let k = AUTO_FIT_SIZE / largest;                     // 何倍にするか
let new_min_y = origin.y + (min.y - origin.y) * k;    // 縮めた後の一番下の高さ
object.placement.scale *= k;
object.placement.position.y += FLOOR_Y - new_min_y;  // 一番下が床に来るよう上下に動かす
```

> ここで SceneLayout を書き換えるので、次のフレームで `sync_scene_objects` が動いてモデルの位置が直ります。

### `split_world`（248〜277行目）　ワールドを部品に分ける

```
1. peekScreen カメラがあれば、スクリーンをその位置に合わせる
2. 分けられる部品が無ければ、丸ごと1つのまま
3. あれば:
   ├ ワールド全体のオブジェクトを消す
   └ 部品ごとに Model::part(同じファイル, 部品名) のオブジェクトを足す（Blender の位置のまま）
```

ファイルは1つのまま、部品の数だけオブジェクトができます。
部品ごとに同じファイルを読み込みますが、Bevy が中身を共有するので、メモリは1つ分で済みます。

---

## 変更するときは

| やりたいこと | どこ |
|---|---|
| 取り込んだモデルの大きさ | `AUTO_FIT_SIZE` |
| 新しい種類のオブジェクト（例: 球）を足す | [10 layout](10_layout.md) の `ObjectKind` に追加 → `spawn_object` の `match` に作り方を追加 |
