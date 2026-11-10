//! Paints a row of the commit graph from its print elements, and finds the element under the mouse.
//!
//! Port of `platform/vcs-log/impl/src/com/intellij/vcs/log/paint/SimpleGraphCellPainter.kt`,
//! `HeadNodePainter.kt`, `PaintParameters.java`, `GraphCommitCellUtil.kt` (`getGraphWidth`) and
//! the `PaintUtil.alignToInt` device-pixel alignment they use.
//!
//! The painter is split in two: [`GraphGeometry::shapes`] turns print elements into lines and
//! circles (pure, tested), and [`paint_shapes`] draws them with GPUI.
//!
//! Deviations:
//! - Dashed lines start with a full dash: GPUI's dash arrays have no phase (upstream offsets the
//!   pattern by half a dash).
//! - The `OUTLINE` node stroke uses the ordinary line thickness (upstream draws it with whatever
//!   stroke the previous element left on the `Graphics2D`).

use gpui::{
    App, Bounds, ContentMask, Hsla, PathBuilder, PathStyle, Pixels, StrokeOptions, Window, point,
    px, quad, transparent_black,
};
use ig_log::GraphPrintElement;
use ig_log::print_element::{
    EdgePrintElement, EdgePrintElementType, LineStyle, NodePrintElementType, PrintElement,
};
use ig_ui_kit::metrics::{
    GRAPH_CIRCLE_RADIUS, GRAPH_MAX_WIDTH_ELEMENTS, GRAPH_SELECT_THICK_LINE, GRAPH_TEXT_GAP,
    GRAPH_THICK_LINE, GRAPH_WIDTH_NODE, scale_with_row_height,
};
use lyon::tessellation::{LineCap, LineJoin};

/// `SimpleGraphCellPainter.ARROW_ANGLE_COS2`.
const ARROW_ANGLE_COS2: f32 = 0.7;
/// `SimpleGraphCellPainter.ARROW_LENGTH`.
const ARROW_LENGTH: f32 = 0.3;
/// `HeadNodePainter.RADIUS_DELTA`.
const RADIUS_DELTA: f32 = 2.0;
/// `JBColor.GRAY` (`Gray._128`), the `OUTLINE` node color.
pub const NEUTRAL_COLOR: u32 = 0x808080;

/// `PaintUtil.RoundingMode` subset.
#[derive(Clone, Copy)]
enum Rounding {
    Floor,
}

/// `PaintUtil.ParityMode`.
#[derive(Clone, Copy, PartialEq)]
enum Parity {
    Odd,
}

/// `PaintUtil.alignToInt(usrValue, ctx, FLOOR, pm)`.
fn align_to_int(value: f32, scale: f32, _rm: Rounding, parity: Option<Parity>) -> f32 {
    if scale == 0.0 {
        return 0.0;
    }
    let mut dev = (value * scale).floor() as i32;
    if parity == Some(Parity::Odd) && dev % 2 == 0 {
        dev -= 1;
    }
    dev as f32 / scale
}

/// A colored shape of a row, in the row's coordinates (logical pixels).
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Line {
        from: (f32, f32),
        to: (f32, f32),
        width: f32,
        /// `(dash, space)` for dashed lines.
        dash: Option<(f32, f32)>,
        color: u32,
    },
    /// A filled circle given by its bounding box (`Ellipse2D(x, y, d, d)`).
    Circle {
        x: f32,
        y: f32,
        diameter: f32,
        color: Color,
    },
    /// An outlined circle.
    Ring {
        x: f32,
        y: f32,
        diameter: f32,
        width: f32,
        color: u32,
    },
}

/// A shape color: a branch color, or the row background (the head node's middle ring).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Color {
    Rgb(u32),
    RowBackground,
}

/// The painter's metrics for a row height and display scale (`SimpleGraphCellPainter.MyPainter`).
#[derive(Clone, Debug, PartialEq)]
pub struct GraphGeometry {
    pub row_height: f32,
    row_center: f32,
    element_width: f32,
    element_center: f32,
    line_thickness: f32,
    selected_line_thickness: f32,
    circle_diameter: f32,
    selected_circle_diameter: f32,
    circle_radius: f32,
    selected_circle_radius: f32,
    // HeadNodePainter
    outer_circle_diameter: f32,
    outer_circle_radius: f32,
    selected_outer_circle_diameter: f32,
    selected_outer_circle_radius: f32,
    head_delta: f32,
}

