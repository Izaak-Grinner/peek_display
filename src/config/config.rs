use bevy::log::tracing_subscriber::EnvFilter;
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

// 印刷するマーカーの大きさの初期値 (cm)。4cm より大きいほうが、離れても認識でき、ブレも小さい
// (離れて座れるので、左右・上下に動ける範囲も広がる)。OpenTrack との違いは marker_scale で直す
pub const DEFAULT_MARKER_SIZE_CM: f32 = 6.0;

// OpenTrack の aruco トラッカーで選べるカメラの解像度。並びは OpenTrack の番号 (force-resolution) の順。
// default はカメラにまかせる (OpenTrack は勧めていない)
pub const CAMERA_RESOLUTIONS: &[&str] = &["640x480", "320x240", "1280x720", "1920x1080", "default"];

// ターミナルに出すログの初期値 (Bevy の初期値と同じ。wgpu と naga は細かいので警告以上だけ)
pub const DEFAULT_LOG_FILTER: &str = "info,wgpu=error,naga=warn";

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
    // カメラの解像度 (CAMERA_RESOLUTIONS のどれか)。内蔵カメラは 4:3 だと映像の左右が切られることが多い
    pub camera_resolution: String,
    // 印刷した AR マーカーの大きさ (黒い四角の一辺, cm)
    pub marker_size_cm: f32,
    // 頭の動きをなめらかにするか (tracking/smoothing.rs)
    pub smooth_tracking: bool,
    // ターミナルに出すログ (RUST_LOG と同じ書き方)。起動するときに読むので、変えたら起動し直す
    pub log_filter: String,
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
            // 内蔵の FHD カメラによくある値。正確には測って直す
            camera_fov_deg: 78.0,
            camera_resolution: "1280x720".to_string(),
            marker_size_cm: DEFAULT_MARKER_SIZE_CM,
            smooth_tracking: true,
            log_filter: DEFAULT_LOG_FILTER.to_string(),
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

    // OpenTrack のプロファイルに書く解像度の番号 (force-resolution)
    pub fn opentrack_resolution_index(&self) -> usize {
        CAMERA_RESOLUTIONS.iter().position(|r| *r == self.camera_resolution).unwrap_or(0)
    }

    // OpenTrack のプロファイルに書く値が同じか (違えば OpenTrack を起動し直す)
    pub fn same_opentrack_settings(&self, other: &Self) -> bool {
        self.camera_fov_deg.round() == other.camera_fov_deg.round()
            && self.camera_resolution == other.camera_resolution
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

        for (key, value) in entries(&text) {
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
                "camera_resolution" => match CAMERA_RESOLUTIONS.iter().find(|r| r.eq_ignore_ascii_case(value)) {
                    Some(r) => config.camera_resolution = r.to_string(),
                    None => warn!(
                        "{}: {} の値 '{}' を読めませんでした ({} のどれか)。デフォルト値 {} を使います",
                        CONFIG_FILE_NAME,
                        key,
                        value,
                        CAMERA_RESOLUTIONS.join(" / "),
                        config.camera_resolution
                    ),
                },
                "marker_size_cm" => {
                    config.marker_size_cm =
                        parse_or_warn(CONFIG_FILE_NAME, key, value, config.marker_size_cm)
                }
                "smooth_tracking" => {
                    config.smooth_tracking =
                        parse_or_warn(CONFIG_FILE_NAME, key, value, config.smooth_tracking)
                }
                "log_filter" => match EnvFilter::try_new(value) {
                    Ok(_) => config.log_filter = value.to_string(),
                    Err(_) => warn!(
                        "{}: {} の値 '{}' を読めませんでした。デフォルト値 {} を使います",
                        CONFIG_FILE_NAME, key, value, DEFAULT_LOG_FILTER
                    ),
                },
                // 知らない項目 (前の版で使っていたものなど) は無視する
                _ => {}
            }
        }

        // 足りない項目はデフォルト値を使い、エラーで知らせる (項目は save で書くものすべて)
        let template = Self::default().to_text();
        let missing: Vec<String> = entries(&template)
            .filter(|(key, _)| !entries(&text).any(|(k, _)| k == *key))
            .map(|(key, value)| format!("{}={}", key, value))
            .collect();
        if !missing.is_empty() {
            error!(
                "{} に次の項目がありません。デフォルト値を使います: {}",
                CONFIG_FILE_NAME,
                missing.join(", ")
            );
        }

        config
    }

    // ログの準備より前に、config.ini から log_filter だけを読む (読めなければ初期値。警告は load で出す)
    pub fn read_log_filter() -> String {
        std::fs::read_to_string(CONFIG_FILE_NAME)
            .ok()
            .and_then(|text| {
                entries(&text)
                    .filter(|(key, value)| *key == "log_filter" && EnvFilter::try_new(value).is_ok())
                    .last()
                    .map(|(_, value)| value.to_string())
            })
            .unwrap_or_else(|| DEFAULT_LOG_FILTER.to_string())
    }

    // 現在の値で config.ini を書き直す (コメントはテンプレートのものになる)
    pub fn save(&self) -> std::io::Result<()> {
        let text = self.to_text();
        std::fs::File::create(CONFIG_FILE_NAME).and_then(|mut f| f.write_all(text.as_bytes()))
    }

    // config.ini に書く内容
    fn to_text(&self) -> String {
        format!(
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
             # (アプリが起動した OpenTrack は、画面が隠れても遅くならないよう Windows の省電力を切る。\n\
             #  そのぶん消費電力が少し増えるので、ノートPCは電源につなぐとよい。くわしくは README)\n\
             opentrack_path={}\n\
             # OpenTrackをウィンドウを出さずにタスクトレイで起動するか(true/false)\n\
             # マーカーの認識をカメラ映像で確認したいときは false にする(出すのは検査モードの間だけ。ほかのモードでは最小化する)\n\
             # (true でも、タスクトレイのアイコンをクリックすれば画面を開ける)\n\
             minimize_opentrack={}\n\
             # カメラの対角の視野角(度)。OpenTrack が距離の計算に使う\n\
             camera_fov_deg={}\n\
             # カメラの解像度。640x480 / 320x240 / 1280x720 / 1920x1080 / default(カメラにまかせる)\n\
             # 内蔵カメラは 4:3 (640x480) だと映像の左右が切られ、動ける範囲が狭くなることが多い。\n\
             # 上げると遠くのマーカーも見つけやすいが、1回の計測が遅くなる。変えたら視野角も測り直す\n\
             camera_resolution={}\n\
             # 印刷した AR マーカーの大きさ(黒い四角の一辺, cm)。定規で測った実寸を入れる\n\
             # OpenTrack は 4cm として距離を出すので、その比で直す\n\
             marker_size_cm={}\n\
             # 頭の動きをなめらかにするか(true/false)。カメラが測る間を予測でつなぎ、細かいブレをならす\n\
             smooth_tracking={}\n\
             \n\
             # ターミナルに出すログ(RUST_LOG と同じ書き方。RUST_LOG より優先)。変えたら起動し直す\n\
             # 例: warn (警告とエラーだけ) / info,peek_display::tracking=debug (トラッキングだけ細かく)\n\
             # warn より絞ると(error など)、このアプリの警告も画面に出なくなる\n\
             log_filter={}\n",
            self.screen_diagonal_inches,
            self.screen_width_mm,
            self.viewer_base_distance,
            self.viewer_min_distance,
            self.opentrack_path,
            self.minimize_opentrack,
            self.camera_fov_deg,
            self.camera_resolution,
            self.marker_size_cm,
            self.smooth_tracking,
            self.log_filter,
        )
    }
}

// 設定ファイルの「キー=値」を順に返す (空行とコメントは飛ばす)
fn entries(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines().filter_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            return None;
        }
        let (key, value) = line.split_once('=')?;
        Some((key.trim(), value.trim()))
    })
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
