# 13 scene/package_store.rs　シーンの中のファイルをメモリに置く

**一言で**: 今開いているシーン（.peek）に入っているモデルファイルの中身をメモリに置き、Bevy がそこから読み込めるようにする。
**場所**: `src/scene/package_store.rs`（91行）
**関係するファイル**: [01 main](01_main.md)（登録）、[11 scene](11_scene.md)（読み込み）、[14 package](14_package.md)・[15 library](15_library.md)（保存・開く）、[17 import](17_import.md)（取り込み）

---

## なぜ必要か

Bevy は普通、`assets/` フォルダのファイルを読み込みます。
しかしこのアプリでは、モデルは `.peek`（zip）の **中** に入っていて、フォルダに展開していません。

そこで、
1. .peek から取り出したファイルの中身をメモリに置き、
2. Bevy に「`scene://` で始まるパスは、このメモリから読んで」と登録する

ことで、ファイルを展開せずにモデルを表示しています。

```
.peek (zip)                       メモリ (PackageStore)                   Bevy
├ scene.ini                       
├ models/room/room.gltf   ──→    3/models/room/room.gltf   ←── scene://3/models/room/room.gltf
└ models/room/room.bin    ──→    3/models/room/room.bin
```

---

## `PackageStore`（17〜23行目）

```rust
pub struct PackageStore {
    dir: Dir,                          // Bevy が読みに来るメモリ上のフォルダ
    generation: u64,                   // 何回目に開いたシーンか（世代番号）
    files: BTreeMap<String, Vec<u8>>,  // パス → 中身（保存するときはここから書く）
}
```

---

## 関数ごとの説明

### `register(app)`（27〜43行目）

`scene` という名前の読み込み元を Bevy に登録します。中身は `MemoryAssetReader`（メモリから読む仕組み）です。
**DefaultPlugins より前** に呼ぶ必要があるので、[01 main.rs](01_main.md) で最初に呼ばれています。

### `asset_path(rel)`（46〜48行目）

```rust
format!("{}://{}/{}", SOURCE, self.generation, rel)
// 例: "scene://3/models/room/room.gltf"
```

シーンの中のパスを、Bevy で読み込むときのパスに変えます。

### `replace(files)`（55〜65行目）　別のシーンに入れ替える

```
1. 前のシーンのファイルをメモリから消す
2. 世代番号を +1 する
3. 新しいファイルを「新しい世代番号/パス」に置く
```

> **世代番号の意味**: Bevy は一度読み込んだファイルをパスで覚えておきます（キャッシュ）。
> シーンAにもシーンBにも `models/room/room.gltf` があると、Bevy はAのを使い回してしまいます。
> 開くたびに番号を変えて `scene://3/...` → `scene://4/...` とすることで、別のファイルとして読み直させています。

### `insert(rel, bytes)`（68〜71行目）

ファイルを1つ足します（モデルを取り込んだとき）。

### `unique_folder(dir, name)`（74〜86行目）

`models/cat` が使われていたら `models/cat_2`、それも使われていたら `models/cat_3`…と、空いているフォルダ名を返します。
同じ名前のモデルを2回取り込んでも上書きしないためです。
