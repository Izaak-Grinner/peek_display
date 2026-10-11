# 20 ui/mod.rs　UI の土台

**一言で**: 画面の UI（ボタンやパネル）を描く仕組み（egui）を準備し、日本語フォントを読み込み、モードごとの UI を登録する。検査モードの左上の説明もここ。
**場所**: `src/ui/mod.rs`（125行）
**関係するファイル**: [21 home](21_home.md)、[23 edit_panel](23_edit_panel.md)、[22 widgets](22_widgets.md)

---

## egui とは

このアプリの UI は **egui**（bevy_egui）というライブラリで描いています。
egui は「毎フレーム、UI を全部描き直す」方式（イミディエイトモード）です。

```rust
if ui.button("保存").clicked() {   // ボタンを描く。押されたフレームだけ true
    // 押されたときの処理
}
```

ボタンを「作って置いておく」のではなく、**毎フレーム描いて、その場で押されたか確かめる** 書き方になります。
[21 home](21_home.md) や [23 edit_panel](23_edit_panel.md) を読むときの前提です。

---

## 定数・データ

### `JAPANESE_FONTS`（18〜22行目）

```rust
r"C:\Windows\Fonts\YuGothM.ttc",   // 游ゴシック
r"C:\Windows\Fonts\meiryo.ttc",    // メイリオ
r"C:\Windows\Fonts\msgothic.ttc",  // MS ゴシック
```

上から順に探して、最初に見つかったものを使います。egui は標準で日本語フォントを持っていないためです。

### `UiInputState`（26〜29行目）

```rust
pub struct UiInputState {
    pub pointer: bool,   // マウスが UI の上にある / UI を操作中
    pub keyboard: bool,  // 文字を入力中
}
```

3D側の操作（シーンビューの回転、キーでの移動、クリックの選択）は、これを見て **UI を操作しているときは反応しない** ようにしています。
例: パネルの数値欄をドラッグしているのに、後ろの3Dが回ってしまうのを防ぐ。

---

## 関数ごとの説明

### `UiPlugin::build`（33〜51行目）

```rust
(
    setup_fonts.run_if(run_once),                          // 最初の1回だけ
    home::home_ui.run_if(in_state(AppMode::Home)),         // ホームのとき
    inspection_overlay.run_if(in_state(AppMode::Inspection)), // 検査のとき
    edit_panel::edit_panel.run_if(in_state(AppMode::Edit)),   // 編集のとき
    update_ui_input_state,                                 // 毎回
).chain()
```

`EguiPrimaryContextPass` は「egui の UI を描くタイミング」です。本番モードでは UI を何も描きません。

### `setup_ui_camera`（53〜70行目）　UI 専用カメラ

UI を描くためだけのカメラを作ります。

| 設定 | 意味 |
|---|---|
| `RenderLayers::none()` | 3Dのモノは何も映さない |
| `order: 10` | 一番最後に描く（＝一番上に重なる） |
| `ALPHA_BLENDING` / `clear_color: None` | 下の3D映像を消さずに、上に重ねて描く |

> **UI 専用カメラにしている理由**: モードによってメインカメラもシーンビューもオフになることがあるので、どちらかに UI を乗せると UI が消えてしまうためです。

### `setup_fonts`（72〜96行目）

日本語フォントを読み込み、通常の文字（Proportional）の **一番優先** に設定します。
英数字も同じフォントで描いているのは、日本語と英数字で文字の高さ（ベースライン）がずれるのを防ぐためです。

### `inspection_overlay`（98〜118行目）　検査モードの説明

検査モードの左上に、操作方法と **頭の位置（cm）** を表示します。

```rust
ui.label(format!("頭の位置 (cm): x {:.1}  y {:.1}  z {:.1}", pose.x, pose.y, pose.z));
```

トラッキングが動いているか確認するとき、ここの数字が変わるかを見ます。

### `update_ui_input_state`（120〜125行目）

egui に「マウスやキーボードを使っているか」を聞いて、`UiInputState` に書きます。
