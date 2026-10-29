//! The mix-tree: a custom-painted, pannable/zoomable node graph (spec §3C).
//!
//! Hierarchical top-down layout matching the design: each genre hub sits at a
//! designed position (Techno top, Trance/House mid, Ambient/Hardstyle bottom)
//! with its tracks fanned below. Hubs are glowing pills; tracks are neon-ringed
//! discs; genre branches and the red recommended path are curved. Rendering is
//! immediate-mode on the GPU — no DOM, no webview.

use crate::app::MixtreeApp;
use crate::theme;
use crate::views::tree_layout;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use mixtree_core::music_graph::GraphNode;
use std::collections::HashMap;

pub fn show(app: &mut MixtreeApp, ui: &mut egui::Ui) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Dein Mix-Baum").size(18.0).strong());
            ui.label(
                egui::RichText::new(
                    "Themen verbinden. Übergänge entdecken. Den perfekten Flow finden.",
                )
                .size(12.0)
                .color(theme::TEXT_DIM),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            ui.add_space(16.0);
            ui.vertical(|ui| {
                legend(ui, theme::RED, "Empfohlener Pfad");
                legend(ui, theme::CYAN, "Alternativen");
                ui.label(
                    egui::RichText::new("Klick = laden · Ziehen = schwenken · Rad = Zoom")
                        .size(10.0)
                        .color(theme::TEXT_DIM),
                );
            });
        });
    });

    let rect = ui.available_rect_before_wrap().shrink(10.0);
    let response = ui.allocate_rect(rect, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect,
        egui::Rounding::same(12.0),
        Color32::from_rgb(0x06, 0x09, 0x0f),
    );

    // --- interaction: pan + zoom ---
    if response.dragged() {
        app.graph_pan += response.drag_delta();
    }
    if let Some(hover) = response.hover_pos() {
        let scroll = ui.input(|i| i.raw_scroll_delta.y);
        if scroll != 0.0 {
            let old = app.graph_zoom;
            app.graph_zoom = (app.graph_zoom * (1.0 + scroll * 0.0015)).clamp(0.3, 3.0);
            // Zoom around the cursor.
            let factor = app.graph_zoom / old;
            let anchor = hover - rect.center().to_vec2() - app.graph_pan;
            app.graph_pan += anchor.to_vec2() * (1.0 - factor);
        }
    }

    // --- virtual positions (hub-and-spoke) ---
    let positions = layout_positions(app);
    if positions.is_empty() {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Keine Tracks — Filter anpassen oder Ordner importieren.",
            FontId::proportional(14.0),
            theme::TEXT_DIM,
        );
        return;
    }

    // Map the FIXED logical viewBox onto the panel with one proportional
    // transform (spec §5: scale the whole system, never re-distribute nodes).
    let scale = (rect.width() / tree_layout::VIEW_W).min(rect.height() / tree_layout::VIEW_H)
        * 0.96
        * app.graph_zoom;
    // Capture pan by value so the closure does not borrow `app` (the click
    // handler below mutates `app`).
    let pan = app.graph_pan;
    let origin =
        rect.center() - Vec2::new(tree_layout::VIEW_W, tree_layout::VIEW_H) * 0.5 * scale + pan;
    let to_screen = |p: Pos2| -> Pos2 { origin + p.to_vec2() * scale };

    // Node radii in screen space (also used to trim edges to node borders).
    let s_node = scale.clamp(0.6, 1.3);
    let track_r = 19.0 * s_node;
    let hub_r = 22.0 * s_node;

    let color_of = genre_colors(app);

    // Build & cache a cover-art texture for every track node: the real embedded
    // album cover when the file has one, else a procedural placeholder. Real
    // covers are read + decoded on background threads (file reads can be slow)
    // and uploaded here as they arrive; the window never blocks on them.
    let ctx = ui.ctx().clone();
    let file_of: HashMap<i64, String> = app
        .tracks
        .iter()
        .map(|t| (t.id, t.file_path.clone()))
        .collect();
    // Upload any covers finished by background threads.
    while let Ok((id, img)) = app.cover_rx.try_recv() {
        let tex = ctx.load_texture(format!("cover-{id}"), img, egui::TextureOptions::LINEAR);
        app.node_textures.insert(id, tex);
    }
    // Request decodes for tracks that still have no texture and aren't in flight.
    let to_request: Vec<(i64, Color32, Option<String>)> = app
        .graph
        .nodes
        .iter()
        .filter_map(|n| match n {
            GraphNode::Track { id, genre, .. }
                if !app.node_textures.contains_key(id) && !app.cover_requested.contains(id) =>
            {
                let col = genre
                    .as_deref()
                    .and_then(|g| color_of.get(g).copied())
                    .unwrap_or(theme::GREY);
                Some((*id, col, file_of.get(id).cloned()))
            }
            _ => None,
        })
        .collect();
    for (id, col, file) in to_request {
        app.cover_requested.insert(id);
        match file
            .filter(|p| !p.starts_with("sample://") && std::path::Path::new(p).exists())
        {
            Some(path) => {
                // Real file: read + decode the embedded cover off-thread.
                let tx = app.cover_tx.clone();
                std::thread::spawn(move || {
                    let img = mixtree_core::metadata::cover_bytes(std::path::Path::new(&path))
                        .and_then(|bytes| decode_cover(&bytes))
                        .unwrap_or_else(|| cover_image(id, col));
                    let _ = tx.send((id, img));
                });
            }
            None => {
                // Sample / missing file: cheap procedural placeholder, inline.
                let img = cover_image(id, col);
                let tex =
                    ctx.load_texture(format!("cover-{id}"), img, egui::TextureOptions::LINEAR);
                app.node_textures.insert(id, tex);
            }
        }
    }
    // Keep repainting while real covers are still decoding so they pop in.
    if app
        .cover_requested
        .iter()
        .any(|id| !app.node_textures.contains_key(id))
    {
        ctx.request_repaint_after(std::time::Duration::from_millis(120));
    }

    // --- very subtle ambient theme lighting behind each genre area (depth) ---
    for n in &app.graph.nodes {
        if let GraphNode::Group { id, color, .. } = n {
            if let Some(&p) = positions.get(&node_key_group(id)) {
                let c = to_screen(p);
                let col = theme::hex(color);
                for k in 0..5 {
                    let rr = (95.0 - k as f32 * 18.0) * scale;
                    if rr > 0.0 {
                        painter.circle_filled(c, rr, with_alpha(col, 3));
                    }
                }
            }
        }
    }

    // --- genre branches (hub → its tracks), the coloured tree ---
    for l in &app.graph.group_links {
        let (Some(&h), Some(&t)) = (
            positions.get(&node_key_group(&l.group)),
            positions.get(&node_key_track(l.track)),
        ) else {
            continue;
        };
        let col = color_of.get(&l.group).copied().unwrap_or(theme::GREY);
        let (sa, sb) = trim(to_screen(h), to_screen(t), hub_r + 2.0, track_r + 7.0);
        curve(
            &painter,
            sa,
            sb,
            Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 130),
            1.6,
            true,
        );
    }

    // BPM pills are collected here and drawn last, so they sit on top of nodes
    // and hub plates instead of being painted over by them.
    let mut pills: Vec<(Pos2, String, Color32)> = Vec::new();

    // --- alternative transitions from the current track (cyan, sparse) ---
    let current = app.current;
    for e in &app.graph.edges {
        if e.on_red_path || Some(e.from) != current {
            continue;
        }
        let (Some(&a), Some(&b)) = (
            positions.get(&node_key_track(e.from)),
            positions.get(&node_key_track(e.to)),
        ) else {
            continue;
        };
        let (sa, sb) = trim(to_screen(a), to_screen(b), track_r + 2.0, track_r + 7.0);
        let mid = curve(
            &painter,
            sa,
            sb,
            Color32::from_rgba_unmultiplied(0x2d, 0x9c, 0xff, 170),
            2.0,
            true,
        );
        if let Some(d) = e.score.bpm_diff {
            pills.push((mid, format!("{:+.0} BPM", d), theme::CYAN));
        }
    }

    // --- the red recommended path as an animated plasma ribbon ---
    let time = ui.input(|i| i.time) as f32;
    if let Some(path) = &app.path {
        let pts: Vec<Pos2> = path
            .track_ids
            .iter()
            .filter_map(|id| positions.get(&node_key_track(*id)).map(|&p| to_screen(p)))
            .collect();
        if pts.len() >= 2 {
            draw_plasma(&painter, &pts, time, app.reduced_motion);
        }
        // BPM pills at each hop midpoint.
        for w in path.track_ids.windows(2) {
            if let (Some(&a), Some(&b)) = (
                positions.get(&node_key_track(w[0])),
                positions.get(&node_key_track(w[1])),
            ) {
                if let Some(e) = app
                    .graph
                    .edges
                    .iter()
                    .find(|e| e.from == w[0] && e.to == w[1])
                {
                    if let Some(d) = e.score.bpm_diff {
                        pills.push((
                            to_screen(a).lerp(to_screen(b), 0.5),
                            format!("{:+.0} BPM", d),
                            theme::RED,
                        ));
                    }
                }
            }
        }
    }

    // --- nodes ---
    for n in &app.graph.nodes {
        match n {
            GraphNode::Group {
                id,
                name,
                color,
                bpm_range,
            } => {
                let Some(&p) = positions.get(&node_key_group(id)) else {
                    continue;
                };
                let c = to_screen(p);
                let col = theme::hex(color);
                let s = scale.clamp(0.6, 1.3);
                // Genre circle: soft glow ring + dark centre + symbol (initial).
                let gr = hub_r;
                painter.circle_filled(c, gr - 1.0, Color32::from_rgb(0x0b, 0x12, 0x1e));
                glow_ring(&painter, c, gr, col, 3.0);
                // Genre glyph (drawn, matching the reference icons), not a letter.
                hub_icon(&painter, c, 11.0 * s, id, with_alpha(col, 235));
                // Attached rounded label plate to the right (name + BPM range).
                let plate_h = 40.0 * s;
                let plate = Rect::from_min_size(
                    egui::pos2(c.x + gr + 6.0 * s, c.y - plate_h / 2.0),
                    Vec2::new(104.0 * s, plate_h),
                );
                painter.rect(
                    plate.expand(3.0),
                    egui::Rounding::same(11.0),
                    Color32::TRANSPARENT,
                    Stroke::new(5.0, with_alpha(col, 20)),
                );
                painter.rect(
                    plate,
                    egui::Rounding::same(9.0),
                    Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 32),
                    Stroke::new(1.5, col),
                );
                let tx = plate.left() + 9.0 * s;
                painter.text(
                    egui::pos2(tx, c.y - 7.0 * s),
                    Align2::LEFT_CENTER,
                    name,
                    FontId::proportional(12.5 * s),
                    theme::TEXT,
                );
                if let Some((lo, hi)) = bpm_range {
                    painter.text(
                        egui::pos2(tx, c.y + 8.0 * s),
                        Align2::LEFT_CENTER,
                        format!("BPM {lo}–{hi}"),
                        FontId::proportional(9.0 * s),
                        theme::TEXT_DIM,
                    );
                }
            }
            GraphNode::Track {
                id,
                title,
                genre,
                bpm,
                energy,
                ..
            } => {
                let Some(&p) = positions.get(&node_key_track(*id)) else {
                    continue;
                };
                let c = to_screen(p);
                let col = genre
                    .as_deref()
                    .and_then(|g| color_of.get(g).copied())
                    .unwrap_or(theme::GREY);
                let is_current = app.current == Some(*id);
                let ring = if is_current { theme::RED } else { col };
                let radius = (if is_current { 24.0 } else { 19.0 }) * scale.clamp(0.6, 1.3);
                // Cover-art disc (procedural placeholder, or a real embedded cover
                // for imported tracks).
                if let Some(tex) = app.node_textures.get(id) {
                    textured_circle(&painter, tex, c, radius);
                } else {
                    painter.circle_filled(c, radius, Color32::from_rgb(0x0b, 0x12, 0x1e));
                }
                // Soft neon ring (multi-pass bloom).
                glow_ring(
                    &painter,
                    c,
                    radius,
                    ring,
                    if is_current { 3.5 } else { 2.5 },
                );
                if is_current {
                    // Extra outer halo for the active track (as in the reference).
                    painter.circle_stroke(c, radius + 6.0, Stroke::new(2.0, red_a(120)));
                    painter.circle_stroke(c, radius + 10.0, Stroke::new(3.0, red_a(30)));
                }
                let title_g =
                    painter.layout_no_wrap(title.clone(), FontId::proportional(9.5), theme::TEXT);
                painter.galley(
                    egui::pos2(c.x - title_g.size().x / 2.0, c.y + radius + 4.0),
                    title_g,
                    theme::TEXT,
                );
                // Two stacked meta lines, as in the reference: "BPM 138" / "Energie 85%".
                painter.text(
                    c + Vec2::new(0.0, radius + 15.0),
                    Align2::CENTER_TOP,
                    format!(
                        "BPM {}",
                        bpm.map(|b| format!("{b:.0}")).unwrap_or_else(|| "—".into()),
                    ),
                    FontId::proportional(8.0),
                    theme::TEXT_DIM,
                );
                painter.text(
                    c + Vec2::new(0.0, radius + 25.0),
                    Align2::CENTER_TOP,
                    format!(
                        "Energie {}",
                        energy
                            .map(|e| format!("{e}%"))
                            .unwrap_or_else(|| "—".into()),
                    ),
                    FontId::proportional(8.0),
                    theme::TEXT_DIM,
                );
            }
        }
    }

    // --- BPM pills on top (so hub plates never cover them) ---
    for (pos, text, color) in pills {
        pill_label(&painter, pos, text, color);
    }

    // --- beat-synced playback ring on the currently-playing deck-A track ---
    if let Some(tid) = app.deck_a.track.as_ref().map(|t| t.id) {
        if let Some(&p) = positions.get(&node_key_track(tid)) {
            let playing = app.audio.is_playing(0);
            let pos_s = app.audio.position_ms(0) as f64 / 1000.0;
            let bpm = app.deck_a.effective_bpm().unwrap_or(0.0);
            // Beat phase from the real playback position (not a free timer).
            let phase = if playing && bpm > 0.0 {
                let beat = 60.0 / bpm;
                ((pos_s / beat).rem_euclid(1.0)) as f32
            } else {
                0.0
            };
            let ring_r = 19.0 * scale.clamp(0.6, 1.3) + 9.0;
            draw_beat_ring(
                &painter,
                to_screen(p),
                ring_r,
                phase,
                playing,
                app.reduced_motion,
            );
        }
    }

    // Keep the plasma/beat animation flowing without a busy-loop when static.
    if !app.reduced_motion {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }

    // --- click selection (nearest track node) ---
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let mut best: Option<(i64, f32)> = None;
            for n in &app.graph.nodes {
                if let GraphNode::Track { id, .. } = n {
                    if let Some(&p) = positions.get(&node_key_track(*id)) {
                        let d = to_screen(p).distance(pos);
                        if d < 30.0 && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                            best = Some((*id, d));
                        }
                    }
                }
            }
            if let Some((id, _)) = best {
                app.set_current(id);
            }
        }
    }
}

