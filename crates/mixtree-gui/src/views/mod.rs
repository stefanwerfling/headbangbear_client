//! egui view modules, one per region of the MixTree layout.

pub mod decks;
pub mod footer;
pub mod graph;
pub mod header;
pub mod library;
pub mod nextbest;
pub mod playlist;
pub mod settings;
pub mod sidebar;
pub mod tree_layout;

use mixtree_core::camelot::CamelotKey;

/// Formats a Camelot key for display, e.g. `8A`.
pub fn key_str(k: Option<CamelotKey>) -> String {
    k.map(|k| k.to_string()).unwrap_or_else(|| "—".into())
}

/// Capitalizes a genre id for display, e.g. `deep-house` → `Deep house`.
pub fn cap(s: Option<&str>) -> String {
    match s {
        Some(s) if !s.is_empty() => {
            let s = s.replace('-', " ");
            let mut c = s.chars();
            match c.next() {
                Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                None => s,
            }
        }
        _ => "—".into(),
    }
}
