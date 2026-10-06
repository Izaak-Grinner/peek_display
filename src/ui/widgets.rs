// 編集パネル用の部品。egui 標準のウィジェットでは見た目を合わせにくいものを自前で描く
use bevy_egui::egui::{
    self, Align, Align2, Color32, CornerRadius, DragValue, FontId, Galley, Layout, Painter, Rect,
    Response, Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2,
};
use std::ops::RangeInclusive;
use std::sync::Arc;

pub mod color {
    use bevy_egui::egui::Color32;

    const fn hex(rgb: u32) -> Color32 {
        Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    }

    pub const PANEL: Color32 = hex(0x262626);
    pub const PANEL_EDGE: Color32 = hex(0x101010);
    // ヘッダー・フッター
    pub const BAR: Color32 = hex(0x1d1d1d);
    pub const BAR_BORDER: Color32 = hex(0x111111);
    pub const SECTION: Color32 = hex(0x2e2e2e);
    pub const SECTION_BORDER: Color32 = hex(0x1a1a1a);
    // 一覧などのくぼんだ背景
    pub const WELL: Color32 = hex(0x1c1c1c);
    pub const WELL_BORDER: Color32 = hex(0x333333);
    pub const FIELD: Color32 = hex(0x3a3a3a);
    pub const DIVIDER: Color32 = hex(0x353535);
    pub const ROW_HOVER: Color32 = hex(0x2a2a2a);

    pub const TEXT: Color32 = hex(0xd9d9d9);
    pub const TEXT_STRONG: Color32 = hex(0xeeeeee);
    pub const TEXT_HEADING: Color32 = hex(0xe6e6e6);
    pub const TEXT_VALUE: Color32 = hex(0xececec);
    pub const TEXT_LABEL: Color32 = hex(0xb5b5b5);
    pub const TEXT_SUB: Color32 = hex(0xbdbdbd);
    pub const TEXT_DIM: Color32 = hex(0xa8a8a8);
    pub const TEXT_MUTED: Color32 = hex(0x9a9a9a);
    pub const TEXT_HINT: Color32 = hex(0x949494);
    pub const TEXT_FAINT: Color32 = hex(0x8f8f8f);
    pub const TEXT_UNIT: Color32 = hex(0xa0a0a0);

    pub const SELECTED: Color32 = hex(0x2f5597);
    pub const SELECTED_BORDER: Color32 = hex(0x3f66ab);
    pub const SELECTED_TAG: Color32 = hex(0xdce6f7);
    pub const FOCUS: Color32 = hex(0x4d7fd4);

    pub const AXIS_X: Color32 = hex(0xff6b6b);
    pub const AXIS_Y: Color32 = hex(0x8fd14f);
    pub const AXIS_Z: Color32 = hex(0x5aa9ff);

    pub const ICON_WINDOW: Color32 = hex(0x6fa8ff);
    pub const ICON_MODEL: Color32 = hex(0xe6a95c);
    pub const ICON_LIGHT: Color32 = hex(0xf2d06b);
    pub const ICON_CUBE: Color32 = hex(0xb9b9b9);

    pub const WARN: Color32 = hex(0xf0a640);
    pub const DANGER: Color32 = hex(0xff8a8a);
}

use color::*;

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

// 太さ 1px の線
pub fn hairline(color: Color32) -> Stroke {
    Stroke::new(1.0_f32, color)
}

// ---------------------------------------------------------------- アイコン

#[derive(Clone, Copy)]
pub enum Icon {
    Monitor,
    Sun,
    Model,
    Cube,
    Copy,
    Trash,
    Warning,
    Check,
    ChevronDown,
    ChevronRight,
    Cursor,
    Spinner,
    Plus,
}