fn legend(ui: &mut egui::Ui, color: Color32, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 4.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, egui::Rounding::same(2.0), color);
        ui.label(egui::RichText::new(text).size(11.0).color(theme::TEXT_DIM));
    });
}

fn genre_colors(app: &MixtreeApp) -> HashMap<String, Color32> {
    app.genres
        .iter()
        .filter(|g| g.parent.is_none())
        .map(|g| (g.id.clone(), theme::hex(&g.color)))
        .collect()
}

fn node_key_track(id: i64) -> String {
    format!("t:{id}")
}
fn node_key_group(id: &str) -> String {
    format!("g:{id}")
}

/// Computes the deterministic layout in the fixed logical viewBox: hubs and
/// known sample tracks take their explicit reference positions; any other track
/// is fanned deterministically below its hub (for real, imported libraries).
fn layout_positions(app: &MixtreeApp) -> HashMap<String, Pos2> {
    let groups: Vec<&str> = app
        .graph
        .nodes
        .iter()
        .filter_map(|n| match n {
            GraphNode::Group { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    let title_of: HashMap<i64, &str> = app
        .graph
        .nodes
        .iter()
        .filter_map(|n| match n {
            GraphNode::Track { id, title, .. } => Some((*id, title.as_str())),
            _ => None,
        })
        .collect();
    let mut tracks_by_group: HashMap<&str, Vec<i64>> = HashMap::new();
    for l in &app.graph.group_links {
        tracks_by_group
            .entry(l.group.as_str())
            .or_default()
            .push(l.track);
    }
    let unknown_hubs: Vec<&str> = groups
        .iter()
        .copied()
        .filter(|g| tree_layout::hub_pos(g).is_none())
        .collect();

    let mut pos = HashMap::new();
    for g in &groups {
        let (hx, hy) = tree_layout::hub_pos(g).unwrap_or_else(|| {
            let k = unknown_hubs.iter().position(|u| u == g).unwrap_or(0) as f32;
            (90.0 + k * 130.0, tree_layout::VIEW_H - 30.0)
        });
        pos.insert(node_key_group(g), Pos2::new(hx, hy));

        let members = tracks_by_group.get(g).cloned().unwrap_or_default();
        // Designed sample tracks keep their exact reference positions (mockup
        // fidelity); everything else (an imported library) is fanned below the
        // hub. Unknown tracks are indexed among *themselves* so the fan spacing
        // is independent of how many sample tracks the group also has.
        let mut unknown: Vec<i64> = Vec::new();
        for tid in &members {
            match title_of
                .get(tid)
                .and_then(|t| tree_layout::sample_track_pos(t))
            {
                Some((x, y)) => {
                    pos.insert(node_key_track(*tid), Pos2::new(x, y));
                }
                None => unknown.push(*tid),
            }
        }
        // Fan unknown tracks in rows of up to `PER_ROW`, with wide horizontal
        // spacing (so the title labels don't collide) and a slight per-column
        // vertical stagger for extra legibility.
        const PER_ROW: usize = 4;
        const COL_W: f32 = 94.0;
        const ROW_H: f32 = 96.0;
        let m = unknown.len();
        for (k, tid) in unknown.iter().enumerate() {
            let row = k / PER_ROW;
            let col = k % PER_ROW;
            let in_row = (m - row * PER_ROW).min(PER_ROW);
            let dx = (col as f32 - (in_row as f32 - 1.0) / 2.0) * COL_W;
            let stagger = if col % 2 == 1 { 24.0 } else { 0.0 };
            let dy = 120.0 + row as f32 * ROW_H + stagger;
            pos.insert(node_key_track(*tid), Pos2::new(hx + dx, hy + dy));
        }
    }
    pos
}

/// Trims a segment so it starts/ends on the node borders (radii `ra`/`rb`),
/// not at the centres — so edges and arrowheads stay visible outside the discs.
fn trim(a: Pos2, b: Pos2, ra: f32, rb: f32) -> (Pos2, Pos2) {
    let d = b - a;
    let len = d.length().max(1.0);
    let u = d / len;
    (a + u * ra, b - u * rb)
}

fn arrowhead(painter: &egui::Painter, from: Pos2, to: Pos2, color: Color32, size: f32) {
    let dir = (to - from).normalized();
    // The endpoint is already trimmed to the node border; sit just before it.
    let tip = to - dir * 2.0;
    let perp = Vec2::new(-dir.y, dir.x);
    let p1 = tip - dir * size + perp * (size * 0.5);
    let p2 = tip - dir * size - perp * (size * 0.5);
    painter.add(egui::Shape::convex_polygon(
        vec![tip, p1, p2],
        color,
        Stroke::NONE,
    ));
}

/// Draws a gently-curved edge from `a` to `b` and returns its apex (for a label).
fn curve(
    painter: &egui::Painter,
    a: Pos2,
    b: Pos2,
    color: Color32,
    width: f32,
    arrow: bool,
) -> Pos2 {
    let delta = b - a;
    let len = delta.length().max(1.0);
    let perp = Vec2::new(-delta.y, delta.x) / len;
    let ctrl = a.lerp(b, 0.5) + perp * (len * 0.12);
    let stroke_curve = |w: f32, col: Color32| {
        painter.add(egui::epaint::QuadraticBezierShape::from_points_stroke(
            [a, ctrl, b],
            false,
            Color32::TRANSPARENT,
            Stroke::new(w, col),
        ));
    };
    // Soft multi-pass bloom (wide+faint → narrow+bright core).
    for (mul, alpha) in [(3.6, 14u8), (2.2, 30), (1.4, 70)] {
        stroke_curve(width * mul, with_alpha(color, alpha));
    }
    stroke_curve(width, color);
    if arrow {
        arrowhead(painter, ctrl, b, color, 8.0);
    }
    // Point at t=0.5 on the quadratic Bézier.
    egui::pos2(
        0.25 * a.x + 0.5 * ctrl.x + 0.25 * b.x,
        0.25 * a.y + 0.5 * ctrl.y + 0.25 * b.y,
    )
}

fn red_a(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(0xff, 0x2d, 0x55, alpha)
}

/// The colour with a specific alpha (keeps RGB, used for glow passes).
fn with_alpha(c: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
}

/// A soft neon ring: several faint wide passes bloom outward, then a bright core.
fn glow_ring(painter: &egui::Painter, c: Pos2, r: f32, color: Color32, core_w: f32) {
    for (mul, alpha) in [(5.0, 10u8), (3.2, 22), (2.0, 48)] {
        painter.circle_stroke(c, r, Stroke::new(core_w * mul, with_alpha(color, alpha)));
    }
    painter.circle_stroke(c, r, Stroke::new(core_w, color));
}

/// Draws a small genre glyph centred at `c`, roughly `size` radius, matching the
/// reference hub icons: a home for house, equaliser bars for techno/hardstyle,
/// ascending chevrons for trance, a sparkle for ambient, else a music dot.
fn hub_icon(painter: &egui::Painter, c: Pos2, size: f32, genre_id: &str, col: Color32) {
    let w = Stroke::new((size * 0.22).max(1.6), col);
    match genre_id {
        "house" => {
            let s = size;
            let roof = s * 0.5;
            // Roof.
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(c.x, c.y - s),
                    Pos2::new(c.x - s, c.y - roof * 0.2),
                    Pos2::new(c.x + s, c.y - roof * 0.2),
                ],
                col,
                Stroke::NONE,
            ));
            // Body.
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(c.x - s * 0.62, c.y - roof * 0.2),
                    Pos2::new(c.x + s * 0.62, c.y + s * 0.75),
                ),
                egui::Rounding::same(1.5),
                col,
            );
        }
        "techno" | "hardstyle" => {
            // Equaliser bars.
            let heights = [0.5f32, 1.0, 0.7, 0.95, 0.55];
            let n = heights.len();
            let bw = size * 1.9 / n as f32;
            for (i, h) in heights.iter().enumerate() {
                let x = c.x - size * 0.95 + (i as f32 + 0.5) * bw;
                let bh = size * h;
                painter.line_segment(
                    [Pos2::new(x, c.y + bh * 0.6), Pos2::new(x, c.y - bh * 0.6)],
                    Stroke::new((bw * 0.55).max(1.6), col),
                );
            }
        }
        "trance" => {
            // Two ascending chevrons.
            for k in 0..2 {
                let dy = c.y + size * 0.55 - k as f32 * size * 0.7;
                painter.line_segment([Pos2::new(c.x - size * 0.8, dy), Pos2::new(c.x, dy - size * 0.7)], w);
                painter.line_segment([Pos2::new(c.x, dy - size * 0.7), Pos2::new(c.x + size * 0.8, dy)], w);
            }
        }
        "ambient" => {
            // Four-point sparkle.
            painter.line_segment([Pos2::new(c.x, c.y - size), Pos2::new(c.x, c.y + size)], w);
            painter.line_segment([Pos2::new(c.x - size, c.y), Pos2::new(c.x + size, c.y)], w);
            let d = size * 0.55;
            painter.line_segment([Pos2::new(c.x - d, c.y - d), Pos2::new(c.x + d, c.y + d)], Stroke::new(w.width * 0.7, col));
            painter.line_segment([Pos2::new(c.x - d, c.y + d), Pos2::new(c.x + d, c.y - d)], Stroke::new(w.width * 0.7, col));
        }
        _ => {
            // Generic "music" mark: a note head + stem.
            painter.circle_filled(Pos2::new(c.x - size * 0.3, c.y + size * 0.4), size * 0.4, col);
            painter.line_segment(
                [Pos2::new(c.x + size * 0.1, c.y + size * 0.4), Pos2::new(c.x + size * 0.1, c.y - size * 0.8)],
                w,
            );
        }
    }
}

