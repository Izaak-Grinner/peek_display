// 実際の画面の大きさ。比率はモニターの画素数から求め、大きさは機材プロファイル (対角か幅) から求める
use bevy::{
    prelude::*,
    window::{Monitor, PrimaryMonitor, PrimaryWindow},
};

use crate::config::AppConfig;

#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct DisplayInfo {
    // モニターの画素数
    pub pixels: UVec2,
    // 画面 (映る部分) の幅と高さ (m)
    pub size: Vec2,
}

impl Default for DisplayInfo {
    fn default() -> Self {
        Self {
            pixels: UVec2::new(1920, 1080),
            size: AppConfig::default().screen_size(16.0 / 9.0),
        }
    }
}

impl DisplayInfo {
    pub fn aspect(&self) -> f32 {
        self.size.x / self.size.y
    }

    // シーンを作ったときの画面 (基準の画面) に映っていた範囲が全部入るように、シーンを見る縮尺。
    // スクリーンの大きさと目の位置にこれを掛ける。1 より大きければ、シーンは小さく見える (CSS の contain)
    pub fn view_scale(&self, reference: Option<Vec2>) -> f32 {
        match reference {
            Some(r) if r.x > 0.0 && r.y > 0.0 => (r.x / self.size.x).max(r.y / self.size.y),
            _ => 1.0,
        }
    }
}

pub struct DisplayPlugin;

impl Plugin for DisplayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DisplayInfo>()
            .add_systems(PreUpdate, update_display_info);
    }
}

// モニターの画素数と機材プロファイルから、画面の大きさを求める。
// モニターが分からないときは、ウィンドウの画素数を使う
fn update_display_info(
    config: Res<AppConfig>,
    monitors: Query<&Monitor, With<PrimaryMonitor>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut info: ResMut<DisplayInfo>,
) {
    let pixels = monitors
        .iter()
        .next()
        .map(|m| UVec2::new(m.physical_width, m.physical_height))
        .or_else(|| window.iter().next().map(|w| UVec2::new(w.physical_width(), w.physical_height())))
        .filter(|p| p.x > 0 && p.y > 0);
    let Some(pixels) = pixels else {
        return;
    };
    let next = DisplayInfo {
        pixels,
        size: config.screen_size(pixels.x as f32 / pixels.y as f32),
    };
    if *info != next {
        *info = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_from_diagonal_and_width() {
        let mut config = AppConfig::default();
        config.screen_diagonal_inches = 16.0;
        let size = config.screen_size(16.0 / 10.0);
        assert!((size.x - 0.3446).abs() < 1e-3 && (size.y - 0.2154).abs() < 1e-3);
        config.screen_width_mm = 300.0;
        let size = config.screen_size(16.0 / 9.0);
        assert!((size.x - 0.3).abs() < 1e-6 && (size.y - 0.16875).abs() < 1e-6);
    }

    #[test]
    fn view_scale_contains_reference() {
        let info = DisplayInfo {
            pixels: UVec2::new(1920, 1200),
            size: Vec2::new(0.32, 0.20),
        };
        // 基準が無ければ実物大
        assert_eq!(info.view_scale(None), 1.0);
        // 同じ大きさなら実物大
        assert_eq!(info.view_scale(Some(Vec2::new(0.32, 0.20))), 1.0);
        // 基準が 16:9 で幅が同じ: 幅で決まる (上下が少し余分に見える)
        assert_eq!(info.view_scale(Some(Vec2::new(0.32, 0.18))), 1.0);
        // 基準のほうが 2 倍大きい: 2 倍の範囲を見る (シーンは半分の大きさに見える)
        assert_eq!(info.view_scale(Some(Vec2::new(0.64, 0.40))), 2.0);
        // 基準が縦に長い: 高さで決まる
        assert!((info.view_scale(Some(Vec2::new(0.32, 0.30))) - 1.5).abs() < 1e-6);
    }
}