// 14×14 の座標系で描いたアイコンを rect に合わせて描く
pub fn draw_icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32, width: f32) {
    let s = rect.width() / 14.0;
    let p = |x: f32, y: f32| rect.min + vec2(x * s, y * s);
    let stroke = Stroke::new(width * s, color);
    let line = |points: &[(f32, f32)]| {
        painter.add(Shape::line(
            points.iter().map(|&(x, y)| p(x, y)).collect(),
            stroke,
        ));
    };
    let ellipse = |rx: f32, ry: f32| {
        let points = (0..32)
            .map(|i| {
                let a = i as f32 / 32.0 * std::f32::consts::TAU;
                p(7.0 + rx * a.cos(), 7.0 + ry * a.sin())
            })
            .collect();
        painter.add(Shape::closed_line(points, stroke));
    };

    match icon {
        Icon::Monitor => {
            painter.rect_stroke(
                Rect::from_min_max(p(1.5, 2.5), p(12.5, 10.0)),
                CornerRadius::same(1),
                stroke,
                StrokeKind::Middle,
            );
            line(&[(5.0, 12.5), (9.0, 12.5)]);
            line(&[(7.0, 10.0), (7.0, 12.5)]);
        }
        Icon::Sun => {
            painter.circle_stroke(p(7.0, 7.0), 2.6 * s, stroke);
            for i in 0..8 {
                let a = i as f32 / 8.0 * std::f32::consts::TAU;
                let (c, sn) = (a.cos(), a.sin());
                line(&[(7.0 + c * 4.3, 7.0 + sn * 4.3), (7.0 + c * 6.0, 7.0 + sn * 6.0)]);
            }
        }
        Icon::Model => {
            painter.circle_stroke(p(7.0, 7.0), 5.5 * s, stroke);
            line(&[(1.5, 7.0), (12.5, 7.0)]);
            ellipse(2.4, 5.5);
        }
        Icon::Cube => {
            line(&[
                (7.0, 1.5),
                (12.5, 4.5),
                (12.5, 9.5),
                (7.0, 12.5),
                (1.5, 9.5),
                (1.5, 4.5),
                (7.0, 1.5),
            ]);
            line(&[(1.5, 4.5), (7.0, 7.5), (12.5, 4.5)]);
            line(&[(7.0, 7.5), (7.0, 12.5)]);
        }
        Icon::Copy => {
            painter.rect_stroke(
                Rect::from_min_max(p(4.5, 4.5), p(12.0, 12.0)),
                CornerRadius::same(1),
                stroke,
                StrokeKind::Middle,
            );
            line(&[(9.5, 4.5), (9.5, 2.0), (2.0, 2.0), (2.0, 9.5), (4.5, 9.5)]);
        }
        Icon::Trash => {
            line(&[(2.0, 4.0), (12.0, 4.0)]);
            line(&[(5.5, 4.0), (5.5, 2.5), (8.5, 2.5), (8.5, 4.0)]);
            line(&[(3.5, 4.0), (4.1, 12.0), (9.9, 12.0), (10.5, 4.0)]);
        }
        Icon::Warning => {
            line(&[(7.0, 1.8), (12.8, 12.0), (1.2, 12.0), (7.0, 1.8)]);
            line(&[(7.0, 5.5), (7.0, 8.5)]);
            painter.circle_filled(p(7.0, 10.3), width * s * 0.6, color);
        }
        Icon::Plus => {
            line(&[(7.0, 2.5), (7.0, 11.5)]);
            line(&[(2.5, 7.0), (11.5, 7.0)]);
        }
        Icon::Check => line(&[(2.5, 7.5), (5.5, 10.5), (11.5, 4.0)]),
        Icon::ChevronDown => line(&[(3.5, 5.5), (7.0, 9.0), (10.5, 5.5)]),
        Icon::ChevronRight => line(&[(5.5, 3.5), (9.0, 7.0), (5.5, 10.5)]),
        Icon::Cursor => {
            line(&[(3.0, 2.0), (10.5, 6.0), (7.3, 6.9), (5.9, 10.2), (3.0, 2.0)]);
            line(&[(7.4, 7.0), (11.0, 11.5)]);
        }
        Icon::Spinner => {
            painter.circle_stroke(p(7.0, 7.0), 5.0 * s, Stroke::new(stroke.width, color.gamma_multiply(0.3)));
            let start = painter.ctx().input(|i| i.time) as f32 * std::f32::consts::TAU;
            let points = (0..=12)
                .map(|i| {
                    let a = start + i as f32 / 12.0 * std::f32::consts::FRAC_PI_2;
                    p(7.0 + 5.0 * a.cos(), 7.0 + 5.0 * a.sin())
                })
                .collect();
            painter.add(Shape::line(points, stroke));
            painter.ctx().request_repaint();
        }
    }
}

