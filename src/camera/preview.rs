use crate::app_mode::AppMode;
use crate::camera::main_camera::MainDisplayCamera;
use crate::config::AppConfig;
use crate::scene::editor::{EditState, Selection};
use crate::scene::highlight::SELECTION_COLOR;
use crate::scene::layout::SceneLayout;
use crate::ui::UiInputState;
use bevy::{
    camera::visibility::RenderLayers,
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

// 座標軸ギズモをプレビューカメラだけに表示するための専用レイヤー
pub const GIZMO_LAYER: usize = 1;

// ホーム画面で背景の俯瞰カメラを自動で回す速さ(rad/s)
const HOME_ROTATE_SPEED: f32 = 0.1;

// 選択中ののぞき窓の縁取り。同じ形を太いオレンジ線で先に描き、その上に細い線を重ねる
#[derive(Default, Reflect, GizmoConfigGroup)]
struct SelectionOutlineGizmos;

// 縁取りの線の太さ (ピクセル)。通常の線 (2px) の両側に 3px ずつオレンジが見える
const OUTLINE_LINE_WIDTH: f32 = 8.0;

// ホイールを押しながらのドラッグで平行移動する量 (カメラまでの距離 1m あたり、1ピクセルで動く長さ)
const PAN_SPEED: f32 = 0.0015;

// 俯瞰プレビュー用カメラ（マウス操作可能）。focus を中心に回る
#[derive(Component)]
pub struct PreviewCamera {
    radius: f32,
    alpha: f32, // 横回転
    beta: f32,  // 縦回転
    // 見ている点 (回転・ズームの中心)
    focus: Vec3,
}

impl Default for PreviewCamera {
    fn default() -> Self {
        Self {
            radius: 4.0,
            alpha: 0.8,
            beta: 0.5,
            focus: Vec3::ZERO,
        }
    }
}

//プレビューカメラをプラグイン化
pub struct PreviewPlugin;

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_gizmo_group::<SelectionOutlineGizmos>()
            .add_systems(Startup, (setup_preview_camera, setup_gizmo_layer))
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

    // 縁取りは通常の線より少し奥に描き、通常の線が上に重なるようにする
    let (config, _) = config_store.config_mut::<SelectionOutlineGizmos>();
    config.render_layers = RenderLayers::layer(GIZMO_LAYER);
    config.line.width = OUTLINE_LINE_WIDTH;
    config.line.joints = GizmoLineJoint::Miter;
    config.depth_bias = 0.0005;
}

// 空間グリッド・軸線・カメラ視覚化ギズモの描画
fn draw_grid_and_gizmos(
    mut gizmos: Gizmos,
    mut outline: Gizmos<SelectionOutlineGizmos>,
    config: Res<AppConfig>,
    layout: Res<SceneLayout>,
    mode: Res<State<AppMode>>,
    state: Res<EditState>,
    main_cam_query: Query<&Transform, With<MainDisplayCamera>>,
) {
    //原点に XYZ 軸線を描く (R=X, G=Y, B=Z)
    gizmos.axes(Transform::IDENTITY, 5.0);

    // のぞき込みカメラの視覚化 (目の位置の球・視線・ディスプレイ枠)。
    // 座標軸 (赤・緑・青) や選択の色 (オレンジ) と見分けられるよう、マゼンタで描く。
    // のぞき窓を選んでいるときは、同じ形を太いオレンジ線でも描いて縁取る
    let camera_color = LinearRgba::new(1.0, 0.35, 0.85, 1.0);
    let selected = *mode.get() == AppMode::Edit && state.selection == Some(Selection::Window);
    let window_center = layout.window.position;
    if let Ok(cam_transform) = main_cam_query.single() {
        let cam_pos = cam_transform.translation;

        gizmos.sphere(cam_pos, 0.03, camera_color);
        gizmos.line(cam_pos, window_center, camera_color);
        if selected {
            outline.sphere(cam_pos, 0.03, SELECTION_COLOR);
            outline.line(cam_pos, window_center, SELECTION_COLOR);
        }
    }

    let (screen_width, screen_height) = config.screen_size();
    let frame = Isometry3d::new(window_center, layout.window.rotation());
    let frame_size = Vec2::new(screen_width, screen_height);
    gizmos.rect(frame, frame_size, camera_color);
    if selected {
        outline.rect(frame, frame_size, SELECTION_COLOR);
    }
}

// プレビューカメラのマウス操作 (Orbit Camera)
// カーソルがこのカメラの描画範囲内にあり、UIの上にないときだけ操作を受け付ける
fn control_preview_camera(
    mode: Res<State<AppMode>>,
    time: Res<Time>,
    ui_input: Res<UiInputState>,
    window: Single<(&mut Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut camera_query: Query<(&mut Transform, &mut PreviewCamera, &Camera)>,
    mut mouse_motion: MessageReader<MouseMotion>,
    mut mouse_wheel: MessageReader<MouseWheel>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    // ドラッグを始めたときのカーソルの位置と、押しているボタン (ドラッグ中だけ Some)
    mut drag: Local<Option<(Vec2, MouseButton)>>,
) {
    let (mut window, mut cursor) = window.into_inner();
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

    // 右ドラッグで視点回転、ホイールを押しながらのドラッグで平行移動。
    // ドラッグ中はカーソルを隠してその場に止め、離したら元の位置に戻す
    if drag.is_none() && accepts_input {
        let button = [MouseButton::Right, MouseButton::Middle]
            .into_iter()
            .find(|b| mouse_button.just_pressed(*b));
        if let (Some(button), Some(position)) = (button, window.cursor_position()) {
            *drag = Some((position, button));
            cursor.visible = false;
            cursor.grab_mode = CursorGrabMode::Locked;
        }
    }
    let dragging = drag
        .filter(|(_, button)| mouse_button.pressed(*button) && camera.is_active && *mode.get() != AppMode::Home)
        .map(|(_, button)| button);
    if dragging.is_none() {
        if let Some((position, _)) = drag.take() {
            window.set_cursor_position(Some(position));
            cursor.visible = true;
            cursor.grab_mode = CursorGrabMode::None;
        }
    }

    match dragging {
        Some(MouseButton::Right) => {
            for event in mouse_motion.read() {
                cam.alpha -= event.delta.x * 0.005;
                // 上下は制限せず、真上・真下を越えて回れる
                cam.beta = (cam.beta + event.delta.y * 0.005).rem_euclid(std::f32::consts::TAU);
            }
        }
        Some(_) => {
            // 画面をつかんで動かすように、マウスと同じ向きにシーンが動く
            let (right, up) = (transform.right(), transform.up());
            for event in mouse_motion.read() {
                let scale = cam.radius * PAN_SPEED;
                cam.focus += (-right * event.delta.x + up * event.delta.y) * scale;
            }
        }
        None => mouse_motion.clear(),
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

    // alpha (横回転) と beta (真上からの角度) から、カメラの向きを回転で求める。
    // look_at と違って真上・真下で向きが急に変わらず、越えた先では上下が逆さまになる
    let yaw = std::f32::consts::FRAC_PI_2 - cam.alpha;
    let elevation = std::f32::consts::FRAC_PI_2 - cam.beta;
    let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-elevation);
    transform.rotation = rotation;
    transform.translation = cam.focus + rotation * Vec3::new(0.0, 0.0, cam.radius);
}
