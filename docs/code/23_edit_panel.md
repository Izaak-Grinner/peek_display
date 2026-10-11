# 23 ui/edit_panel.rs　編集モードのメニューとパネル

**一言で**: 編集モードの画面（上のメニューバー、右のパネル、右下の小窓、ダイアログ、ショートカットキー）を全部描き、操作を SceneLayout / AppConfig / EditState に反映する。
**場所**: `src/ui/edit_panel.rs`（1639行。一番大きいファイル）
**関係するファイル**: [18 editor](18_editor.md)（状態と保存）、[15 library](15_library.md)、[22 widgets](22_widgets.md)（部品）、[17 import](17_import.md)、[14 package](14_package.md)

---

## 画面と関数の対応

```
┌ menu_bar ───────────────────────────────────────────────┐
│ ホーム │ ファイル 編集 表示 ヘルプ        シーン default ●未保存 │ ← save_indicator
├─────────────────────────────────────┬───────────────────┤
│ viewport_overlay                     │ save_error（失敗時） │
│ [シーンビュー]                        │ scene_section       │
│                                      │  ├ スクリーン        │
│      （シーンビューのカメラの映像）     │  ├ ライト           │
│                                      │  ├ ＋モデルを追加…   │
│                                      │  └ 部屋, izayoi ...  │ ← scene_list
│                                      │ プロパティ           │
│                      ┌────────────┐  │ properties           │
│                      │メインカメラ  │  │  ├ screen_properties │
│                      │（小窓）      │  │  ├ light_properties  │
│                      └────────────┘  │  └ object_properties │
└─────────────────────────────────────┴───────────────────┘
        ＋ ダイアログ（discard / save_as / delete / delete_scene）、help_window
```

---

## このファイルの設計: 「Action」で操作をまとめる

UI を描く部分と、操作を実行する部分を分けています。

```
① UI を描く（menu_bar, properties, ダイアログ…）
   └ ボタンが押されたら action = Some(Action::Save) のように「やりたいこと」を書くだけ
② ショートカットキーを見る
   └ Ctrl+S なら action = Some(Action::Save)
③ action に応じて実際の処理をする（ここで1か所にまとめて実行）
```

こうすると、メニューの「保存」と Ctrl+S が同じ処理を通るので、動作がずれません。

### `Action`（67〜99行目）

| Action | 意味 |
|---|---|
| `Request(Pending)` | 未保存なら確認してから「戻る／新規／開く／シーンのインポート」 |
| `Save` / `SaveAs` / `ConfirmSaveAs` / `CancelSaveAs` | 保存関係 |
| `SaveAndContinue` / `DiscardAndContinue` / `CancelDiscard` | 確認ダイアログのボタン |
| `Undo` / `Redo` | 元に戻す / やり直し |
| `ExportScene` / `Import(kind)` / `OpenScenesFolder` | ファイル関係 |
| `ResetToBlender(id)` | ワールドの部品を Blender の位置に戻す |
| `FitScreen` | スクリーンを peekScreen カメラに合わせる |
| `ResetLight` | ライトを初期値に |
| `SetReference` | 今の画面を基準にする |
| `AskDeleteScene` / `DeleteScene` / `CancelDeleteScene` | シーンの削除 |
| `Duplicate(id)` / `AskDelete(id)` / `Delete(id)` / `CancelDelete` | オブジェクトの複製・削除 |

---

## 定数（20〜58行目）

| 定数 | 値 | 意味 |
|---|---|---|
| `PANEL_WIDTH` | 330 | 右パネルの幅 |
| `MENU_BAR_HEIGHT` | 28 | メニューバーの高さ |
| `NEW_SCENE_NAME` | "新しいシーン" | 新規シーンの名前の初期値 |
| `PREVIEW_*` | | 小窓の大きさ（シーンビューの幅の30%、200〜480px、高さは45%まで） |
| `DUPLICATE_OFFSET` | (0.05, 0, 0) | 複製したものを右に5cmずらす |
| `HELP_ROWS` | | 「ヘルプ → 操作方法」の表（19行） |

`PanelUi`（60〜65行目）: 折りたたみの開閉状態とヘルプの表示など、**見た目だけの状態**（保存しない）。

---

## メインの関数 `edit_panel`（101〜339行目）

編集モードの間、毎フレーム呼ばれます。

### ① 準備（118〜127行目）

```rust
let preview_texture = contexts.add_image(...);  // 小窓の画像を egui で使えるようにする
let before_layout = layout.clone();            // 変更前を覚えておく
let before_config = config.clone();
let dirty = state.is_dirty(...);               // 未保存の変更があるか
let typing = ctx.wants_keyboard_input();       // 文字を入力中か
```

