//! Banners (inline notices at the top of a panel) and notification balloons.
//!
//! Look ported from `platform/platform-api/src/com/intellij/ui/InlineBanner.kt` /
//! `EditorNotificationPanel` (`Banner.*` colors: tinted background, 1px border, icon, text,
//! actions) and `platform/platform-impl/src/com/intellij/notification/impl/NotificationsManagerImpl.java`
//! balloons (`Notification.*` colors, title in bold, content, actions as links).
//!
//! Deviations: no balloon animation or timeout, no "Don't show again", no event log.

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::icons::icon;
use crate::metrics::UI_FONT_SIZE;
use crate::theme::{ActiveIjTheme, color};

type Handler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Severity of a banner or balloon (`NotificationType` / `EditorNotificationPanel.Status`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    fn icon(self) -> &'static str {
        match self {
            Severity::Info => "general/information",
            Severity::Warning => "general/warning",
            Severity::Error => "general/error",
        }
    }
}

/// A link-style action (`ActionLink`).
pub struct Action {
    label: SharedString,
    handler: Handler,
}

impl Action {
    pub fn new(
        label: impl Into<SharedString>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Action {
        Action {
            label: label.into(),
            handler: Rc::new(handler),
        }
    }
}

fn render_actions(id: &ElementId, actions: Vec<Action>, cx: &App) -> Vec<AnyElement> {
    let link = color(cx.ij().tokens.link);
    actions
        .into_iter()
        .enumerate()
        .map(|(ix, action)| {
            let handler = action.handler;
            div()
                .id((SharedString::from(format!("{id}-action")), ix))
                .text_color(link)
                .cursor_pointer()
                .hover(|d| d.underline())
                .child(action.label)
                .on_click(move |e, w, cx| handler(e, w, cx))
                .into_any_element()
        })
        .collect()
}

/// An inline banner (`InlineBanner`).
#[derive(IntoElement)]
pub struct Banner {
    id: ElementId,
    severity: Severity,
    text: SharedString,
    actions: Vec<Action>,
}

impl Banner {
    pub fn new(
        id: impl Into<ElementId>,
        severity: Severity,
        text: impl Into<SharedString>,
    ) -> Banner {
        Banner {
            id: id.into(),
            severity,
            text: text.into(),
            actions: Vec::new(),
        }
    }

    pub fn action(mut self, action: Action) -> Banner {
        self.actions.push(action);
        self
    }
}

impl RenderOnce for Banner {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.ij();
        let t = theme.tokens;
        let (bg, border) = match self.severity {
            Severity::Info => (t.banner_info_background, t.banner_info_border),
            Severity::Warning => (t.banner_warning_background, t.banner_warning_border),
            Severity::Error => (t.banner_error_background, t.banner_error_border),
        };
        let actions = render_actions(&self.id, self.actions, cx);
        div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(color(border))
            .bg(color(bg))
            .text_size(px(UI_FONT_SIZE))
            .text_color(color(t.foreground))
            .child(icon(self.severity.icon(), theme.dark, 16.0))
            .child(div().flex_1().child(self.text))
            .children(actions)
    }
}

/// A notification balloon.
#[derive(IntoElement)]
pub struct Balloon {
    id: ElementId,
    severity: Severity,
    title: SharedString,
    content: SharedString,
    actions: Vec<Action>,
}

impl Balloon {
    pub fn new(
        id: impl Into<ElementId>,
        severity: Severity,
        title: impl Into<SharedString>,
        content: impl Into<SharedString>,
    ) -> Balloon {
        Balloon {
            id: id.into(),
            severity,
            title: title.into(),
            content: content.into(),
            actions: Vec::new(),
        }
    }

    pub fn action(mut self, action: Action) -> Balloon {
        self.actions.push(action);
        self
    }
}

impl RenderOnce for Balloon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.ij();
        let t = theme.tokens;
        let actions = render_actions(&self.id, self.actions, cx);
        div()
            .id(self.id)
            .w(px(360.0))
            .flex()
            .gap(px(8.0))
            .p(px(12.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(color(t.notification_border))
            .bg(color(t.notification_background))
            .text_color(color(t.notification_foreground))
            .text_size(px(UI_FONT_SIZE))
            .shadow_lg()
            .child(icon(self.severity.icon(), theme.dark, 16.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .flex_1()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.title),
                    )
                    .when(!self.content.is_empty(), |d| d.child(self.content))
                    .child(div().flex().gap(px(12.0)).children(actions)),
            )
    }
}
