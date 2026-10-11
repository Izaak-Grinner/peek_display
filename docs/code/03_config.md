# 03 config/config.rs　config.ini の読み書き

**一言で**: このPCと設置場所の設定（機材プロファイル）を `config.ini` から読み、書き戻す。
**場所**: `src/config/config.rs`（185行）
**関係するファイル**: [04 display](04_display.md)、[05 tracking](05_tracking.md)、[18 editor](18_editor.md)（保存）、[23 edit_panel](23_edit_panel.md)（画面から変更）

---

## このファイルの役割

- `config.ini` を読んで `AppConfig`（リソース）を作る
- 編集モードで保存したときに `config.ini` を書き直す
- 設定値から計算する便利な関数（画面の実寸、マーカーの補正倍率）を持つ

---

## 定数（6〜14行目）

```rust
pub const OFF_AXIS_NEAR: f32 = 0.01;  // メインカメラが描く一番手前の距離 (m)
pub const OFF_AXIS_FAR: f32 = 10.0;   // メインカメラが描く一番奥の距離 (m)
const INCHES_TO_METERS: f32 = 0.0254; // 1インチ = 0.0254m
const CONFIG_FILE_NAME: &str = "config.ini";
pub const ARUCO_MARKER_SIZE_CM: f32 = 4.0;
```

| 定数 | 意味 |
|---|---|
| `OFF_AXIS_NEAR` / `OFF_AXIS_FAR` | メインカメラは **目から 1cm〜10m の範囲** だけを描く。10mより奥のモデルは映らない |
| `ARUCO_MARKER_SIZE_CM` | OpenTrack はマーカーの一辺を 4cm と決め打ちで距離を出している（OpenTrack 側で変えられない） |

> `config.ini` はパスなしで書かれているので、**アプリを起動したフォルダ（カレントディレクトリ）** から読み書きされます。

---

## `AppConfig`（19〜33行目）

```rust
#[derive(Resource, Clone, PartialEq)]
pub struct AppConfig {
    pub screen_diagonal_inches: f32,  // 画面の対角(インチ)
    pub screen_width_mm: f32,         // 測った画面の幅(mm)。0より大きければこちらを優先
    pub viewer_base_distance: f32,    // センター合わせしたときの目と画面の距離(m)
    pub viewer_min_distance: f32,     // これ以上近づかないとみなす距離(m)
    pub opentrack_path: String,       // OpenTrack の場所
    pub minimize_opentrack: bool,     // OpenTrack をタスクトレイに隠すか
    pub camera_fov_deg: f32,          // カメラの視野角(度)
    pub marker_size_cm: f32,          // 印刷したマーカーの一辺(cm)
}
```

- `Resource` … アプリ全体で1つのデータとして使える
- `Clone` … コピーできる（元に戻す機能で、変更前の状態をとっておくのに使う）
- `PartialEq` … `==` で比べられる（「保存していない変更があるか」の判定に使う）

### 初期値 `Default`（35〜48行目）

config.ini が無いときや、値が読めなかったときに使われる値です。

| 項目 | 初期値 |
|---|---|
| screen_diagonal_inches | 16.0 |
| screen_width_mm | 0.0（＝対角を使う） |
| viewer_base_distance | 0.5 |
| viewer_min_distance | 0.05 |
| opentrack_path | `C:\Program Files (x86)\opentrack\opentrack.exe` |
| minimize_opentrack | true |
| camera_fov_deg | 56.0 |
| marker_size_cm | 4.0 |

---

## 関数ごとの説明

### `screen_size(aspect)`（52〜61行目）　画面の幅と高さ(m)を出す

引数 `aspect` は「幅 ÷ 高さ」（16:9 なら 1.777…）。モニターの画素数から求めたものが渡されます。

```rust
if self.screen_width_mm > 0.0 {
    // 幅が測ってあれば: 幅 = 測った値、高さ = 幅 ÷ 比率
    let width = self.screen_width_mm / 1000.0;
    return Vec2::new(width, width / aspect);
}
// 測っていなければ対角から: 三平方の定理で高さを出す
let diagonal_m = self.screen_diagonal_inches * INCHES_TO_METERS;
let height = diagonal_m / (aspect * aspect + 1.0).sqrt();
Vec2::new(height * aspect, height)
```

対角からの計算の中身:
対角² = 幅² + 高さ² = (比率 × 高さ)² + 高さ² = 高さ² × (比率² + 1)
→ 高さ = 対角 ÷ √(比率² + 1)

### `marker_scale()`（64〜66行目）　マーカーの補正倍率

```rust
self.marker_size_cm / ARUCO_MARKER_SIZE_CM
```

例: 8cm で印刷した → 8 ÷ 4 = **2倍**。
OpenTrack は 4cm だと思って距離を計算するので、実際より半分の距離だと報告してきます。
それを2倍に直して使います（使っているのは [05 tracking](05_tracking.md) の `receive_udp`）。

### `same_opentrack_settings(other)`（69〜72行目）

OpenTrack に書き込む値（視野角と、トレイに隠すか）が同じかを比べます。
違えば OpenTrack を起動し直します（[05 tracking](05_tracking.md) の `restart_opentrack_on_change`）。

> 視野角は `round()`（四捨五入）してから比べています。OpenTrack には整数で書くので、56.2 と 56.4 は同じ扱いです。

### `load()`（74〜136行目）　config.ini を読む

```
1. 初期値を用意する
2. config.ini を読む
   └ 読めなかったら（ファイルが無い）→ 初期値で config.ini を作って、初期値を返す
3. 1行ずつ見る
   ├ 空行、# か ; で始まる行（コメント）→ 飛ばす
   ├ = が無い行 → 飛ばす
   └ 「キー=値」の行 → キーに応じて値を入れる
       ├ 数値が読めない → 警告を出して、その項目は初期値のまま
       └ 知らないキー → 警告を出して無視
```

`parse_or_warn(...)` が「読めなかったら警告して初期値を使う」部分です。

### `save()`（139〜174行目）　config.ini を書く

`format!` で、説明コメント付きの config.ini 全体を文字列として作り、ファイルに上書きします。

> **注意**: 自分で config.ini に書いたコメントは、アプリから保存すると消えます（毎回テンプレートで書き直すため）。

### `parse_or_warn`（177〜185行目）

文字列を数値などに変換し、できなければ警告ログを出して `fallback`（代わりの値）を返します。
シーンファイルの読み込み（[10 layout](10_layout.md)）でも使い回しています。

`<T: FromStr + Display>` は「文字列から作れて、表示もできる型なら何でも」という意味で、`f32` でも `bool` でも使えます。

---

## 変更するときは

| やりたいこと | どうする |
|---|---|
| config.ini に項目を足す | ① `AppConfig` に追加 ② `Default` に初期値 ③ `load()` の `match key` に追加 ④ `save()` の文字列に追加 ⑤ 編集パネルに入力欄を追加（[23 edit_panel](23_edit_panel.md) の `screen_properties`） |
| 遠くのモデルが映らない | `OFF_AXIS_FAR` を大きくする |
| config.ini の場所を変える | `CONFIG_FILE_NAME` |