### ② 変更検知の工夫（130〜131行目、332〜337行目）

```rust
let layout = layout.bypass_change_detection();   // 書き換えても「変更」扱いにしない
...
if *layout != before_layout { layout.set_changed(); }  // 最後に、本当に変わっていたら「変更」にする
```

egui の数値欄などは、値を変えなくても毎フレーム `&mut` で触ります。
そのままだと毎フレーム「SceneLayout が変わった」扱いになり、[11 scene.rs](11_scene.md) の同期が毎回走ってしまいます。
そこで、前後を比べて **本当に変わったときだけ** 変更扱いにしています。

### ③ UI を描く（133〜191行目）

1. メニューバー（`TopBottomPanel::top`）
2. 右パネル（`SidePanel::right`）
   - 失敗時のエラー表示（`save_error`）
   - スクロールする中身: シーン一覧（`scene_section`）、プロパティ（`properties`）
3. パネルの幅・メニューの高さを `insets` に書く → [02 app_mode](02_app_mode.md) がシーンビューの範囲に使う
4. 小窓とラベル（`viewport_overlay`）、ヘルプ（`help_window`）
5. ダイアログ（出ているものを1つだけ）

### ④ ショートカットキー（193〜244行目）

ダイアログが出ているかどうかで、キーの意味が変わります。

| 状態 | Esc | Enter |
|---|---|---|
| 削除の確認中 | キャンセル | 削除する |
| 名前を付けて保存中 | キャンセル | 保存 |
| 未保存の確認中 | キャンセル | 保存して続ける |
| ヘルプ表示中 | ヘルプを閉じる | |
| 何もない（文字入力中を除く） | ホームへ（未保存なら確認） | |

何もないときのショートカット: Ctrl+S（保存）、Ctrl+Shift+S（名前を付けて保存）、Ctrl+Z（元に戻す）、Ctrl+Shift+Z / Ctrl+Y（やり直し）、Ctrl+N（新規）、Ctrl+D（複製）、Delete（削除）

### ⑤ Action の実行（246〜329行目）

`FileContext` にまとめた道具を使って、`match action { ... }` で実行します。

---

## `FileContext`（347〜454行目）　ファイル操作

ファイル操作に必要なリソースをひとまとめにした入れ物です（関数の引数が多くなりすぎるのを防ぐ）。

| 関数 | 意味 |
|---|---|
| `request(pending)` | 未保存なら確認ダイアログを出す。無ければすぐ `run` |
| `run(pending)` | 実行する。戻る／新規（部屋だけのシーン、基準は今の画面）／開く／シーンのインポート |
| `delete_scene(name)` | シーンを削除。開いていたシーンなら、ほかのシーン（無ければ新規）を開く |
| `save(then)` | 上書き保存。名前が無ければ「名前を付けて保存」を出す。成功したら `then` を実行 |
| `ensure_reference()` | 基準の画面が無ければ、保存時に今の画面を基準にする |
| `confirm_save_as()` | 「名前を付けて保存」の保存ボタン |

「保存して戻る」の流れの例:
```
Esc → Request(Exit) → 未保存 → 確認ダイアログ
  → 「保存して戻る」→ SaveAndContinue → save(Some(Exit))
      ├ 名前あり → 保存 → run(Exit) → ホームへ
      └ 名前なし → 名前を付けて保存ダイアログ（then = Exit）
                     → 保存 → run(Exit) → ホームへ
```

`open_scenes_folder`（456〜469行目）: エクスプローラーで scenes フォルダを開く。

---

## メニューバー `menu_bar`（472〜600行目）

| メニュー | 項目 |
|---|---|
| ホーム | （すぐ戻る。未保存なら確認） |
| ファイル | 新規シーン / 開く ▶ / シーンを削除 ▶ / 保存 / 名前を付けて保存… / インポート ▶（ワールド、モデル、シーン）/ エクスポート ▶（シーン） |
| 編集 | 元に戻す / やり直し（できないときは押せない） |
| 表示 | カメラプレビュー（チェック） |
| ヘルプ | 操作方法 |
| 右端 | シーン名と保存の状態（`save_indicator`: 未保存／保存した時刻／失敗） |

取り込み中は「インポート」の項目が押せなくなります（`add_enabled(!import.busy(), ...)`）。

---

## 右パネル

### シーン一覧 `scene_section` / `scene_list`（762〜893行目）

