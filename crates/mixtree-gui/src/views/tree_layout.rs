//! Deterministic layout for the mix-tree, derived from the reference image.
//!
//! The tree lives in a **fixed logical coordinate system** ([`VIEW_W`] × [`VIEW_H`]).
//! Node positions are explicit and never depend on data order, BPM, energy or
//! playback — only on identity. Real (non-sample) libraries fall back to a
//! deterministic hub-and-fan placement. The caller maps these logical
//! coordinates onto the screen with a single proportional transform, so a window
//! resize scales the whole tree without re-distributing nodes.

/// Logical tree coordinate system — the reference tree area is
/// `(242,50)…(1148,751)` in the 1536×1024 image, i.e. 906×701, so the viewBox
/// keeps that exact aspect and positions are `normalized × view`.
pub const VIEW_W: f32 = 906.0;
/// Logical height of the tree coordinate system.
pub const VIEW_H: f32 = 701.0;

/// Maps a normalized `(0..1, 0..1)` reference position to logical coordinates.
const fn n(x: f32, y: f32) -> (f32, f32) {
    (x * VIEW_W, y * VIEW_H)
}

/// Fixed position of a top-level genre hub, from the reference's normalized
/// coordinates: Techno top-center, Trance mid-left, House mid-right, Ambient
/// bottom-left, Hardstyle bottom-right.
pub fn hub_pos(genre_id: &str) -> Option<(f32, f32)> {
    Some(match genre_id {
        "techno" => n(0.434, 0.123),
        "trance" => n(0.150, 0.367),
        "house" => n(0.717, 0.378),
        "ambient" => n(0.248, 0.739),
        "hardstyle" => n(0.619, 0.729),
        "electro" => n(0.05, 0.20),
        "breakbeat" => n(0.95, 0.20),
        "rock-indie" => n(0.5, 0.95),
        _ => return None,
    })
}

/// Fixed position of a known sample track, from the reference's normalized
/// coordinates (each genre's three tracks share a horizontal row below its hub).
pub fn sample_track_pos(title: &str) -> Option<(f32, f32)> {
    Some(match title {
        // Techno row
        "Rising Sun" => n(0.258, 0.237),
        "Hypnotic Flow" => n(0.436, 0.238),
        "Dark Matter" => n(0.674, 0.233),
        // Trance row
        "Skyline" => n(0.077, 0.488),
        "Euphoria" => n(0.204, 0.494),
        "Beyond" => n(0.320, 0.489),
        // House row
        "Night Drive" => n(0.599, 0.510),
        "Deep Horizon" => n(0.751, 0.515),
        "Lost in Time" => n(0.912, 0.511),
        // Ambient row (bottom-left)
        "Morning Light" => n(0.138, 0.843),
        "Floating" => n(0.254, 0.848),
        "Last Horizon" => n(0.380, 0.848),
        // Hardstyle row (bottom-right)
        "Raw Energy" => n(0.558, 0.843),
        "Unstoppable" => n(0.684, 0.845),
        "Inferno" => n(0.815, 0.843),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_sample_nodes_within_viewbox() {
        for title in [
            "Rising Sun",
            "Hypnotic Flow",
            "Dark Matter",
            "Skyline",
            "Euphoria",
            "Beyond",
            "Night Drive",
            "Deep Horizon",
            "Lost in Time",
            "Morning Light",
            "Floating",
            "Last Horizon",
            "Raw Energy",
            "Unstoppable",
            "Inferno",
        ] {
            let (x, y) = sample_track_pos(title).unwrap();
            assert!(
                (0.0..=VIEW_W).contains(&x) && (0.0..=VIEW_H).contains(&y),
                "{title} out of viewBox"
            );
        }
        for g in ["techno", "trance", "house", "ambient", "hardstyle"] {
            let (x, y) = hub_pos(g).unwrap();
            assert!(
                (0.0..=VIEW_W).contains(&x) && (0.0..=VIEW_H).contains(&y),
                "{g} hub out of viewBox"
            );
        }
    }

    #[test]
    fn layout_is_deterministic() {
        assert_eq!(
            sample_track_pos("Deep Horizon"),
            sample_track_pos("Deep Horizon")
        );
        assert_eq!(hub_pos("techno"), hub_pos("techno"));
    }
}