/// Opaque linear blend of two colours.
fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 * (1.0 - t) + y as f32 * t) as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

/// Decodes embedded cover-art bytes (PNG/JPEG) into a small egui image.
fn decode_cover(bytes: &[u8]) -> Option<egui::ColorImage> {
    let img = image::load_from_memory(bytes).ok()?;
    let thumb = img.thumbnail(96, 96).to_rgba8();
    let (w, h) = (thumb.width() as usize, thumb.height() as usize);
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [w, h],
        thumb.as_raw(),
    ))
}

/// Generates a procedural "cover art" image for a track: a genre-tinted sunset
/// gradient with a horizon glow and a small sun, varied per track id. Stands in
/// for real embedded cover art until tracks provide one.
fn cover_image(id: i64, base: Color32) -> egui::ColorImage {
    let s: usize = 56;
    let hvar = id.rem_euclid(6) as f32 / 6.0;
    let horizon = (s as f32 * (0.40 + 0.20 * hvar)) as i32;
    let sky_top = Color32::from_rgb(8, 11, 22);
    let sky_bot = blend(base, Color32::BLACK, 0.5);
    let glow = blend(base, Color32::WHITE, 0.3);
    let sun = blend(base, Color32::WHITE, 0.6);
    let mut pixels = Vec::with_capacity(s * s);
    for y in 0..s as i32 {
        for x in 0..s as i32 {
            let ty = y as f32 / s as f32;
            let mut c = blend(sky_top, sky_bot, ty);
            let d = (y - horizon).abs();
            if d < 5 {
                c = blend(c, glow, (1.0 - d as f32 / 5.0) * 0.8);
            }
            let sx = s as f32 * 0.5;
            let sy = horizon as f32 - 5.0;
            let r = ((x as f32 - sx).powi(2) + (y as f32 - sy).powi(2)).sqrt();
            if r < 8.0 {
                c = blend(c, sun, 1.0 - r / 8.0);
            }
            pixels.push(c);
        }
    }
    egui::ColorImage {
        size: [s, s],
        pixels,
    }
}

