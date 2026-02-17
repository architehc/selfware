//! Loading Phrases for LLM Response
//!
//! Witty phrases shown while waiting for the LLM to respond.
//! Rotated periodically to keep the user entertained.

use rand::prelude::IndexedRandom;

/// Witty loading phrases shown while waiting for LLM response
pub const LOADING_PHRASES: &[&str] = &[
    "Thinking deeply...",
    "Analyzing code patterns...",
    "Consulting the documentation...",
    "Tending the garden of ideas...",
    "Connecting the dots...",
    "Weighing the options...",
    "Searching for the right approach...",
    "Exploring possibilities...",
    "Crafting a thoughtful response...",
    "Reviewing the codebase...",
    "Processing your request...",
    "Considering edge cases...",
    "Building mental model...",
    "Reading between the lines...",
    "Synthesizing knowledge...",
    "Checking the map...",
    "Navigating the code...",
    "Forging a solution...",
    "Untangling dependencies...",
    "Tracing the execution path...",
    "Contemplating architecture...",
    "Polishing the approach...",
    "Aligning the stars...",
    "Gathering context...",
    "Calibrating response...",
    "Sifting through patterns...",
    "Computing possibilities...",
    "Brewing something good...",
    "Channeling best practices...",
    "Mining for insights...",
    "Piecing together the puzzle...",
    "Consulting the oracle...",
    "Pondering the question...",
    "Warming up the neurons...",
    "Dusting off the algorithms...",
    "Sharpening the tools...",
    "Loading wisdom modules...",
    "Parsing the intent...",
    "Optimizing the approach...",
    "Running thought experiments...",
    "Compiling ideas...",
    "Debugging my thoughts...",
    "Refactoring my response...",
    "Testing hypotheses...",
    "Benchmarking solutions...",
    "Profiling the problem space...",
    "Iterating on the design...",
    "Reviewing pull requests of the mind...",
    "Rebasing my understanding...",
    "Merging knowledge branches...",
    "Resolving cognitive conflicts...",
    "Deploying thought pipeline...",
    "Spinning up inference...",
    "Hydrating the context...",
    "Indexing relevant knowledge...",
    "Querying the knowledge graph...",
    "Traversing the syntax tree...",
    "Evaluating type constraints...",
    "Checking invariants...",
    "Verifying assumptions...",
    "Rust-ling up an answer...",
    "Borrowing some wisdom...",
    "Lifetime-checking the response...",
    "Unwrapping the solution...",
    "Pattern matching on the problem...",
    "Trait-implementing a response...",
    "Async-awaiting brilliance...",
    "Zero-cost abstracting...",
    "Cargo-building thoughts...",
    "Clippy-checking the approach...",
    "Formatting with rustfmt...",
    "Growing the solution organically...",
    "Composting old ideas...",
    "Pruning unnecessary complexity...",
    "Watering the seeds of thought...",
    "Harvesting insights...",
    "Cultivating understanding...",
    "Grafting ideas together...",
    "Letting ideas photosynthesize...",
    "Nurturing the code garden...",
    "Planting the right abstractions...",
    "Weeding out bugs...",
    "Cross-pollinating concepts...",
    "Composing the symphony...",
    "Tuning the frequencies...",
    "Harmonizing components...",
    "Orchestrating the solution...",
    "Finding the right rhythm...",
    "Improvising elegantly...",
    "Building the bridge...",
    "Laying the foundation...",
    "Raising the scaffolding...",
    "Painting the big picture...",
    "Sketching the blueprint...",
    "Measuring twice...",
    "Cutting once...",
    "Hammering out the details...",
    "Sanding the rough edges...",
    "Applying the finishing touches...",
    "Connecting the dots...",
];

/// Get a random loading phrase
pub fn random_phrase() -> &'static str {
    LOADING_PHRASES
        .choose(&mut rand::rng())
        .unwrap_or(&"Thinking...")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loading_phrases_not_empty() {
        assert!(!LOADING_PHRASES.is_empty());
    }

    #[test]
    fn test_loading_phrases_count() {
        assert!(LOADING_PHRASES.len() >= 100);
    }

    #[test]
    fn test_random_phrase_returns_valid() {
        let phrase = random_phrase();
        assert!(!phrase.is_empty());
        assert!(LOADING_PHRASES.contains(&phrase));
    }

    #[test]
    fn test_all_phrases_non_empty() {
        for phrase in LOADING_PHRASES {
            assert!(!phrase.is_empty());
        }
    }

    #[test]
    fn test_all_phrases_end_with_dots() {
        for phrase in LOADING_PHRASES {
            assert!(
                phrase.ends_with("...") || phrase.ends_with(".."),
                "Phrase '{}' should end with dots",
                phrase
            );
        }
    }
}
