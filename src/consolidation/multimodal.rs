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
#[path = "../../tests/unit/consolidation/multimodal/multimodal_test.rs"]
mod tests;
