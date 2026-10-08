use bevy::prelude::*;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::config::AppConfig;
use crate::scene::editor::EditState;

const OPENTRACK_DEFAULT_INI: &str = include_str!("../../assets/opentrack_default.ini");

// OpenTrack の「Start tracking」ショートカットに割り当てるキー
// (INIに書くQtのキー名と、SendInputで送るDirectInputのスキャンコード)
const START_KEY_NAME: &str = "F12";
const START_KEY_SCANCODE: u16 = 0x58;

// 起動直後はOpenTrackのUIが準備できていないので少し待ってから押し、
// UDPが届くまで一定間隔で押し直す (トラッキング中の Start は無視されるので安全)
const AUTO_START_FIRST_DELAY: Duration = Duration::from_secs(3);
const AUTO_START_RETRY_INTERVAL: Duration = Duration::from_secs(2);
const AUTO_START_TIMEOUT: Duration = Duration::from_secs(30);

fn opentrack_profile_path() -> Option<PathBuf> {
    let profile = std::env::var("USERPROFILE").ok()?;
    Some(PathBuf::from(profile).join(r"Documents\opentrack-2.3\display.ini"))
}

// アプリが前提にする OpenTrack の設定 (どの機材でも同じ値にする)
// UDP で 127.0.0.1:4242 に送る、センター合わせは C、開始は F12、動きの範囲は ±75cm (x・y は向きを反転)
const FIXED_PROFILE: &[(&str, &[(&str, &str)])] = &[
    ("modules", &[("tracker-dll", "aruco"), ("protocol-dll", "udp")]),
    ("udp-proto", &[("ip1", "127"), ("ip2", "0"), ("ip3", "0"), ("ip4", "1")]),
    (
        "opentrack-ui",
        &[
            ("keycode-center", "C"),
            ("keycode-start-tracking", START_KEY_NAME),
            ("y-invert-sign", "false"),
        ],
    ),
    (
        "opentrack-mappings",
        &[
            ("x-max-value", "75"),
            ("y-max-value", "75"),
            ("z-max-value", "75"),
            ("x-max-output-value", "-75"),
            ("y-max-output-value", "-75"),
        ],
    ),
];

// OpenTrack のプロファイルを、アプリの前提の値と機材プロファイルの値で書く。
// ほかの項目 (OpenTrack の画面で変えたものなど) はそのまま残す
fn ensure_opentrack_profile(config: &AppConfig) {
    let Some(path) = opentrack_profile_path() else {
        warn!("USERPROFILE を取得できず、OpenTrack のプロファイルを初期化できませんでした");
        return;
    };

    if !path.exists() {
        write_default_profile(&path);
    }
    let Ok(text) = std::fs::read_to_string(&path) else {
        warn!("OpenTrack のプロファイルを読み込めませんでした: {}", path.display());
        return;
    };

    let mut new_text = text.clone();
    for (section, settings) in FIXED_PROFILE {
        new_text = set_profile_settings(&new_text, section, settings);
    }
    // トレイ起動にすると、OpenTrack のウィンドウは一度も表示されない
    // (タスクトレイのアイコンをクリックすると表示できる)
    let in_tray = if config.minimize_opentrack { "true" } else { "false" };
    new_text = set_profile_settings(
        &new_text,
        "opentrack-ui",
        &[("use-system-tray", in_tray), ("start-in-tray", in_tray)],
    );
    let fov = format!("{}", config.camera_fov_deg.round() as i32);
    let mut aruco = vec![("field-of-view", fov.as_str())];
    // カメラ名が空なら書かない (OpenTrack の設定のまま)
    if !config.opentrack_camera.trim().is_empty() {
        aruco.push(("camera-name", config.opentrack_camera.trim()));
    }
    new_text = set_profile_settings(&new_text, "aruco-tracker", &aruco);

    if new_text.replace("\r\n", "\n") == text.replace("\r\n", "\n") {
        return;
    }
    match std::fs::write(&path, new_text) {
        Ok(_) => info!("OpenTrack のプロファイルを更新しました: {}", path.display()),
        Err(e) => warn!("OpenTrack のプロファイルを書き込めませんでした: {}", e),
    }
}

fn write_default_profile(path: &std::path::Path) {
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            warn!(
                "OpenTrack のプロファイルディレクトリを作成できませんでした: {}",
                e
            );
            return;
        }
    }

    match std::fs::write(&path, OPENTRACK_DEFAULT_INI) {
        Ok(_) => info!(
            "OpenTrack のプロファイルを初期化しました: {}",
            path.display()
        ),
        Err(e) => warn!("OpenTrack のプロファイルを書き込めませんでした: {}", e),
    }
}

