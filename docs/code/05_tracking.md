# 05 tracking/tracking.rs　OpenTrack と頭の位置

**一言で**: OpenTrack を設定・起動・終了し、OpenTrack から送られてくる頭の位置を受け取って `HeadPose` に入れる。
**場所**: `src/tracking/tracking.rs`（439行）
**関係するファイル**: [03 config](03_config.md)、[06 off_axis_projection](06_off_axis_projection.md)（HeadPose を使う）、[18 editor](18_editor.md)（保存済みの設定）

---

## 全体の流れ

```
[起動時]  launch_opentrack
            └ start_opentrack
                ├ ensure_opentrack_profile … OpenTrack の設定ファイル display.ini を書き換える
                ├ OpenTrack を起動する
                └ AutoStart を作る（F12 を押す係を始める）

[毎フレーム]
  receive_udp           … UDP で届いた頭の位置を読む → HeadPose
                          （届いたら AutoStart.started = true）
  auto_start_tracking   … 届くまで 2秒おきに F12 を押す（最大30秒）
  restart_opentrack_on_change … 視野角などが保存で変わったら起動し直す

[終了時]  close_opentrack_on_exit … OpenTrack も終了させる
```

---

## 定数（10〜26行目）

| 定数 | 値 | 意味 |
|---|---|---|
| `OPENTRACK_DEFAULT_INI` | `assets/opentrack_default.ini` の中身 | `include_str!` で **ビルド時に exe に埋め込む**。実行時に assets フォルダは不要 |
| `START_KEY_NAME` | `"F12"` | OpenTrack の「トラッキング開始」キー（設定ファイルに書く名前） |
| `START_KEY_SCANCODE` | `0x58` | F12 キーのスキャンコード（キーを押すまねをするときの番号） |
| `AUTO_START_FIRST_DELAY` | 3秒 | OpenTrack を起動してから最初に F12 を押すまで |
| `AUTO_START_RETRY_INTERVAL` | 2秒 | 押し直す間隔 |
| `AUTO_START_TIMEOUT` | 30秒 | あきらめるまで |

`opentrack_profile_path()`（23〜26行目）: `C:\Users\<ユーザー名>\Documents\opentrack-2.3\display.ini` の場所を返します。

---

## OpenTrack の設定を書き換える部分

### `FIXED_PROFILE`（30〜51行目）　アプリが必ず書き込む設定

```rust
("modules", [("tracker-dll", "aruco"), ("protocol-dll", "udp")]),
```

| セクション | キー | 値 | 意味 |
|---|---|---|---|
| modules | tracker-dll | aruco | ARマーカーで追跡する |
| modules | protocol-dll | udp | 結果を UDP で送る |
| udp-proto | ip1〜ip4 | 127.0.0.1 | 送り先はこのPC自身 |
| opentrack-ui | keycode-center | C | C キーでセンター合わせ |
| opentrack-ui | keycode-start-tracking | F12 | F12 でトラッキング開始 |
| opentrack-ui | y-invert-sign | false | |
| opentrack-mappings | x/y/z-max-value | 75 | 動きの範囲 ±75cm |
| opentrack-mappings | x/y-max-output-value | -75 | x と y の向きを反転して出力 |

### `ensure_opentrack_profile(config)`（55〜91行目）

```
1. display.ini が無ければ、埋め込んだ初期値で作る（write_default_profile）
2. display.ini を読む
3. FIXED_PROFILE の値を書き込む
4. config の「トレイに隠すか」を書き込む（use-system-tray, start-in-tray）
5. config の視野角を整数にして書き込む（aruco-tracker の field-of-view）
6. 元と同じなら書かない。違えば保存する
```

> **ほかの項目は残す**: OpenTrack の画面で選んだカメラなどは、そのまま残ります。

84行目の `.replace("\r\n", "\n")` は、Windows の改行（CRLF）と Unix の改行（LF）の違いを無視して比べるためです。

### `set_profile_settings(text, section, settings)`（115〜166行目）　INI の書き換え

INI ファイルの文字列を受け取り、指定した `[section]` の指定したキーを書き換えた文字列を返します。

```
1行ずつ見ていく:
├ [xxx] の行（セクションの始まり）
│   ├ 直前まで目的のセクションだった → まだ出てきていないキーをここに足す
│   └ 目的のセクションか覚えておく
├ 目的のセクションの中で、書き換えたいキーの行 → 新しい値で置き換える
└ それ以外 → そのまま
最後:
├ 目的のセクションが一度も無かった → ファイルの最後に [section] を足す
└ まだ足していないキーを足す
```

`push_missing` は、セクションの最後の空行より前にキーを入れる工夫をしています（見た目が崩れないように）。

ファイルの最後のテスト（411〜439行目）で、この関数の動きが4パターン確認されています。

---

## OpenTrack の起動と終了

### `OpentrackProcess`（260〜281行目）

起動した OpenTrack のプロセスを持っておくリソースです。

```rust
impl Drop for OpentrackProcess {
    fn drop(&mut self) { self.stop(); }
}
```

`Drop` は「このデータが消えるときに自動で呼ばれる処理」です。
リソースが消える（アプリ終了、起動し直し）と、自動で OpenTrack も終了します。

`stop()` は OpenTrack を強制終了（`kill`）し、終わるまで待ちます（`wait`）。
待つ理由: 起動し直すとき、前の OpenTrack がカメラを手放す前に次を起動すると、カメラを開けないためです。

> 残ったままだと次回起動時に OpenTrack が二重になり、カメラを開けなくなる、とコメントにあります。

