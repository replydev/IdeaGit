//! Reference labels at the right of the Subject cell.
//!
//! Port of `platform/vcs-log/impl/src/com/intellij/vcs/log/ui/render/LabelPainter.java` (New UI
//! path: no grey background, icon-text padding), `LabelIcon.java` (`paintTagsForNewUi`, widths)
//! and `TagPainter.kt` (the tag outline from `expui/vcs/currentBranch.svg`), plus the
//! `GraphCommitCellRenderer.getAvailableWidth` rule.
//!
//! [`layout`] is pure (text widths come from a measuring function); [`paint`] draws with GPUI.
//!
//! Deviations: no bookmarks; string lengths count chars, not UTF-16 units; the detached-HEAD
//! warning icon is drawn as a single-color mask (GPUI `paint_svg`) at the label height.

use gpui::{
    App, Bounds, Font, Hsla, PathBuilder, PathStyle, Pixels, Point, SharedString, StrokeOptions,
    TextAlign, TextRun, Window, point, px,
};
use ig_ui_kit::metrics::label::{
    COMPACT_MIDDLE_PADDING, DISPLAYED_MESSAGE_PART, ICON_ADDITIONAL_PADDING, ICON_SIZE_UNITS,
    ICON_TEXT_PADDING, LEFT_PADDING, MAX_LENGTH, MIDDLE_PADDING, RIGHT_PADDING,
};

use super::ref_groups::{RefColor, RefGroup};

const TWO_DOTS: &str = "..";
const SEPARATOR: &str = "/";
const ELLIPSIS: &str = "\u{2026}";

/// One painted label: an icon (tag shapes or the warning icon) and text.
#[derive(Clone, Debug, PartialEq)]
pub struct Presentation {
    pub text: String,
    pub icon: LabelIcon,
    pub width: f32,
    pub icon_padding: f32,
    /// The detached-HEAD warning text color instead of the label foreground.
    pub warning: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LabelIcon {
    /// Overlapping tag shapes, one per color.
    Tags(Vec<RefColor>),
    Warning,
}

impl LabelIcon {
    fn width(&self, height: f32) -> f32 {
        match self {
            LabelIcon::Tags(colors) => tags_width(height, colors.len()),
            LabelIcon::Warning => height,
        }
    }
}

/// `LabelIcon.getWidth(height, labelsCount)`.
pub fn tags_width(height: f32, count: usize) -> f32 {
    let scale = height / ICON_SIZE_UNITS;
    ((ICON_SIZE_UNITS + 2.0 * (count as f32 - 1.0)) * scale).round()
}

/// The labels of a row and their total width (0 when none).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Labels {
    pub list: Vec<Presentation>,
    pub width: f32,
    pub compact: bool,
}

/// `GraphCommitCellRenderer.getAvailableWidth`: the space labels may take in a Subject cell of
/// `column_width` with `graph_width` of graph and a subject `text_width` wide.
pub fn available_width(column_width: f32, graph_width: f32, text_width: f32, compact: bool) -> f32 {
    let text_and_labels = column_width - graph_width;
    let free = text_and_labels - text_width;
    let allowed = if compact {
        free.min(text_and_labels / 3.0)
    } else {
        free.max((text_and_labels / 2.0).max(text_and_labels - DISPLAYED_MESSAGE_PART))
    };
    allowed.max(0.0).trunc()
}

/// `calculatePresentation` (right-aligned labels, no bookmarks). `height` is the reference font's
/// height (`fontMetrics.getHeight()`), `measure` its `stringWidth`.
pub fn layout(
    groups: &[RefGroup],
    available: f32,
    height: f32,
    compact: bool,
    measure: &dyn Fn(&str) -> f32,
) -> Labels {
    if groups.is_empty() {
        return Labels {
            compact,
            ..Labels::default()
        };
    }
    let side = LEFT_PADDING + RIGHT_PADDING;
    let middle = if compact {
        COMPACT_MIDDLE_PADDING
    } else {
        MIDDLE_PADDING
    };
    let detached = groups.first().filter(|g| g.detached_head).map(|g| {
        presentation(
            &g.name,
            LabelIcon::Warning,
            height,
            ICON_ADDITIONAL_PADDING,
            true,
            measure,
        )
    });
    let detached_width = detached.as_ref().map_or(0.0, |d| d.width + middle);
    let remaining = available - detached_width - side;
    let mut list: Vec<Presentation> = detached.into_iter().collect();
    list.extend(reference_labels(
        groups, remaining, height, compact, measure,
    ));
    let width = if list.is_empty() {
        0.0
    } else {
        list.iter().map(|p| p.width).sum::<f32>() + middle * (list.len() - 1) as f32 + side
    };
    Labels {
        list,
        width,
        compact,
    }
}