// 指定した大きさの場所を取ってアイコンを描く
pub fn icon(ui: &mut Ui, icon: Icon, size: f32, color: Color32, width: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    draw_icon(ui.painter(), rect, icon, color, width);
    response
}

// ---------------------------------------------------------------- 文字

pub fn text(ui: &Ui, text: impl Into<String>, size: f32, color: Color32) -> Arc<Galley> {
    ui.painter()
        .layout_no_wrap(text.into(), FontId::proportional(size), color)
}

// 入りきらない部分を … にする
pub fn truncated_text(ui: &Ui, text: &str, size: f32, color: Color32, max_width: f32) -> Arc<Galley> {
    let mut job =
        egui::text::LayoutJob::simple_singleline(text.to_owned(), FontId::proportional(size), color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width.max(0.0));
    ui.painter().layout_job(job)
}

// 幅を決めたラベル (表の左の列など)
pub fn fixed_label(ui: &mut Ui, width: f32, height: f32, label: &str, size: f32, color: Color32, align: Align) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let (anchor, x) = match align {
        Align::Min => (Align2::LEFT_CENTER, rect.left()),
        Align::Center => (Align2::CENTER_CENTER, rect.center().x),
        Align::Max => (Align2::RIGHT_CENTER, rect.right() - 4.0),
    };
    ui.painter()
        .text(pos2(x, rect.center().y), anchor, label, FontId::proportional(size), color);
}

pub fn wrapped_label(ui: &mut Ui, label: &str, size: f32, color: Color32) {
    ui.add(egui::Label::new(egui::RichText::new(label).size(size).color(color)).wrap());
}

// ---------------------------------------------------------------- ボタン

#[derive(Clone, Copy)]
pub struct ButtonStyle {
    pub fill: Color32,
    pub hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub bold: bool,
}

pub const BUTTON: ButtonStyle = ButtonStyle {
    fill: hex(0x333333),
    hover: hex(0x3d3d3d),
    border: hex(0x484848),
    text: hex(0xe0e0e0),
    bold: false,
};
pub const BUTTON_SECONDARY: ButtonStyle = ButtonStyle {
    border: hex(0x4a4a4a),
    text: hex(0xdddddd),
    ..BUTTON
};
pub const BUTTON_PRIMARY: ButtonStyle = ButtonStyle {
    fill: hex(0x3563b8),
    hover: hex(0x3d6fc8),
    border: hex(0x4d7fd4),
    text: Color32::WHITE,
    bold: true,
};
pub const BUTTON_DANGER: ButtonStyle = ButtonStyle {
    hover: hex(0x3d3232),
    border: hex(0x5a3a3a),
    text: hex(0xff9a9a),
    ..BUTTON
};
pub const BUTTON_DANGER_GHOST: ButtonStyle = ButtonStyle {
    fill: Color32::TRANSPARENT,
    hover: hex(0x352626),
    ..BUTTON_DANGER
};
pub const BUTTON_DANGER_SOLID: ButtonStyle = ButtonStyle {
    fill: hex(0xb23a3a),
    hover: hex(0xc24444),
    border: hex(0xc95050),
    text: Color32::WHITE,
    bold: true,
};
pub const BUTTON_CHIP: ButtonStyle = ButtonStyle {
    fill: Color32::TRANSPARENT,
    hover: hex(0x303030),
    border: hex(0x414141),
    text: hex(0xcfcfcf),
    ..BUTTON
};
pub const BUTTON_CHIP_ON: ButtonStyle = ButtonStyle {
    fill: SELECTED,
    hover: SELECTED,
    border: SELECTED_BORDER,
    text: Color32::WHITE,
    ..BUTTON
};