### `close_opentrack_on_exit`（287〜291行目）

アプリ終了のメッセージ（`AppExit`）が来たら、`OpentrackProcess` を取り除きます（→ `Drop` で OpenTrack が終わる）。
`Last`（毎フレームの最後）で動きます。

### `start_opentrack(commands, config)`（316〜343行目）

```
1. 起動時の設定を OpentrackLaunched として覚える
2. display.ini を書き換える（ensure_opentrack_profile）
3. OpenTrack を起動する
   ├ 成功 → OpentrackProcess と AutoStart を作る
   └ 失敗（パスが違うなど）→ 警告を出すだけ（アプリは動き続ける）
```

`.stdin(Stdio::null())` などは、OpenTrack にこのアプリの入出力を引き継がせない設定です
（引き継ぐと、デバッガがアプリの終了を待ち続ける問題があったため）。

### `restart_opentrack_on_change`（299〜314行目）

```rust
let saved = state.saved_config();   // 最後に「保存」した設定
if launched.is_none_or(|l| l.0.same_opentrack_settings(saved)) {
    return;  // 変わっていなければ何もしない
}
```

**保存した値** で比べているのがポイントです。編集中に視野角の数値をドラッグするたびに再起動しないよう、「保存」を押したときだけ反映されます。

> 比べているのは視野角とトレイ設定だけです。**`opentrack_path` を変えた場合は、アプリを起動し直すまで反映されません。**

---

## トラッキングの自動開始

### `AutoStart`（169〜174行目）

```rust
struct AutoStart {
    launched_at: Instant,  // OpenTrack を起動した時刻
    next_press: Instant,   // 次に F12 を押す時刻
    started: bool,         // データが届いたか
}
```

### `auto_start_tracking`（345〜365行目）

`AutoStart` があるときだけ動きます（`run_if(resource_exists::<AutoStart>)`）。

```
├ データが届いた（started）→ 「開始されました」と出して AutoStart を消す（終了）
├ 30秒たった → 「自動で開始できませんでした。Start を押してください」と出して終了
└ 押す時刻になった → F12 を押す。次は2秒後
```

トラッキング中に F12 を押しても OpenTrack は無視するので、押しすぎても安全です。

### `press_start_key()`（178〜211行目）　F12 を押すまね

Windows の `SendInput` で、キーボードの入力を作って送ります。

- 押す → 100ミリ秒待つ → 離す
- 待っている間アプリが止まらないよう、別スレッド（`std::thread::spawn`）で行う
- OpenTrack はキーをスキャンコードで見ているので、スキャンコード指定（`KEYEVENTF_SCANCODE`）で送る
- `unsafe` は Windows の機能を直接呼ぶために必要な印

`#[cfg(not(windows))]` の版は何もしません（Windows 以外でもコンパイルできるようにするため）。

> **注意**: キーを押すまねなので、そのとき前面にあるウィンドウにも F12 が届きます。

---

## 頭の位置の受け取り

### `HeadPose`（218〜227行目）

```rust
pub struct HeadPose {
    pub x: f32,  // 左右 (cm)
    pub y: f32,  // 上下 (cm)
    pub z: f32,  // 前後 (cm)
    // yaw, pitch, roll（頭の向き）はコメントアウトされていて使っていない
}
```

窓の見え方には **目の位置だけ** が必要で、頭の向きは関係ないため使っていません。

### `TrackingPlugin::build`（231〜256行目）

```rust
match UdpSocket::bind("127.0.0.1:4242") { ... }
```

このPCの 4242 番ポートで UDP を受け取る準備をします。
`set_nonblocking(true)` で、データが無いときに待たずにすぐ戻るようにしています（待つと画面が止まる）。

> ポートが使われていて失敗した場合（アプリを2つ起動したなど）は、エラーを出して、頭の位置なしで動きます。

### `receive_udp`（367〜409行目）

`PreUpdate`（毎フレームの最初のほう）で動きます。

```rust
// 溜まっているデータを全部読み、最後（最新）の1つだけ使う
while let Ok((len, _)) = receiver.socket.recv_from(&mut buf) {
    if len == 48 { latest_data = Some(buf); }
}
```

OpenTrack の UDP のデータは **48バイト = 8バイトの小数(f64) × 6個**（x, y, z, yaw, pitch, roll）です。

```rust
let [x, y, z] = std::array::from_fn(|i| {
    let bytes = buf[i * 8..(i + 1) * 8].try_into().unwrap();  // 8バイトずつ切り出す
    f64::from_le_bytes(bytes) as f32                          // 小数に変換
});
```

最後に、マーカーの補正倍率を掛けて `HeadPose` に入れます。

```rust
let k = config.marker_scale();   // 例: 8cm で印刷したら 2.0
*pose = HeadPose { x: -x * k, y: -y * k, z: z * k };
```

x と y の符号を反転しているのは、OpenTrack の座標の向きとアプリの座標の向きを合わせるためです。

---

## 変更するときは

| やりたいこと | どこ |
|---|---|
| OpenTrack に書き込む設定を増やす | `FIXED_PROFILE` に追加 |
| 自動開始の待ち時間を変える | `AUTO_START_*` の定数 |
| 頭の向き（yaw など）も使う | `HeadPose` と `receive_udp` のコメントアウトを戻す |
| 頭の動きが逆向き | `receive_udp` の `-x`, `-y` の符号 |
| UDP のポートを変える | `TrackingPlugin::build` の `4242` と、OpenTrack 側の設定 |
