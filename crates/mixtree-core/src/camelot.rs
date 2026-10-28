//! Camelot wheel representation of musical keys and harmonic compatibility.
//!
//! The Camelot system maps the 24 major/minor keys onto a clock face of
//! 12 numbers with an `A` (minor) / `B` (major) suffix, e.g. `8A` = A minor,
//! `8B` = C major. Harmonic ("key") mixing works best between keys that are
//! close on the wheel. See [`CamelotKey::compatibility`] for the scoring model.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// A key on the Camelot wheel.
///
/// `number` is the position on the wheel (1..=12). `is_major` selects the
/// outer (`B`, major) or inner (`A`, minor) ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CamelotKey {
    /// Wheel position, 1..=12.
    pub number: u8,
    /// `true` for the major ring (`B`), `false` for the minor ring (`A`).
    pub is_major: bool,
}

impl CamelotKey {
    /// Creates a key, validating the wheel number is within 1..=12.
    pub fn new(number: u8, is_major: bool) -> Option<Self> {
        if (1..=12).contains(&number) {
            Some(Self { number, is_major })
        } else {
            None
        }
    }

    /// Harmonic compatibility with another key, normalized to `0.0..=1.0`.
    ///
    /// The model follows established harmonic-mixing practice:
    /// - identical key → `1.0`
    /// - relative major/minor (same number, other ring) → `0.9`
    /// - adjacent on the wheel, same ring (±1) → `0.85`
    /// - energy-boost mix (+1 and switch ring) → `0.7`
    /// - dominant / sub-dominant (±2 on same ring) → `0.5`
    /// - everything else → `0.2`
    pub fn compatibility(self, other: CamelotKey) -> f64 {
        if self == other {
            return 1.0;
        }
        let step = wheel_distance(self.number, other.number);
        match (self.is_major == other.is_major, step) {
            // Relative major/minor.
            (false, 0) => 0.9,
            // Adjacent, same ring.
            (true, 1) => 0.85,
            // Energy boost: one step and ring change.
            (false, 1) => 0.7,
            // Dominant / sub-dominant on the same ring.
            (true, 2) => 0.5,
            _ => 0.2,
        }
    }
}

/// Smallest number of steps between two wheel positions (wrapping at 12).
fn wheel_distance(a: u8, b: u8) -> u8 {
    let diff = (a as i16 - b as i16).unsigned_abs() as u8;
    diff.min(12 - diff)
}

impl fmt::Display for CamelotKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}",
            self.number,
            if self.is_major { 'B' } else { 'A' }
        )
    }
}

impl FromStr for CamelotKey {
    type Err = String;

    /// Parses strings like `"8A"`, `"12B"`, `"8a"`.
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let s = s.trim();
        let (digits, letter) = s.split_at(s.len().saturating_sub(1));
        let is_major = match letter.to_ascii_uppercase().as_str() {
            "B" => true,
            "A" => false,
            other => return Err(format!("invalid Camelot ring '{other}', expected A or B")),
        };
        let number: u8 = digits
            .parse()
            .map_err(|_| format!("invalid Camelot number '{digits}'"))?;
        CamelotKey::new(number, is_major)
            .ok_or_else(|| format!("Camelot number out of range: {number}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_display_roundtrip() {
        for s in ["1A", "8A", "12B", "6B"] {
            let k: CamelotKey = s.parse().unwrap();
            assert_eq!(k.to_string(), s);
        }
    }

    #[test]
    fn parse_is_case_insensitive() {
        assert_eq!("8a".parse::<CamelotKey>().unwrap(), "8A".parse().unwrap());
    }

    #[test]
    fn parse_rejects_bad_input() {
        assert!("13A".parse::<CamelotKey>().is_err());
        assert!("8C".parse::<CamelotKey>().is_err());
        assert!("A".parse::<CamelotKey>().is_err());
    }

    #[test]
    fn identical_key_is_perfect() {
        let k: CamelotKey = "8A".parse().unwrap();
        assert_eq!(k.compatibility(k), 1.0);
    }

    #[test]
    fn relative_major_minor_is_high() {
        let a: CamelotKey = "8A".parse().unwrap();
        let b: CamelotKey = "8B".parse().unwrap();
        assert_eq!(a.compatibility(b), 0.9);
    }

    #[test]
    fn adjacent_same_ring_is_high() {
        let a: CamelotKey = "8A".parse().unwrap();
        let b: CamelotKey = "9A".parse().unwrap();
        assert_eq!(a.compatibility(b), 0.85);
        // Wrap-around: 12 and 1 are adjacent.
        let x: CamelotKey = "12B".parse().unwrap();
        let y: CamelotKey = "1B".parse().unwrap();
        assert_eq!(x.compatibility(y), 0.85);
    }

    #[test]
    fn distant_keys_are_low() {
        let a: CamelotKey = "8A".parse().unwrap();
        let b: CamelotKey = "2A".parse().unwrap();
        assert!(a.compatibility(b) <= 0.2);
    }

    #[test]
    fn compatibility_is_symmetric() {
        let a: CamelotKey = "4A".parse().unwrap();
        let b: CamelotKey = "5B".parse().unwrap();
        assert_eq!(a.compatibility(b), b.compatibility(a));
    }
}