一覧の並び: メインカメラ（スクリーン）→ 照明（ライト）→ オブジェクト（＋モデルを追加…、各オブジェクト）

`object_kind`（603〜615行目）が、オブジェクトの種類名とアイコンを決めます。

| オブジェクト | 表示 |
|---|---|
| ワールドの部品で、ライトが入っている | ライト |
| ワールドの部品 | ワールド |
| モデル | モデル |
| 箱 | キューブ |
| クリックで選ばない設定 | 「クリック除外」 |

`package_status`（794〜840行目）: エクスポート・インポートの結果を一覧の上に1行で出す。

### プロパティ `properties`（925〜947行目）

選んでいるものに応じて表示を切り替えます。

| 選択 | 関数 | 内容 |
|---|---|---|
| なし | `nothing_selected`（949〜967行目） | 「何も選択されていません」＋スクリーン設定を開くボタン |
| ライト | `light_properties`（969〜1034行目） | 太陽光の明るさ・傾き・方角、環境光、初期値に戻す。モデルのライトで太陽光が消えていれば注意書き |
| オブジェクト | `object_properties`（1036〜1092行目） | 名前、種類、位置・回転・大きさ、クリックで選ぶか、複製・削除、Blender の値に戻す |
| スクリーン | `screen_properties`（1216〜1409行目） | 下で説明 |

### スクリーンのプロパティ `screen_properties`（1216〜1409行目）

上半分は **シーンに保存**、下半分は **config.ini に保存** です。

| 見出し | 内容 | 保存先 |
|---|---|---|
| トランスフォーム | スクリーンの位置・回転 | シーン |
| 「〜の peekScreen カメラに合わせる」 | peekScreen 入りのモデルがあるときだけ | シーン |
| 基準の画面（シーン） | 基準の大きさ、縮尺の注意書き、今の画面を基準にする | シーン |
| 機材（このPC）› 画面サイズ | 対角、幅、比率（モニターから）、実寸 | config.ini |
| 機材（このPC）› 目と画面の距離 | 基準距離、最小距離 | config.ini |
| 機材（このPC）› トラッキング | マーカーの大きさ、視野角 | config.ini |

### 共通の部品

| 関数 | 行 | 意味 |
|---|---|---|
| `placement_editor` | 1147〜1198 | 位置・回転（・大きさ）の X/Y/Z の表。スクリーン・オブジェクトで共通 |
| `identity` | 1094〜1112 | プロパティ上部のアイコン＋名前 |
| `name_edit` | 1121〜1145 | 名前の入力欄 |
| `info_row` | 1411〜1426 | 「ラベル … 値」の表示だけの行 |

---

## 小窓 `viewport_overlay`（618〜690行目）

1. シーンビューの左上に「シーンビュー」のラベル
2. 表示オンなら、右下に小窓
   - 幅はシーンビューの30%（200〜480px）、縦横比は実際の画面と同じ
   - 必要な画像の大きさを `preview.size` に書く → [09 camera_preview](09_camera_preview.md) が画像を作り直す
   - 画像を貼って、枠と「メインカメラ」のラベルを描く

---

## ダイアログ（1459〜1639行目）

| 関数 | いつ出る | ボタン |
|---|---|---|
| `discard_dialog` | 未保存のまま戻る・新規・開く・インポート | 保存せずに◯◯ / キャンセル / 保存して◯◯ |
| `save_as_dialog` | 名前を付けて保存 | キャンセル / 保存（同じ名前があれば「上書き保存」と警告） |
| `delete_scene_dialog` | シーンを削除 | キャンセル / 削除する |
| `delete_dialog` | オブジェクトを削除 | キャンセル / 削除する |
| `help_window` | ヘルプ → 操作方法 | （×で閉じる） |

`dialog` / `dialog_frame` が共通の見た目（暗い背景、影、アイコン付きタイトル）を作ります。
`egui::Modal` なので、ダイアログが出ている間は後ろを操作できません。

---

## 変更するときは

| やりたいこと | どこ |
|---|---|
| メニューに項目を足す | ① `Action` に追加 ② `menu_bar` にボタン ③ `edit_panel` の `match action` に処理 |
| ショートカットを足す | `edit_panel` の ④ のキー判定 ＋ `HELP_ROWS` |
| config.ini の項目を画面から変えられるようにする | `screen_properties` に `NumberField` を追加（[03 config](03_config.md) も参照） |
| パネルの幅 | `PANEL_WIDTH` |
| 小窓の大きさ | `PREVIEW_*` |