// プロファイルの [section] の指定キーを上書きする (キーが無ければセクションの終わりに、
// セクションが無ければファイルの終わりに追加する)。書き換えた文字列を返す
fn set_profile_settings(text: &str, section: &str, settings: &[(&str, &str)]) -> String {
    let header = format!("[{}]", section);
    let mut lines: Vec<String> = Vec::new();
    let mut in_section = false;
    let mut has_section = false;
    let mut found = vec![false; settings.len()];

    // セクションの終わりで、まだ出てきていないキーを追加する (セクション間の空行より前に入れる)
    let push_missing = |lines: &mut Vec<String>, found: &mut Vec<bool>| {
        let mut at = lines.len();
        while at > 0 && lines[at - 1].trim().is_empty() {
            at -= 1;
        }
        for (i, (key, value)) in settings.iter().enumerate() {
            if !found[i] {
                lines.insert(at, format!("{}={}", key, value));
                at += 1;
                found[i] = true;
            }
        }
    };

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if in_section {
                push_missing(&mut lines, &mut found);
            }
            in_section = trimmed == header;
            has_section |= in_section;
        } else if in_section {
            let key = trimmed.split_once('=').map(|(k, _)| k.trim());
            if let Some(i) = settings.iter().position(|(k, _)| Some(*k) == key) {
                lines.push(format!("{}={}", settings[i].0, settings[i].1));
                found[i] = true;
                continue;
            }
        }
        lines.push(line.to_string());
    }
    if !has_section {
        if lines.last().is_some_and(|l| !l.trim().is_empty()) {
            lines.push(String::new());
        }
        lines.push(header);
        in_section = true;
    }
    if in_section {
        push_missing(&mut lines, &mut found);
    }
    lines.join("\n") + "\n"
}

// OpenTrack を起動してから、UDPでデータが届くまで Start tracking キーを押し続ける
#[derive(Resource)]
struct AutoStart {
    launched_at: Instant,
    next_press: Instant,
    started: bool, // UDPでデータが届いたら receive_udp が立てる
}

// DirectInput はスキャンコードで入力を拾うので、スキャンコード指定で送る。
// OpenTrack はキー状態をポーリングするため、押下と解放の間を少し空ける。
#[cfg(windows)]
fn press_start_key() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
        SendInput,
    };

    fn send(flags: u32) {
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: START_KEY_SCANCODE,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
        }
    }

    std::thread::spawn(|| {
        send(KEYEVENTF_SCANCODE);
        std::thread::sleep(Duration::from_millis(100));
        send(KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP);
    });
}

#[cfg(not(windows))]
fn press_start_key() {}

#[derive(Resource)]
pub struct UdpReceiver {
    pub socket: UdpSocket,
}

#[derive(Resource, Default, Debug)]
pub struct HeadPose {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /*
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,*/
}

pub struct TrackingPlugin;

impl Plugin for TrackingPlugin {
    fn build(&self, app: &mut App) {
        // UDPソケットの初期化
        match UdpSocket::bind("127.0.0.1:4242") {
            Ok(socket) => {
                let _ = socket.set_nonblocking(true);
                app.insert_resource(UdpReceiver { socket });
            }
            Err(e) => {
                error!("UDP Socket のバインドに失敗しました (Port 4242): {}", e);
            }
        }

        app.init_resource::<HeadPose>()
            .add_systems(Startup, launch_opentrack)
            .add_systems(PreUpdate, receive_udp)
            .add_systems(
                Update,
                (
                    auto_start_tracking.run_if(resource_exists::<AutoStart>),
                    restart_opentrack_on_change,
                ),
            )
            .add_systems(Last, close_opentrack_on_exit);
    }
}

// このアプリが起動した OpenTrack。アプリと一緒に終了させる
// (残ったままだと次回起動時に OpenTrack が二重になり、カメラを開けなくなる)
#[derive(Resource)]
struct OpentrackProcess(Option<Child>);

impl OpentrackProcess {
    // 終わるまで待つ (起動し直すときに、カメラを手放させてから次を起動するため)
    fn stop(&mut self) {
        let Some(mut child) = self.0.take() else {
            return;
        };
        match child.kill() {
            Ok(_) => info!("OpenTrack を終了しました"),
            Err(e) => warn!("OpenTrack を終了できませんでした: {}", e),
        }
        let _ = child.wait();
    }
}

impl Drop for OpentrackProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

// OpenTrack を起動したときの機材プロファイル (変わったら起動し直す)
#[derive(Resource)]
struct OpentrackLaunched(AppConfig);

fn close_opentrack_on_exit(mut commands: Commands, mut exit: MessageReader<AppExit>) {
    if exit.read().next().is_some() {
        commands.remove_resource::<OpentrackProcess>();
    }
}

fn launch_opentrack(mut commands: Commands, config: Res<AppConfig>) {
    start_opentrack(&mut commands, &config);
}