impl GraphGeometry {
    /// `scale` is the device pixel ratio (`ScaleContext`'s system scale).
    pub fn new(row_height: f32, scale: f32) -> GraphGeometry {
        let s = |v: f32| scale_with_row_height(v, row_height);
        let floor_odd = |v: f32| align_to_int(v, scale, Rounding::Floor, Some(Parity::Odd));
        let floor = |v: f32| align_to_int(v, scale, Rounding::Floor, None);
        let pixel = 1.0 / scale;

        let line_thickness = floor_odd(s(GRAPH_THICK_LINE)).max(pixel);
        let selected_line_thickness =
            floor_odd(s(GRAPH_SELECT_THICK_LINE)).max(line_thickness + 2.0 * pixel);
        let circle_diameter = floor_odd(2.0 * s(GRAPH_CIRCLE_RADIUS));
        let selected_circle_diameter = circle_diameter + selected_line_thickness - line_thickness;
        let outer_circle_diameter = floor_odd(2.0 * s(GRAPH_CIRCLE_RADIUS + RADIUS_DELTA));
        let selected_outer_circle_diameter =
            outer_circle_diameter + selected_line_thickness - line_thickness;
        GraphGeometry {
            row_height,
            row_center: floor_odd(row_height / 2.0),
            element_width: floor_odd(s(GRAPH_WIDTH_NODE)),
            element_center: floor(s(GRAPH_WIDTH_NODE) / 2.0),
            line_thickness,
            selected_line_thickness,
            circle_diameter,
            selected_circle_diameter,
            circle_radius: floor(circle_diameter / 2.0),
            selected_circle_radius: floor(selected_circle_diameter / 2.0),
            outer_circle_diameter,
            outer_circle_radius: floor(outer_circle_diameter / 2.0),
            selected_outer_circle_diameter,
            selected_outer_circle_radius: floor(selected_outer_circle_diameter / 2.0),
            head_delta: floor(s(RADIUS_DELTA)),
        }
    }

    fn x(&self, position: i32) -> f32 {
        self.element_width * position as f32 + self.element_center
    }

    /// The center of a node at `position` in a row (row coordinates), where
    /// [`element_under`](Self::element_under) finds it.
    pub fn node_center(&self, position: i32) -> (f32, f32) {
        (self.x(position), self.row_center)
    }

    /// `GraphCommitCellUtil.getGraphWidth`: the width taken by the graph before the text.
    pub fn graph_width(&self, elements: &[GraphPrintElement], recommended_width: i32) -> f32 {
        if elements.is_empty() {
            return 0.0;
        }
        let mut max_index: f32 = 0.0;
        for element in elements {
            let position = element.position_in_current_row() as f32;
            max_index = max_index.max(position);
            if let GraphPrintElement::Edge(edge) = element {
                let other = edge.position_in_other_row() as f32;
                max_index = max_index.max((position + other) / 2.0);
            }
        }
        max_index += 1.0;
        max_index = max_index.max(GRAPH_MAX_WIDTH_ELEMENTS.min(recommended_width as f32));
        let element_width = scale_with_row_height(GRAPH_WIDTH_NODE, self.row_height);
        let gap = scale_with_row_height(GRAPH_TEXT_GAP, self.row_height);
        (max_index * element_width).trunc() + gap.trunc()
    }

    /// `SimpleGraphCellPainter.paint`: non-selected elements, then the selected ones twice (a
    /// thick `mark` outline, then the normal color on top). `color_of` maps a color id.
    pub fn shapes(
        &self,
        elements: &[GraphPrintElement],
        color_of: impl Fn(i32) -> u32,
        mark_color: u32,
    ) -> Vec<Shape> {
        let mut shapes = Vec::new();
        let (selected, normal): (Vec<_>, Vec<_>) = elements.iter().partition(|e| e.is_selected());
        for element in normal {
            self.element(&mut shapes, element, color_of(element.color_id()), false);
        }
        for element in &selected {
            self.element(&mut shapes, element, mark_color, true);
        }
        for element in &selected {
            self.element(&mut shapes, element, color_of(element.color_id()), false);
        }
        shapes
    }

