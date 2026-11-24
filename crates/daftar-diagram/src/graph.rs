//! Small layered graph layout used by flowcharts, class diagrams, state machines and ER.
//!
//! The algorithm ranks nodes by longest path (ignoring cycle-forming edges), orders each rank
//! by neighbour barycentres, then packs ranks and layers with measured node sizes.

use std::collections::{BTreeMap, HashMap, VecDeque};

use daftar_core::direction::detect_direction;

use crate::scene::{Label, PathOp, Rgba8, Scene, SceneStroke};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphDirection {
    TopBottom,
    BottomTop,
    LeftRight,
    RightLeft,
}

impl GraphDirection {
    pub fn from_keyword(key: &str) -> Option<Self> {
        match key.to_ascii_uppercase().as_str() {
            "TD" | "TB" => Some(Self::TopBottom),
            "BT" => Some(Self::BottomTop),
            "LR" => Some(Self::LeftRight),
            "RL" => Some(Self::RightLeft),
            _ => None,
        }
    }

    pub fn is_vertical(self) -> bool {
        matches!(self, Self::TopBottom | Self::BottomTop)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeShape {
    Rect,
    Rounded,
    Stadium,
    Diamond,
    Circle,
    Hexagon,
    Parallelogram,
    Cylinder,
    Note,
    Subroutine,
    Asymmetric,
}

#[derive(Debug, Clone)]
pub struct GraphNode {
    pub id: String,
    pub label: Label,
    pub shape: NodeShape,
    pub width: f32,
    pub height: f32,
    pub group: Option<usize>,
    /// Optional accent override (start/end states, classes).
    pub accent: Option<Rgba8>,
    pub fill: Option<Rgba8>,
    pub stroke: Option<Rgba8>,
    pub invisible: bool,
}

impl GraphNode {
    pub fn new(id: impl Into<String>, label: Label, shape: NodeShape) -> Self {
        let (width, height) = shape_dimensions(shape, &label);
        Self {
            id: id.into(),
            label,
            shape,
            width,
            height,
            group: None,
            accent: None,
            fill: None,
            stroke: None,
            invisible: false,
        }
    }
}

fn shape_dimensions(shape: NodeShape, label: &Label) -> (f32, f32) {
    let pad_x = 24.0;
    let pad_y = 18.0;
    match shape {
        NodeShape::Diamond => (
            (label.width + pad_x * 2.0).max(110.0),
            (label.height + pad_y * 2.2).max(76.0),
        ),
        NodeShape::Circle => {
            let d = (label.width.powi(2) + label.height.powi(2)).sqrt() + 38.0;
            (d.max(74.0), d.max(74.0))
        }
        NodeShape::Stadium => (
            (label.width + 40.0).max(90.0),
            (label.height + pad_y).max(44.0),
        ),
        NodeShape::Note => (
            (label.width + 28.0).max(90.0),
            (label.height + 22.0).max(46.0),
        ),
        _ => (
            (label.width + pad_x).max(90.0),
            (label.height + pad_y).max(44.0),
        ),
    }
}

#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub from: usize,
    pub to: usize,
    pub label: Option<Label>,
    pub dashed: bool,
    pub thick: bool,
    pub bidir: bool,
    pub arrow_end: bool,
    pub arrow_start: bool,
    pub color: Option<Rgba8>,
}

impl GraphEdge {
    pub fn new(from: usize, to: usize) -> Self {
        Self {
            from,
            to,
            label: None,
            dashed: false,
            thick: false,
            bidir: false,
            arrow_end: true,
            arrow_start: false,
            color: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GraphGroup {
    pub title: Label,
    pub parent: Option<usize>,
    pub members: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct GraphLayout {
    pub centers: Vec<(f32, f32)>,
    pub ranks: Vec<usize>,
    pub width: f32,
    pub height: f32,
}

/// Compute layered positions. Node sizes are already measured.
pub fn layout(
    nodes: &[GraphNode],
    edges: &[GraphEdge],
    direction: GraphDirection,
    rank_gap: f32,
    node_gap: f32,
) -> GraphLayout {
    let n = nodes.len();
    if n == 0 {
        return GraphLayout {
            centers: vec![],
            ranks: vec![],
            width: 1.0,
            height: 1.0,
        };
    }
    let ranks = compute_ranks(n, edges);
    let max_rank = ranks.iter().copied().max().unwrap_or(0);

    // Order nodes within each rank by barycentre of connected neighbours.
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for (i, rank) in ranks.iter().enumerate() {
        layers[*rank].push(i);
    }
    let mut order: HashMap<usize, f32> = HashMap::new();
    for (layer_idx, layer) in layers.iter().enumerate() {
        for (pos, node) in layer.iter().enumerate() {
            order.insert(*node, pos as f32 + layer_idx as f32 * 0.001);
        }
    }
    for _ in 0..3 {
        let mut changed = false;
        for (layer_idx, layer) in layers.iter_mut().enumerate() {
            let mut scored: Vec<(f32, usize)> = layer
                .iter()
                .map(|node| {
                    let mut sum = 0.0;
                    let mut count = 0.0;
                    for edge in edges {
                        let other = if edge.from == *node {
                            Some(edge.to)
                        } else if edge.to == *node {
                            Some(edge.from)
                        } else {
                            None
                        };
                        if let Some(other) = other {
                            if let Some(pos) = order.get(&other) {
                                sum += *pos;
                                count += 1.0;
                            }
                        }
                    }
                    let bary = if count > 0.0 {
                        sum / count
                    } else {
                        order.get(node).copied().unwrap_or(0.0)
                    };
                    (bary, *node)
                })
                .collect();
            scored.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
            let new_order: Vec<usize> = scored.into_iter().map(|(_, id)| id).collect();
            if *layer != new_order {
                changed = true;
            }
            *layer = new_order;
            for (pos, node) in layer.iter().enumerate() {
                order.insert(*node, pos as f32 + layer_idx as f32 * 0.001);
            }
        }
        if !changed {
            break;
        }
    }

    let vertical = direction.is_vertical();
    // Rank-axis sizes and layer cross-axis sizes.
    let mut rank_sizes: Vec<f32> = vec![0.0; max_rank + 1];
    let mut layer_cross: Vec<f32> = vec![0.0; max_rank + 1];
    for (layer_idx, layer) in layers.iter().enumerate() {
        for node in layer {
            let node = &nodes[*node];
            let rank_dim = if vertical { node.height } else { node.width };
            let cross_dim = if vertical { node.width } else { node.height };
            rank_sizes[layer_idx] = rank_sizes[layer_idx].max(rank_dim);
            layer_cross[layer_idx] += cross_dim;
        }
        layer_cross[layer_idx] += node_gap * (layer.len().saturating_sub(1)) as f32;
    }
    let mut rank_pos = vec![0.0f32; max_rank + 1];
    let mut cursor = 0.0;
    for (i, size) in rank_sizes.iter().enumerate() {
        rank_pos[i] = cursor + size * 0.5;
        cursor += size + rank_gap;
    }
    let total_rank = cursor - rank_gap;
    let max_cross = layer_cross.iter().copied().fold(0.0f32, f32::max);

    let mut centers = vec![(0.0f32, 0.0f32); n];
    for (layer_idx, layer) in layers.iter().enumerate() {
        let mut cross = (max_cross - layer_cross[layer_idx]) * 0.5;
        for node_idx in layer {
            let node = &nodes[*node_idx];
            let cross_dim = if vertical { node.width } else { node.height };
            let sub_cross = cross + cross_dim * 0.5;
            let rank = rank_pos[layer_idx];
            let (x, y) = if vertical {
                (sub_cross, rank)
            } else {
                (rank, sub_cross)
            };
            centers[*node_idx] = (x, y);
            cross += cross_dim + node_gap;
        }
    }

    // Orient bottom-to-top / right-to-left by mirroring the rank axis.
    if matches!(direction, GraphDirection::BottomTop) {
        for center in &mut centers {
            center.1 = total_rank - center.1;
        }
    } else if matches!(direction, GraphDirection::RightLeft) {
        for center in &mut centers {
            center.0 = total_rank - center.0;
        }
    }

    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for (i, center) in centers.iter().enumerate() {
        min_x = min_x.min(center.0 - nodes[i].width * 0.5);
        min_y = min_y.min(center.1 - nodes[i].height * 0.5);
        max_x = max_x.max(center.0 + nodes[i].width * 0.5);
        max_y = max_y.max(center.1 + nodes[i].height * 0.5);
    }
    GraphLayout {
        centers,
        ranks,
        width: (max_x - min_x).max(1.0),
        height: (max_y - min_y).max(1.0),
    }
}

fn compute_ranks(n: usize, edges: &[GraphEdge]) -> Vec<usize> {
    let mut indegree = vec![0usize; n];
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    for edge in edges {
        if edge.from != edge.to && edge.from < n && edge.to < n {
            indegree[edge.to] += 1;
            out[edge.from].push(edge.to);
        }
    }
    let mut rank = vec![0usize; n];
    let mut queue: VecDeque<usize> = indegree
        .iter()
        .enumerate()
        .filter(|(_, d)| **d == 0)
        .map(|(i, _)| i)
        .collect();
    let mut processed = vec![false; n];
    while let Some(node) = queue.pop_front() {
        processed[node] = true;
        for next in &out[node] {
            rank[*next] = rank[*next].max(rank[node] + 1);
            indegree[*next] = indegree[*next].saturating_sub(1);
            if indegree[*next] == 0 {
                queue.push_back(*next);
            }
        }
    }
    // Cycle members: assign after their processed predecessors, otherwise keep rank 0.
    for _ in 0..n {
        let mut changed = false;
        for edge in edges {
            if edge.from == edge.to || edge.from >= n || edge.to >= n {
                continue;
            }
            let candidate = rank[edge.from] + 1;
            if candidate < n && rank[edge.to] < candidate && !processed[edge.to] {
                rank[edge.to] = candidate;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // Normalise into compact ranks.
    let mut unique: Vec<usize> = rank.clone();
    unique.sort_unstable();
    unique.dedup();
    let map: BTreeMap<usize, usize> = unique
        .into_iter()
        .enumerate()
        .map(|(new, old)| (old, new))
        .collect();
    rank.into_iter().map(|r| map[&r]).collect()
}

/// Compute the point where a line from the node centre towards `target` leaves the node box.
fn boundary(center: (f32, f32), size: (f32, f32), target: (f32, f32)) -> (f32, f32) {
    let dx = target.0 - center.0;
    let dy = target.1 - center.1;
    if dx.abs() < 0.001 && dy.abs() < 0.001 {
        return center;
    }
    let hx = size.0 * 0.5;
    let hy = size.1 * 0.5;
    let scale_x = if dx.abs() > 0.001 {
        hx / dx.abs()
    } else {
        f32::INFINITY
    };
    let scale_y = if dy.abs() > 0.001 {
        hy / dy.abs()
    } else {
        f32::INFINITY
    };
    let scale = scale_x.min(scale_y);
    (center.0 + dx * scale, center.1 + dy * scale)
}

/// Draw edges first (below nodes), then node shapes and labels, then edge labels.
pub fn draw(
    scene: &mut Scene,
    nodes: &[GraphNode],
    edges: &[GraphEdge],
    layout: &GraphLayout,
    groups: &[GraphGroup],
    theme: &crate::DiagramTheme,
    origin: (f32, f32),
) {
    let ox = origin.0;
    let oy = origin.1;
    let centers: Vec<(f32, f32)> = layout
        .centers
        .iter()
        .map(|(x, y)| (x + ox, y + oy))
        .collect();
    let node_rect = |i: usize| {
        let c = centers[i];
        (
            c.0 - nodes[i].width * 0.5,
            c.1 - nodes[i].height * 0.5,
            nodes[i].width,
            nodes[i].height,
        )
    };

    // Groups behind everything.
    for (group_idx, group) in groups.iter().enumerate() {
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for member in &group.members {
            if *member >= nodes.len() {
                continue;
            }
            let (x, y, w, h) = node_rect(*member);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x + w);
            max_y = max_y.max(y + h);
        }
        if min_x > max_x {
            continue;
        }
        let padding = 20.0;
        let title_h = group.title.height + 6.0;
        let bx = min_x - padding;
        let by = min_y - padding - title_h;
        let bw = (max_x - min_x) + padding * 2.0;
        let bh = (max_y - min_y) + padding * 2.0 + title_h;
        let depth = group_depth(groups, group_idx);
        let tint = theme.group_fill.mix(theme.accent, (depth as f32 * 0.04).min(0.2));
        scene.rect(
            bx,
            by,
            bw,
            bh,
            10.0,
            Some(tint),
            Some(SceneStroke {
                color: theme.group_stroke,
                width: 1.0,
                dash: Some((7.0, 5.0)),
            }),
        );
        scene.text(
            group.title.shaped.clone(),
            bx + 12.0,
            by + 6.0,
            theme.muted,
        );
    }

    // Edges.
    for edge in edges {
        if edge.from >= nodes.len() || edge.to >= nodes.len() {
            continue;
        }
        let from_center = centers[edge.from];
        let to_center = centers[edge.to];
        let from_size = (nodes[edge.from].width, nodes[edge.from].height);
        let to_size = (nodes[edge.to].width, nodes[edge.to].height);
        let color = edge.color.unwrap_or(theme.line);
        let mut stroke = if edge.dashed {
            SceneStroke::dashed(color, if edge.thick { 2.2 } else { 1.4 })
        } else {
            SceneStroke::solid(color, if edge.thick { 2.2 } else { 1.4 })
        };
        if edge.bidir {
            stroke.width = stroke.width.max(1.4);
        }
        if edge.from == edge.to {
            let (x, y, w, h) = node_rect(edge.from);
            let right = x + w;
            let top = y + h * 0.3;
            let loop_w = 54.0;
            scene.path(
                vec![
                    PathOp::Move(right, top),
                    PathOp::Cubic(
                        right + loop_w,
                        top - 24.0,
                        right + loop_w,
                        top + 34.0,
                        right,
                        top + 22.0,
                    ),
                ],
                None,
                Some(stroke),
            );
            scene.arrowhead((right + 8.0, top + 26.0), (right, top + 24.0), color, 8.0);
            if let Some(label) = &edge.label {
                scene.rect(
                    right + loop_w - 8.0,
                    top - 30.0,
                    label.width + 10.0,
                    label.height + 6.0,
                    4.0,
                    Some(theme.surface),
                    None,
                );
                scene.text(
                    label.shaped.clone(),
                    right + loop_w - 3.0,
                    top - 27.0,
                    theme.text,
                );
            }
            continue;
        }
        let source = boundary(from_center, from_size, to_center);
        let target = boundary(to_center, to_size, from_center);
        let back = if layout.ranks[edge.from] > layout.ranks[edge.to] {
            true
        } else if layout.ranks[edge.from] == layout.ranks[edge.to] {
            true
        } else {
            false
        };
        let horizontal_bias = (target.0 - source.0).abs() > (target.1 - source.1).abs();
        let (c1, c2) = if back {
            let nudge = 70.0;
            if horizontal_bias {
                (
                    (source.0 + nudge, source.1 - nudge * 0.5),
                    (target.0 - nudge, target.1 + nudge * 0.5),
                )
            } else {
                (
                    (source.0 + nudge * 0.5, source.1 + nudge),
                    (target.0 - nudge * 0.5, target.1 - nudge),
                )
            }
        } else if horizontal_bias {
            (
                (source.0 + (target.0 - source.0) * 0.45, source.1),
                (target.0 - (target.0 - source.0) * 0.45, target.1),
            )
        } else {
            (
                (source.0, source.1 + (target.1 - source.1) * 0.45),
                (target.0, target.1 - (target.1 - source.1) * 0.45),
            )
        };
        scene.path(
            vec![
                PathOp::Move(source.0, source.1),
                PathOp::Cubic(c1.0, c1.1, c2.0, c2.1, target.0, target.1),
            ],
            None,
            Some(stroke),
        );
        if edge.arrow_start {
            scene.arrowhead(c1, source, color, 8.0);
        }
        if edge.arrow_end {
            scene.arrowhead(c2, target, color, 8.0);
        }
        if let Some(label) = &edge.label {
            let mx = source.0 * 0.125 + c1.0 * 0.375 + c2.0 * 0.375 + target.0 * 0.125;
            let my = source.1 * 0.125 + c1.1 * 0.375 + c2.1 * 0.375 + target.1 * 0.125;
            let w = label.width + 10.0;
            let h = label.height + 6.0;
            scene.rect(
                mx - w * 0.5,
                my - h * 0.5,
                w,
                h,
                4.0,
                Some(theme.surface),
                None,
            );
            scene.text(
                label.shaped.clone(),
                mx - label.width * 0.5,
                my - label.height * 0.5,
                theme.muted,
            );
        }
    }

    // Nodes.
    for (i, node) in nodes.iter().enumerate() {
        if node.invisible {
            continue;
        }
        let (x, y, w, h) = node_rect(i);
        let fill = node.fill.unwrap_or(theme.node_fill);
        let stroke_color = node.stroke.unwrap_or(theme.node_stroke);
        let accent = node.accent.unwrap_or(theme.accent);
        let border = SceneStroke::solid(stroke_color, 1.2);
        match node.shape {
            NodeShape::Diamond => {
                scene.polygon(
                    vec![
                        (x + w * 0.5, y),
                        (x + w, y + h * 0.5),
                        (x + w * 0.5, y + h),
                        (x, y + h * 0.5),
                    ],
                    Some(fill),
                    Some(border),
                );
            }
            NodeShape::Circle => {
                scene.ellipse(
                    x + w * 0.5,
                    y + h * 0.5,
                    w * 0.5,
                    h * 0.5,
                    Some(fill),
                    Some(border),
                );
            }
            NodeShape::Stadium => {
                scene.rect(x, y, w, h, h * 0.5, Some(fill), Some(border));
            }
            NodeShape::Hexagon => {
                let cut = (h * 0.45).min(w * 0.2);
                scene.polygon(
                    vec![
                        (x + cut, y),
                        (x + w - cut, y),
                        (x + w, y + h * 0.5),
                        (x + w - cut, y + h),
                        (x + cut, y + h),
                        (x, y + h * 0.5),
                    ],
                    Some(fill),
                    Some(border),
                );
            }
            NodeShape::Parallelogram => {
                let cut = 16.0;
                scene.polygon(
                    vec![
                        (x + cut, y),
                        (x + w, y),
                        (x + w - cut, y + h),
                        (x, y + h),
                    ],
                    Some(fill),
                    Some(border),
                );
            }
            NodeShape::Cylinder => {
                let ry = 7.0;
                scene.rect(x, y + ry, w, h - ry * 2.0, 0.0, Some(fill), None);
                scene.path(
                    vec![
                        PathOp::Move(x, y + ry),
                        PathOp::Cubic(
                            x,
                            y + ry * 0.1,
                            x + w,
                            y + ry * 0.1,
                            x + w,
                            y + ry,
                        ),
                    ],
                    None,
                    Some(border),
                );
                scene.path(
                    vec![
                        PathOp::Move(x, y + h - ry),
                        PathOp::Cubic(
                            x,
                            y + h - ry * 0.1,
                            x + w,
                            y + h - ry * 0.1,
                            x + w,
                            y + h - ry,
                        ),
                    ],
                    None,
                    Some(border),
                );
                scene.line(x, y + ry, x, y + h - ry, border);
                scene.line(x + w, y + ry, x + w, y + h - ry, border);
            }
            NodeShape::Note => {
                scene.rect(
                    x,
                    y,
                    w,
                    h,
                    2.0,
                    Some(theme.note_fill),
                    Some(SceneStroke::solid(theme.note_stroke, 1.1)),
                );
                let fold = 12.0;
                scene.line(x + w - fold, y, x + w - fold, y + fold, border);
                scene.line(x + w - fold, y + fold, x + w, y + fold, border);
            }
            NodeShape::Subroutine => {
                scene.rect(x, y, w, h, 2.0, Some(fill), Some(border));
                scene.line(x + 7.0, y, x + 7.0, y + h, border);
                scene.line(x + w - 7.0, y, x + w - 7.0, y + h, border);
            }
            NodeShape::Asymmetric => {
                scene.polygon(
                    vec![(x, y), (x + w, y), (x + w, y + h), (x + 12.0, y + h)],
                    Some(fill),
                    Some(border),
                );
            }
            NodeShape::Rect | NodeShape::Rounded => {
                let radius = if node.shape == NodeShape::Rect { 0.0 } else { 6.0 };
                scene.rect(x, y, w, h, radius, Some(fill), Some(border));
            }
        }
        if node.accent.is_some() {
            scene.rect(x - 3.0, y - 3.0, w + 6.0, h + 6.0, 8.0, None, Some(SceneStroke::solid(accent, 1.0)));
        }
        let tx = x + (w - node.label.width) * 0.5;
        let ty = y + (h - node.label.height) * 0.5;
        scene.text(node.label.shaped.clone(), tx, ty, theme.text);
    }
}

fn group_depth(groups: &[GraphGroup], index: usize) -> usize {
    let mut depth = 0;
    let mut current = groups.get(index).and_then(|g| g.parent);
    while let Some(parent) = current {
        depth += 1;
        current = groups.get(parent).and_then(|g| g.parent);
        if depth > 32 {
            break;
        }
    }
    depth
}

/// Guess whether a label's first strong direction is RTL (used by builders for alignment).
pub fn is_rtl_label(text: &str) -> bool {
    detect_direction(text).is_rtl()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextFactory;
    use daftar_core::settings::TextAlign;
    use daftar_text::TextEngine;

    #[test]
    fn ranks_a_simple_chain() {
        let mut engine = TextEngine::new(None, "en-US");
        let mut factory = TextFactory::new(&mut engine);
        let a = factory.layout("A", 13.0, 400, 100.0, TextAlign::Center, None);
        let b = factory.layout("B", 13.0, 400, 100.0, TextAlign::Center, None);
        let nodes = vec![
            GraphNode::new("a", a, NodeShape::Rounded),
            GraphNode::new("b", b, NodeShape::Rounded),
        ];
        let edges = vec![GraphEdge::new(0, 1)];
        let laid = layout(&nodes, &edges, GraphDirection::TopBottom, 50.0, 40.0);
        assert_eq!(laid.ranks, vec![0, 1]);
        assert!(laid.centers[1].1 > laid.centers[0].1);
    }

    #[test]
    fn cycles_do_not_loop_forever() {
        let mut engine = TextEngine::new(None, "en-US");
        let mut factory = TextFactory::new(&mut engine);
        let mut nodes = Vec::new();
        for id in 0..4 {
            let label = factory.layout(&format!("N{id}"), 13.0, 400, 100.0, TextAlign::Center, None);
            nodes.push(GraphNode::new(format!("n{id}"), label, NodeShape::Rect));
        }
        let edges = vec![
            GraphEdge::new(0, 1),
            GraphEdge::new(1, 2),
            GraphEdge::new(2, 0),
            GraphEdge::new(2, 3),
        ];
        let laid = layout(&nodes, &edges, GraphDirection::LeftRight, 50.0, 30.0);
        assert!(laid.ranks.iter().all(|r| *r < 10));
        assert_eq!(laid.centers.len(), 4);
    }

    #[test]
    fn persian_label_detection() {
        assert!(is_rtl_label("سلام"));
        assert!(!is_rtl_label("Hello"));
    }
}
/// Draw edges and their labels without drawing node bodies. Used by diagrams that render
/// custom node shapes (classes, ER entities, states).
pub fn draw_edges_simple(
    scene: &mut Scene,
    nodes: &[GraphNode],
    edges: &[GraphEdge],
    layout: &GraphLayout,
    theme: &crate::DiagramTheme,
    origin: (f32, f32),
) {
    let centers: Vec<(f32, f32)> = layout
        .centers
        .iter()
        .map(|(x, y)| (x + origin.0, y + origin.1))
        .collect();
    for edge in edges {
        if edge.from >= nodes.len() || edge.to >= nodes.len() {
            continue;
        }
        let color = edge.color.unwrap_or(theme.line);
        let stroke = if edge.dashed {
            SceneStroke::dashed(color, if edge.thick { 2.0 } else { 1.3 })
        } else {
            SceneStroke::solid(color, if edge.thick { 2.0 } else { 1.3 })
        };
        if edge.from == edge.to {
            let c = centers[edge.from];
            let x = c.0 + nodes[edge.from].width * 0.5;
            let y = c.1 - nodes[edge.from].height * 0.2;
            scene.path(
                vec![
                    PathOp::Move(x, y),
                    PathOp::Cubic(x + 45.0, y - 30.0, x + 45.0, y + 30.0, x, y + 16.0),
                ],
                None,
                Some(stroke),
            );
            scene.arrowhead((x + 12.0, y + 19.0), (x + 1.0, y + 16.0), color, 8.0);
            continue;
        }
        let from_center = centers[edge.from];
        let to_center = centers[edge.to];
        let source = boundary(
            from_center,
            (nodes[edge.from].width, nodes[edge.from].height),
            to_center,
        );
        let target = boundary(
            to_center,
            (nodes[edge.to].width, nodes[edge.to].height),
            from_center,
        );
        let horizontal = (target.0 - source.0).abs() > (target.1 - source.1).abs();
        let (c1, c2) = if horizontal {
            (
                (source.0 + (target.0 - source.0) * 0.45, source.1),
                (target.0 - (target.0 - source.0) * 0.45, target.1),
            )
        } else {
            (
                (source.0, source.1 + (target.1 - source.1) * 0.45),
                (target.0, target.1 - (target.1 - source.1) * 0.45),
            )
        };
        scene.path(
            vec![
                PathOp::Move(source.0, source.1),
                PathOp::Cubic(c1.0, c1.1, c2.0, c2.1, target.0, target.1),
            ],
            None,
            Some(stroke),
        );
        if edge.arrow_end {
            scene.arrowhead(c2, target, color, 8.0);
        }
        if edge.arrow_start {
            scene.arrowhead(c1, source, color, 8.0);
        }
        if let Some(label) = &edge.label {
            let mx = (source.0 + target.0) * 0.5;
            let my = (source.1 + target.1) * 0.5;
            let w = label.width + 10.0;
            let h = label.height + 6.0;
            scene.rect(
                mx - w * 0.5,
                my - h * 0.5,
                w,
                h,
                4.0,
                Some(theme.surface),
                None,
            );
            scene.text(
                label.shaped.clone(),
                mx - label.width * 0.5,
                my - label.height * 0.5,
                theme.muted,
            );
        }
    }
}
