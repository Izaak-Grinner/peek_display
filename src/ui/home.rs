use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::app_mode::AppMode;
use crate::scene::editor::EditState;
use crate::scene::layout::SceneLayout;
use crate::scene::library::{self, SceneLibrary};
use crate::scene::package_store::PackageStore;

const BUTTON_SIZE: egui::Vec2 = egui::vec2(320.0, 56.0);

#[allow(clippy::too_many_arguments)]
pub fn home_ui(
    mut contexts: EguiContexts,
    mut next: ResMut<NextState<AppMode>>,
    mut exit: MessageWriter<AppExit>,
    mut library: ResMut<SceneLibrary>,
    mut layout: ResMut<SceneLayout>,
    mut store: ResMut<PackageStore>,
    mut state: ResMut<EditState>,
    // シーンを開けなかった理由
    mut open_error: Local<Option<String>>,
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

                            // 表示するシーン。選んだシーンを開き、どのモードでもそれを使う
                            // (ホームでは未保存の変更は無い。編集モードから戻るときに保存するか捨てている)
                            ui.horizontal(|ui| {
                                ui.label("シーン");
                                let current = library.current.clone();
                                egui::ComboBox::from_id_salt("home_scene")
                                    .width(ui.available_width())
                                    .selected_text(current.as_deref().unwrap_or("（未保存のシーン）"))
                                    .show_ui(ui, |ui| {
                                        for name in library::list() {
                                            let selected = current.as_deref() == Some(name.as_str());
                                            if ui.selectable_label(selected, &name).clicked() && !selected {
                                                match library.open_scene(&name, &mut store) {
                                                    Ok(loaded) => {
                                                        layout.replace_with(loaded);
                                                        state.scene_switched(&layout);
                                                        *open_error = None;
                                                    }
                                                    Err(e) => {
                                                        warn!("{}", e);
                                                        *open_error = Some(e);
                                                    }
                                                }
                                            }
                                        }
                                    });
                            });
                            if let Some(e) = open_error.as_ref() {
                                ui.colored_label(egui::Color32::from_rgb(0xff, 0x8f, 0x8f), e);
                            }
                            ui.add_space(12.0);

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
