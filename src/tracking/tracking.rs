use bevy::prelude::*;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::config::AppConfig;

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

// display.ini が無い場合のみ、既知の動作実績があるプロファイルで初期化する
// (既にファイルがある場合は、OpenTrackのUIで調整した内容を保持するため上書きしない)
// その上で、このアプリが前提とする設定だけは毎回書き込む
fn ensure_opentrack_profile(config: &AppConfig) {
    let Some(path) = opentrack_profile_path() else {
        warn!("USERPROFILE を取得できず、OpenTrack のプロファイルを初期化できませんでした");
        return;
    };

    if !path.exists() {
        write_default_profile(&path);
    }

    // トレイ起動にすると、OpenTrack のウィンドウは一度も表示されない
    // (タスクトレイのアイコンをクリックすると表示できる)
    let in_tray = if config.minimize_opentrack { "true" } else { "false" };
    set_ui_settings(
        &path,
        &[
            ("keycode-start-tracking", START_KEY_NAME),
            ("use-system-tray", in_tray),
            ("start-in-tray", in_tray),
        ],
    );
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

// プロファイルの [opentrack-ui] セクションの指定キーを上書きする (無ければ追加する)
fn set_ui_settings(path: &std::path::Path, settings: &[(&str, &str)]) {
    let Ok(text) = std::fs::read_to_string(path) else {
        warn!("OpenTrack のプロファイルを読み込めませんでした: {}", path.display());
        return;
    };

    let mut lines: Vec<String> = Vec::new();
    let mut in_ui_section = false;
    let mut has_ui_section = false;
    let mut found = vec![false; settings.len()];

    // [opentrack-ui] セクションの終わりで、まだ出てきていないキーを追加する
    // (セクション間の空行より前に入れる)
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
            if in_ui_section {
                push_missing(&mut lines, &mut found);
            }
            in_ui_section = trimmed == "[opentrack-ui]";
            has_ui_section |= in_ui_section;
        } else if in_ui_section {
            let key = trimmed.split_once('=').map(|(k, _)| k.trim());
            if let Some(i) = settings.iter().position(|(k, _)| Some(*k) == key) {
                lines.push(format!("{}={}", settings[i].0, settings[i].1));
                found[i] = true;
                continue;
            }
        }
        lines.push(line.to_string());
    }
    if !has_ui_section {
        lines.push("[opentrack-ui]".to_string());
        in_ui_section = true;
    }
    if in_ui_section {
        push_missing(&mut lines, &mut found);
    }

    let new_text = lines.join("\n") + "\n";
    if new_text.replace("\r\n", "\n") == text.replace("\r\n", "\n") {
        return;
    }
    match std::fs::write(path, new_text) {
        Ok(_) => info!("OpenTrack のプロファイルを更新しました: {}", path.display()),
        Err(e) => warn!("OpenTrack のプロファイルを書き込めませんでした: {}", e),
    }
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
                auto_start_tracking.run_if(resource_exists::<AutoStart>),
            )
            .add_systems(Last, close_opentrack_on_exit);
    }
}

// このアプリが起動した OpenTrack。アプリと一緒に終了させる
// (残ったままだと次回起動時に OpenTrack が二重になり、カメラを開けなくなる)
#[derive(Resource)]
struct OpentrackProcess(Child);

impl Drop for OpentrackProcess {
    fn drop(&mut self) {
        match self.0.kill() {
            Ok(_) => info!("OpenTrack を終了しました"),
            Err(e) => warn!("OpenTrack を終了できませんでした: {}", e),
        }
        let _ = self.0.wait();
    }
}

fn close_opentrack_on_exit(mut commands: Commands, mut exit: MessageReader<AppExit>) {
    if exit.read().next().is_some() {
        commands.remove_resource::<OpentrackProcess>();
    }
}

fn launch_opentrack(mut commands: Commands, config: Res<AppConfig>) {
    ensure_opentrack_profile(&config);

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
            commands.insert_resource(OpentrackProcess(child));
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

        // 座標系変換の適用
        *pose = HeadPose {
            x: -x,
            y: -y,
            z: z,
            /*yaw: roll,
            pitch: -yaw,
            roll: -pitch,*/
        };
    }
}
