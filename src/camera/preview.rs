use crate::app_mode::AppMode;
use crate::camera::main_camera::MainDisplayCamera;
use crate::config::AppConfig;
use crate::scene::layout::SceneLayout;
use crate::ui::UiInputState;
use bevy::{
    camera::visibility::RenderLayers,
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
    window::PrimaryWindow,
};

// 座標軸ギズモをプレビューカメラだけに表示するための専用レイヤー
pub const GIZMO_LAYER: usize = 1;

// ホーム画面で背景の俯瞰カメラを自動で回す速さ(rad/s)
const HOME_ROTATE_SPEED: f32 = 0.1;

// 俯瞰プレビュー用カメラ（マウス操作可能）
#[derive(Component)]
pub struct PreviewCamera {
    radius: f32,
    alpha: f32, // 横回転
    beta: f32,  // 縦回転
}

impl Default for PreviewCamera {
    fn default() -> Self {
        Self {
            radius: 4.0,
            alpha: 0.8,
            beta: 0.5,
        }
    }
}

//プレビューカメラをプラグイン化
pub struct PreviewPlugin;

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (setup_preview_camera, setup_gizmo_layer))
            .add_systems(Update, (control_preview_camera, draw_grid_and_gizmos));
    }
}

// 有効/無効と描画範囲は、モードに合わせて app_mode が切り替える
fn setup_preview_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 0,
            is_active: false,
            ..default()
        },
        Transform::from_xyz(2.0, 2.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
        PreviewCamera::default(),
        // 通常のシーン(レイヤー0)に加えて、ギズモ専用レイヤーも見えるようにする
        RenderLayers::from_layers(&[0, GIZMO_LAYER]),
    ));
}

// 座標軸ギズモをプレビューカメラ専用レイヤーに割り当てる
fn setup_gizmo_layer(mut config_store: ResMut<GizmoConfigStore>) {
    let (config, _) = config_store.config_mut::<DefaultGizmoConfigGroup>();
    config.render_layers = RenderLayers::layer(GIZMO_LAYER);
}

// 空間グリッド・軸線・カメラ視覚化ギズモの描画
fn draw_grid_and_gizmos(
    mut gizmos: Gizmos,
    config: Res<AppConfig>,
    layout: Res<SceneLayout>,
    main_cam_query: Query<&Transform, With<MainDisplayCamera>>,
) {
    //原点に XYZ 軸線を描く (R=X, G=Y, B=Z)
    gizmos.axes(Transform::IDENTITY, 5.0);

    //のぞき込みカメラの視覚化（緑の球・赤の視線線・青のディスプレイ枠）
    let window_center = layout.window.position;
    if let Ok(cam_transform) = main_cam_query.single() {
        let cam_pos = cam_transform.translation;

        gizmos.sphere(cam_pos, 0.03, LinearRgba::new(0.0, 1.0, 0.0, 1.0));
        gizmos.line(cam_pos, window_center, LinearRgba::new(1.0, 0.0, 0.0, 1.0));
    }

    let (screen_width, screen_height) = config.screen_size();
    gizmos.rect(
        Isometry3d::new(window_center, layout.window.rotation()),
        Vec2::new(screen_width, screen_height),
        LinearRgba::new(0.0, 0.0, 1.0, 1.0),
    );
}

// プレビューカメラのマウス操作 (Orbit Camera)
// カーソルがこのカメラの描画範囲内にあり、UIの上にないときだけ操作を受け付ける
fn control_preview_camera(
    mode: Res<State<AppMode>>,
    time: Res<Time>,
    ui_input: Res<UiInputState>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut camera_query: Query<(&mut Transform, &mut PreviewCamera, &Camera)>,
    mut mouse_motion: MessageReader<MouseMotion>,
    mut mouse_wheel: MessageReader<MouseWheel>,
    mouse_button: Res<ButtonInput<MouseButton>>,
) {
    let Ok((mut transform, mut cam, camera)) = camera_query.single_mut() else {
        return;
    };

    let hovered = window
        .cursor_position()
        .zip(camera.logical_viewport_rect())
        .is_some_and(|(cursor, rect)| rect.contains(cursor));
    let accepts_input =
        camera.is_active && hovered && !ui_input.pointer && *mode.get() != AppMode::Home;

    if *mode.get() == AppMode::Home {
        cam.alpha += HOME_ROTATE_SPEED * time.delta_secs();
    }

    // 右ドラッグで視点回転
    if accepts_input && mouse_button.pressed(MouseButton::Right) {
        for event in mouse_motion.read() {
            cam.alpha -= event.delta.x * 0.005;
            cam.beta += event.delta.y * 0.005;
            cam.beta = cam.beta.clamp(0.1, std::f32::consts::FRAC_PI_2 - 0.1);
        }
    } else {
        mouse_motion.clear();
    }

    // ホイールでズーム
    if accepts_input {
        for event in mouse_wheel.read() {
            cam.radius -= event.y * 0.3;
            cam.radius = cam.radius.clamp(0.5, 20.0);
        }
    } else {
        mouse_wheel.clear();
    }

    // 球座標系変換
    let x = cam.radius * cam.beta.sin() * cam.alpha.cos();
    let y = cam.radius * cam.beta.sin() * cam.alpha.sin(); // Y/Zの軸方向に合わせた計算
    let z = cam.radius * cam.beta.cos();

    transform.translation = Vec3::new(x, z, y);
    transform.look_at(Vec3::ZERO, Vec3::Y);
}
