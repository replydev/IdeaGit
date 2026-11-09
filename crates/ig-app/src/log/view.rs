//! The Log tab: a virtualized commit table (graph and subject with labels, author, date, hash).
//!
//! IdeaGit's own table on GPUI's `uniform_list` (spike decision: gpui-component's table keeps a
//! single selected row and draws shadcn-style borders and padding, while the Log needs
//! multi-selection, edge-to-edge graph cells and per-cell mouse hit testing). Rendering follows
//! `VcsLogGraphTable.java` and `GraphCommitCellRenderer.kt`: no header and no grid, the graph
//! painted from the row's print elements, the subject after the graph, labels right-aligned over
//! the subject's end.
//!
//! Deviations: fixed column widths (upstream sizes Author/Date/Hash from their content and lets the
//! user resize and hide columns); loading rows show "Loading…" (upstream shows nothing until the
//! details arrive); no Root column, no current-branch row highlighting, no tooltips.

use std::cell::Cell;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, Font, Hsla,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, TextRun, Window, canvas, div, font,
    px, uniform_list,
};
use gpui_component::dock::PanelEvent;
use ig_log::data::VcsLogRefManager;
use ig_ui_kit::table::TableColors;
use ig_ui_kit::{ActiveIjTheme, color};

use super::date::{DateTexts, format_pretty};
use super::graph_painter::{GraphGeometry, paint_shapes};
use super::labels::{self, LabelStyle, Labels};
use super::metadata::Lookup;
use super::model::{LogModel, LogState};
use super::ref_groups::{RefColor, RepoRefsContext, group_for_table};
use crate::actions::{LOG_TABLE_CONTEXT, RefreshLog, table};
use crate::strings;

const AUTHOR_WIDTH: f32 = 160.0;
const DATE_WIDTH: f32 = 150.0;
const HASH_WIDTH: f32 = 80.0;
const CELL_PADDING: f32 = 8.0;
/// `CommonUiProperties.COMPACT_REFERENCES_VIEW` / `SHOW_TAG_NAMES` defaults
/// (`VcsLogApplicationSettings.State`).
const COMPACT_REFERENCES: bool = true;
const SHOW_TAG_NAMES: bool = false;
/// Short hash length (spec: 8).
const HASH_LENGTH: usize = 8;

pub struct LogView {
    pub model: Entity<LogModel>,
    focus: FocusHandle,
    date_texts: DateTexts,
    /// The table width of the last frame (the Subject column takes what the others leave).
    width: Rc<Cell<f32>>,
}

impl EventEmitter<PanelEvent> for LogView {}

impl Focusable for LogView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl gpui_component::dock::BasePanel for LogView {
    fn panel_name(&self) -> &'static str {
        "GitLog"
    }
}

impl gpui_component::dock::Panel for LogView {
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some(strings::text("VcsLogBundle", "vcs.log.tab.name").into())
    }
}

fn ref_color(color: RefColor, cx: &App) -> Hsla {
    let t = cx.ij().tokens;
    ig_ui_kit::color(match color {
        RefColor::Head => t.ref_head,
        RefColor::LocalBranch => t.ref_local_branch,
        RefColor::RemoteBranch => t.ref_remote_branch,
        RefColor::Tag => t.ref_tag,
        RefColor::Other => t.ref_other,
    })
}

/// Font metrics height (`FontMetrics.getHeight`): ascent + descent, rounded up.
fn font_height(font: &Font, size: f32, window: &Window) -> f32 {
    let ts = window.text_system();
    let id = ts.resolve_font(font);
    f32::from(ts.ascent(id, px(size)) + ts.descent(id, px(size))).ceil()
}

impl LogView {
    pub fn new(model: Entity<LogModel>, cx: &mut Context<LogView>) -> LogView {
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        LogView {
            model,
            focus: cx.focus_handle(),
            date_texts: DateTexts::load(),
            width: Rc::new(Cell::new(1000.0)),
        }
    }

    fn row_height(window: &Window) -> f32 {
        let ui = font(ig_ui_kit::fonts::UI_FAMILY);
        ig_ui_kit::metrics::log_row_height(font_height(
            &ui,
            ig_ui_kit::metrics::UI_FONT_SIZE,
            window,
        ))
    }

