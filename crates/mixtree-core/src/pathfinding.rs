//! Pathfinding through the music graph — the "red path" (spec §4).
//!
//! Two strategies:
//! - [`shortest_path`]: Dijkstra to an explicit target track, minimizing
//!   `Σ (1 − transition_score)` so higher-scoring chains are preferred.
//! - [`recommend_path`]: goal-directed beam search that grows a musically
//!   coherent arc of a bounded length when there is no single target track.
//!
//! Both return a [`RecommendedPath`] carrying the per-step scores and a
//! human-readable rationale (explainability, spec §4).

use crate::configuration::{MixGoal, TransitionWeights};
use crate::models::{Genre, Track};
use crate::transition_engine::{score_transition, GenreContext, TransitionScore};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

/// A recommended path through the graph with full, explainable scoring.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecommendedPath {
    /// Ordered track ids, starting with the given start track.
    pub track_ids: Vec<i64>,
    /// Score of each hop (`track_ids.len() - 1` entries).
    pub steps: Vec<TransitionScore>,
    /// Average hop score — the overall quality of the arc, `0.0..=1.0`.
    pub average_score: f64,
    /// Short rationale describing the arc and its trade-offs.
    pub rationale: String,
}

fn average(steps: &[TransitionScore]) -> f64 {
    if steps.is_empty() {
        0.0
    } else {
        steps.iter().map(|s| s.total).sum::<f64>() / steps.len() as f64
    }
}

/// Dijkstra shortest (highest-scoring) path from `start` to `target`.
///
/// Edge cost is `1 − transition_score`, so a lower total cost means a
/// higher-quality chain. Returns `None` if no path exists.
pub fn shortest_path(
    tracks: &[Track],
    genres: &[Genre],
    start: i64,
    target: i64,
    weights: &TransitionWeights,
) -> Option<RecommendedPath> {
    let ctx = GenreContext::from_genres(genres);
    let by_id: HashMap<i64, &Track> = tracks.iter().map(|t| (t.id, t)).collect();
    if !by_id.contains_key(&start) || !by_id.contains_key(&target) {
        return None;
    }
    let played = HashSet::new();

    // Min-heap on accumulated cost via Reverse ordering.
    let mut dist: HashMap<i64, f64> = HashMap::new();
    let mut prev: HashMap<i64, i64> = HashMap::new();
    let mut heap = BinaryHeap::new();
    dist.insert(start, 0.0);
    heap.push(State {
        cost: 0.0,
        node: start,
    });

    while let Some(State { cost, node }) = heap.pop() {
        if node == target {
            break;
        }
        if cost > *dist.get(&node).unwrap_or(&f64::INFINITY) {
            continue;
        }
        let from = by_id[&node];
        for to in tracks {
            if to.id == node {
                continue;
            }
            let s = score_transition(from, to, weights, &MixGoal::Neutral, &ctx, &played);
            let next_cost = cost + (1.0 - s.total);
            if next_cost < *dist.get(&to.id).unwrap_or(&f64::INFINITY) {
                dist.insert(to.id, next_cost);
                prev.insert(to.id, node);
                heap.push(State {
                    cost: next_cost,
                    node: to.id,
                });
            }
        }
    }

    if !prev.contains_key(&target) && start != target {
        return None;
    }

    // Reconstruct.
    let mut ids = vec![target];
    let mut cur = target;
    while cur != start {
        cur = *prev.get(&cur)?;
        ids.push(cur);
    }
    ids.reverse();
    Some(materialize(&ids, &by_id, &ctx, weights, &MixGoal::Neutral))
}

/// Goal-directed beam search that grows a coherent path of up to `max_len`
/// tracks (including the start) from `start`.
///
/// Avoids revisiting tracks within the same path and respects `played` (the
/// repetition penalty already present in the score discourages reuse further).
pub fn recommend_path(
    tracks: &[Track],
    genres: &[Genre],
    start: i64,
    goal: &MixGoal,
    weights: &TransitionWeights,
    played: &HashSet<i64>,
    max_len: usize,
) -> Option<RecommendedPath> {
    let ctx = GenreContext::from_genres(genres);
    let by_id: HashMap<i64, &Track> = tracks.iter().map(|t| (t.id, t)).collect();
    by_id.get(&start)?;
    let max_len = max_len.max(1);

    // If the goal names a reachable target track, prefer the exact chain to it.
    if let MixGoal::TargetTrack(target) = goal {
        if let Some(p) = shortest_path(tracks, genres, start, *target, weights) {
            return Some(p);
        }
    }

    const BEAM_WIDTH: usize = 6;
    // A beam entry: the path so far and its accumulated score.
    struct Beam {
        ids: Vec<i64>,
        acc: f64,
    }
    let mut beams = vec![Beam {
        ids: vec![start],
        acc: 0.0,
    }];

    for _ in 1..max_len {
        let mut candidates: Vec<Beam> = Vec::new();
        for beam in &beams {
            let last = *beam.ids.last().unwrap();
            let visited: HashSet<i64> = beam.ids.iter().copied().collect();
            let from = by_id[&last];
            for to in tracks {
                if visited.contains(&to.id) {
                    continue;
                }
                let s = score_transition(from, to, weights, goal, &ctx, played);
                let mut ids = beam.ids.clone();
                ids.push(to.id);
                candidates.push(Beam {
                    ids,
                    acc: beam.acc + s.total,
                });
            }
        }
        if candidates.is_empty() {
            break;
        }
        // Rank by average score so far so long and short beams compete fairly.
        candidates.sort_by(|a, b| {
            let aa = a.acc / (a.ids.len() - 1) as f64;
            let bb = b.acc / (b.ids.len() - 1) as f64;
            bb.partial_cmp(&aa).unwrap_or(Ordering::Equal)
        });
        candidates.truncate(BEAM_WIDTH);
        beams = candidates;
    }

    let best = beams.into_iter().max_by(|a, b| {
        let aa = if a.ids.len() > 1 {
            a.acc / (a.ids.len() - 1) as f64
        } else {
            0.0
        };
        let bb = if b.ids.len() > 1 {
            b.acc / (b.ids.len() - 1) as f64
        } else {
            0.0
        };
        aa.partial_cmp(&bb).unwrap_or(Ordering::Equal)
    })?;

    Some(materialize(&best.ids, &by_id, &ctx, weights, goal))
}

