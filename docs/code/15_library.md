# 15 scene/library.rs　scenes フォルダのシーン一覧

**一言で**: `scenes/` フォルダに保存されたシーンの一覧・開く・保存・削除と、最後に開いたシーン（current.txt）の管理。
**場所**: `src/scene/library.rs`（224行）
**関係するファイル**: [14 package](14_package.md)（.peek の読み書き）、[16 default_room](16_default_room.md)（新規シーン）、[18 editor](18_editor.md)・[21 home](21_home.md)・[23 edit_panel](23_edit_panel.md)（呼び出し元）

---

## 定数（11〜17行目）

| 定数 | 値 | 意味 |
|---|---|---|
| `SCENES_DIR` | `"scenes"` | シーンを置くフォルダ（起動したフォルダからの相対パス） |
| `CURRENT_FILE` | `"current.txt"` | 最後に開いたシーンの名前を書くファイル |
| `FIRST_SCENE_NAME` | `"default"` | シーンが1つも無いときに作るシーンの名前 |
| `INVALID_CHARS` | `\ / : * ? " < > \|` | Windows のファイル名に使えない文字 |
| `MAX_NAME_CHARS` | 64 | 名前の最大文字数 |

---

## `SceneLibrary`（19〜25行目）

```rust
pub struct SceneLibrary {
    pub current: Option<String>,  // 今開いているシーンの名前。None = まだ名前を付けて保存していない
    pub suggested_name: String,   // 「名前を付けて保存」の初期値
}
```

---

## 関数ごとの説明

### `open()`（29〜70行目）　起動時

```
1. scenes フォルダを作る（無ければ）
2. current.txt の名前を読む。そのシーンが無ければ、名前順で最初のシーン
3. どれも無い → 新しいシーン（部屋だけ）を作って default として保存
4. ある → 読み込む
   └ 読めなかった → 警告して空のシーン
```

### シーンの操作

| 関数 | 行 | 意味 |
|---|---|---|
| `untitled(name)` | 72〜77 | 名前の無いシーンの状態を作る |
| `open_scene(name, store)` | 80〜87 | シーンを読み、ファイルを store に入れ、current.txt を更新する |
| `save(layout, store)` | 90〜96 | 今の名前で上書き保存。名前が無ければエラー |
| `save_as(name, layout, store)` | 99〜106 | 名前を確かめてから保存し、今のシーンをその名前にする |
| `delete(name)` | 110〜118 | ファイルを削除する。今のシーンだったら名前無しにする |
| `reopen_last(store)` | 121〜134 | current.txt のシーンを開き直す（無ければ新規）。「保存せずに戻る」のときに使う |
| `remember_current()` | 136〜143 | 今のシーン名を current.txt に書く |

### 補助の関数

| 関数 | 行 | 意味 |
|---|---|---|
| `dir()` | 146〜148 | `scenes` フォルダのパス |
| `list()` | 151〜167 | フォルダの中の `.peek` の名前（拡張子なし）を名前順で返す。大文字小文字は区別せずに並べる |
| `exists(name)` | 169〜171 | その名前のシーンがあるか |
| `validate_name(name)` | 174〜189 | 名前が使えるか確かめる（空、長すぎ、使えない文字、先頭・末尾の「.」） |
| `unique_name(base)` | 192〜208 | 使えない文字を `_` にし、重なれば `_2`, `_3`… を付ける（インポート時） |
| `scene_path(name)` | 211〜213 | `scenes/<名前>.peek` |
| `load` / `write` | 215〜224 | [14 package](14_package.md) の読み書きを呼び、エラーメッセージを付ける |

---

## ポイント

- シーンの一覧は、どこかに記録しているのではなく、**毎回 scenes フォルダの中身を見て** 作っています。エクスプローラーで .peek ファイルを足したり消したりしても反映されます
- 「最後に開いたシーン」は、ホーム画面・編集モードのどちらで選んでも current.txt に記録され、本番モードと次回起動で使われます
