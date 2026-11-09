//! The status bar at the bottom of a project window and its text widgets.
//!
//! Look ported from `platform/platform-impl/src/com/intellij/openapi/wm/impl/status/IdeStatusBarImpl.kt`
//! and `TextPanel.kt` with `JBUI.CurrentTheme.StatusBar` colors: a top border, widgets as plain
//! text with a hover background.
//!
//! Deviations: no breadcrumbs (no editor), no widget popups, no progress bar - background work is
//! shown as text ([`StatusWidget::progress`]).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px, svg,
};

use crate::metrics::{STATUS_BAR_HEIGHT, STATUS_BAR_WIDGET_PADDING, UI_FONT_SIZE};
use crate::theme::{ActiveIjTheme, color};

/// A text widget (`TextPanel`), optionally with an icon.
#[derive(IntoElement)]
pub struct StatusWidget {
    id: ElementId,
    text: SharedString,
    icon: Option<SharedString>,
    tooltip: Option<SharedString>,
}

impl StatusWidget {
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> StatusWidget {
        StatusWidget {
            id: id.into(),
            text: text.into(),
            icon: None,
            tooltip: None,
        }
    }

    /// An icon name (see [`crate::icons`]) drawn before the text.
    pub fn icon(mut self, icon: impl Into<SharedString>) -> StatusWidget {
        self.icon = Some(icon.into());
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> StatusWidget {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// Background work in progress, e.g. "Loading log…".
    pub fn progress(id: impl Into<ElementId>, text: impl Into<SharedString>) -> StatusWidget {
        StatusWidget::new(id, text).icon("general/refresh")
    }
}

impl RenderOnce for StatusWidget {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.ij();
        let t = theme.tokens;
        div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(4.0))
            .h(px(STATUS_BAR_HEIGHT - 6.0))
            .px(px(STATUS_BAR_WIDGET_PADDING))
            .rounded(px(4.0))
            .text_color(color(t.status_bar_widget_foreground))
            .hover(|d| d.bg(color(t.status_bar_widget_hover_background)))
            .when_some(self.icon, |d, icon| {
                d.child(
                    svg()
                        .path(crate::icons::path(&icon, theme.dark))
                        .size(px(16.0))
                        .text_color(color(t.status_bar_widget_foreground)),
                )
            })
            .child(self.text)
            .when_some(self.tooltip, |d, tooltip| {
                d.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
            })
    }
}

/// The status bar: `left` widgets (messages) and `right` widgets (branch, progress).
pub fn status_bar(left: Vec<AnyElement>, right: Vec<AnyElement>, cx: &App) -> impl IntoElement {
    let t = cx.ij().tokens;
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(px(STATUS_BAR_HEIGHT))
        .px(px(STATUS_BAR_WIDGET_PADDING))
        .border_t_1()
        .border_color(color(t.status_bar_border))
        .bg(color(t.status_bar_background))
        .text_size(px(UI_FONT_SIZE))
        .children(left)
        .child(div().flex_1())
        .children(right)
}
