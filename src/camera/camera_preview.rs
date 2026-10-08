use bevy::{
    camera::RenderTarget,
    prelude::*,
    render::render_resource::{Extent3d, TextureFormat},
};

use crate::app_mode::AppMode;
use crate::camera::off_axis_projection::OffAxisProjection;

// 編集モードのカメラプレビューに出す、メインカメラと同じ見え方のカメラ。
// 画面に直接描くとシーンビューと描画先を共有して背景が混ざるので、画像に描いて UI で表示する。
// 位置と視錐台は off_axis_projection がメインカメラと同じように更新する
#[derive(Component)]
pub struct MainCameraPreview;

// カメラプレビューの表示状態と、描き込む画像
#[derive(Resource)]
pub struct CameraPreview {
    pub visible: bool,
    pub image: Handle<Image>,
    // 画像の大きさ (物理ピクセル)。UI がカメラプレビューの大きさに合わせて決める
    pub size: UVec2,
}

pub struct CameraPreviewPlugin;

impl Plugin for CameraPreviewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_preview_camera)
            .add_systems(Update, (update_preview_camera, resize_preview_image));
    }
}

fn setup_preview_camera(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let size = UVec2::new(16, 10);
    let image = images.add(Image::new_target_texture(
        size.x,
        size.y,
        TextureFormat::bevy_default(),
    ));
    commands.spawn((
        Camera3d::default(),
        Projection::custom(OffAxisProjection::default()),
        Camera {
            // 画面に描くカメラより先に描いておく
            order: -1,
            is_active: false,
            target: RenderTarget::Image(image.clone().into()),
            ..default()
        },
        Transform::default(),
        MainCameraPreview,
    ));
    commands.insert_resource(CameraPreview {
        visible: true,
        image,
        size,
    });
}

// 編集モードでカメラプレビューを表示しているときだけ描く
fn update_preview_camera(
    mode: Res<State<AppMode>>,
    preview: Res<CameraPreview>,
    mut camera: Single<&mut Camera, With<MainCameraPreview>>,
) {
    let active = *mode.get() == AppMode::Edit && preview.visible;
    if camera.is_active != active {
        camera.is_active = active;
    }
}

fn resize_preview_image(preview: Res<CameraPreview>, mut images: ResMut<Assets<Image>>) {
    if !preview.is_changed() {
        return;
    }
    let size = preview.size.max(UVec2::ONE);
    let Some(image) = images.get(&preview.image) else {
        return;
    };
    if image.size() == size {
        return;
    }
    if let Some(image) = images.get_mut(&preview.image) {
        image.resize(Extent3d {
            width: size.x,
            height: size.y,
            ..default()
        });
    }
}