    fn element(
        &self,
        out: &mut Vec<Shape>,
        element: &GraphPrintElement,
        color: u32,
        selected: bool,
    ) {
        match element {
            GraphPrintElement::Edge(edge) => self.edge(out, edge, color, selected),
            GraphPrintElement::Node(node) => {
                let x0 = self.x(node.position_in_current_row());
                let y0 = self.row_center;
                let (radius, diameter) = if selected {
                    (self.selected_circle_radius, self.selected_circle_diameter)
                } else {
                    (self.circle_radius, self.circle_diameter)
                };
                match ig_log::print_element::NodePrintElement::node_type(node) {
                    NodePrintElementType::Fill => out.push(Shape::Circle {
                        x: x0 - radius,
                        y: y0 - radius,
                        diameter,
                        color: Color::Rgb(color),
                    }),
                    NodePrintElementType::OutlineAndFill => {
                        self.head_node(out, x0, y0, color, selected)
                    }
                    // AWT centres the stroke on the circle, GPUI draws quad borders inside the
                    // bounds: grow the ring by half the stroke on each side
                    NodePrintElementType::Outline => out.push(Shape::Ring {
                        x: x0 - radius - self.line_thickness / 2.0,
                        y: y0 - radius - self.line_thickness / 2.0,
                        diameter: diameter + self.line_thickness,
                        width: self.line_thickness,
                        color: NEUTRAL_COLOR,
                    }),
                }
            }
        }
    }

    /// `HeadNodePainter.paint`.
    fn head_node(&self, out: &mut Vec<Shape>, x0: f32, y0: f32, color: u32, selected: bool) {
        if selected {
            let r = self.selected_outer_circle_radius;
            out.push(Shape::Circle {
                x: x0 - r,
                y: y0 - r,
                diameter: self.selected_outer_circle_diameter,
                color: Color::Rgb(color),
            });
            return;
        }
        let mut x = x0 - self.outer_circle_radius;
        let mut y = y0 - self.outer_circle_radius;
        let mut diameter = self.outer_circle_diameter;
        out.push(Shape::Circle {
            x,
            y,
            diameter,
            color: Color::Rgb(color),
        });
        for fill in [Color::RowBackground, Color::Rgb(color)] {
            diameter -= self.head_delta * 2.0;
            x += self.head_delta;
            y += self.head_delta;
            out.push(Shape::Circle {
                x,
                y,
                diameter,
                color: fill,
            });
        }
    }

    /// `MyPainter.paintEdge`.
    fn edge(
        &self,
        out: &mut Vec<Shape>,
        edge: &ig_log::print::elements::EdgePrintElementImpl,
        color: u32,
        selected: bool,
    ) {
        let down = edge.edge_print_type() == EdgePrintElementType::Down;
        let usual = edge.line_style() == LineStyle::Solid;
        let from = edge.position_in_current_row();
        let to = edge.position_in_other_row();
        let x1 = self.x(from);
        let y1 = self.row_center;
        if from == to {
            let arrow_gap = if edge.is_terminal() {
                self.circle_radius / 2.0 + 1.0
            } else {
                0.0
            };
            let y2 = if down {
                self.row_height - arrow_gap
            } else {
                arrow_gap
            };
            self.line(
                out,
                (x1, y1),
                (x1, y2),
                (x1, y2),
                edge.has_arrow(),
                usual,
                selected,
                color,
            );
        } else {
            // paint non-vertical lines twice the size to make them dock with each other well
            let x2 = self.x(to);
            let y2 = if down {
                self.row_height + self.row_center
            } else {
                self.row_center - self.row_height
            };
            let mid = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
            self.line(
                out,
                (x1, y1),
                (x2, y2),
                mid,
                edge.has_arrow(),
                usual,
                selected,
                color,
            );
        }
    }