pub struct Button<'a> {
    label: &'a str,
    style: ButtonStyle,
    icon: Option<(Icon, Option<Color32>)>,
    width: Option<f32>,
    height: f32,
    padding: f32,
    font_size: f32,
    radius: u8,
    enabled: bool,
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str, style: ButtonStyle) -> Self {
        Self {
            label,
            style,
            icon: None,
            width: None,
            height: 28.0,
            padding: 10.0,
            font_size: 12.0,
            radius: 4,
            enabled: true,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some((icon, None));
        self
    }

    pub fn colored_icon(mut self, icon: Icon, color: Color32) -> Self {
        self.icon = Some((icon, Some(color)));
        self
    }

    // 指定すると中身を中央寄せにする
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }

    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    pub fn radius(mut self, radius: u8) -> Self {
        self.radius = radius;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        const GAP: f32 = 6.0;
        const ICON_SIZE: f32 = 12.0;
        let style = self.style;

        let label = text(ui, self.label, self.font_size, style.text);

        let icon_width = if self.icon.is_some() { ICON_SIZE + GAP } else { 0.0 };
        let content_width = icon_width + label.size().x;
        let width = self.width.unwrap_or(content_width + self.padding * 2.0);

        let sense = if self.enabled { Sense::click() } else { Sense::hover() };
        let (rect, response) = ui.allocate_exact_size(vec2(width, self.height), sense);
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let fill = if self.enabled && response.hovered() { style.hover } else { style.fill };
            painter.rect(
                rect,
                CornerRadius::same(self.radius),
                fill,
                hairline(style.border),
                StrokeKind::Inside,
            );

            let center_y = rect.center().y;
            let mut x = match self.width {
                Some(_) => rect.center().x - content_width / 2.0,
                None => rect.left() + self.padding,
            };
            if let Some((icon, color)) = self.icon {
                let icon_rect = Rect::from_center_size(pos2(x + ICON_SIZE / 2.0, center_y), Vec2::splat(ICON_SIZE));
                draw_icon(painter, icon_rect, icon, color.unwrap_or(style.text), 1.5);
                x += icon_width;
            }
            let label_pos = pos2(x, center_y - label.size().y / 2.0);
            if style.bold {
                // 太字のフォントを読み込んでいないので、少しずらして重ね描きする
                painter.galley(label_pos + vec2(0.4, 0.0), label.clone(), style.text);
            }
            painter.galley(label_pos, label, style.text);
        }
        if self.enabled {
            response.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            response
        }
    }
}

// ---------------------------------------------------------------- 見出し

// 「シーン」「プロパティ」などの帯。right には右端に置くものを描く
pub fn section_header(ui: &mut Ui, title: &str, count: Option<String>, right: impl FnOnce(&mut Ui)) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::ZERO, SECTION);
    painter.hline(rect.x_range(), rect.top() + 0.5, hairline(SECTION_BORDER));
    painter.hline(rect.x_range(), rect.bottom() - 0.5, hairline(SECTION_BORDER));

    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(10.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 6.0;
    child.label(egui::RichText::new(title).size(11.5).color(TEXT_HEADING));
    if let Some(count) = count {
        child.label(egui::RichText::new(count).size(11.0).color(TEXT_MUTED));
    }
    child.with_layout(Layout::right_to_left(Align::Center), right);
}

// 「トランスフォーム」などの小見出し (下線つき)
pub fn sub_header(ui: &mut Ui, title: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    let painter = ui.painter();
    painter.hline(rect.x_range(), rect.bottom() - 0.5, hairline(DIVIDER));
    painter.text(
        rect.left_center(),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(11.0),
        TEXT_SUB,
    );
}