fn text_width(text: &str, measure: &dyn Fn(&str) -> f32) -> f32 {
    let w = measure(text);
    if w > 0.0 { ICON_TEXT_PADDING + w } else { 0.0 }
}

fn presentation(
    text: &str,
    icon: LabelIcon,
    height: f32,
    icon_padding: f32,
    warning: bool,
    measure: &dyn Fn(&str) -> f32,
) -> Presentation {
    let width = icon.width(height) + icon_padding + text_width(text, measure);
    Presentation {
        text: text.to_string(),
        icon,
        width,
        icon_padding,
        warning,
    }
}

/// `LabelPainter.getColors(groups)`: colors in order of first use, twice when used more than once.
fn merged_colors(groups: &[RefGroup]) -> Vec<RefColor> {
    let mut used: Vec<(RefColor, usize)> = Vec::new();
    for color in groups.iter().flat_map(|g| g.colors()) {
        match used.iter_mut().find(|(c, _)| *c == color) {
            Some((_, n)) => *n += 1,
            None => used.push((color, 1)),
        }
    }
    used.into_iter()
        .flat_map(|(c, n)| std::iter::repeat_n(c, if n > 1 { 2 } else { 1 }))
        .collect()
}

/// `processReferenceLabels`.
fn reference_labels(
    groups: &[RefGroup],
    available: f32,
    height: f32,
    compact: bool,
    measure: &dyn Fn(&str) -> f32,
) -> Vec<Presentation> {
    let mut labels = Vec::new();
    let mut references_width = 0.0;
    let middle = if compact {
        COMPACT_MIDDLE_PADDING
    } else {
        MIDDLE_PADDING
    };
    for (i, group) in groups.iter().enumerate() {
        if group.detached_head {
            continue;
        }
        let do_not_fit = if !compact && i < groups.len() - 1 {
            tags_width(height, merged_colors(&groups[i + 1..]).len())
        } else {
            0.0
        };
        let icon = LabelIcon::Tags(group.colors());
        let is_last = i == groups.len() - 1;
        let mut new_width =
            references_width + icon.width(height) + if is_last { 0.0 } else { middle };
        let text = shorten_ref_name(&group.name, available - new_width - do_not_fit, measure);
        new_width += text_width(&text, measure);
        if !compact && available - new_width - do_not_fit < 0.0 {
            let last_icon = LabelIcon::Tags(merged_colors(&groups[i..]));
            let name = if labels.is_empty() {
                text
            } else {
                String::new()
            };
            labels.push(presentation(&name, last_icon, height, 0.0, false, measure));
            break;
        }
        labels.push(presentation(&text, icon, height, 0.0, false, measure));
        references_width = new_width;
    }
    labels
}

/// `StringUtil.shortenTextWithEllipsis(text, maxLength, 0, symbol)`.
fn shorten_with_ellipsis(text: &str, max_length: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_length {
        return text.to_string();
    }
    let prefix = max_length.saturating_sub(ELLIPSIS.chars().count());
    chars[..prefix].iter().collect::<String>() + ELLIPSIS
}

/// `LabelPainter.shortenRefName` with `VcsLogUiUtil.shortenTextToFit`.
fn shorten_ref_name(name: &str, available: f32, measure: &dyn Fn(&str) -> f32) -> String {
    if measure(name) > available && name.chars().count() > MAX_LENGTH {
        let mut name = name.to_string();
        if let Some(separator) = name.find(SEPARATOR)
            && separator > TWO_DOTS.len()
        {
            name = format!("{TWO_DOTS}{}", &name[separator..]);
        }
        if available > 0.0 {
            if measure(&name) <= available {
                return name;
            }
            for length in (MAX_LENGTH + 1..=name.chars().count()).rev() {
                let shortened = shorten_with_ellipsis(&name, length);
                if measure(&shortened) <= available {
                    return shortened;
                }
            }
        }
        return shorten_with_ellipsis(&name, MAX_LENGTH);
    }
    name.to_string()
}