    /// `MyPainter.paintLine`.
    #[allow(clippy::too_many_arguments)]
    fn line(
        &self,
        out: &mut Vec<Shape>,
        from: (f32, f32),
        to: (f32, f32),
        arrow_start: (f32, f32),
        has_arrow: bool,
        usual: bool,
        selected: bool,
        color: u32,
    ) {
        let width = if selected {
            self.selected_line_thickness
        } else {
            self.line_thickness
        };
        let dash = if usual || has_arrow {
            None
        } else {
            let length = if from.0 == to.0 {
                self.row_height
            } else {
                (from.0 - to.0).hypot(from.1 - to.1)
            };
            Some(dash_length(length, self.row_height))
        };
        out.push(Shape::Line {
            from,
            to,
            width,
            dash,
            color,
        });
        if has_arrow {
            let cos = ARROW_ANGLE_COS2.sqrt();
            let sin = (1.0 - ARROW_ANGLE_COS2).sqrt();
            let length = ARROW_LENGTH * self.row_height;
            for sin in [sin, -sin] {
                let end = rotate(from, arrow_start, cos, sin, length);
                out.push(Shape::Line {
                    from: arrow_start,
                    to: end,
                    width,
                    dash: None,
                    color,
                });
            }
        }
    }

    /// `getElementUnderCursor`: nodes first, then edges; `(x, y)` relative to the row.
    pub fn element_under(
        &self,
        elements: &[GraphPrintElement],
        x: f32,
        y: f32,
    ) -> Option<GraphPrintElement> {
        let over_node =
            |position: i32| (self.x(position) - x).hypot(self.row_center - y) <= self.circle_radius;
        if let Some(node) = elements.iter().find(
            |e| matches!(e, GraphPrintElement::Node(n) if over_node(n.position_in_current_row())),
        ) {
            return Some(node.clone());
        }
        elements
            .iter()
            .find(|e| match e {
                GraphPrintElement::Edge(edge) => {
                    let x1 = self.x(edge.position_in_current_row());
                    let y1 = self.row_center;
                    let x2 = self.x(edge.position_in_other_row());
                    let y2 = if edge.edge_print_type() == EdgePrintElementType::Down {
                        self.row_height + self.row_center
                    } else {
                        self.row_center - self.row_height
                    };
                    (x1 - x).hypot(y1 - y) + (x2 - x).hypot(y2 - y)
                        < (x1 - x2).hypot(y1 - y2) + self.line_thickness
                }
                GraphPrintElement::Node(_) => false,
            })
            .cloned()
    }
}

/// `SimpleGraphCellPainter.rotate`.
fn rotate(p: (f32, f32), center: (f32, f32), cos: f32, sin: f32, length: f32) -> (f32, f32) {
    let (tx, ty) = (p.0 - center.0, p.1 - center.1);
    let d = tx.hypot(ty);
    let (sx, sy) = (length * tx / d, length * ty / d);
    (
        sx * cos - sy * sin + center.0,
        sx * sin + sy * cos + center.1,
    )
}

/// `SimpleGraphCellPainter.getDashLength`: `(dash, space)`.
fn dash_length(edge_length: f32, row_height: f32) -> (f32, f32) {
    let dash_count = ((edge_length / row_height).floor() as i32).max(1);
    let space = row_height / 2.0 - 2.0;
    (edge_length / dash_count as f32 - space, space)
}