// 折りたためる小見出し。右端に今の値のまとめを出す
pub fn collapsing_header(ui: &mut Ui, open: &mut bool, title: &str, summary: &str) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    if response.clicked() {
        *open = !*open;
    }
    let painter = ui.painter();
    painter.hline(rect.x_range(), rect.bottom() - 0.5, hairline(DIVIDER));
    let chevron = if *open { Icon::ChevronDown } else { Icon::ChevronRight };
    let color = if response.hovered() { TEXT_STRONG } else { TEXT_SUB };
    draw_icon(
        painter,
        Rect::from_center_size(pos2(rect.left() + 5.0, rect.center().y), Vec2::splat(10.0)),
        chevron,
        color,
        2.0,
    );
    painter.text(
        pos2(rect.left() + 16.0, rect.center().y),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(11.0),
        color,
    );
    painter.text(
        rect.right_center(),
        Align2::RIGHT_CENTER,
        summary,
        FontId::proportional(11.0),
        TEXT_MUTED,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand);
}

// ---------------------------------------------------------------- 一覧

// シーン一覧の 1 行
pub fn list_row(ui: &mut Ui, selected: bool, icon: Icon, icon_color: Color32, name: &str, tag: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if selected {
            SELECTED
        } else if response.hovered() {
            ROW_HOVER
        } else {
            Color32::TRANSPARENT
        };
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(3), fill);

        let center_y = rect.center().y;
        draw_icon(
            painter,
            Rect::from_center_size(pos2(rect.left() + 15.0, center_y), Vec2::splat(14.0)),
            icon,
            icon_color,
            1.35,
        );
        let tag = text(ui, tag, 10.5, if selected { SELECTED_TAG } else { TEXT_MUTED });
        let tag_x = rect.right() - 8.0 - tag.size().x;
        let name_x = rect.left() + 30.0;
        let name = truncated_text(
            ui,
            name,
            12.0,
            if selected { Color32::WHITE } else { hex(0xd6d6d6) },
            tag_x - 8.0 - name_x,
        );
        painter.galley(pos2(name_x, center_y - name.size().y / 2.0), name, TEXT);
        painter.galley(pos2(tag_x, center_y - tag.size().y / 2.0), tag, TEXT_MUTED);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// 一覧の先頭に置く「＋ 追加…」の行 (枠だけで塗らない)。busy の間は押せない
pub fn add_row(ui: &mut Ui, label: &str, tag: &str, busy: bool) -> Response {
    let sense = if busy { Sense::hover() } else { Sense::click() };
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), sense);
    if ui.is_rect_visible(rect) {
        let hovered = !busy && response.hovered();
        let painter = ui.painter();
        let fill = if hovered { ROW_HOVER } else { Color32::TRANSPARENT };
        painter.rect(
            rect.shrink(0.5),
            CornerRadius::same(3),
            fill,
            hairline(if hovered { hex(0x555555) } else { hex(0x3c3c3c) }),
            StrokeKind::Inside,
        );
        let center_y = rect.center().y;
        let color = if busy { TEXT_MUTED } else if hovered { TEXT_STRONG } else { TEXT_SUB };
        draw_icon(
            painter,
            Rect::from_center_size(pos2(rect.left() + 15.0, center_y), Vec2::splat(12.0)),
            if busy { Icon::Spinner } else { Icon::Plus },
            color,
            1.6,
        );
        let label = text(ui, label, 12.0, color);
        painter.galley(pos2(rect.left() + 30.0, center_y - label.size().y / 2.0), label, color);
        let tag = text(ui, tag, 10.5, TEXT_MUTED);
        painter.galley(
            pos2(rect.right() - 8.0 - tag.size().x, center_y - tag.size().y / 2.0),
            tag,
            TEXT_MUTED,
        );
    }
    if busy {
        response
    } else {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    }
}

