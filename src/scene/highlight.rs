// 編集モードで選んでいるオブジェクトを、Blender のように形に沿ったオレンジの輪郭線で縁取る。
// 輪郭線は画面上で縁を探して描く方式 (jump flood) なので、細い物や複雑な形でも太さがそろう。
// 俯瞰ビューにだけ描き、覗き込みカメラ (小窓・本番) には映さない
use bevy::{camera::visibility::RenderLayers, prelude::*};
use bevy_mod_outline::{InheritOutline, OutlineMode, OutlinePlugin, OutlineRenderLayers, OutlineVolume};

use crate::app_mode::AppMode;
use crate::camera::preview::GIZMO_LAYER;
use crate::scene::editor::{EditState, Selection};
use crate::scene::scene::Placeable;
use crate::scene::world::WorldInfo;

// 選択の色 (Blender の選択に近いオレンジ)
pub const SELECTION_COLOR: Color = Color::srgb(1.0, 0.55, 0.1);
// 輪郭線の太さ (論理ピクセル)
const OUTLINE_WIDTH: f32 = 3.0;

// 輪郭線を付けたエンティティ (選択が外れたら外す)
#[derive(Component)]
struct Outlined;

pub struct HighlightPlugin;

impl Plugin for HighlightPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(OutlinePlugin)
            .add_systems(PostUpdate, update_outline);
    }
}

// 選んでいるもののエンティティ (編集モードのときだけ)
fn selected_entity(
    mode: &State<AppMode>,
    state: &EditState,
    world: &WorldInfo,
    placeables: &Query<(Entity, &Placeable)>,
) -> Option<Entity> {
    if *mode.get() != AppMode::Edit {
        return None;
    }
    match state.selection.as_ref()? {
        Selection::Object(id) => placeables.iter().find(|(_, p)| p.0 == *id).map(|(e, _)| e),
        Selection::WorldNode(name) => world.node(name).map(|n| n.entity),
        Selection::Window | Selection::Light => None,
    }
}

// 選んでいるものに輪郭線を付け、選ばれなくなったものからは外す。
// モデルの読み込みが終わって子が増えた場合も、毎フレーム調べて追いかける
fn update_outline(
    mut commands: Commands,
    mode: Res<State<AppMode>>,
    state: Res<EditState>,
    world: Res<WorldInfo>,
    placeables: Query<(Entity, &Placeable)>,
    children: Query<&Children>,
    outlined: Query<Entity, With<Outlined>>,
    inherits: Query<(), With<InheritOutline>>,
) {
    let target = selected_entity(&mode, &state, &world, &placeables);

    // 前に選んでいたものの輪郭線を外す
    for entity in &outlined {
        if Some(entity) == target {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(Outlined, OutlineVolume, OutlineMode, OutlineRenderLayers)>();
        for descendant in children.iter_descendants(entity) {
            commands.entity(descendant).remove::<InheritOutline>();
        }
    }

    let Some(target) = target else {
        return;
    };
    if !outlined.contains(target) {
        commands.entity(target).insert((
            Outlined,
            OutlineVolume {
                visible: true,
                width: OUTLINE_WIDTH,
                colour: SELECTION_COLOR,
            },
            OutlineMode::FloodFlat,
            // 俯瞰カメラだけが見るレイヤーに描く
            OutlineRenderLayers(RenderLayers::layer(GIZMO_LAYER)),
        ));
    }
    // 子のメッシュは親の輪郭線を引き継ぐ
    for descendant in children.iter_descendants(target) {
        if !inherits.contains(descendant) {
            commands.entity(descendant).insert(InheritOutline);
        }
    }
}
