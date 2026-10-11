# 21 ui/home.rs　ホーム画面

**一言で**: 起動直後のホーム画面（シーンの選択、モードのボタン、終了ボタン）を描く。
**場所**: `src/ui/home.rs`（93行）
**関係するファイル**: [15 library](15_library.md)（シーン一覧）、[02 app_mode](02_app_mode.md)（モード）

---

## 画面の構成

```
┌──────────────────────────────┐
│     のぞけるディスプレイ      │
│  シーン [ default      ▼]    │
│  (開けなかったときのエラー)   │
│  ┌────────────────────┐     │
│  │    本番モード       │     │
│  └────────────────────┘     │
│   メインカメラを全画面で表示  │
│  ┌────────────────────┐     │
│  │    検査モード       │     │
│  └────────────────────┘     │
│  ┌────────────────────┐     │
│  │    編集モード       │     │
│  └────────────────────┘     │
│           [終了]             │
│ どのモードからも Esc で…      │
└──────────────────────────────┘
   背景: シーンビュー（ゆっくり回る）
```

---

## `home_ui`（13〜93行目）

この1つの関数で全部描いています。毎フレーム呼ばれます（[20 ui/mod](20_ui_mod.md) の egui の説明を参照）。

### 引数

| 引数 | 何に使う |
|---|---|
| `next: ResMut<NextState<AppMode>>` | モードを切り替える |
| `exit: MessageWriter<AppExit>` | アプリを終了する |
| `library`, `layout`, `store`, `state` | シーンを切り替える |
| `open_error: Local<Option<String>>` | シーンを開けなかった理由（次のフレームも表示し続けるため Local） |

### シーンの選択（38〜66行目）

```rust
egui::ComboBox::from_id_salt("home_scene")
    .selected_text(current.unwrap_or("（未保存のシーン）"))
    .show_ui(ui, |ui| {
        for name in library::list() {          // scenes フォルダの .peek を全部
            if ui.selectable_label(...).clicked() && !selected {
                library.open_scene(&name, &mut store)   // 開く（current.txt も更新）
                layout.replace_with(loaded);             // シーンを入れ替える
                state.scene_switched(&layout);           // 編集の状態をリセット
            }
        }
    });
```

> ホームでは「保存していない変更」の確認をしていません。編集モードから戻るときに、必ず保存するか捨てるかを選んでいるので、ホームには未保存の変更が無いためです（コメント 38〜39行目）。

### モードのボタン（70〜80行目）

```rust
let mut mode_button = |ui, label, detail, mode| {
    if ui.add_sized(BUTTON_SIZE, egui::Button::new(text)).clicked() {
        next.set(mode);   // 押されたらモードを切り替える
    }
    ui.label(detail);
};
```

同じ形のボタンを3つ作るので、クロージャ（関数の中の小さな関数）にまとめています。
ボタンの大きさは `BUTTON_SIZE`（320×56）。

### 終了ボタン（83〜85行目）

`exit.write(AppExit::Success)` でアプリを終了します（[05 tracking](05_tracking.md) がこれを受けて OpenTrack も終了させる）。

---

## 変更するときは

| やりたいこと | どこ |
|---|---|
| タイトル | 35行目の `"のぞけるディスプレイ"` |
| ボタンの説明文 | `mode_button(ui, "本番モード", "...", ...)` の2つ目 |
| ボタンの大きさ | `BUTTON_SIZE` |
