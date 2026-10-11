# 10 scene/layout.rs　シーンのデータの形 ★

**一言で**: 「シーン」をプログラムの中でどういうデータとして持つかを決め、それを scene.ini の文字列と相互に変換する。
**場所**: `src/scene/layout.rs`（486行）
**関係するファイル**: ほぼ全部。特に [11 scene](11_scene.md)（3Dに反映）、[14 package](14_package.md)（.peek に保存）、[23 edit_panel](23_edit_panel.md)（画面から変更）

---

## このファイルの役割

**シーンの中身＝`SceneLayout`** です。
編集パネル・キー操作・取り込みはすべて `SceneLayout` を書き換え、[11 scene.rs](11_scene.md) がそれを見て3D空間を合わせます。

```
SceneLayout
├ screen: Placement          … スクリーン（実際のディスプレイ）の位置と向き
├ reference: Option<Vec2>    … 作ったときの画面の大きさ（基準の画面）
├ light: LightSettings       … 太陽光と環境光
└ objects: Vec<SceneObject>  … 置いてあるモノの一覧
    └ SceneObject
       ├ id, name
       ├ kind: ObjectKind    … Model（.glb/.gltf）か Cube（箱）
       ├ placement: Placement … 位置・回転・大きさ
       └ selectable          … シーンビューのクリックで選べるか
```

---

## 定数（6〜10行目）

```rust
pub const FLOOR_Y: f32 = -0.15;   // 床の高さ (m)
```

スクリーンの中心（原点）から **15cm 下** を床としています。部屋のモデルと、取り込んだモデルの置き場所に使います。

---

## データの型

### `Placement`（13〜73行目）　位置・回転・大きさ

```rust
pub struct Placement {
    pub position: Vec3,   // 位置 (m)
    pub pitch_deg: f32,   // X軸まわりの回転（うなずく向き）
    pub yaw_deg: f32,     // Y軸まわりの回転（首を振る向き）
    pub roll_deg: f32,    // Z軸まわりの回転（首をかしげる向き）
    pub scale: f32,       // 大きさの倍率（縦横高さ同じ）
}
```

| 関数 | 意味 |
|---|---|
| `at(position)` | 位置だけ指定して、ほかは初期値 |
| `rotation()` | 3つの角度から回転（クォータニオン）を作る。**Y → X → Z の順** にかける |
| `from_transform(t)` | Bevy の `Transform` から作る（大きさは 1 にする） |
| `to_transform()` | Bevy の `Transform` に変換する |

> 回転の順番（YXZ）が大事です。同じ角度でも順番が違うと別の向きになります。
> 人が数値で入れたときに直感的になるよう、まず方角（Y）、次に傾き（X）、最後にかしげ（Z）の順にしています。

### `Model`（75〜99行目）

```rust
pub struct Model {
    pub path: String,          // シーンの中のファイルの場所 (例: models/room/room.gltf)
    pub node: Option<String>,  // ワールドの部品なら、ファイルの中のどの部品か
}
```

- `Model::new(path)` … ファイルの中身を丸ごと表示する普通のモデル
- `Model::part(path, node)` … Blender で作ったワールドの「部品」。同じファイルの中の、名前が `node` のものだけを表示する

例: 部屋の .gltf の中に「床」「奥の壁」「天井」…が入っていて、それぞれが別のオブジェクトとして一覧に並ぶ。

### `OnLoad`（101〜108行目）

取り込んだ直後のモデルを、読み込みが終わったらどうするか。

| 値 | 意味 |
|---|---|
| `Fit` | 大きさを約30cmに合わせ、床に置く（モデルとして取り込んだとき） |
| `Split` | 部品ごとに分ける（ワールドとして取り込んだとき） |

実際の処理は [11 scene.rs](11_scene.md) の `place_imported`。

### `ObjectKind`（110〜114行目）

```rust
pub enum ObjectKind {
    Model(Model),
    Cube { color: Vec3, size: f32 },  // 色付きの箱
}
```

> Cube は今の UI からは追加できません。テスト用のシーンと、scene.ini に手で `[cube]` を書いたときだけ使われます。

### `SceneObject`（116〜136行目）

| フィールド | 意味 | 保存される? |
|---|---|---|
| `id` | 実行中だけ使う番号（保存しない。開くたびに振り直す） | ✗ |
| `name` | 一覧に出る名前 | ○ |
| `kind` | モデルか箱か | ○ |
| `placement` | 位置・回転・大きさ | ○ |
| `selectable` | シーンビューのクリックで選べるか | ○ |
| `on_load` | 取り込み直後の処理 | ✗ |

`model()` は、モデルなら中身を、箱なら `None` を返す便利関数です。

