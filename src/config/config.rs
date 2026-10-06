use bevy::prelude::*;
use std::fmt::Display;
use std::io::Write;
use std::str::FromStr;

pub const OFF_AXIS_NEAR: f32 = 0.01;
pub const OFF_AXIS_FAR: f32 = 10.0;

const INCHES_TO_METERS: f32 = 0.0254;
const CONFIG_FILE_NAME: &str = "config.ini";

// 実行ファイルと同じ場所に置く config.ini から読み込む設定値。
// ファイルが無い場合はデフォルト値でテンプレートを新規作成する。
#[derive(Resource, Clone, PartialEq)]
pub struct AppConfig {
    pub screen_diagonal_inches: f32,
    pub screen_aspect_width: f32,
    pub screen_aspect_height: f32,
    pub viewer_base_distance: f32,
    pub viewer_min_distance: f32,
    pub opentrack_path: String,
    pub minimize_opentrack: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            screen_diagonal_inches: 16.0,
            screen_aspect_width: 16.0,
            screen_aspect_height: 10.0,
            viewer_base_distance: 0.5,
            viewer_min_distance: 0.05,
            opentrack_path: r"C:\Program Files (x86)\opentrack\opentrack.exe".to_string(),
            minimize_opentrack: true,
        }
    }
}

impl AppConfig {
    /// (width, height) in meters
    pub fn screen_size(&self) -> (f32, f32) {
        let diagonal_m = self.screen_diagonal_inches * INCHES_TO_METERS;
        let aspect = self.screen_aspect_width / self.screen_aspect_height;
        let height = diagonal_m / (aspect * aspect + 1.0).sqrt();
        let width = height * aspect;
        (width, height)
    }

    pub fn load() -> Self {
        let config = Self::default();

        let Ok(text) = std::fs::read_to_string(CONFIG_FILE_NAME) else {
            match config.save() {
                Ok(_) => info!(
                    "{} が見つからなかったのでデフォルト設定で作成しました",
                    CONFIG_FILE_NAME
                ),
                Err(e) => warn!("{} を作成できませんでした: {}", CONFIG_FILE_NAME, e),
            }
            return config;
        };

        let mut config = config;

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();

            match key {
                "screen_diagonal_inches" => {
                    config.screen_diagonal_inches =
                        parse_or_warn(CONFIG_FILE_NAME, key, value, config.screen_diagonal_inches)
                }
                "screen_aspect_width" => {
                    config.screen_aspect_width =
                        parse_or_warn(CONFIG_FILE_NAME, key, value, config.screen_aspect_width)
                }
                "screen_aspect_height" => {
                    config.screen_aspect_height =
                        parse_or_warn(CONFIG_FILE_NAME, key, value, config.screen_aspect_height)
                }
                "viewer_base_distance" => {
                    config.viewer_base_distance =
                        parse_or_warn(CONFIG_FILE_NAME, key, value, config.viewer_base_distance)
                }
                "viewer_min_distance" => {
                    config.viewer_min_distance =
                        parse_or_warn(CONFIG_FILE_NAME, key, value, config.viewer_min_distance)
                }
                "opentrack_path" => config.opentrack_path = value.to_string(),
                "minimize_opentrack" => {
                    config.minimize_opentrack =
                        parse_or_warn(CONFIG_FILE_NAME, key, value, config.minimize_opentrack)
                }
                _ => warn!("{}: 不明な設定項目を無視しました: {}", CONFIG_FILE_NAME, key),
            }
        }

        config
    }

    // 現在の値で config.ini を書き直す (コメントはテンプレートのものになる)
    pub fn save(&self) -> std::io::Result<()> {
        let text = format!(
            "# peek_display 設定ファイル\n\
             # 画面のサイズは対角インチとアスペクト比から自動計算されます\n\
             screen_diagonal_inches={}\n\
             screen_aspect_width={}\n\
             screen_aspect_height={}\n\
             \n\
             # 基準姿勢(OpenTrackでCenterを押した位置)での、目から画面までの距離(メートル)\n\
             viewer_base_distance={}\n\
             # 画面に近づいたときにこれ以上近づけない、最小距離(メートル)\n\
             viewer_min_distance={}\n\
             \n\
             # OpenTrackの実行ファイルのパス\n\
             opentrack_path={}\n\
             # OpenTrackをウィンドウを出さずにタスクトレイで起動するか(true/false)\n\
             # マーカーの認識をカメラ映像で確認したいときは false にする\n\
             # (true でも、タスクトレイのアイコンをクリックすれば画面を開ける)\n\
             minimize_opentrack={}\n",
            self.screen_diagonal_inches,
            self.screen_aspect_width,
            self.screen_aspect_height,
            self.viewer_base_distance,
            self.viewer_min_distance,
            self.opentrack_path,
            self.minimize_opentrack,
        );

        std::fs::File::create(CONFIG_FILE_NAME).and_then(|mut f| f.write_all(text.as_bytes()))
    }
}

pub fn parse_or_warn<T: FromStr + Display>(file: &str, key: &str, value: &str, fallback: T) -> T {
    value.parse().unwrap_or_else(|_| {
        warn!(
            "{}: {} の値 '{}' を読めませんでした。デフォルト値 {} を使います",
            file, key, value, fallback
        );
        fallback
    })
}
