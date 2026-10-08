use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::app_mode::AppMode;

const BUTTON_SIZE: egui::Vec2 = egui::vec2(320.0, 56.0);

pub fn home_ui(
    mut contexts: EguiContexts,
    mut next: ResMut<NextState<AppMode>>,
    mut exit: MessageWriter<AppExit>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.2).max(16.0));
                egui::Frame::popup(ui.style())
                    .inner_margin(24.0)
                    .show(ui, |ui| {
                        ui.set_width(BUTTON_SIZE.x);
                        ui.vertical_centered(|ui| {
                            ui.heading(egui::RichText::new("のぞけるディスプレイ").size(28.0));
                            ui.add_space(16.0);

                            let mut mode_button = |ui: &mut egui::Ui, label: &str, detail: &str, mode| {
                                let text = egui::RichText::new(label).size(20.0);
                                if ui.add_sized(BUTTON_SIZE, egui::Button::new(text)).clicked() {
                                    next.set(mode);
                                }
                                ui.label(detail);
                                ui.add_space(10.0);
                            };
                            mode_button(ui, "本番モード", "メインカメラを全画面で表示", AppMode::Production);
                            mode_button(ui, "検査モード", "シーンビューとメインカメラを並べて表示", AppMode::Inspection);
                            mode_button(ui, "編集モード", "オブジェクトとスクリーンの配置", AppMode::Edit);

                            ui.add_space(6.0);
                            if ui.button("終了").clicked() {
                                exit.write(AppExit::Success);
                            }
                            ui.add_space(6.0);
                            ui.small("どのモードからも Esc でこの画面に戻れます");
                        });
                    });
            });
        });
    Ok(())
}