/// How labels are painted.
pub struct LabelStyle {
    pub font: Font,
    pub font_size: Pixels,
    /// The font's line height (`fontMetrics.getHeight()`).
    pub height: f32,
    pub foreground: Hsla,
    pub warning_foreground: Hsla,
    pub background: Hsla,
    pub color_of: fn(RefColor, &App) -> Hsla,
    /// Asset path of the warning icon and the color it is drawn with.
    pub warning_icon: SharedString,
    pub warning_icon_color: Hsla,
}

/// `LabelPainter.paint(g2, x, y, height)`: `bounds` is the label area of the row.
pub fn paint(
    labels: &Labels,
    style: &LabelStyle,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    if labels.list.is_empty() {
        return;
    }
    // opaque: hides subject text running under the labels
    window.paint_quad(gpui::fill(bounds, style.background));
    let row_height = bounds.size.height;
    let mut x = bounds.origin.x + px(LEFT_PADDING);
    let middle = if labels.compact {
        COMPACT_MIDDLE_PADDING
    } else {
        MIDDLE_PADDING
    };
    for label in &labels.list {
        let icon_width = label.icon.width(style.height);
        let icon_y = bounds.origin.y + (row_height - px(style.height)) / 2.0;
        match &label.icon {
            LabelIcon::Tags(colors) => {
                let colors: Vec<Hsla> = colors.iter().map(|c| (style.color_of)(*c, cx)).collect();
                paint_tags(
                    point(x, icon_y),
                    style.height,
                    style.background,
                    &colors,
                    window,
                );
            }
            LabelIcon::Warning => {
                let icon = Bounds::new(
                    point(x, icon_y),
                    gpui::size(px(style.height), px(style.height)),
                );
                let _ = window.paint_svg(
                    icon,
                    style.warning_icon.clone(),
                    None,
                    gpui::TransformationMatrix::unit(),
                    style.warning_icon_color,
                    cx,
                );
            }
        }
        x += px(icon_width + ICON_TEXT_PADDING + label.icon_padding);
        if !label.text.is_empty() {
            let color = if label.warning {
                style.warning_foreground
            } else {
                style.foreground
            };
            let run = TextRun {
                len: label.text.len(),
                font: style.font.clone(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window.text_system().shape_line(
                label.text.clone().into(),
                style.font_size,
                &[run],
                None,
            );
            let width = line.width();
            let _ = line.paint(
                point(x, bounds.origin.y),
                row_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
            x += width;
        }
        x += px(middle);
    }
}

/// `LabelIcon.paintTagsForNewUi`: tags left to right, the last color drawn first.
fn paint_tags(
    origin: Point<Pixels>,
    size: f32,
    background: Hsla,
    colors: &[Hsla],
    window: &mut Window,
) {
    let scale = size / ICON_SIZE_UNITS;
    let count = colors.len();
    for i in 0..count {
        let color = colors[count - 1 - i];
        paint_tag(
            window,
            origin,
            scale * 2.0 * i as f32,
            i == count - 1,
            background,
            color,
            size,
        );
        if i != count - 1 {
            let x0 = scale * 2.0 * i as f32 + 2.0 * scale - 2f32.sqrt();
            paint_tag(window, origin, x0, false, background, background, size);
        }
    }
}

/// `TagPainter.paintTag`: the tag outline of `expui/vcs/currentBranch.svg` (a 16x16 design).
fn paint_tag(
    window: &mut Window,
    origin: Point<Pixels>,
    offset: f32,
    with_dot: bool,
    background: Hsla,
    color: Hsla,
    size: f32,
) {
    let s = size / 16.0;
    let p = |x: f32, y: f32| point(origin.x + px(x * s + offset), origin.y + px(y * s));
    let outline = |mut b: PathBuilder| {
        b.move_to(p(13.4495, 7.5001));
        b.line_to(p(7.01028, 13.9394));
        b.cubic_bezier_to(
            p(6.30317, 13.9394),
            p(6.81502, 14.1347),
            p(6.49843, 14.1347),
        );
        b.line_to(p(2.06053, 9.69679));
        b.cubic_bezier_to(
            p(2.06053, 8.98968),
            p(1.86527, 9.50152),
            p(1.86527, 9.18494),
        );
        b.line_to(p(8.49978, 2.55035));
        b.cubic_bezier_to(
            p(8.92404, 2.40893),
            p(8.61106, 2.43907),
            p(8.76824, 2.38667),
        );
        b.line_to(p(12.6363, 2.93926));
        b.cubic_bezier_to(
            p(13.0606, 3.36352),
            p(12.8563, 2.97069),
            p(13.0292, 3.14354),
        );
        b.line_to(p(13.5909, 7.07584));
        b.cubic_bezier_to(p(13.4495, 7.5001), p(13.6132, 7.23163), p(13.5608, 7.38882));
        b.close();
        b
    };
    if let Ok(path) = outline(PathBuilder::fill()).build() {
        window.paint_path(path, background);
    }
    let stroke = StrokeOptions::default()
        .with_line_width(s)
        .with_line_cap(lyon::tessellation::LineCap::Round)
        .with_line_join(lyon::tessellation::LineJoin::Round);
    if let Ok(path) =
        outline(PathBuilder::stroke(px(s)).with_style(PathStyle::Stroke(stroke))).build()
    {
        window.paint_path(path, color);
    }
    if with_dot {
        // oval(offset, scale, r = 1, center (10.5, 5.5))
        let center = p(10.5, 5.5);
        let r = px(s);
        window.paint_quad(gpui::quad(
            Bounds::new(
                point(center.x - r, center.y - r),
                gpui::size(r * 2.0, r * 2.0),
            ),
            r,
            color,
            px(0.0),
            gpui::transparent_black(),
            Default::default(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use ig_git::log::ref_manager::{LOCAL_BRANCH, TAG};
    use ig_log::data::{Hash, VcsRef, vcs_root};

    use super::*;

    fn group(name: &str, n: usize) -> RefGroup {
        let refs = (0..n)
            .map(|i| {
                VcsRef::new(
                    Hash::build("ab"),
                    &format!("{name}{i}"),
                    if i == 0 { LOCAL_BRANCH } else { TAG },
                    vcs_root("/r"),
                )
            })
            .collect();
        RefGroup {
            name: name.to_string(),
            refs,
            detached_head: false,
        }
    }

    /// Every char is 7px wide.
    fn measure(s: &str) -> f32 {
        7.0 * s.chars().count() as f32
    }

    #[test]
    fn icon_width_grows_by_two_units_per_extra_tag() {
        assert_eq!(tags_width(15.0, 1), 15.0);
        assert_eq!(tags_width(15.0, 2), 20.0); // (6.25 + 2) * 2.4 = 19.8
        assert_eq!(tags_width(15.0, 3), 25.0); // 10.25 * 2.4 = 24.6
    }

    #[test]
    fn single_label_width_includes_paddings() {
        let labels = layout(&[group("main", 1)], 500.0, 15.0, true, &measure);
        assert_eq!(labels.list.len(), 1);
        // icon 15 + text padding 1 + 28, plus left/right paddings 4 + 4
        assert_eq!(labels.list[0].width, 44.0);
        assert_eq!(labels.width, 52.0);
    }

    #[test]
    fn labels_that_do_not_fit_collapse_into_one_icon() {
        let groups = [group("first", 1), group("second", 1), group("third", 1)];
        let labels = layout(&groups, 90.0, 15.0, false, &measure);
        assert_eq!(labels.list.len(), 1);
        assert_eq!(labels.list[0].text, "first");
        assert_eq!(
            labels.list[0].icon,
            LabelIcon::Tags(vec![RefColor::LocalBranch, RefColor::LocalBranch])
        );
    }

    #[test]
    fn long_names_are_shortened_from_the_separator() {
        let name = "feature/a-very-long-branch-name-indeed";
        assert_eq!(shorten_ref_name(name, 1000.0, &measure), name);
        let short = shorten_ref_name(name, 120.0, &measure);
        assert!(
            short.starts_with("../a-very") && short.ends_with(ELLIPSIS),
            "{short}"
        );
        assert_eq!(short.chars().count(), MAX_LENGTH);
        assert_eq!(
            shorten_ref_name(name, 0.0, &measure).chars().count(),
            MAX_LENGTH
        );
    }

    #[test]
    fn available_width_follows_the_renderer() {
        // compact: at most a third of the text area
        assert_eq!(available_width(600.0, 60.0, 100.0, true), 180.0);
        // non-compact: at least half, or all but 80px of the text
        assert_eq!(available_width(600.0, 60.0, 500.0, false), 460.0);
    }
}
