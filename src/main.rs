// 配布用ビルド (dev feature なし) では、ログを流すコンソールを出さない。問題は画面とダイアログで知らせる (ui/problems.rs)
#![cfg_attr(not(feature = "dev"), windows_subsystem = "windows")]

use bevy::{log::LogPlugin, picking::mesh_picking::MeshPickingPlugin, prelude::*};

mod app_mode;
mod camera;
mod config;
mod scene;
mod tracking;
mod ui;

fn main() {
    ui::problems::install_panic_dialog();
    let mut app = App::new();
    // シーンの中のファイルの読み込み元は、DefaultPlugins より前に登録する
    let mut store = scene::package_store::PackageStore::register(&mut app);
    app.add_plugins((
        DefaultPlugins.set(LogPlugin {
            custom_layer: ui::problems::log_layer,
            ..default()
        }),
        MeshPickingPlugin,
    ));
    // ログの準備ができてから読み込む (読めなかったときの警告を画面に出すため)
    let (library, layout, files) = scene::library::SceneLibrary::open();
    store.replace(files);
    app.insert_resource(config::AppConfig::load())
        .insert_resource(store)
        .insert_resource(library)
        .insert_resource(layout)
        .add_plugins((
            app_mode::AppModePlugin,
            config::DisplayPlugin,
            ui::UiPlugin,
            scene::scene::ScenePlugin,
            scene::editor::EditorPlugin,
            scene::import::ImportPlugin,
            scene::package::PackagePlugin,
            scene::model::ModelPlugin,
            scene::highlight::HighlightPlugin,
            camera::main_camera::MainCameraPlugin,
            camera::camera_preview::CameraPreviewPlugin,
            camera::scene_view::SceneViewPlugin,
            camera::off_axis_projection::OffAxisPlugin,
            tracking::tracking::TrackingPlugin,
        ));
    // Debug: set PEEK_SHOT=<path.png> to save a screenshot after 8s and exit.
    if let Ok(path) = std::env::var("PEEK_SHOT") {
        app.add_systems(Update, move |commands: Commands, time: Res<Time>, done: Local<u8>, exit: MessageWriter<AppExit>| {
            test_shot(commands, time, done, exit, &path)
        });
    }
    app.run();
}

fn test_shot(mut commands: Commands, time: Res<Time>, mut done: Local<u8>, mut exit: MessageWriter<AppExit>, path: &str) {
    use bevy::render::view::screenshot::{Screenshot, save_to_disk};
    let t = time.elapsed_secs();
    if *done == 0 && t > 8.0 {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.to_owned()));
        *done = 1;
    }
    if *done == 1 && t > 16.0 {
        exit.write(AppExit::Success);
    }
}
