//! The music graph: tracks and genre groups as nodes, scored transitions as
//! edges (spec §3C).
//!
//! For large libraries the graph is kept sparse by keeping only the top
//! `max_edges_per_node` outgoing transitions per track whose score clears
//! `min_edge_score` (spec: filtering / on-demand edge loading). The genre group
//! nodes give the tree its recognizable structure from the design.

use crate::configuration::{Configuration, MixGoal};
use crate::models::{Genre, Track};
use crate::transition_engine::{score_transition, GenreContext, TransitionScore};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A node in the music graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GraphNode {
    /// A genre / theme group (the labelled hubs in the design).
    Group {
        /// Genre id.
        id: String,
        /// Display name.
        name: String,
        /// Hex color.
        color: String,
        /// Optional BPM range label.
        bpm_range: Option<(u32, u32)>,
    },
    /// A single track.
    Track {
        /// Track id.
        id: i64,
        /// Title.
        title: String,
        /// Artist.
        artist: Option<String>,
        /// Top-level genre id (for coloring).
        genre: Option<String>,
        /// BPM.
        bpm: Option<f64>,
        /// Energy indicator.
        energy: Option<u8>,
        /// Camelot key as string.
        key: Option<String>,
        /// Cover-art path.
        cover_art_path: Option<String>,
    },
}

/// A directed, scored edge between two track nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    /// Source track id.
    pub from: i64,
    /// Target track id.
    pub to: i64,
    /// Full transition score.
    pub score: TransitionScore,
    /// `true` when this edge is part of the recommended red path.
    pub on_red_path: bool,
}

/// A membership edge linking a track to its genre group (for layout).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupLink {
    /// Genre group id.
    pub group: String,
    /// Track id.
    pub track: i64,
}

/// The assembled graph handed to the frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MusicGraph {
    /// All nodes (groups first, then tracks).
    pub nodes: Vec<GraphNode>,
    /// Scored transition edges between tracks.
    pub edges: Vec<GraphEdge>,
    /// Track → genre-group membership links.
    pub group_links: Vec<GroupLink>,
}

/// Builds a sparse music graph from a set of tracks and the genre taxonomy.
///
/// `genres` provides the group nodes and colors. `config` controls edge density
/// and the scoring weights / goal used for edge scores.
pub fn build_graph(tracks: &[Track], genres: &[Genre], config: &Configuration) -> MusicGraph {
    let ctx = GenreContext::from_genres(genres);
    let mut nodes = Vec::new();

    // Only include groups that actually contain tracks (keeps the tree tidy).
    let used_top: HashSet<String> = tracks
        .iter()
        .filter_map(|t| t.genre.clone())
        .map(|g| ctx.top_level(&g))
        .collect();
    for g in genres {
        if g.parent.is_none() && used_top.contains(&g.id) {
            nodes.push(GraphNode::Group {
                id: g.id.clone(),
                name: g.name.clone(),
                color: g.color.clone(),
                bpm_range: g.bpm_range,
            });
        }
    }

    let mut group_links = Vec::new();
    for t in tracks {
        nodes.push(GraphNode::Track {
            id: t.id,
            title: t.title.clone(),
            artist: t.artist.clone(),
            genre: t.genre.as_ref().map(|g| ctx.top_level(g)),
            bpm: t.bpm,
            energy: t.energy,
            key: t.key.map(|k| k.to_string()),
            cover_art_path: t.cover_art_path.clone(),
        });
        if let Some(g) = &t.genre {
            group_links.push(GroupLink {
                group: ctx.top_level(g),
                track: t.id,
            });
        }
    }

    // Top-K outgoing edges per track, above the score threshold.
    let played = HashSet::new();
    let mut edges = Vec::new();
    for from in tracks {
        let mut scored: Vec<(i64, TransitionScore)> = tracks
            .iter()
            .filter(|to| to.id != from.id)
            .map(|to| {
                (
                    to.id,
                    score_transition(from, to, &config.weights, &config.goal, &ctx, &played),
                )
            })
            .filter(|(_, s)| s.total >= config.min_edge_score)
            .collect();
        scored.sort_by(|a, b| {
            b.1.total
                .partial_cmp(&a.1.total)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(config.max_edges_per_node);
        for (to, score) in scored {
            edges.push(GraphEdge {
                from: from.id,
                to,
                score,
                on_red_path: false,
            });
        }
    }

    MusicGraph {
        nodes,
        edges,
        group_links,
    }
}

/// Marks the edges that lie along `path` (consecutive track ids) as the red path.
pub fn mark_red_path(graph: &mut MusicGraph, path: &[i64]) {
    let pairs: HashSet<(i64, i64)> = path.windows(2).map(|w| (w[0], w[1])).collect();
    for e in &mut graph.edges {
        e.on_red_path = pairs.contains(&(e.from, e.to));
    }
}

/// Convenience: build a graph and immediately mark a recommended path on it.
pub fn build_graph_with_path(
    tracks: &[Track],
    genres: &[Genre],
    config: &Configuration,
    start: i64,
    goal: &MixGoal,
) -> (MusicGraph, Vec<i64>) {
    let mut graph = build_graph(tracks, genres, config);
    let path = crate::pathfinding::recommend_path(
        tracks,
        genres,
        start,
        goal,
        &config.weights,
        &HashSet::new(),
        5,
    )
    .map(|p| p.track_ids)
    .unwrap_or_default();
    mark_red_path(&mut graph, &path);
    (graph, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::default_genres;
    use crate::sample_data::sample_tracks;

    #[test]
    fn graph_respects_edge_cap() {
        let tracks = sample_tracks();
        let genres = default_genres();
        let config = Configuration::default();
        let graph = build_graph(&tracks, &genres, &config);
        // Per-source edge count never exceeds the cap.
        for from in &tracks {
            let count = graph.edges.iter().filter(|e| e.from == from.id).count();
            assert!(count <= config.max_edges_per_node);
        }
        // Every track has a membership link and a node.
        assert_eq!(graph.group_links.len(), tracks.len());
    }

    #[test]
    fn groups_only_for_present_genres() {
        let tracks = sample_tracks();
        let genres = default_genres();
        let graph = build_graph(&tracks, &genres, &Configuration::default());
        let group_ids: HashSet<_> = graph
            .nodes
            .iter()
            .filter_map(|n| match n {
                GraphNode::Group { id, .. } => Some(id.clone()),
                _ => None,
            })
            .collect();
        // "rock-indie" has no sample tracks → no group node.
        assert!(!group_ids.contains("rock-indie"));
        assert!(group_ids.contains("techno"));
    }

    #[test]
    fn red_path_marks_edges() {
        let tracks = sample_tracks();
        let genres = default_genres();
        let (graph, path) = build_graph_with_path(
            &tracks,
            &genres,
            &Configuration::default(),
            1,
            &MixGoal::Neutral,
        );
        if path.len() >= 2 {
            let marked = graph.edges.iter().filter(|e| e.on_red_path).count();
            assert!(marked >= 1);
        }
    }
}
