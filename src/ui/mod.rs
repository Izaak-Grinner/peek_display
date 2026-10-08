use bevy::{
    camera::{CameraOutputMode, visibility::RenderLayers},
    prelude::*,
    render::render_resource::BlendState,
};
use bevy_egui::{
    EguiContexts, EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext, egui,
};
use std::sync::Arc;

use crate::app_mode::AppMode;

pub mod edit_panel;
pub mod home;
mod widgets;

// 日本語を表示するためのフォント (上から順に、見つかったものを使う)
const JAPANESE_FONTS: &[&str] = &[
    r"C:\Windows\Fonts\YuGothM.ttc",
    r"C:\Windows\Fonts\meiryo.ttc",
    r"C:\Windows\Fonts\msgothic.ttc",
];

// UI がマウス・キーボードを使っているか。3D 側の操作はこれを見て入力を無視する
#[derive(Resource, Default)]
pub struct UiInputState {
    pub pointer: bool,
    pub keyboard: bool,
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .init_resource::<UiInputState>()
            .add_systems(Startup, setup_ui_camera)
            .add_systems(
                EguiPrimaryContextPass,
                (
                    setup_fonts.run_if(run_once),
                    home::home_ui.run_if(in_state(AppMode::Home)),
                    inspection_overlay.run_if(in_state(AppMode::Inspection)),
                    edit_panel::edit_panel.run_if(in_state(AppMode::Edit)),
                    update_ui_input_state,
                )
                    .chain(),
            );
    }
}

// UI 専用カメラ。3D カメラの有効/無効と関係なく、常に一番上に UI を描く
fn setup_ui_camera(mut commands: Commands, mut egui_settings: ResMut<EguiGlobalSettings>) {
    egui_settings.auto_create_primary_context = false;
    commands.spawn((
        PrimaryEguiContext,
        Camera3d::default(),
        // シーンは何も映さない
        RenderLayers::none(),
        Camera {
            order: 10,
            output_mode: CameraOutputMode::Write {
                blend_state: Some(BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
    ));
}

fn setup_fonts(mut contexts: EguiContexts) -> Result {
    let ctx = contexts.ctx_mut()?;
    let Some(bytes) = JAPANESE_FONTS.iter().find_map(|path| std::fs::read(path).ok()) else {
        warn!("日本語フォントが見つからなかったため、UIの日本語が表示されない可能性があります");
        return Ok(());
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("japanese".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
    // 英数字も同じフォントで描く (別のフォントに任せると、日本語とベースラインがずれる)
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "japanese".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("japanese".to_owned());
    ctx.set_fonts(fonts);
    Ok(())
}

fn inspection_overlay(
    mut contexts: EguiContexts,
    pose: Res<crate::tracking::tracking::HeadPose>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    egui::Area::new(egui::Id::new("inspection_overlay"))
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(12.0, 12.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.strong("検査モード");
                ui.label("左: シーンビュー (右ドラッグ: 回転 / ホイール: ズーム / ホイールを押しながらドラッグ: 平行移動)");
                ui.label("右: メインカメラ");
                ui.label(format!(
                    "頭の位置 (cm): x {:.1}  y {:.1}  z {:.1}",
                    pose.x, pose.y, pose.z
                ));
                ui.label("Esc: ホームへ戻る");
            });
        });
    Ok(())
}

fn update_ui_input_state(mut contexts: EguiContexts, mut state: ResMut<UiInputState>) -> Result {
    let ctx = contexts.ctx_mut()?;
    state.pointer = ctx.wants_pointer_input() || ctx.is_pointer_over_area();
    state.keyboard = ctx.wants_keyboard_input();
    Ok(())
}
