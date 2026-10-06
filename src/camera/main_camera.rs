use bevy::prelude::*;

use crate::camera::off_axis_projection::OffAxisProjection;

#[derive(Component)]
pub struct MainDisplayCamera;

pub struct MainCameraPlugin;

impl Plugin for MainCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_main_camera);
    }
}

// 有効/無効と描画範囲は、モードに合わせて app_mode が切り替える
fn setup_main_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Projection::custom(OffAxisProjection::default()),
        Camera {
            order: 1,
            is_active: false,
            ..default()
        },
        Transform::default(),
        MainDisplayCamera,
    ));
}
