//! IntelliJ New UI tool windows on top of gpui-component's dock (the unstyled `gpui-base` dock with
//! an IntelliJ renderer).
//!
//! Look ported from `platform/platform-impl/src/com/intellij/toolWindow/ToolWindowHeader.kt`,
//! `.../openapi/wm/impl/content/ContentTabLabel.java` / `TabContentLayout.java` and the
//! `JBUI.CurrentTheme.ToolWindow` metrics: a 41px header with the tool-window title, the content
//! tabs (12px side insets, a 2px underline under the selected tab) and a right-aligned toolbar
//! with the Hide button; a 1px border and a resize edge between the dock and the editor area.
//!
//! Deviations: one title per renderer (IdeaGit has one bottom tool window so far); no tab drag and
//! drop between docks, no gear/options menu, no split/float view modes.

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, Div, InteractiveElement, IntoElement, ParentElement,
    SharedString, Stateful, StatefulInteractiveElement, Styled, Window, div, px, svg,
};
use gpui_component::dock::{
    DockAreaRenderer, DockContext, DockPlacement, PanelHandle, TabGroupContext, TabGroupRenderer,
    TileContext, TilesRenderer,
};

use crate::metrics::{
    TOOL_WINDOW_HEADER_HEIGHT, TOOL_WINDOW_HEADER_LABEL_LEFT, TOOL_WINDOW_HEADER_TAB_ARC,
    TOOL_WINDOW_HEADER_TAB_PADDING, TOOL_WINDOW_HEADER_TOOLBAR_RIGHT,
    TOOL_WINDOW_TAB_UNDERLINE_HEIGHT, UI_FONT_SIZE,
};
use crate::theme::{ActiveIjTheme, color};

type HideHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// Renders the dock area as IntelliJ tool windows.
pub struct IjDockRenderer {
    title: SharedString,
    on_hide: HideHandler,
    headerless: Vec<&'static str>,
}

impl IjDockRenderer {
    /// `title` is the tool window name shown in the header (e.g. "Git"); `on_hide` runs for the
    /// header's Hide button (upstream `HideToolWindowAction`).
    pub fn new(
        title: impl Into<SharedString>,
        on_hide: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Rc<IjDockRenderer> {
        IjDockRenderer::with_headerless(title, on_hide, Vec::new())
    }

    /// Like [`new`](Self::new); groups whose panels are all named in `headerless`
    /// (`Panel::panel_name`) get no header (e.g. the editor area).
    pub fn with_headerless(
        title: impl Into<SharedString>,
        on_hide: impl Fn(&mut Window, &mut App) + 'static,
        headerless: Vec<&'static str>,
    ) -> Rc<IjDockRenderer> {
        Rc::new(IjDockRenderer {
            title: title.into(),
            on_hide: Rc::new(on_hide),
            headerless,
        })
    }
}

/// Drag payload of the dock's resize edge.
#[derive(Clone)]
struct ResizeDock;

impl gpui::Render for ResizeDock {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
    }
}

impl DockAreaRenderer for IjDockRenderer {
    fn frame(&self, _: &mut Window, cx: &mut App) -> Stateful<Div> {
        div()
            .id("ij-dock-area")
            .bg(color(cx.ij().tokens.panel_background))
    }

    fn render_dock(
        &self,
        dock: &DockContext,
        content: AnyElement,
        _: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let t = cx.ij().tokens;
        let placement = dock.placement();
        let resize = dock.clone();
        let edge = div()
            .id(match placement {
                DockPlacement::Left => "dock-edge-left",
                DockPlacement::Right => "dock-edge-right",
                DockPlacement::Bottom => "dock-edge-bottom",
                DockPlacement::Center => "dock-edge-center",
            })
            .absolute()
            .map(|d| match placement {
                DockPlacement::Bottom => d.top_0().left_0().w_full().h(px(4.0)).cursor_row_resize(),
                DockPlacement::Left => d.top_0().right_0().h_full().w(px(4.0)).cursor_col_resize(),
                _ => d.top_0().left_0().h_full().w(px(4.0)).cursor_col_resize(),
            })
            .on_drag(ResizeDock, |_, _, _, cx| cx.new(|_| ResizeDock))
            .on_drag_move::<ResizeDock>(move |e, window, cx| {
                resize.resize_to(e.event.position, window, cx)
            });
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .border_color(color(t.border))
            .map(|d| match placement {
                DockPlacement::Bottom => d.border_t_1(),
                DockPlacement::Left => d.border_r_1(),
                DockPlacement::Right => d.border_l_1(),
                DockPlacement::Center => d,
            })
            .child(content)
            .child(edge)
            .into_any_element()
    }

