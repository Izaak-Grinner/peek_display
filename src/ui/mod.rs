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
pub mod problems;
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
            .init_resource::<problems::Problems>()
            .add_systems(Update, problems::collect_problems)
            .add_systems(Startup, setup_ui_camera)
            .add_systems(
                EguiPrimaryContextPass,
                (
                    setup_fonts.run_if(run_once),
                    home::home_ui.run_if(in_state(AppMode::Home)),
                    inspection_overlay.run_if(in_state(AppMode::Inspection)),
                    lost_border.run_if(in_state(AppMode::Production)),
                    edit_panel::edit_panel.run_if(in_state(AppMode::Edit)),
                    problems::problems_ui,
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
    stats: Res<crate::tracking::tracking::TrackingStats>,
    mut config: ResMut<crate::config::AppConfig>,
    mut edit_state: ResMut<crate::scene::editor::EditState>,
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
                ui.separator();
                ui.label(format!("アプリ: {:.0} fps", stats.fps));
                ui.label(format!("OpenTrack から: {:.0} 回/秒", stats.received_per_second))
                    .on_hover_text("OpenTrack はカメラとは関係なく一定の間隔で送ってくる。マーカーを測れていない間は同じ位置が届く");
                ui.label(format!("計測: {:.0} 回/秒", stats.measured_per_second))
                    .on_hover_text("位置が新しくなった回数 (カメラでマーカーを測れた回数)。カメラのフレームレートより少なければ、ときどき見失っている");
                if stats.lost {
                    ui.colored_label(egui::Color32::from_rgb(0xf0, 0xb0, 0x5a), "マーカー: 見失っている");
                } else {
                    ui.label("マーカー: 見えている");
                }
                ui.label(format!("見失った回数: {} 回", stats.lost_count))
                    .on_hover_text("OpenTrack から届いているのに、位置が変わらなくなった回数。速く動いて映像がぶれたり、カメラの範囲から出たりすると増える");
                ui.label(format!(
                    "動きをなめらかにする: {}",
                    if config.smooth_tracking { "オン" } else { "オフ" }
                ));
                match stats.last_received {
                    Some(at) => {
                        let ms = at.elapsed().as_millis();
                        let text = format!("最後に届いてから: {} ms", ms);
                        // 0.5 秒以上届いていなければ、OpenTrack のトラッキングが止まっている (マーカーを見失っても送り続けるので、それとは別)
                        if at.elapsed() >= crate::tracking::tracking::RECEIVING_TIMEOUT {
                            ui.colored_label(egui::Color32::from_rgb(0xf0, 0xb0, 0x5a), text);
                        } else {
                            ui.label(text);
                        }
                    }
                    None => {
                        ui.colored_label(egui::Color32::from_rgb(0xf0, 0xb0, 0x5a), "OpenTrack からまだ届いていません");
                    }
                }
                ui.separator();
                let mut show_window = !config.minimize_opentrack;
                if ui
                    .checkbox(&mut show_window, "OpenTrack の画面を出す")
                    .on_hover_text("マーカーの認識をカメラ映像で確かめたいときに出す。出すのは検査モードの間だけで、ほかのモードでは最小化する。切り替えると config.ini に保存し、OpenTrack を起動し直す")
                    .changed()
                {
                    edit_state.save_config_change(&mut config, |c| c.minimize_opentrack = !show_window);
                }
                ui.label("Esc: ホームへ戻る");
            });
        });
    Ok(())
}

// 本番モードでマーカーを見失っている間、画面のふちに薄い赤の線を出す (見ている人に、映像が止まっている理由を知らせる)
// 一瞬見失っただけでちらつかないよう、少しかけて出し入れする
fn lost_border(mut contexts: EguiContexts, stats: Res<crate::tracking::tracking::TrackingStats>) -> Result {
    const WIDTH: f32 = 6.0;
    const FADE_SECONDS: f32 = 0.2;
    let ctx = contexts.ctx_mut()?;
    let id = egui::Id::new("lost_border");
    let strength = ctx.animate_bool_with_time(id, stats.lost, FADE_SECONDS);
    if strength > 0.0 {
        let color = egui::Color32::from_rgba_unmultiplied(0xe0, 0x40, 0x40, (110.0 * strength) as u8);
        ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, id)).rect_stroke(
            ctx.content_rect(),
            0.0,
            egui::Stroke::new(WIDTH, color),
            egui::StrokeKind::Inside,
        );
    }
    Ok(())
}

fn update_ui_input_state(mut contexts: EguiContexts, mut state: ResMut<UiInputState>) -> Result {
    let ctx = contexts.ctx_mut()?;
    state.pointer = ctx.wants_pointer_input() || ctx.is_pointer_over_area();
    state.keyboard = ctx.wants_keyboard_input();
    Ok(())
}
