use bevy::prelude::*;
use std::fmt::Display;
use std::io::Write;
use std::str::FromStr;

pub const OFF_AXIS_NEAR: f32 = 0.01;
pub const OFF_AXIS_FAR: f32 = 10.0;

const INCHES_TO_METERS: f32 = 0.0254;
const CONFIG_FILE_NAME: &str = "config.ini";

// OpenTrack の aruco トラッカーが前提にしているマーカーの大きさ (cm)。
// OpenTrack のコードに固定されていて (40mm)、設定では変えられない
pub const ARUCO_MARKER_SIZE_CM: f32 = 4.0;

// このPCと設置場所の機材の値 (機材プロファイル)。実行ファイルと同じ場所の config.ini に保存する。
// ディスプレイの大きさ、見る人の距離、OpenTrack の較正をまとめて持ち、シーンには入れない。
// ファイルが無い場合はデフォルト値でテンプレートを新規作成する。
#[derive(Resource, Clone, PartialEq)]
pub struct AppConfig {
    // 画面 (映る部分) の対角 (インチ)。比率はモニターの画素数から求める
    pub screen_diagonal_inches: f32,
    // 定規で測った画面の幅 (mm)。0 より大きければ、対角の代わりにこれを使う
    pub screen_width_mm: f32,
    pub viewer_base_distance: f32,
    pub viewer_min_distance: f32,
    pub opentrack_path: String,
    pub minimize_opentrack: bool,
    // カメラの対角の視野角 (度)。OpenTrack が距離の計算に使う
    pub camera_fov_deg: f32,
    // 印刷した AR マーカーの大きさ (黒い四角の一辺, cm)
    pub marker_size_cm: f32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            screen_diagonal_inches: 16.0,
            screen_width_mm: 0.0,
            viewer_base_distance: 0.5,
            viewer_min_distance: 0.05,
            opentrack_path: r"C:\Program Files (x86)\opentrack\opentrack.exe".to_string(),
            minimize_opentrack: true,
            camera_fov_deg: 56.0,
            marker_size_cm: ARUCO_MARKER_SIZE_CM,
        }
    }
}

impl AppConfig {
    // 画面の幅と高さ (m)。aspect は幅 / 高さ (モニターの画素数から求めたもの)
    pub fn screen_size(&self, aspect: f32) -> Vec2 {
        if self.screen_width_mm > 0.0 {
            let width = self.screen_width_mm / 1000.0;
            return Vec2::new(width, width / aspect);
        }
        let diagonal_m = self.screen_diagonal_inches * INCHES_TO_METERS;
        let height = diagonal_m / (aspect * aspect + 1.0).sqrt();
        Vec2::new(height * aspect, height)
    }

    // OpenTrack から届いた頭の位置に掛ける倍率。
    // OpenTrack はマーカーを 4cm として距離を出すので、印刷した大きさとの比で直す
    pub fn marker_scale(&self) -> f32 {
        self.marker_size_cm / ARUCO_MARKER_SIZE_CM
    }

    // OpenTrack のプロファイルに書く値が同じか (違えば OpenTrack を起動し直す)
    pub fn same_opentrack_settings(&self, other: &Self) -> bool {
        self.camera_fov_deg.round() == other.camera_fov_deg.round()
            && self.minimize_opentrack == other.minimize_opentrack
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
                "screen_width_mm" => {
                    config.screen_width_mm =
                        parse_or_warn(CONFIG_FILE_NAME, key, value, config.screen_width_mm)
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
                "camera_fov_deg" => {
                    config.camera_fov_deg =
                        parse_or_warn(CONFIG_FILE_NAME, key, value, config.camera_fov_deg)
                }
                "marker_size_cm" => {
                    config.marker_size_cm =
                        parse_or_warn(CONFIG_FILE_NAME, key, value, config.marker_size_cm)
                }
                _ => warn!("{}: 不明な設定項目を無視しました: {}", CONFIG_FILE_NAME, key),
            }
        }

        config
    }

    // 現在の値で config.ini を書き直す (コメントはテンプレートのものになる)
    pub fn save(&self) -> std::io::Result<()> {
        let text = format!(
            "# peek_display 機材プロファイル (このPCと設置場所の値。シーンには入りません)\n\
             # 画面(映る部分)の大きさ。比率はモニターの画素数から自動で求めます\n\
             # 対角(インチ)\n\
             screen_diagonal_inches={}\n\
             # 定規で測った画面の幅(mm)。0 より大きければ、対角の代わりにこれを使う\n\
             screen_width_mm={}\n\
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
             minimize_opentrack={}\n\
             # カメラの対角の視野角(度)。OpenTrack が距離の計算に使う\n\
             camera_fov_deg={}\n\
             # 印刷した AR マーカーの大きさ(黒い四角の一辺, cm)。OpenTrack は 4cm として距離を出すので、その比で直す\n\
             marker_size_cm={}\n",
            self.screen_diagonal_inches,
            self.screen_width_mm,
            self.viewer_base_distance,
            self.viewer_min_distance,
            self.opentrack_path,
            self.minimize_opentrack,
            self.camera_fov_deg,
            self.marker_size_cm,
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
