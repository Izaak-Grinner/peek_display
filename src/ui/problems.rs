// 起きた問題を画面に出す。
// ログのうち、このアプリの警告とエラーを拾って、画面の上に並べる (閉じるまで残す)。
// Bevy や wgpu などのログは、使う人には関係ないことが多いので拾わない (動かなくなるものは異常終了のダイアログで出る)。
// 異常終了 (panic) したときは、理由をダイアログで出す (配布用ビルドにはログを流すコンソールが無いため)
use bevy::log::{
    BoxedLayer,
    tracing::{
        Event, Level, Subscriber,
        field::{Field, Visit},
    },
    tracing_subscriber::{Layer, layer::Context},
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use std::sync::Mutex;

use super::widgets::{self, Icon};

// 一度に出す問題の数 (それより多いときは「ほか N 件」とまとめる)
const MAX_SHOWN: usize = 5;
const CARD_WIDTH: f32 = 520.0;

// ログから拾って、まだ画面に渡していない問題 (ログはどのスレッドからも来るので、ここに貯める)
static PENDING: Mutex<Vec<String>> = Mutex::new(Vec::new());

// 画面に出している問題
#[derive(Resource, Default)]
pub struct Problems(Vec<Problem>);

struct Problem {
    message: String,
    // 同じ問題が起きた回数
    count: u32,
}

// LogPlugin に足す層。警告とエラーを PENDING に入れる
pub fn log_layer(_app: &mut App) -> Option<BoxedLayer> {
    Some(Box::new(ProblemLayer))
}

struct ProblemLayer;

impl<S: Subscriber> Layer<S> for ProblemLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let ours = metadata.target().starts_with(env!("CARGO_CRATE_NAME"));
        let level = *metadata.level();
        if !ours || (level != Level::ERROR && level != Level::WARN) {
            return;
        }
        let mut message = MessageVisitor(String::new());
        event.record(&mut message);
        if let Ok(mut pending) = PENDING.lock() {
            pending.push(message.0);
        }
    }
}

struct MessageVisitor(String);

impl Visit for MessageVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.0 = value.to_string();
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{:?}", value);
        }
    }
}

// 貯まった問題を画面に渡す。同じ問題は1つにまとめて回数を数える
pub fn collect_problems(mut problems: ResMut<Problems>) {
    let Ok(mut pending) = PENDING.lock() else {
        return;
    };
    if pending.is_empty() {
        return;
    }
    for message in pending.drain(..) {
        match problems.0.iter_mut().find(|p| p.message == message) {
            Some(problem) => problem.count += 1,
            None => problems.0.push(Problem { message, count: 1 }),
        }
    }
}

// 問題を画面の上の中央に並べる (どのモードでも出す)
pub fn problems_ui(mut contexts: EguiContexts, mut problems: ResMut<Problems>) -> Result {
    if problems.0.is_empty() {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let mut close = None;
    let mut close_all = false;
    egui::Area::new(egui::Id::new("problems"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 40.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(0x2e, 0x1d, 0x1d))
                .stroke(widgets::hairline(egui::Color32::from_rgb(0x6a, 0x3a, 0x3a)))
                .corner_radius(6)
                .inner_margin(egui::Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    ui.set_width(CARD_WIDTH);
                    ui.style_mut().interaction.selectable_labels = true;
                    let accent = egui::Color32::from_rgb(0xff, 0x8f, 0x8f);
                    ui.horizontal(|ui| {
                        widgets::icon(ui, Icon::Warning, 14.0, accent, 1.6);
                        ui.label(egui::RichText::new("問題が起きました").size(13.0).color(accent));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if problems.0.len() > 1 && ui.small_button("すべて閉じる").clicked() {
                                close_all = true;
                            }
                        });
                    });
                    // 新しいものから出す
                    let start = problems.0.len().saturating_sub(MAX_SHOWN);
                    for (i, problem) in problems.0.iter().enumerate().skip(start).rev() {
                        ui.separator();
                        ui.horizontal_top(|ui| {
                            let text = if problem.count > 1 {
                                format!("{} (×{})", problem.message, problem.count)
                            } else {
                                problem.message.clone()
                            };
                            // 左寄せで折り返す
                            ui.allocate_ui_with_layout(
                                egui::vec2(CARD_WIDTH - 32.0, 0.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.set_width(CARD_WIDTH - 32.0);
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(text).size(12.0).color(egui::Color32::from_rgb(0xe3, 0xb2, 0xb2)),
                                        )
                                        .wrap(),
                                    );
                                },
                            );
                            if close_button(ui).on_hover_text("閉じる").clicked() {
                                close = Some(i);
                            }
                        });
                    }
                    if start > 0 {
                        ui.separator();
                        ui.label(egui::RichText::new(format!("ほか {} 件", start)).size(11.0).color(accent));
                    }
                });
        });
    if close_all {
        problems.0.clear();
    } else if let Some(i) = close {
        problems.0.remove(i);
    }
    Ok(())
}

// 閉じるボタン (✕ は日本語フォントに無く □ になるので、線で描く)
fn close_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
    let color = if response.hovered() { egui::Color32::WHITE } else { egui::Color32::from_gray(0xb0) };
    if response.hovered() {
        ui.painter().rect_filled(rect, 3.0_f32, egui::Color32::from_white_alpha(20));
    }
    let r = rect.shrink(6.0);
    let stroke = egui::Stroke::new(1.5_f32, color);
    ui.painter().line_segment([r.left_top(), r.right_bottom()], stroke);
    ui.painter().line_segment([r.right_top(), r.left_bottom()], stroke);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// 異常終了したときに、理由をダイアログで出す (メッセージは Ctrl+C でコピーできる)
pub fn install_panic_dialog() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_hook(info);
        show_error_dialog(&format!(
            "エラーが起きたため、アプリを終了します。\n\nこの画面で Ctrl+C を押すと内容をコピーできます。\n\n{}",
            info
        ));
    }));
}

#[cfg(windows)]
fn show_error_dialog(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let text = wide(message);
    let caption = wide("のぞけるディスプレイ");
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), MB_OK | MB_ICONERROR);
    }
}

#[cfg(not(windows))]
fn show_error_dialog(_message: &str) {}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::log::tracing_subscriber::{Registry, layer::SubscriberExt};

    // このアプリの警告は拾い、ほかの情報ログは拾わない
    #[test]
    fn layer_collects_own_warnings() {
        let subscriber = Registry::default().with(ProblemLayer);
        bevy::log::tracing::subscriber::with_default(subscriber, || {
            info!("拾わない情報");
            warn!("config.ini を作成できませんでした: {}", "テスト");
        });
        bevy::log::tracing::subscriber::with_default(Registry::default().with(ProblemLayer), || {
            bevy::log::tracing::error!(target: "wgpu_hal::vulkan", "拾わないほかのエラー");
        });
        let pending = PENDING.lock().unwrap();
        assert!(pending.iter().any(|m| m == "config.ini を作成できませんでした: テスト"));
        assert!(!pending.iter().any(|m| m == "拾わない情報"));
        assert!(!pending.iter().any(|m| m == "拾わないほかのエラー"));
    }
}
