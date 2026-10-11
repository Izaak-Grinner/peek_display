# 19 scene/highlight.rs　選択中のオレンジの輪郭線

**一言で**: 編集モードで選んでいるオブジェクトを、Blender のようにオレンジの輪郭線で縁取る。
**場所**: `src/scene/highlight.rs`（93行）
**関係するファイル**: [18 editor](18_editor.md)（選択）、[08 scene_view](08_scene_view.md)（GIZMO_LAYER）

---

## 使っているライブラリ

`bevy_mod_outline`（Cargo.toml に書いてある外部ライブラリ）で輪郭線を描いています。

コメント（2行目）より、**jump flood** という方式で、画面上でモノの縁を探して描きます。
細いものや複雑な形でも、線の太さがそろうのが利点です。

---

## 定数

| 定数 | 値 | 意味 |
|---|---|---|
| `SELECTION_COLOR` | オレンジ (1.0, 0.55, 0.1) | 選択の色。ほかのファイルでも使う |
| `OUTLINE_WIDTH` | 3.0 | 輪郭線の太さ（ピクセル） |

---

## 関数ごとの説明

### `selected_entity`（31〜43行目）

編集モードでオブジェクトを選んでいれば、そのモノ（エンティティ）を返します。
スクリーン・ライトを選んでいるとき、編集モード以外では `None`。

### `update_outline`（47〜93行目）

`PostUpdate`（毎フレームの後のほう）で動きます。

```
1. 前に輪郭線を付けたモノで、今は選ばれていないもの
   → 輪郭線のシール（Outlined, OutlineVolume, OutlineMode, OutlineRenderLayers）を外す
   → 子孫の InheritOutline も外す
2. 選んでいるモノに、まだ付いていなければ輪郭線を付ける
3. 選んでいるモノの子孫全部に InheritOutline を付ける（親の輪郭線を引き継ぐ）
```

ポイント:
- `OutlineRenderLayers(RenderLayers::layer(GIZMO_LAYER))` … 輪郭線は補助線用のレイヤーに描くので、**シーンビューにだけ映り、本番の映像には映らない**
- モデルの中のメッシュは子孫なので、`InheritOutline` を付けないと輪郭線が出ない
- モデルは読み込みが終わってから子が増えるので、毎フレーム確認して付け直している
