//! Deterministic layout for the mix-tree, derived from the reference image.
//!
//! The tree lives in a **fixed logical coordinate system** ([`VIEW_W`] × [`VIEW_H`]).
//! Node positions are explicit and never depend on data order, BPM, energy or
//! playback — only on identity. Real (non-sample) libraries fall back to a
//! deterministic hub-and-fan placement. The caller maps these logical
//! coordinates onto the screen with a single proportional transform, so a window
//! resize scales the whole tree without re-distributing nodes.

/// Logical width of the tree coordinate system.
pub const VIEW_W: f32 = 860.0;
/// Logical height of the tree coordinate system.
pub const VIEW_H: f32 = 600.0;

/// Fixed position of a top-level genre hub (logical coords), matching the
/// reference: Techno top-center, Trance mid-left, House mid-right, Ambient
/// bottom-left, Hardstyle bottom-right.
pub fn hub_pos(genre_id: &str) -> Option<(f32, f32)> {
    Some(match genre_id {
        "techno" => (430.0, 48.0),
        "trance" => (175.0, 215.0),
        "house" => (685.0, 215.0),
        "ambient" => (275.0, 462.0),
        "hardstyle" => (610.0, 462.0),
        "electro" => (70.0, 110.0),
        "breakbeat" => (800.0, 110.0),
        "rock-indie" => (430.0, 560.0),
        _ => return None,
    })
}

/// Fixed position of a known sample track (logical coords), matching the
/// reference image's arrangement.
pub fn sample_track_pos(title: &str) -> Option<(f32, f32)> {
    Some(match title {
        // Techno row
        "Rising Sun" => (215.0, 122.0),
        "Hypnotic Flow" => (395.0, 128.0),
        "Dark Matter" => (595.0, 116.0),
        // Trance cluster (left)
        "Skyline" => (55.0, 300.0),
        "Euphoria" => (170.0, 318.0),
        "Beyond" => (285.0, 310.0),
        // House cluster (right)
        "Night Drive" => (520.0, 312.0),
        "Deep Horizon" => (665.0, 326.0),
        "Lost in Time" => (805.0, 305.0),
        // Ambient tracks (bottom-left)
        "Morning Light" => (110.0, 548.0),
        "Floating" => (215.0, 556.0),
        "Last Horizon" => (330.0, 562.0),
        // Hardstyle tracks (bottom-right)
        "Raw Energy" => (490.0, 548.0),
        "Unstoppable" => (600.0, 556.0),
        "Inferno" => (710.0, 548.0),
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