/// Draws a texture clipped to a circle (a filled disc showing the cover art).
fn textured_circle(painter: &egui::Painter, tex: &egui::TextureHandle, center: Pos2, radius: f32) {
    use egui::epaint::{Mesh, Vertex};
    let n = 30u32;
    let mut mesh = Mesh::with_texture(tex.id());
    mesh.vertices.push(Vertex {
        pos: center,
        uv: egui::pos2(0.5, 0.5),
        color: Color32::WHITE,
    });
    for i in 0..=n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        let (sn, cs) = a.sin_cos();
        mesh.vertices.push(Vertex {
            pos: egui::pos2(center.x + cs * radius, center.y + sn * radius),
            uv: egui::pos2(0.5 + 0.5 * cs, 0.5 + 0.5 * sn),
            color: Color32::WHITE,
        });
    }
    for i in 1..=n {
        mesh.indices.extend_from_slice(&[0, i, i + 1]);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Samples the same gentle quadratic curve `curve()` draws, into `n` points.
fn bezier_poly(a: Pos2, b: Pos2, n: usize) -> Vec<Pos2> {
    let delta = b - a;
    let len = delta.length().max(1.0);
    let perp = Vec2::new(-delta.y, delta.x) / len;
    let ctrl = a.lerp(b, 0.5) + perp * (len * 0.12);
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let mt = 1.0 - t;
            Pos2::new(
                mt * mt * a.x + 2.0 * mt * t * ctrl.x + t * t * b.x,
                mt * mt * a.y + 2.0 * mt * t * ctrl.y + t * t * b.y,
            )
        })
        .collect()
}