    fn render_rows(
        &mut self,
        range: std::ops::Range<usize>,
        window: &mut Window,
        cx: &mut Context<LogView>,
    ) -> Vec<AnyElement> {
        let row_height = Self::row_height(window);
        let geometry = GraphGeometry::new(row_height, window.scale_factor());
        let colors = TableColors::get(cx);
        let focused = self.focus.contains_focused(window, cx);
        let theme = cx.ij();
        let t = theme.tokens;
        let palette = theme.graph_palette();
        let ui_font = font(ig_ui_kit::fonts::UI_FAMILY);
        let ui_size = ig_ui_kit::metrics::UI_FONT_SIZE;
        // `LabelPainter.getReferenceFont`: the label font, 1pt smaller
        let label_size = ui_size - 1.0;
        let label_height = font_height(&ui_font, label_size, window);
        let measure_with = |size: f32, s: &str, window: &Window| -> f32 {
            if s.is_empty() {
                return 0.0;
            }
            let run = TextRun {
                len: s.len(),
                font: ui_font.clone(),
                color: colors.foreground,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            window
                .text_system()
                .shape_line(SharedString::from(s.to_string()), px(size), &[run], None)
                .width()
                .into()
        };
        let subject_width = (self.width.get() - AUTHOR_WIDTH - DATE_WIDTH - HASH_WIDTH).max(100.0);
        let now = chrono::Local::now().timestamp_millis();
        let loading = SharedString::from("Loading\u{2026}");
        let detached = strings::text("VcsLogBundle", "vcs.log.references.detached.head");
        let view = cx.entity().downgrade();

        let model = self.model.clone();
        model.update(cx, |model, cx| {
            model.request_metadata(range.clone());
            let hand = model.hand_cursor;
            let snapshot = model.snapshot();
            let recommended_width = snapshot.map_or(0, |s| s.recommended_width);
            let mut rows = Vec::with_capacity(range.len());
            for row in range {
                let Some((commit, elements)) = model.row(row) else {
                    continue;
                };
                let meta = match model.metadata(commit) {
                    Lookup::Loaded(meta) => Some(meta),
                    _ => None,
                };
                let selected = model.is_selected(commit);
                let row_colors = colors.row(selected, focused, false);
                let graph_width = geometry.graph_width(&elements, recommended_width);
                let shapes = geometry.shapes(
                    &elements,
                    |id| palette.color(id),
                    if theme.dark { t.foreground >> 8 } else { 0 },
                );

                // labels: refs sorted by the root's ref manager, grouped like `groupForTable`
                let snapshot = model.snapshot().expect("rows come from a snapshot");
                let mut refs = snapshot.data.refs_model().refs_to_commit(commit);
                let labels = if refs.is_empty() {
                    Labels::default()
                } else {
                    let root = refs[0].root().clone();
                    if let Some(provider) = snapshot.data.log_providers().get(&root) {
                        let manager: std::sync::Arc<dyn VcsLogRefManager> =
                            provider.reference_manager();
                        manager.sort_labels(&mut refs);
                    }
                    let repo = snapshot.repo(&root);
                    let context = RepoRefsContext {
                        current_branch: repo.and_then(|r| r.current_branch.as_deref()),
                        remotes: repo.map(|r| r.remotes.as_slice()).unwrap_or(&[]),
                    };
                    let groups = group_for_table(
                        &refs,
                        &context,
                        COMPACT_REFERENCES,
                        SHOW_TAG_NAMES,
                        &detached,
                    );
                    let subject = meta.as_ref().map(|m| m.subject.as_str()).unwrap_or("");
                    let text_width = measure_with(ui_size, subject, window) + CELL_PADDING;
                    let available = labels::available_width(
                        subject_width,
                        graph_width,
                        text_width,
                        COMPACT_REFERENCES,
                    );
                    labels::layout(&groups, available, label_height, COMPACT_REFERENCES, &|s| {
                        measure_with(label_size, s, window)
                    })
                };
                let label_style = LabelStyle {
                    font: ui_font.clone(),
                    font_size: px(label_size),
                    height: label_height,
                    foreground: color(t.log_reference_foreground),
                    warning_foreground: color(t.banner_warning_border),
                    background: row_colors.background,
                    color_of: ref_color,
                    warning_icon: ig_ui_kit::icons::path("general/warning", theme.dark),
                    warning_icon_color: color(t.ref_head),
                };

                let (subject, author, date, hash): (
                    SharedString,
                    SharedString,
                    SharedString,
                    SharedString,
                ) = match &meta {
                    Some(m) => (
                        m.subject.clone().into(),
                        m.author.clone().into(),
                        format_pretty(m.author_time, now, &chrono::Local, &self.date_texts).into(),
                        m.hash
                            .as_string()
                            .chars()
                            .take(HASH_LENGTH)
                            .collect::<String>()
                            .into(),
                    ),
                    None => (loading.clone(), "".into(), "".into(), "".into()),
                };
                let fg = if meta.is_some() {
                    row_colors.foreground
                } else {
                    colors.secondary_foreground
                };
                let background = row_colors.background;
                let (geometry_move, geometry_down) = (geometry.clone(), geometry.clone());
                let (view_move, view_down) = (view.clone(), view.clone());
                let labels_width = labels.width;

                let graph = canvas(
                    |_, _, _| {},
                    move |bounds, _, window, cx| {
                        paint_shapes(&shapes, bounds, background, window, cx);
                        window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                            if !phase.bubble() || !bounds.contains(&e.position) {
                                return;
                            }
                            let (x, y): (f32, f32) = (
                                (e.position.x - bounds.origin.x).into(),
                                (e.position.y - bounds.origin.y).into(),
                            );
                            let g = geometry_move.clone();
                            let _ = view_move.update(cx, |view, cx| {
                                if view.model.update(cx, |m, _| m.hover_graph(row, x, y, &g)) {
                                    cx.notify();
                                }
                            });
                        });
                        window.on_mouse_event(move |e: &MouseDownEvent, phase, _, cx| {
                            if !phase.bubble()
                                || e.button != MouseButton::Left
                                || !bounds.contains(&e.position)
                            {
                                return;
                            }
                            let (x, y): (f32, f32) = (
                                (e.position.x - bounds.origin.x).into(),
                                (e.position.y - bounds.origin.y).into(),
                            );
                            let g = geometry_down.clone();
                            let handled = view_down
                                .update(cx, |view, cx| {
                                    let handled =
                                        view.model.update(cx, |m, _| m.click_graph(row, x, y, &g));
                                    cx.notify();
                                    handled
                                })
                                .unwrap_or(false);
                            if handled {
                                cx.stop_propagation();
                            }
                        });
                    },
                )
                .absolute()
                .left_0()
                .top_0()
                .h_full()
                .w(px(graph_width));

                let label_canvas = canvas(
                    |_, _, _| {},
                    move |bounds, _, window, cx| {
                        labels::paint(&labels, &label_style, bounds, window, cx)
                    },
                )
                .absolute()
                .right_0()
                .top_0()
                .h_full()
                .w(px(labels_width));

                let cell = |text: SharedString, width: f32| {
                    div()
                        .flex_none()
                        .w(px(width))
                        .px(px(CELL_PADDING))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(text)
                };
                let view_click = view.clone();
                rows.push(
                    div()
                        .id(("git-log-row", row))
                        .h(px(row_height))
                        .w_full()
                        .flex()
                        .items_center()
                        .bg(background)
                        .text_color(fg)
                        .when(!selected, |d| d.hover(|d| d.bg(colors.hover)))
                        .on_mouse_down(MouseButton::Left, move |e, window, cx| {
                            let (toggle, extend) = (e.modifiers.secondary(), e.modifiers.shift);
                            let _ = view_click.update(cx, |view, cx| {
                                window.focus(&view.focus, cx);
                                view.model.update(cx, |m, cx| {
                                    m.click_row(row, toggle, extend);
                                    cx.notify();
                                });
                            });
                        })
                        .child(
                            div()
                                .id(("git-log-subject", row))
                                .relative()
                                .flex_1()
                                .h_full()
                                .flex()
                                .items_center()
                                .overflow_hidden()
                                .when(hand, |d| d.cursor_pointer())
                                .on_hover({
                                    let view = view.clone();
                                    move |hovered, _, cx| {
                                        if !*hovered {
                                            let _ = view.update(cx, |view, cx| {
                                                if view.model.update(cx, |m, _| m.unhover_graph()) {
                                                    cx.notify();
                                                }
                                            });
                                        }
                                    }
                                })
                                .child(graph)
                                .child(
                                    div()
                                        .pl(px(graph_width))
                                        .pr(px(labels_width))
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .child(subject),
                                )
                                .child(label_canvas),
                        )
                        .child(cell(author, AUTHOR_WIDTH))
                        .child(cell(date, DATE_WIDTH))
                        .child(cell(hash, HASH_WIDTH))
                        .into_any_element(),
                );
            }
            if !model.first_rows_logged && model.metadata_loaded_for(rows.len()) {
                model.first_rows_logged = true;
                log::info!(
                    "first {} rows with metadata painted after {:?}",
                    rows.len(),
                    model.started.elapsed()
                );
            }
            let _ = cx;
            rows
        })
    }

    fn render_message(&self, text: String, cx: &App) -> AnyElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(color(cx.ij().tokens.secondary_foreground))
            .child(text)
            .into_any_element()
    }
}