### `LightSettings`（138〜171行目）

```rust
pub struct LightSettings {
    pub illuminance: f32,  // 太陽光の明るさ（ルクス）初期値 10000
    pub ambient: f32,      // 環境光の明るさ 初期値 80
    pub pitch_deg: f32,    // 太陽光の傾き 初期値 -57.3°（斜め上から）
    pub yaw_deg: f32,      // 太陽光の方角 初期値 0°
}
```

`direction()` は光が進む向き（編集モードで矢印を描くのに使う）。

### `SceneLayout`（173〜183行目）

```rust
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct SceneLayout {
    pub screen: Placement,
    pub reference: Option<Vec2>,
    pub light: LightSettings,
    pub objects: Vec<SceneObject>,
    next_id: u64,   // 次に振る id
}
```

`Resource` なので、アプリ全体で1つだけあります（今開いているシーン）。

---

## `SceneLayout` の関数

| 関数 | 行 | 意味 |
|---|---|---|
| `default()` | 185〜211 | **テスト用** のシーン（izayoi と赤緑青の箱）。アプリの実行中には使われない |
| `empty()` | 215〜223 | スクリーンだけの空のシーン |
| `add_object(name, kind, placement)` | 225〜237 | オブジェクトを足して、振った id を返す |
| `object(id)` / `object_mut(id)` | 239〜245 | id でオブジェクトを探す（読む / 書き換える） |
| `same_content(other)` | 248〜256 | 保存される内容が同じか（id は比べない）。「未保存の変更があるか」の判定に使う |
| `restore(snapshot)` | 260〜264 | 元に戻す・やり直しで、前の状態に戻す（id はそのまま） |
| `replace_with(other)` | 268〜277 | 別のシーンに入れ替える（id は振り直す） |
| `parse(text)` | 280〜385 | scene.ini の文字列 → SceneLayout |
| `to_ini()` | 388〜441 | SceneLayout → scene.ini の文字列 |

### `restore` と `replace_with` の違い（大事）

[11 scene.rs](11_scene.md) は「id が同じなら同じモノ」として3Dのモノを使い回します。

- **元に戻す**（`restore`）: 同じシーンなので、id を **そのまま** 使う → 残っているモノは作り直さずに済む
- **別のシーンを開く**（`replace_with`）: id を **今の続きから振り直す** → 前のシーンのモノと取り違えない

---

## scene.ini の読み書き

### `parse(text)`（280〜385行目）

1行ずつ読みます。

```
├ 空行・コメント（# ;）→ 飛ばす
├ [screen]    → 以降はスクリーンの設定
├ [reference] → 以降は基準の画面の幅・高さ
├ [light]     → 以降はライトの設定
├ [model]     → 新しいモデルを足し、以降はそのモデルの設定
├ [cube]      → 新しい箱を足し、以降はその箱の設定
├ [知らない名前] → 警告して、そのセクションを無視
└ キー=値     → 今のセクションに応じて値を入れる
最後に、path が空の [model] を取り除く
```

オブジェクトのセクションで使えるキー:

| キー | 対象 | 意味 |
|---|---|---|
| name | 両方 | 名前 |
| selectable | 両方 | `false` ならクリックで選ばない |
| path | model | ファイルの場所 |
| node | model | ワールドの部品の名前 |
| color | cube | 色 `r,g,b` |
| size | cube | 一辺の長さ |
| position / pitch / yaw / roll / scale | 両方 | 配置（`parse_placement`） |

### `to_ini()`（388〜441行目）

逆に、SceneLayout から scene.ini の文字列を作ります。説明コメントも一緒に書きます。

- `[reference]` は基準の画面があるときだけ書く
- `node=` はワールドの部品のときだけ書く
- `selectable=false` は false のときだけ書く（true が初期値なので省略）

> アプリから保存すると、scene.ini は毎回丸ごと書き直されます。手で書いたコメントは消えます。

### 補助の関数（444〜486行目）

| 関数 | 意味 |
|---|---|
| `parse_placement` | position / pitch / yaw / roll / scale を読む。該当しないキーなら false |
| `write_placement` | 上の逆 |
| `format_vec3` | `Vec3` → `"x,y,z"` |
| `parse_vec3` | `"x,y,z"` → `Vec3`。読めなければ警告して元の値 |

---

## 変更するときは

| やりたいこと | どうする |
|---|---|
| シーンに保存する項目を足す | ① 型にフィールドを追加 ② `same_content` に比較を追加 ③ `parse` に読み込みを追加 ④ `to_ini` に書き出しを追加 ⑤ [14 package](14_package.md) のテストで確認 |
| 床の高さを変える | `FLOOR_Y` |
| ライトの初期値 | `LightSettings::default` |
