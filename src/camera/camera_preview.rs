use bevy::{
    camera::RenderTarget,
    prelude::*,
    render::render_resource::{Extent3d, TextureFormat},
};

use crate::app_mode::AppMode;
use crate::camera::off_axis_projection::OffAxisProjection;

// 編集モードで小窓に出す、覗き込みカメラと同じ見え方のカメラ。
// 画面に直接描くと俯瞰カメラと描画先を共有して背景が混ざるので、画像に描いて UI で表示する。
// 位置と視錐台は off_axis_projection が覗き込みカメラと同じように更新する
#[derive(Component)]
pub struct InsetCamera;

// 小窓の表示状態と、描き込む画像
#[derive(Resource)]
pub struct MainCameraInset {
    pub visible: bool,
    pub image: Handle<Image>,
    // 画像の大きさ (物理ピクセル)。UI が小窓の大きさに合わせて決める
    pub size: UVec2,
}

pub struct InsetPlugin;

impl Plugin for InsetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_inset_camera)
            .add_systems(Update, (update_inset_camera, resize_inset_image));
    }
}

fn setup_inset_camera(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
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
        InsetCamera,
    ));
    commands.insert_resource(MainCameraInset {
        visible: true,
        image,
        size,
    });
}

// 編集モードで小窓を表示しているときだけ描く
fn update_inset_camera(
    mode: Res<State<AppMode>>,
    inset: Res<MainCameraInset>,
    mut camera: Single<&mut Camera, With<InsetCamera>>,
) {
    let active = *mode.get() == AppMode::Edit && inset.visible;
    if camera.is_active != active {
        camera.is_active = active;
    }
}

fn resize_inset_image(inset: Res<MainCameraInset>, mut images: ResMut<Assets<Image>>) {
    if !inset.is_changed() {
        return;
    }
    let size = inset.size.max(UVec2::ONE);
    let Some(image) = images.get(&inset.image) else {
        return;
    };
    if image.size() == size {
        return;
    }
    if let Some(image) = images.get_mut(&inset.image) {
        image.resize(Extent3d {
            width: size.x,
            height: size.y,
            ..default()
        });
    }
}
