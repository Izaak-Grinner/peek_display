use bevy::{
    camera::Viewport,
    prelude::*,
    window::{MonitorSelection, PrimaryWindow, WindowMode},
};

use crate::camera::{main_camera::MainDisplayCamera, preview::PreviewCamera};
use crate::config::DisplayInfo;

#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppMode {
    // モードを選ぶ画面 (背景に俯瞰カメラ)。起動したらここから始まる
    #[default]
    Home,
    // 展示用。覗き込みカメラだけを全画面表示する
    Production,
    // 左に俯瞰カメラ、右に覗き込みカメラ
    Inspection,
    // オブジェクトの配置と覗き込みカメラの設定
    Edit,
}

// 編集モードで UI が占める幅と高さ (論理ピクセル)。俯瞰カメラの描画範囲から除く
#[derive(Resource, Default)]
pub struct EditViewInsets {
    // 右側のパネルの幅
    pub right: f32,
    // 上のメニューバーの高さ
    pub top: f32,
}

pub struct AppModePlugin;

impl Plugin for AppModePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppMode>()
            .init_resource::<EditViewInsets>()
            .add_systems(
                Update,
                (
                    // 編集モードは未保存の確認があるので、編集パネル側で Esc を扱う
                    return_home_on_esc
                        .run_if(not(in_state(AppMode::Edit)).and(not(in_state(AppMode::Home)))),
                    update_camera_layout,
                ),
            )
            // 本番モードは全画面にする (映る範囲 = 画面全体にして、実物の大きさと合わせるため)
            .add_systems(OnEnter(AppMode::Production), |mut window: Single<&mut Window, With<PrimaryWindow>>| {
                window.mode = WindowMode::BorderlessFullscreen(MonitorSelection::Current);
            })
            .add_systems(OnExit(AppMode::Production), |mut window: Single<&mut Window, With<PrimaryWindow>>| {
                window.mode = WindowMode::Windowed;
            });
    }
}

fn return_home_on_esc(keyboard: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<AppMode>>) {
    if keyboard.just_pressed(KeyCode::Escape) {
        next.set(AppMode::Home);
    }
}

// モードとウィンドウの大きさに合わせて、各カメラの有効/無効と描画範囲を決める
// (毎フレーム計算し、変わったときだけ書き込む。ウィンドウのリサイズにも追従する)
fn update_camera_layout(
    mode: Res<State<AppMode>>,
    display: Res<DisplayInfo>,
    insets: Res<EditViewInsets>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut preview: Single<&mut Camera, (With<PreviewCamera>, Without<MainDisplayCamera>)>,
    mut main: Single<&mut Camera, (With<MainDisplayCamera>, Without<PreviewCamera>)>,
) {
    let size = UVec2::new(window.physical_width(), window.physical_height());
    if size.x == 0 || size.y == 0 {
        return; // 最小化中
    }

    let (preview_layout, main_layout) = match mode.get() {
        AppMode::Home => (Some(None), None),
        AppMode::Production => (None, Some(None)),
        AppMode::Inspection => {
            let half = size.x / 2;
            let left = viewport(UVec2::ZERO, UVec2::new(half, size.y));
            // 右半分の中に、ディスプレイと同じ縦横比の領域を中央寄せで取る
            // (半分の幅に引き伸ばすと、非対称視錐台の比率が実物とずれるため)
            let right = fit_aspect(
                UVec2::new(half, 0),
                UVec2::new(size.x - half, size.y),
                display.aspect(),
            );
            (Some(Some(left)), Some(Some(right)))
        }
        AppMode::Edit => {
            let right_px = (insets.right * window.scale_factor()) as u32;
            let top_px = ((insets.top * window.scale_factor()) as u32).min(size.y.saturating_sub(1));
            let width = size.x.saturating_sub(right_px).max(1);
            let height = size.y - top_px;
            (Some(Some(viewport(UVec2::new(0, top_px), UVec2::new(width, height)))), None)
        }
    };

    apply_layout(&mut preview, preview_layout);
    apply_layout(&mut main, main_layout);
}

// None: 無効 / Some(None): 全画面 / Some(Some(v)): v の範囲に描画
fn apply_layout(camera: &mut Camera, layout: Option<Option<Viewport>>) {
    let is_active = layout.is_some();
    let viewport = layout.flatten();
    if camera.is_active != is_active {
        camera.is_active = is_active;
    }
    let same = match (&camera.viewport, &viewport) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.physical_position == b.physical_position && a.physical_size == b.physical_size
        }
        _ => false,
    };
    if !same {
        camera.viewport = viewport;
    }
}

fn viewport(position: UVec2, size: UVec2) -> Viewport {
    Viewport {
        physical_position: position,
        physical_size: size.max(UVec2::ONE),
        ..default()
    }
}

fn fit_aspect(position: UVec2, area: UVec2, aspect: f32) -> Viewport {
    let area_f = area.as_vec2();
    let size = if area_f.x / area_f.y > aspect {
        Vec2::new(area_f.y * aspect, area_f.y)
    } else {
        Vec2::new(area_f.x, area_f.x / aspect)
    }
    .as_uvec2();
    viewport(position + (area - size.min(area)) / 2, size)
}
