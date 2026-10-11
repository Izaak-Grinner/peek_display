# 04 config/display.rs　画面の実寸の計算

**一言で**: モニターの画素数と config.ini の値から、画面の実際の大きさ(m)を毎フレーム求める。
**場所**: `src/config/display.rs`（108行）
**関係するファイル**: [03 config](03_config.md)、[06 off_axis_projection](06_off_axis_projection.md)、[02 app_mode](02_app_mode.md)

---

## このファイルの役割

窓の見え方を正しく計算するには、**画面が実際に何cmあるか** が必要です。
このファイルはそれを `DisplayInfo`（リソース）として用意します。

```
モニターの画素数 (例 1920×1080) ──→ 比率 16:9 ──┐
                                                ├→ screen_size() → 幅 0.354m × 高さ 0.199m
config.ini (対角 16インチ or 幅 mm) ────────────┘
```

---

## `DisplayInfo`（9〜15行目）

```rust
pub struct DisplayInfo {
    pub pixels: UVec2,  // モニターの画素数
    pub size: Vec2,     // 画面の幅と高さ (m)
}
```

初期値（17〜24行目）は 1920×1080、16インチ。モニターの情報が取れるまでの仮の値です。

---

## 関数ごとの説明

### `aspect()`（27〜29行目）

幅 ÷ 高さ。[02 app_mode](02_app_mode.md) の検査モードで、右側の比率を合わせるのに使います。

### `view_scale(reference)`（33〜38行目）　★大きさの違う画面への対応

```rust
match reference {
    Some(r) if r.x > 0.0 && r.y > 0.0 => (r.x / self.size.x).max(r.y / self.size.y),
    _ => 1.0,
}
```

**背景**: シーンには「作ったときの画面の大きさ（基準の画面 `reference`）」が保存されています。
別の大きさの画面で開いたときに、作ったときに見えていた範囲が全部見えるよう縮尺を決めます。

| 状況 | 戻り値 | 見え方 |
|---|---|---|
| 基準が無い | 1.0 | 実物大 |
| 同じ大きさ | 1.0 | 実物大 |
| 基準が2倍大きい（大画面で作って小さい画面で見る） | 2.0 | シーンが半分の大きさに見える（全部入る） |
| 縦横比が違う | 幅の比と高さの比の **大きいほう** | 全部入る。上下か左右が少し余分に見える |

CSS の `object-fit: contain` と同じ考え方です。

使っている場所: [06 off_axis_projection](06_off_axis_projection.md)（見え方の計算）、[08 scene_view](08_scene_view.md)（灰色の枠）、[23 edit_panel](23_edit_panel.md)（「◯倍の大きさで見えます」の表示）。

### `update_display_info`（52〜74行目）　毎フレームの更新

`PreUpdate`（毎フレームの最初のほう）で動きます。

```
1. 画素数を取る
   ├ メインモニターの情報があればそれ
   ├ 無ければウィンドウの大きさ
   └ どちらも 0 なら何もしない
2. config の値と画素数から DisplayInfo を作る
3. 前と違うときだけ書き込む
```

> モニターの画素数を使っている（ウィンドウの大きさではない）ので、
> ウィンドウを小さくしても「画面の実寸」は変わりません。

---

## テスト（76〜108行目）

`cargo test` で動く確認用のコードです。読むと、関数の使い方の例にもなります。

- `size_from_diagonal_and_width` … 16インチ・16:10 → 幅 0.3446m × 高さ 0.2154m になるか。幅 300mm 指定が優先されるか
- `view_scale_contains_reference` … 上の表のパターンが正しいか

---

## 変更するときは

| やりたいこと | どこ |
|---|---|
| メイン以外のモニターで使う | `update_display_info` の `With<PrimaryMonitor>` |
| 縮尺の決め方を変える（例: はみ出してもいいので画面いっぱい） | `view_scale` の `.max` を `.min` にする |
