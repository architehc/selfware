//! Multimodal references — beyond-text features for consolidated memories.
//!
//! Captures information that doesn't appear in text tokens:
//! - Visual snapshots (screenshots with spatial descriptions)
//! - Interaction traces (click paths, navigation sequences)
//! - Spatial layouts (element positions, visual hierarchy)
//! - Embedding cross-references (links to vector store entries)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Reference to a non-text feature attached to a consolidated memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MultimodalRef {
    /// Reference to a captured screenshot.
    Screenshot {
        /// Path to the screenshot file.
        path: PathBuf,
        /// When the screenshot was captured.
        timestamp: DateTime<Utc>,
        /// LLM-generated description of what the screenshot shows.
        description: String,
        /// Image dimensions (width, height).
        dimensions: (u32, u32),
    },

    /// Reference to a browser interaction trace.
    InteractionTrace {
        /// Unique trace identifier.
        trace_id: String,
        /// Number of actions in the trace.
        action_count: usize,
        /// LLM-generated summary of the interaction sequence.
        summary: String,
        /// Duration of the interaction in milliseconds.
        duration_ms: u64,
    },

    /// Spatial layout description — positions and relationships of visual elements.
    SpatialLayout {
        /// LLM-generated description of the spatial arrangement.
        description: String,
        /// Element positions as (label, x%, y%) relative to viewport.
        element_positions: Vec<(String, f64, f64)>,
        /// Visual hierarchy relationships (parent -> children).
        hierarchy: Vec<(String, Vec<String>)>,
    },

    /// Cross-reference to a vector store embedding.
    EmbeddingRef {
        /// Collection name in the vector store.
        collection: String,
        /// Chunk ID within the collection.
        chunk_id: String,
        /// Similarity score when this reference was created.
        similarity: f32,
    },

    /// Audio/temporal pattern (e.g., command execution timing).
    TemporalPattern {
        /// Description of the temporal pattern.
        description: String,
        /// Timestamps of events in the pattern.
        event_times: Vec<DateTime<Utc>>,
        /// Intervals between events in milliseconds.
        intervals_ms: Vec<u64>,
    },
}

impl MultimodalRef {
    /// Get a human-readable type name for this reference.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Screenshot { .. } => "screenshot",
            Self::InteractionTrace { .. } => "interaction_trace",
            Self::SpatialLayout { .. } => "spatial_layout",
            Self::EmbeddingRef { .. } => "embedding_ref",
            Self::TemporalPattern { .. } => "temporal_pattern",
        }
    }

    /// Get the description/summary for this reference.
    pub fn description(&self) -> &str {
        match self {
            Self::Screenshot { description, .. } => description,
            Self::InteractionTrace { summary, .. } => summary,
            Self::SpatialLayout { description, .. } => description,
            Self::EmbeddingRef { collection, .. } => collection,
            Self::TemporalPattern { description, .. } => description,
        }
    }

    /// Check if this reference points to a file that should exist on disk.
    pub fn has_file_dependency(&self) -> bool {
        matches!(self, Self::Screenshot { .. })
    }

    /// Get the file path if this reference has one.
    pub fn file_path(&self) -> Option<&PathBuf> {
        match self {
            Self::Screenshot { path, .. } => Some(path),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screenshot_ref() {
        let r = MultimodalRef::Screenshot {
            path: PathBuf::from("/tmp/ss.png"),
            timestamp: Utc::now(),
            description: "Search results page".into(),
            dimensions: (1920, 1080),
        };
        assert_eq!(r.type_name(), "screenshot");
        assert!(r.has_file_dependency());
        assert_eq!(r.file_path().unwrap().to_str().unwrap(), "/tmp/ss.png");
    }

    #[test]
    fn test_interaction_trace_ref() {
        let r = MultimodalRef::InteractionTrace {
            trace_id: "trace-001".into(),
            action_count: 5,
            summary: "Navigated to search page".into(),
            duration_ms: 3000,
        };
        assert_eq!(r.type_name(), "interaction_trace");
        assert!(!r.has_file_dependency());
        assert_eq!(r.description(), "Navigated to search page");
    }

    #[test]
    fn test_spatial_layout_ref() {
        let r = MultimodalRef::SpatialLayout {
            description: "Dashboard layout".into(),
            element_positions: vec![("header".into(), 50.0, 5.0), ("sidebar".into(), 10.0, 50.0)],
            hierarchy: vec![("main".into(), vec!["header".into(), "sidebar".into()])],
        };
        assert_eq!(r.type_name(), "spatial_layout");
        assert!(!r.has_file_dependency());
    }

    #[test]
    fn test_serde_roundtrip() {
        let refs = vec![
            MultimodalRef::Screenshot {
                path: PathBuf::from("/tmp/ss.png"),
                timestamp: Utc::now(),
                description: "test".into(),
                dimensions: (800, 600),
            },
            MultimodalRef::EmbeddingRef {
                collection: "consolidated".into(),
                chunk_id: "chunk-42".into(),
                similarity: 0.95,
            },
            MultimodalRef::TemporalPattern {
                description: "command burst".into(),
                event_times: vec![Utc::now()],
                intervals_ms: vec![],
            },
        ];

        let json = serde_json::to_string(&refs).unwrap();
        let parsed: Vec<MultimodalRef> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].type_name(), "screenshot");
        assert_eq!(parsed[1].type_name(), "embedding_ref");
        assert_eq!(parsed[2].type_name(), "temporal_pattern");
    }
}