/// Draws the red recommended path as a plasma ribbon: a wide soft glow, a bright
/// core, a gentle pulse and a travelling light that flows in playback order.
/// Geometry is fixed; only colours/positions of the light vary with `time`.
fn draw_plasma(painter: &egui::Painter, pts: &[Pos2], time: f32, reduced: bool) {
    // One connected polyline across all hops (sampled along the curves).
    let mut poly: Vec<Pos2> = Vec::new();
    for w in pts.windows(2) {
        let seg = bezier_poly(w[0], w[1], 18);
        if poly.is_empty() {
            poly.extend(seg);
        } else {
            poly.extend(seg.into_iter().skip(1));
        }
    }
    if poly.len() < 2 {
        return;
    }

    // Subtle pulse of the core brightness.
    let pulse = if reduced {
        1.0
    } else {
        0.5 + 0.5 * (time * 2.2).sin()
    };
    let glow_a = (26.0 + 22.0 * pulse) as u8;
    let core_a = (190.0 + 60.0 * pulse) as u8;

    for w in poly.windows(2) {
        painter.line_segment([w[0], w[1]], Stroke::new(11.0, red_a(glow_a / 2)));
        painter.line_segment([w[0], w[1]], Stroke::new(7.0, red_a(glow_a)));
        painter.line_segment([w[0], w[1]], Stroke::new(3.5, red_a(core_a)));
    }

    if reduced {
        return;
    }

    // Travelling light ("comet") flowing along the path in playback direction.
    let seg_len: Vec<f32> = poly.windows(2).map(|w| (w[1] - w[0]).length()).collect();
    let total: f32 = seg_len.iter().sum();
    if total <= 1.0 {
        return;
    }
    let head = (time * 0.16).fract() * total; // slow, continuous
    let point_at = |mut d: f32| -> Pos2 {
        d = d.rem_euclid(total);
        for (i, &l) in seg_len.iter().enumerate() {
            if d <= l {
                let f = if l > 0.0 { d / l } else { 0.0 };
                return poly[i].lerp(poly[i + 1], f);
            }
            d -= l;
        }
        *poly.last().unwrap()
    };
    for k in 0..10 {
        let d = head - k as f32 * 10.0;
        if d < 0.0 {
            continue;
        }
        let fade = 1.0 - k as f32 / 10.0;
        let cp = point_at(d);
        painter.circle_filled(cp, 5.0 * fade, red_a((70.0 * fade) as u8));
        painter.circle_filled(
            cp,
            2.2 * fade + 0.8,
            Color32::from_rgba_unmultiplied(255, 220, 230, (230.0 * fade) as u8),
        );
    }
}

