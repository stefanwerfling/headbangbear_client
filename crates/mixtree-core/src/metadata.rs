//! Audio metadata extraction (via `lofty`) and content hashing.
//!
//! Extraction never fails the whole import: unreadable tags fall back to the
//! file stem for the title and leave other fields empty. The Phase-2 analysis
//! pipeline fills in BPM/key/energy later.

use crate::error::{CoreError, Result};
use std::path::Path;

/// Audio file extensions MixTree imports (spec §8). Lower-case, no dot.
pub const SUPPORTED_EXTENSIONS: &[&str] = &["mp3", "flac", "wav", "ogg", "oga", "m4a", "aac"];

/// Returns `true` if `path` has a supported audio extension.
pub fn is_supported_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Metadata read from an audio file's tags and properties.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtractedMetadata {
    /// Title tag, if present.
    pub title: Option<String>,
    /// Artist tag.
    pub artist: Option<String>,
    /// Album tag.
    pub album: Option<String>,
    /// Genre tag (free-text; mapped to the taxonomy elsewhere).
    pub genre: Option<String>,
    /// Release year.
    pub year: Option<i32>,
    /// BPM tag, if the file carries one (rare but possible).
    pub bpm: Option<f64>,
    /// Duration in milliseconds.
    pub duration_ms: Option<u64>,
    /// `true` if the file embeds cover art.
    pub has_cover_art: bool,
}

/// Reads tags and audio properties from `path`.
///
/// Returns [`CoreError::Metadata`] only when the file cannot be opened/probed
/// at all; missing individual tags are simply `None`.
pub fn extract(path: &Path) -> Result<ExtractedMetadata> {
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::prelude::{Accessor, ItemKey};

    let tagged = lofty::read_from_path(path).map_err(|e| CoreError::Metadata {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;

    let duration_ms = Some(tagged.properties().duration().as_millis() as u64);

    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    let mut meta = ExtractedMetadata {
        duration_ms,
        ..Default::default()
    };

    if let Some(tag) = tag {
        meta.title = tag.title().map(|c| c.to_string());
        meta.artist = tag.artist().map(|c| c.to_string());
        meta.album = tag.album().map(|c| c.to_string());
        meta.genre = tag.genre().map(|c| c.to_string());
        // `year` is not an Accessor method in lofty; read the Year item and take
        // the leading 4 digits (the value may be a full date string).
        meta.year = tag
            .get_string(ItemKey::Year)
            .and_then(|s| s.get(0..4).unwrap_or(s).trim().parse::<i32>().ok());
        meta.bpm = tag
            .get_string(ItemKey::IntegerBpm)
            .and_then(|s| s.trim().parse::<f64>().ok())
            .filter(|b| *b > 0.0);
        meta.has_cover_art = tag.picture_count() > 0;
    }

    Ok(meta)
}

/// Computes a cheap, stable content hash for duplicate detection and
/// moved-file resilience.
///
/// To stay fast on large libraries we hash the file size plus the first and
/// last 256 KiB rather than the whole file. This is ample to distinguish
/// distinct tracks while tolerating a file being moved to a new path.
pub fn content_hash(path: &Path) -> Result<String> {
    use std::io::{Read, Seek, SeekFrom};

    const CHUNK: u64 = 256 * 1024;
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();

    let mut hasher = blake3::Hasher::new();
    hasher.update(&len.to_le_bytes());

    let mut buf = vec![0u8; CHUNK as usize];
    // Head.
    let head = file.read(&mut buf)?;
    hasher.update(&buf[..head]);
    // Tail (only if the file is larger than a single chunk).
    if len > CHUNK {
        file.seek(SeekFrom::Start(len.saturating_sub(CHUNK)))?;
        let tail = file.read(&mut buf)?;
        hasher.update(&buf[..tail]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

/// Extracts the raw bytes of the first embedded cover-art picture, if any.
///
/// Returns the encoded image data (PNG/JPEG/…) exactly as stored in the tag; the
/// caller decodes it. `None` when the file has no picture or cannot be read.
pub fn cover_bytes(path: &Path) -> Option<Vec<u8>> {
    use lofty::file::TaggedFileExt;
    let tagged = lofty::read_from_path(path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    tag.pictures().first().map(|p| p.data().to_vec())
}

/// Derives a display title from a file path (used when no title tag exists).
pub fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    #[test]
    fn supported_extensions_detected() {
        assert!(is_supported_audio(&PathBuf::from("/a/b.mp3")));
        assert!(is_supported_audio(&PathBuf::from("/a/b.FLAC")));
        assert!(!is_supported_audio(&PathBuf::from("/a/b.txt")));
        assert!(!is_supported_audio(&PathBuf::from("/a/b")));
    }

    #[test]
    fn title_fallback_from_path() {
        assert_eq!(
            title_from_path(&PathBuf::from("/music/Great Track.mp3")),
            "Great Track"
        );
    }

    #[test]
    fn content_hash_is_stable_and_distinct() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        std::fs::File::create(&a)
            .unwrap()
            .write_all(b"hello world")
            .unwrap();
        std::fs::File::create(&b)
            .unwrap()
            .write_all(b"different!!")
            .unwrap();
        let ha = content_hash(&a).unwrap();
        let hb = content_hash(&b).unwrap();
        assert_eq!(ha, content_hash(&a).unwrap(), "hash is stable");
        assert_ne!(ha, hb, "different content → different hash");
    }

    #[test]
    fn cover_bytes_none_without_picture() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("no-cover.mp3");
        std::fs::File::create(&f)
            .unwrap()
            .write_all(b"not real audio")
            .unwrap();
        // Unreadable / no picture → None, never panics.
        assert!(cover_bytes(&f).is_none());
    }

    #[test]
    fn extract_on_non_audio_errors_gracefully() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("not-audio.mp3");
        std::fs::File::create(&f)
            .unwrap()
            .write_all(b"not really an mp3")
            .unwrap();
        // Should return a structured Metadata error, not panic.
        assert!(matches!(extract(&f), Err(CoreError::Metadata { .. })));
    }
}
