# 22 ui/widgets.rs　UI の部品

**一言で**: 編集パネルで使うボタン・数値欄・見出し・一覧の行・アイコン・色を、見た目をそろえて自作した部品集。
**場所**: `src/ui/widgets.rs`（652行）
**関係するファイル**: [23 edit_panel](23_edit_panel.md)（使う側）

---

## なぜ自作しているか

コメント（1行目）より:
> egui 標準のウィジェットでは見た目を合わせにくいものを自前で描く

egui 標準のボタンなどは見た目の調整に限りがあるので、Blender のような暗い色調の見た目にそろえるため、四角形や文字を直接描いて部品を作っています。
**動作の理解には関係しない、見た目のためのファイル** です。

---

## 中身の一覧

### 色 `mod color`（9〜57行目）

全部の色をここに名前付きで定義しています。色を変えたいときはここを変えます。

| 名前 | 用途 |
|---|---|
| `PANEL`, `BAR`, `SECTION`, `WELL`, `FIELD` | 背景（パネル、メニューバー、見出し、一覧のくぼみ、数値欄） |
| `TEXT_*` | 文字の色（濃さ違い） |
| `SELECTED`, `FOCUS` | 選択中の行、入力中の枠（青） |
| `AXIS_X` / `AXIS_Y` / `AXIS_Z` | X=赤、Y=緑、Z=青（数値欄の下線） |
| `ICON_SCREEN` / `ICON_MODEL` / `ICON_LIGHT` / `ICON_CUBE` | 一覧のアイコンの色 |
| `WARN`, `DANGER` | 警告（オレンジ）、削除（赤） |

`hex(0x262626)` は、16進数の色コードを egui の色に変換する関数です（CSS の `#262626` と同じ）。

### 線 `hairline`（66〜69行目）

太さ1pxの線を作ります。枠線はほぼこれです。

### アイコン `Icon` / `draw_icon`（73〜198行目）

```rust
pub enum Icon { Monitor, Sun, Model, Cube, Copy, Trash, Warning, Check,
                ChevronDown, ChevronRight, Cursor, Spinner, Plus }
```

画像ファイルを使わず、**線と図形で描いたアイコン** です。14×14 の方眼の上の座標で形を書き、表示する大きさに拡大して描きます。

| アイコン | 使う場所 |
|---|---|
| Monitor | スクリーン |
| Sun | ライト |
| Model | モデル |
| Cube | 箱 |
| Copy / Trash | 複製 / 削除ボタン |
| Warning / Check | エラー / 保存済み |
| Spinner | 処理中 |

### 文字（201〜229行目）

| 関数 | 意味 |
|---|---|
| `text` | 文字の形（Galley）を作る |
| `truncated_text` | 幅に収まらなければ「…」で切る |
| `fixed_label` | 決まった幅の中に文字を置く（表のラベル列） |
| `wrapped_label` | 幅で折り返す文字 |

### ボタン `Button`（232〜387行目）

```rust
Button::new("複製", widgets::BUTTON)   // 文字とスタイル
    .icon(Icon::Copy)                  // アイコンを付ける
    .width(120.0)                      // 幅（指定すると中央寄せ）
    .enabled(true)                     // 押せるか
    .show(ui)                          // 描く → 戻り値の .clicked() で押されたか
```

`.icon(...)` や `.width(...)` をつなげて書く方式（ビルダー）です。

スタイル（色の組み合わせ）:

| スタイル | 見た目 | 使う場所 |
|---|---|---|
| `BUTTON` | 灰色 | 通常 |
| `BUTTON_SECONDARY` | 灰色（枠が少し違う） | キャンセル |
| `BUTTON_PRIMARY` | 青・太字 | 保存 |
| `BUTTON_DANGER` | 赤文字 | 削除… |
| `BUTTON_DANGER_GHOST` | 透明・赤文字 | 保存せずに◯◯ |
| `BUTTON_DANGER_SOLID` | 赤・太字 | 削除する |

> 太字のフォントを読み込んでいないので、太字は **文字を0.4pxずらして2回描く** ことで表現しています（373〜376行目）。

### 見出し（389〜457行目）

| 関数 | 見た目 |
|---|---|
| `section_header` | 「シーン」「プロパティ」の帯 |
| `sub_header` | 「トランスフォーム」などの下線付き小見出し |
| `collapsing_header` | 折りたためる見出し（▼/▶）。右端に今の値のまとめを出す |

### 一覧（458〜546行目）

| 関数 | 見た目 |
|---|---|
| `list_row` | 一覧の1行（アイコン、名前、右端の種類）。選択中は青 |
| `add_row` | 「＋ モデルを追加…」の行 |
| `group_label` | 一覧の中の小さな区切り（「オブジェクト」など） |

### 数値欄 `NumberField`（548〜626行目）

```rust
NumberField::new(&mut placement.position.x, 0.005, 3)  // 値、ドラッグ1pxでの増減、小数の桁数
    .accent(AXIS_X)          // 下に色の線
    .unit("m")               // 右に単位
    .prefix("傾き")          // 左にラベル
    .range(0.0..=100.0)      // 範囲
    .show(ui, 幅)
```

中身は egui 標準の `DragValue`（**ドラッグで増減、クリックで入力** できる数値欄）で、`style_value`（628〜652行目）で枠を消して背景になじませています。
引数の `&mut` で値を直接書き換えるので、ドラッグするとそのまま SceneLayout や AppConfig の値が変わります。