/// Draws the beat-synced ring + orbiting light around the playing track. The
/// node itself never moves — only the light on the ring and the glow intensity.
fn draw_beat_ring(
    painter: &egui::Painter,
    c: Pos2,
    r: f32,
    phase: f32,
    playing: bool,
    reduced: bool,
) {
    // Brightness peaks at the beat (phase≈0) and decays.
    let beat = if playing {
        (1.0 - phase).powf(1.6)
    } else {
        0.25
    };
    let ring_a = (70.0 + 150.0 * beat) as u8;
    painter.circle_stroke(c, r + 3.0, Stroke::new(4.0, red_a(24)));
    painter.circle_stroke(c, r, Stroke::new(2.0, red_a(ring_a)));

    if !playing {
        return;
    }
    // Orbiting light: continuous when moving; a resting dot under reduced-motion.
    let ang = if reduced {
        -std::f32::consts::FRAC_PI_2
    } else {
        phase * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2
    };
    let lp = c + Vec2::angled(ang) * r;
    painter.circle_filled(lp, 6.0, red_a(90));
    painter.circle_filled(lp, 3.0, Color32::from_rgb(255, 220, 230));
}

fn pill_label(painter: &egui::Painter, center: Pos2, text: String, color: Color32) {
    let font = FontId::proportional(10.0);
    let galley = painter.layout_no_wrap(text, font.clone(), color);
    let rect = Rect::from_center_size(center, galley.size() + Vec2::new(8.0, 4.0));
    painter.rect_filled(
        rect,
        egui::Rounding::same(4.0),
        Color32::from_rgb(0x2a, 0x08, 0x10),
    );
    painter.galley(rect.center() - galley.size() / 2.0, galley, color);
}