impl Render for LogView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<LogView>) -> impl IntoElement {
        let colors = TableColors::get(cx);
        let (state, count) = {
            let model = self.model.read(cx);
            (model.state(), model.row_count())
        };
        let width = self.width.clone();
        let body = match state {
            LogState::NoRoots => self.render_message(
                strings::text("VcsLogBundle", "vcs.log.is.not.available"),
                cx,
            ),
            LogState::Loading => {
                self.render_message(strings::text("VcsLogBundle", "vcs.log.is.loading"), cx)
            }
            LogState::Error(message) => div()
                .p(px(12.0))
                .child(
                    ig_ui_kit::notification::Banner::new(
                        "git-log-error",
                        ig_ui_kit::notification::Severity::Error,
                        format!(
                            "{}: {message}",
                            strings::text("VcsLogBundle", "vcs.log.error.loading.commits.status")
                        ),
                    )
                    .action(ig_ui_kit::notification::Action::new(
                        strings::text("VcsLogBundle", "vcs.log.refresh.status.action"),
                        |_, window, cx| window.dispatch_action(Box::new(RefreshLog), cx),
                    )),
                )
                .into_any_element(),
            LogState::Ready if count == 0 => self.render_message(
                strings::text("VcsLogBundle", "vcs.log.no.commits.status"),
                cx,
            ),
            LogState::Ready => {
                let scroll = self.model.read(cx).scroll.clone();
                uniform_list(
                    "git-log-rows",
                    count,
                    cx.processor(|view, range, window, cx| view.render_rows(range, window, cx)),
                )
                .size_full()
                .track_scroll(&scroll)
                .into_any_element()
            }
        };
        let _ = window;
        let model = self.model.clone();
        let on = move |f: fn(&mut LogModel)| {
            let model = model.clone();
            move |_: &dyn gpui::Action, _: &mut Window, cx: &mut App| {
                model.update(cx, |m, cx| {
                    f(m);
                    cx.notify();
                })
            }
        };
        let (up, down, ext_up, ext_down, pg_up, pg_down, first, last, all) = (
            on(|m| m.move_selection(-1, false)),
            on(|m| m.move_selection(1, false)),
            on(|m| m.move_selection(-1, true)),
            on(|m| m.move_selection(1, true)),
            on(|m| m.move_selection(-20, false)),
            on(|m| m.move_selection(20, false)),
            on(|m| m.move_selection(isize::MIN / 2, false)),
            on(|m| m.move_selection(isize::MAX / 2, false)),
            on(|m| m.select_all()),
        );
        div()
            .id("git-log")
            .key_context(LOG_TABLE_CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .bg(colors.background)
            .text_size(px(ig_ui_kit::metrics::UI_FONT_SIZE))
            .on_action(move |a: &table::SelectUp, w, cx| up(a, w, cx))
            .on_action(move |a: &table::SelectDown, w, cx| down(a, w, cx))
            .on_action(move |a: &table::ExtendUp, w, cx| ext_up(a, w, cx))
            .on_action(move |a: &table::ExtendDown, w, cx| ext_down(a, w, cx))
            .on_action(move |a: &table::PageUp, w, cx| pg_up(a, w, cx))
            .on_action(move |a: &table::PageDown, w, cx| pg_down(a, w, cx))
            .on_action(move |a: &table::SelectFirst, w, cx| first(a, w, cx))
            .on_action(move |a: &table::SelectLast, w, cx| last(a, w, cx))
            .on_action(move |a: &table::SelectAll, w, cx| all(a, w, cx))
            .child(
                canvas(
                    move |bounds, _, _| width.set(f32::from(bounds.size.width)),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(body)
    }
}