// 機材プロファイルの OpenTrack の値が保存で変わったら、プロファイルを書き直して OpenTrack を起動し直す
// (編集中の値ではなく保存した値で比べ、値を変えるたびに起動し直さないようにする)
fn restart_opentrack_on_change(
    mut commands: Commands,
    state: Res<EditState>,
    launched: Option<Res<OpentrackLaunched>>,
    process: Option<ResMut<OpentrackProcess>>,
) {
    let saved = state.saved_config();
    if launched.is_none_or(|l| l.0.same_opentrack_settings(saved)) {
        return;
    }
    info!("OpenTrack の設定が変わったので、起動し直します");
    if let Some(mut process) = process {
        process.stop();
    }
    start_opentrack(&mut commands, saved);
}

fn start_opentrack(commands: &mut Commands, config: &AppConfig) {
    commands.insert_resource(OpentrackLaunched(config.clone()));
    ensure_opentrack_profile(config);

    // 標準入出力を引き継がせない (引き継ぐと、OpenTrack が残っている間
    // デバッガがこのアプリの終了を待ち続けてしまう)
    match Command::new(&config.opentrack_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => {
            info!("OpenTrack を起動しました: {}", config.opentrack_path);
            commands.insert_resource(OpentrackProcess(Some(child)));
            let now = Instant::now();
            commands.insert_resource(AutoStart {
                launched_at: now,
                next_press: now + AUTO_START_FIRST_DELAY,
                started: false,
            });
        }
        Err(e) => warn!(
            "OpenTrack の起動に失敗しました ({}): {}",
            config.opentrack_path, e
        ),
    }
}

fn auto_start_tracking(mut commands: Commands, mut auto_start: ResMut<AutoStart>) {
    if auto_start.started {
        info!("OpenTrack のトラッキングが開始されました");
        commands.remove_resource::<AutoStart>();
        return;
    }

    let now = Instant::now();
    if now - auto_start.launched_at > AUTO_START_TIMEOUT {
        warn!(
            "OpenTrack のトラッキングを自動で開始できませんでした。\
             タスクトレイの OpenTrack のアイコンから画面を開き、「Start」を押してください"
        );
        commands.remove_resource::<AutoStart>();
        return;
    }
    if now >= auto_start.next_press {
        press_start_key();
        auto_start.next_press = now + AUTO_START_RETRY_INTERVAL;
    }
}

fn receive_udp(
    receiver: Option<Res<UdpReceiver>>, // ソケット未初期化時のクラッシュ防止
    auto_start: Option<ResMut<AutoStart>>,
    config: Res<AppConfig>,
    mut pose: ResMut<HeadPose>,
) {
    let Some(receiver) = receiver else {
        return;
    };

    let mut buf = [0u8; 48];
    let mut latest_data = None;

    // 非ブロックで最新のデータまで読み飛ばす（最新フレームのみ取得）
    while let Ok((len, _)) = receiver.socket.recv_from(&mut buf) {
        if len == 48 {
            latest_data = Some(buf);
        }
    }

    if let Some(buf) = latest_data {
        // データが届いた = トラッキングが開始された
        if let Some(mut auto_start) = auto_start {
            auto_start.started = true;
        }

        let [x, y, z /*, yaw, pitch, roll*/] = std::array::from_fn(|i| {
            let bytes = buf[i * 8..(i + 1) * 8].try_into().unwrap();
            f64::from_le_bytes(bytes) as f32
        });

        // 座標系変換の適用。OpenTrack はマーカーを 4cm として距離を出すので、印刷した大きさとの比で直す
        let k = config.marker_scale();
        *pose = HeadPose {
            x: -x * k,
            y: -y * k,
            z: z * k,
            /*yaw: roll,
            pitch: -yaw,
            roll: -pitch,*/
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overwrites_existing_key_and_adds_missing_one() {
        let text = "[a]\nx=1\n\n[b]\ny=2\n";
        let out = set_profile_settings(text, "a", &[("x", "9"), ("z", "3")]);
        assert_eq!(out, "[a]\nx=9\nz=3\n\n[b]\ny=2\n");
    }

    #[test]
    fn adds_missing_section_at_end() {
        let out = set_profile_settings("[a]\nx=1\n", "b", &[("y", "2")]);
        assert_eq!(out, "[a]\nx=1\n\n[b]\ny=2\n");
    }

    #[test]
    fn handles_crlf_and_last_section() {
        let out = set_profile_settings("[a]\r\nx=1\r\n[b]\r\ny=2", "b", &[("y", "5"), ("w", "6")]);
        assert_eq!(out, "[a]\nx=1\n[b]\ny=5\nw=6\n");
    }

    #[test]
    fn same_key_in_other_section_is_untouched() {
        let out = set_profile_settings("[a]\nx=1\n[b]\nx=2\n", "b", &[("x", "7")]);
        assert_eq!(out, "[a]\nx=1\n[b]\nx=7\n");
    }
}