/// Draws `shapes` in `bounds` (one graph cell), clipped to it.
pub fn paint_shapes(
    shapes: &[Shape],
    bounds: Bounds<Pixels>,
    row_background: Hsla,
    window: &mut Window,
    _: &mut App,
) {
    let origin = bounds.origin;
    let at = |(x, y): (f32, f32)| point(origin.x + px(x), origin.y + px(y));
    let rgb = |c: u32| -> Hsla { gpui::rgb(c).into() };
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        for shape in shapes {
            match shape {
                Shape::Line {
                    from,
                    to,
                    width,
                    dash,
                    color,
                } => {
                    let options = StrokeOptions::default()
                        .with_line_width(*width)
                        .with_line_cap(LineCap::Round)
                        .with_line_join(LineJoin::Round);
                    let mut builder =
                        PathBuilder::stroke(px(*width)).with_style(PathStyle::Stroke(options));
                    if let Some((dash, space)) = dash {
                        builder = builder.dash_array(&[px(*dash), px(*space)]);
                    }
                    builder.move_to(at(*from));
                    builder.line_to(at(*to));
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, rgb(*color));
                    }
                }
                Shape::Circle {
                    x,
                    y,
                    diameter,
                    color,
                } => {
                    let fill = match color {
                        Color::Rgb(c) => rgb(*c),
                        Color::RowBackground => row_background,
                    };
                    let b = Bounds::new(at((*x, *y)), gpui::size(px(*diameter), px(*diameter)));
                    window.paint_quad(quad(
                        b,
                        px(diameter / 2.0),
                        fill,
                        px(0.0),
                        transparent_black(),
                        Default::default(),
                    ));
                }
                Shape::Ring {
                    x,
                    y,
                    diameter,
                    width,
                    color,
                } => {
                    let b = Bounds::new(at((*x, *y)), gpui::size(px(*diameter), px(*diameter)));
                    window.paint_quad(quad(
                        b,
                        px(diameter / 2.0),
                        transparent_black(),
                        px(*width),
                        rgb(*color),
                        Default::default(),
                    ));
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_to_int_matches_paint_util() {
        // PaintUtil javadoc: 2.1 floor'd to odd -> 1
        assert_eq!(
            align_to_int(2.1, 1.0, Rounding::Floor, Some(Parity::Odd)),
            1.0
        );
        assert_eq!(
            align_to_int(1.9, 1.0, Rounding::Floor, Some(Parity::Odd)),
            1.0
        );
        // at 2x: 1.5 -> 3 device px (odd) -> 1.5
        assert_eq!(
            align_to_int(1.5, 2.0, Rounding::Floor, Some(Parity::Odd)),
            1.5
        );
        assert_eq!(align_to_int(13.0, 2.0, Rounding::Floor, None), 13.0);
    }

    #[test]
    fn metrics_at_the_new_ui_row_height() {
        // row height 26 at 2x: everything scales by 26/22
        let g = GraphGeometry::new(26.0, 2.0);
        assert_eq!(g.row_center, 12.5); // 26 dev -> odd 25 -> 12.5
        assert_eq!(g.element_width, 18.5); // 18.909 * 2 = 37.8 -> 37
        assert_eq!(g.element_center, 9.0); // 9.4545 * 2 = 18.9 -> 18
        assert_eq!(g.line_thickness, 1.5); // 1.77 * 2 = 3.5 -> 3
        assert_eq!(g.selected_line_thickness, 2.5); // 2.95 * 2 = 5.9 -> 5
        assert_eq!(g.circle_diameter, 8.5); // 9.45 * 2 = 18.9 -> 17 (odd)
        assert_eq!(g.circle_radius, 4.0); // 4.25 * 2 = 8.5 -> 8
        assert_eq!(g.selected_circle_radius, 4.5); // (8.5 + 2.5 - 1.5) / 2 = 4.75 -> 9.5 dev -> 9
    }

    #[test]
    fn dashes_fit_the_edge() {
        let (dash, space) = dash_length(26.0, 26.0);
        assert_eq!(space, 11.0);
        assert_eq!(dash + space, 26.0);
        let (dash, space) = dash_length(60.0, 26.0);
        assert_eq!(2.0 * (dash + space), 60.0);
    }

    #[test]
    fn arrow_head_is_rotated_around_the_arrow_start() {
        // a vertical edge pointing down: the arrow wings go up-left and up-right
        let end = rotate((0.0, 0.0), (0.0, 10.0), 0.7f32.sqrt(), 0.3f32.sqrt(), 5.0);
        assert!((end.0 - 0.0).abs() > 1.0 && end.1 < 10.0);
        assert!(((end.0).hypot(end.1 - 10.0) - 5.0).abs() < 1e-4);
    }
}
