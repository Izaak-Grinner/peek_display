use bevy::{
    camera::{CameraProjection, SubCameraView},
    prelude::*,
};

use crate::camera::inset::InsetCamera;
use crate::camera::main_camera::MainDisplayCamera;
use crate::tracking::tracking::HeadPose;

use crate::config::{AppConfig, DisplayInfo, OFF_AXIS_FAR, OFF_AXIS_NEAR};
use crate::scene::layout::SceneLayout;

#[derive(Component, Clone, Debug)]
pub struct OffAxisProjection {
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
    pub top: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for OffAxisProjection {
    fn default() -> Self {
        Self {
            left: -0.1,
            right: 0.1,
            bottom: -0.1,
            top: 0.1,
            near: OFF_AXIS_NEAR,
            far: OFF_AXIS_FAR,
        }
    }
}

impl CameraProjection for OffAxisProjection {
    fn get_clip_from_view(&self) -> Mat4 {
        //println!("get_clip_from_view");
        let l = self.left;
        let r = self.right;
        let b = self.bottom;
        let t = self.top;
        let n = self.near;
        let f = self.far;

        // Bevyの標準カメラはreverse-Z(手前=1, 奥=0)のwgpu深度規約を使うため、
        // それに合わせた非対称視錐台行列にする(OpenGL式のnear=-1,far=1ではない)。
        Mat4::from_cols(
            Vec4::new(2.0 * n / (r - l), 0.0, 0.0, 0.0),
            Vec4::new(0.0, 2.0 * n / (t - b), 0.0, 0.0),
            Vec4::new((r + l) / (r - l), (t + b) / (t - b), n / (f - n), -1.0),
            Vec4::new(0.0, 0.0, f * n / (f - n), 0.0),
        )
    }

    fn get_clip_from_view_for_sub(&self, _sub_view: &SubCameraView) -> Mat4 {
        self.get_clip_from_view()
    }

    fn update(&mut self, _width: f32, _height: f32) {}

    fn far(&self) -> f32 {
        self.far
    }

    fn get_frustum_corners(&self, _z_near: f32, _z_far: f32) -> [Vec3A; 8] {
        [Vec3A::ZERO; 8]
    }
}

pub struct OffAxisPlugin;

impl Plugin for OffAxisPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_off_axis_projection);
    }
}

fn update_off_axis_projection(
    pose: Res<HeadPose>,
    config: Res<AppConfig>,
    display: Res<DisplayInfo>,
    layout: Res<SceneLayout>,
    // 編集モードの小窓のカメラも、覗き込みカメラと同じように動かす
    mut query: Query<(&mut Transform, &mut Projection), Or<(With<MainDisplayCamera>, With<InsetCamera>)>>,
) {
    for (mut transform, mut projection) in &mut query {
        let Projection::Custom(custom) = projection.as_mut() else {
            continue;
        };
        let Some(projection) = custom.get_mut::<OffAxisProjection>() else {
            continue;
        };
        apply_off_axis(&pose, &config, &display, &layout, &mut transform, projection);
    }
}

fn apply_off_axis(
    pose: &HeadPose,
    config: &AppConfig,
    display: &DisplayInfo,
    layout: &SceneLayout,
    transform: &mut Transform,
    projection: &mut OffAxisProjection,
) {
    // 画面の大きさがシーンを作ったときと違えば、基準の画面の範囲が全部入るように縮尺を変える。
    // 窓の大きさと目の位置を同じ割合で変えるので、頭を動かしても景色は滑らない
    let k = display.view_scale(layout.reference);

    // OpenTrack(cm) -> m
    let ex = pose.x * 0.01 * k;
    let ey = pose.y * 0.01 * k;

    let ez = (config.viewer_base_distance - pose.z * 0.01).max(config.viewer_min_distance) * k;

    // 視錐台はディスプレイ基準の座標(ex, ey, ez)で計算し、
    // カメラ自体はのぞき窓の位置・向きに合わせて置く(カメラの向き = 窓の向き)
    let window = Transform::from_translation(layout.window.position)
        .with_rotation(layout.window.rotation());
    *transform = window * Transform::from_xyz(ex, ey, ez);

    let n = projection.near;
    let screen_width = display.size.x * k;
    let screen_height = display.size.y * k;

    projection.left = n * (-screen_width * 0.5 - ex) / ez;

    projection.right = n * (screen_width * 0.5 - ex) / ez;

    projection.bottom = n * (-screen_height * 0.5 - ey) / ez;

    projection.top = n * (screen_height * 0.5 - ey) / ez;

    //println!(
    //    "l={:.4}, r={:.4}, b={:.4}, t={:.4}",
    //    projection.left, projection.right, projection.bottom, projection.top
    //);
}