// 一覧の中の小さな区切り (「オブジェクト」など)
pub fn group_label(ui: &mut Ui, label: &str, top: f32) {
    let galley = text(ui, label, 10.0, TEXT_FAINT);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), top + galley.size().y + 2.0), Sense::hover());
    ui.painter()
        .galley(pos2(rect.left() + 8.0, rect.top() + top), galley, TEXT_FAINT);
}

// ---------------------------------------------------------------- 数値入力

// 背景つきの数値欄。左に軸名などのラベル、右に単位を出す。ドラッグでも、クリックして入力でも変えられる
pub struct NumberField<'a> {
    value: &'a mut f32,
    label: Option<(&'a str, Color32)>,
    accent: Option<Color32>,
    unit: Option<&'a str>,
    speed: f64,
    decimals: usize,
    range: Option<RangeInclusive<f32>>,
}

impl<'a> NumberField<'a> {
    pub fn new(value: &'a mut f32, speed: f64, decimals: usize) -> Self {
        Self {
            value,
            label: None,
            accent: None,
            unit: None,
            speed,
            decimals,
            range: None,
        }
    }

    // 欄の下端に引く色の線 (表の列の軸を表す)
    pub fn accent(mut self, color: Color32) -> Self {
        self.accent = Some(color);
        self
    }

    pub fn prefix(mut self, prefix: &'a str) -> Self {
        self.label = Some((prefix, TEXT_UNIT));
        self
    }

    pub fn unit(mut self, unit: &'a str) -> Self {
        self.unit = Some(unit);
        self
    }

    pub fn range(mut self, range: RangeInclusive<f32>) -> Self {
        self.range = Some(range);
        self
    }

    pub fn show(self, ui: &mut Ui, width: f32) -> Response {
        let (rect, _) = ui.allocate_exact_size(vec2(width, 24.0), Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::same(4), FIELD);
        if let Some(color) = self.accent {
            let line = Rect::from_min_max(pos2(rect.left() + 3.0, rect.bottom() - 2.0), pos2(rect.right() - 3.0, rect.bottom()));
            ui.painter().rect_filled(line, CornerRadius::same(1), color.gamma_multiply(0.75));
        }

        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink2(vec2(6.0, 0.0)))
                .layout(Layout::left_to_right(Align::Center)),
        );
        child.spacing_mut().item_spacing.x = 3.0;
        if let Some((label, color)) = self.label {
            child.label(egui::RichText::new(label).size(10.0).color(color));
        }
        child
            .with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(unit) = self.unit {
                    ui.label(egui::RichText::new(unit).size(10.0).color(TEXT_UNIT));
                }
                style_value(ui);
                let mut drag = DragValue::new(self.value)
                    .speed(self.speed)
                    .fixed_decimals(self.decimals);
                if let Some(range) = self.range {
                    drag = drag.range(range);
                }
                ui.add(drag)
            })
            .inner
    }
}

// 数値欄の中の DragValue から枠を消し、欄の背景になじませる
fn style_value(ui: &mut Ui) {
    let style = ui.style_mut();
    style.override_font_id = Some(FontId::proportional(12.0));
    style.spacing.button_padding = vec2(2.0, 0.0);
    style.spacing.interact_size = vec2(40.0, 20.0);
    let visuals = &mut style.visuals;
    visuals.extreme_bg_color = hex(0x2a2a2a);
    visuals.text_edit_bg_color = Some(hex(0x2a2a2a));
    visuals.selection.stroke = hairline(FOCUS);
    let widgets = &mut visuals.widgets;
    for w in [
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        w.bg_fill = Color32::TRANSPARENT;
        w.weak_bg_fill = Color32::TRANSPARENT;
        w.bg_stroke = Stroke::NONE;
        w.expansion = 0.0;
    }
    widgets.inactive.fg_stroke.color = TEXT_VALUE;
    widgets.hovered.fg_stroke.color = Color32::WHITE;
    widgets.active.fg_stroke.color = Color32::WHITE;
}