/// Rebuilds per-step scores and a rationale for a finished id sequence.
fn materialize(
    ids: &[i64],
    by_id: &HashMap<i64, &Track>,
    ctx: &GenreContext,
    weights: &TransitionWeights,
    goal: &MixGoal,
) -> RecommendedPath {
    let played = HashSet::new();
    let mut steps = Vec::new();
    for w in ids.windows(2) {
        let from = by_id[&w[0]];
        let to = by_id[&w[1]];
        steps.push(score_transition(from, to, weights, goal, ctx, &played));
    }
    let avg = average(&steps);
    let rationale = build_rationale(ids, by_id, goal, avg);
    RecommendedPath {
        track_ids: ids.to_vec(),
        steps,
        average_score: avg,
        rationale,
    }
}

fn build_rationale(ids: &[i64], by_id: &HashMap<i64, &Track>, goal: &MixGoal, avg: f64) -> String {
    let names: Vec<&str> = ids
        .iter()
        .filter_map(|id| by_id.get(id).map(|t| t.title.as_str()))
        .collect();
    let goal_txt = match goal {
        MixGoal::Neutral => "ausgewogener Flow",
        MixGoal::RaiseEnergy => "Energie steigern",
        MixGoal::LowerEnergy => "Energie reduzieren",
        MixGoal::KeepGenre => "Genre halten",
        MixGoal::SwitchGenre => "kontrollierter Genrewechsel",
        MixGoal::HarmonicPriority => "harmonischer Mix",
        MixGoal::SurprisingButPlausible => "überraschend, aber plausibel",
        MixGoal::TargetTrack(_) => "Weg zum Zieltrack",
        MixGoal::TargetGenre(_) => "Weg zum Zielgenre",
    };
    format!(
        "Ziel: {goal_txt}. Pfad: {} (Ø Übergangsqualität {:.0} %).",
        names.join(" → "),
        avg * 100.0
    )
}

/// Internal Dijkstra heap state ordered as a min-heap on `cost`.
struct State {
    cost: f64,
    node: i64,
}
impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost
    }
}
impl Eq for State {}
impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse so BinaryHeap (max-heap) behaves as a min-heap on cost.
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
            .then_with(|| self.node.cmp(&other.node))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::default_genres;
    use crate::sample_data::sample_tracks;

    #[test]
    fn shortest_path_reaches_target() {
        let tracks = sample_tracks();
        let genres = default_genres();
        let p = shortest_path(&tracks, &genres, 1, 15, &TransitionWeights::default())
            .expect("path exists in fully-connected sample set");
        assert_eq!(*p.track_ids.first().unwrap(), 1);
        assert_eq!(*p.track_ids.last().unwrap(), 15);
        assert_eq!(p.steps.len(), p.track_ids.len() - 1);
    }

    #[test]
    fn recommend_path_bounded_and_no_repeats() {
        let tracks = sample_tracks();
        let genres = default_genres();
        let p = recommend_path(
            &tracks,
            &genres,
            1,
            &MixGoal::RaiseEnergy,
            &TransitionWeights::default(),
            &HashSet::new(),
            5,
        )
        .unwrap();
        assert!(p.track_ids.len() <= 5);
        let unique: HashSet<_> = p.track_ids.iter().collect();
        assert_eq!(
            unique.len(),
            p.track_ids.len(),
            "no repeated tracks in a path"
        );
        assert_eq!(p.track_ids[0], 1);
    }

    #[test]
    fn raise_energy_tends_upward() {
        let tracks = sample_tracks();
        let genres = default_genres();
        let p = recommend_path(
            &tracks,
            &genres,
            10, // Morning Light, energy 45
            &MixGoal::RaiseEnergy,
            &TransitionWeights::default(),
            &HashSet::new(),
            4,
        )
        .unwrap();
        let by_id: HashMap<i64, &Track> = tracks.iter().map(|t| (t.id, t)).collect();
        let first = by_id[&p.track_ids[0]].energy.unwrap();
        let last = by_id[p.track_ids.last().unwrap()].energy.unwrap();
        assert!(
            last >= first,
            "energy should not drop with RaiseEnergy goal"
        );
    }

    #[test]
    fn target_track_goal_uses_shortest_path() {
        let tracks = sample_tracks();
        let genres = default_genres();
        let p = recommend_path(
            &tracks,
            &genres,
            1,
            &MixGoal::TargetTrack(9),
            &TransitionWeights::default(),
            &HashSet::new(),
            8,
        )
        .unwrap();
        assert_eq!(*p.track_ids.last().unwrap(), 9);
    }

    #[test]
    fn rationale_is_populated() {
        let tracks = sample_tracks();
        let genres = default_genres();
        let p = recommend_path(
            &tracks,
            &genres,
            1,
            &MixGoal::Neutral,
            &TransitionWeights::default(),
            &HashSet::new(),
            4,
        )
        .unwrap();
        assert!(p.rationale.contains('→') || p.track_ids.len() == 1);
    }
}
