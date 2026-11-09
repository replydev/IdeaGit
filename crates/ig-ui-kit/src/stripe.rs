//! New UI tool-window stripes: the vertical bars of square tool-window buttons at the window edges.
//!
//! Look ported from `platform/platform-impl/src/com/intellij/toolWindow/ToolWindowToolbar.kt`,
//! `.../openapi/wm/impl/SquareStripeButton.kt` and `SquareStripeButtonLook.kt`: 40x40 buttons, a
//! 30x30 rounded highlight (hover, or the selected color while the tool window is open), 20px
//! icons, and a separator on the inner edge.
//!
//! Deviations: no drag and drop between stripes, no "more" button, no names under icons.

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px, svg,
};

use crate::metrics::{
    STRIPE_BUTTON_HIGHLIGHT_ARC, STRIPE_BUTTON_HIGHLIGHT_SIZE, STRIPE_BUTTON_ICON_SIZE,
    STRIPE_BUTTON_SIZE, STRIPE_WIDTH,
};
use crate::theme::{ActiveIjTheme, color};

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Which window edge a stripe sits on; the separator is drawn on the inner edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StripeSide {
    Left,
    Right,
}

/// A square tool-window button (`SquareStripeButton`).
#[derive(IntoElement)]
pub struct StripeButton {
    id: ElementId,
    icon: SharedString,
    tooltip: SharedString,
    selected: bool,
    on_click: Option<ClickHandler>,
}

impl StripeButton {
    /// `icon` is an icon name (see [`crate::icons`]), e.g. `toolwindows/git`.
    pub fn new(id: impl Into<ElementId>, icon: impl Into<SharedString>) -> StripeButton {
        StripeButton {
            id: id.into(),
            icon: icon.into(),
            tooltip: SharedString::default(),
            selected: false,
            on_click: None,
        }
    }

    /// The tool window is open (upstream `PUSHED` state).
    pub fn selected(mut self, selected: bool) -> StripeButton {
        self.selected = selected;
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> StripeButton {
        self.tooltip = tooltip.into();
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> StripeButton {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for StripeButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.ij();
        let t = theme.tokens;
        // The expui tool-window icons are single-color; draw them as a mask so the selected state
        // can use `ToolWindow.Button.selectedForeground`, as upstream swaps to a selected icon.
        let icon_color = if self.selected {
            t.stripe_button_selected_foreground
        } else {
            t.stripe_button_foreground
        };
        let on_click = self.on_click.clone();
        let tooltip = self.tooltip.clone();
        div()
            .id(self.id)
            .when(!tooltip.is_empty(), |d| {
                d.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
            })
            .size(px(STRIPE_BUTTON_SIZE))
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .size(px(STRIPE_BUTTON_HIGHLIGHT_SIZE))
                    .rounded(px(STRIPE_BUTTON_HIGHLIGHT_ARC / 2.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(self.selected, |d| {
                        d.bg(color(t.stripe_button_selected_background))
                    })
                    .when(!self.selected, |d| {
                        d.hover(|d| d.bg(color(t.stripe_button_hover_background)))
                    })
                    .child(
                        svg()
                            .path(crate::icons::path(&self.icon, theme.dark))
                            .size(px(STRIPE_BUTTON_ICON_SIZE))
                            .text_color(color(icon_color)),
                    ),
            )
            .when_some(on_click, |d, handler| {
                d.on_click(move |e, w, cx| handler(e, w, cx))
            })
    }
}

/// A stripe: buttons from the top, separator on the inner edge.
pub fn stripe(side: StripeSide, buttons: Vec<StripeButton>, cx: &App) -> impl IntoElement {
    let t = cx.ij().tokens;
    let d = div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(STRIPE_WIDTH))
        .h_full()
        .bg(color(t.stripe_background))
        .border_color(color(t.stripe_separator))
        .children(buttons);
    match side {
        StripeSide::Left => d.border_r_1(),
        StripeSide::Right => d.border_l_1(),
    }
}