    fn tiles_renderer(&self) -> Rc<dyn TilesRenderer> {
        Rc::new(NoTiles)
    }

    fn tab_group_renderer(&self) -> Rc<dyn TabGroupRenderer> {
        Rc::new(IjToolWindowHeader {
            title: self.title.clone(),
            on_hide: self.on_hide.clone(),
            headerless: self.headerless.clone(),
        })
    }
}

/// IdeaGit has no floating tiles; the renderer gpui-component's dock requires draws nothing.
struct NoTiles;

impl TilesRenderer for NoTiles {
    fn render_drag_bar(&self, _: &TileContext, _: &mut Window, _: &mut App) -> AnyElement {
        div().into_any_element()
    }
}

/// The tool-window header with content tabs (`ToolWindowHeader` + `TabContentLayout`).
struct IjToolWindowHeader {
    title: SharedString,
    on_hide: HideHandler,
    headerless: Vec<&'static str>,
}

fn tab_name(panel: &std::sync::Arc<dyn gpui_component::dock::BasePanelView>, cx: &App) -> String {
    PanelHandle::of(panel)
        .and_then(|p| p.tab_name(cx))
        .map(|s| s.to_string())
        .unwrap_or_else(|| panel.panel_name(cx).to_string())
}

impl TabGroupRenderer for IjToolWindowHeader {
    fn frame(&self, _: &TabGroupContext, _: &mut Window, cx: &mut App) -> Stateful<Div> {
        div()
            .id("ij-tool-window")
            .bg(color(cx.ij().tokens.tool_window_background))
    }

    fn render_tab_bar(&self, group: &TabGroupContext, _: &mut Window, cx: &mut App) -> AnyElement {
        if !group.panels().is_empty()
            && group
                .panels()
                .iter()
                .all(|p| self.headerless.contains(&p.panel_name(cx)))
        {
            return div().into_any_element();
        }
        let theme = cx.ij();
        let t = theme.tokens;
        let active = group.active_ix();
        let tabs: Vec<AnyElement> = group
            .panels()
            .iter()
            .enumerate()
            .filter(|(_, p)| p.visible(cx))
            .map(|(ix, panel)| {
                let selected = ix == active;
                let group = group.clone();
                div()
                    .id(("ij-tool-window-tab", ix))
                    .relative()
                    .h_full()
                    .flex()
                    .items_center()
                    .px(px(TOOL_WINDOW_HEADER_TAB_PADDING))
                    .rounded(px(TOOL_WINDOW_HEADER_TAB_ARC))
                    .when(!selected, |d| {
                        d.hover(|d| d.bg(color(t.tool_window_header_tab_hover)))
                    })
                    .child(tab_name(panel, cx))
                    .when(selected, |d| {
                        d.child(
                            div()
                                .absolute()
                                .bottom_0()
                                .left_0()
                                .right_0()
                                .h(px(TOOL_WINDOW_TAB_UNDERLINE_HEIGHT))
                                .bg(color(t.tool_window_tab_underline)),
                        )
                    })
                    .on_click(move |_, window, cx| group.select_tab(ix, window, cx))
                    .into_any_element()
            })
            .collect();
        let on_hide = self.on_hide.clone();
        div()
            .id("ij-tool-window-header")
            .flex()
            .flex_none()
            .items_center()
            .h(px(TOOL_WINDOW_HEADER_HEIGHT))
            .pl(px(TOOL_WINDOW_HEADER_LABEL_LEFT))
            .pr(px(TOOL_WINDOW_HEADER_TOOLBAR_RIGHT))
            .bg(color(t.tool_window_header_background))
            .text_size(px(UI_FONT_SIZE))
            .text_color(color(t.foreground))
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .pr(px(4.0))
                    .child(format!("{}:", self.title)),
            )
            .child(div().flex().h_full().children(tabs))
            .child(div().flex_1())
            .child(
                div()
                    .id("ij-tool-window-hide")
                    .size(px(24.0))
                    .rounded(px(4.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|d| d.bg(color(t.tool_window_header_tab_hover)))
                    .child(
                        svg()
                            .path(crate::icons::path("general/hide", theme.dark))
                            .size(px(16.0))
                            .text_color(color(t.secondary_foreground)),
                    )
                    .on_click(move |_, window, cx| on_hide(window, cx)),
            )
            .into_any_element()
    }
}
