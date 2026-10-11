# 07 camera/main_camera.rs　本番のカメラ

**一言で**: 本番モード・検査モードで映すカメラを1台作るだけのファイル。
**場所**: `src/camera/main_camera.rs`（29行）
**関係するファイル**: [06 off_axis_projection](06_off_axis_projection.md)（位置と見える範囲を決める）、[02 app_mode](02_app_mode.md)（オン／オフを決める）

---

## コード

```rust
#[derive(Component)]
pub struct MainCamera;   // 「これがメインカメラ」という目印のシール（中身なし）
```

```rust
fn setup_main_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),                                  // 3Dを描くカメラ
        Projection::custom(OffAxisProjection::default()),     // 映し方は自作の「窓」
        Camera {
            order: 1,          // 描く順番。シーンビュー(0)より後
            is_active: false,  // 最初はオフ
            ..default()
        },
        Transform::default(),  // 位置は毎フレーム off_axis_projection が書き換える
        MainCamera,            // 目印
    ));
}
```

`Startup`（起動時に1回）で呼ばれます。

## ポイント

このファイル自体は「カメラを置く」だけです。カメラの動きはほかのファイルが担当しています。

| 何を | 誰が |
|---|---|
| どこに置くか・何が見えるか | [06 off_axis_projection](06_off_axis_projection.md) が毎フレーム |
| オン／オフ、画面のどこに描くか | [02 app_mode](02_app_mode.md) の `update_camera_layout` が毎フレーム |

`MainCamera` という目印があるので、ほかのファイルは `Query<..., With<MainCamera>>` で「メインカメラだけ」を探せます。

### カメラの `order`（描く順番）一覧

| order | カメラ | ファイル |
|---|---|---|
| -1 | 編集モードの小窓（画像に描く） | [09 camera_preview](09_camera_preview.md) |
| 0 | シーンビュー | [08 scene_view](08_scene_view.md) |
| 1 | メインカメラ | このファイル |
| 10 | UI 専用（ボタンやパネルを一番上に描く） | [20 ui/mod](20_ui_mod.md) |
