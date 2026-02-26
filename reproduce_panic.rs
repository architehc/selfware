use std::sync::{Arc, RwLock};
use selfware::orchestration::swarm::{MemoryEntry, Swarm};
use selfware::ui::tui::swarm_state::{SwarmUiState, MemoryEntryView};

#[test]
fn test_utf8_truncation_panic() {
    // A string with a multi-byte character at the truncation boundary
    // "A" is 1 byte, "🦀" is 4 bytes. 
    // If we have 49 "A"s and then a "🦀", the 50th byte is in the middle of "🦀".
    let mut value = "A".repeat(49);
    value.push('🦀');
    
    let entry = MemoryEntry {
        key: "test".to_string(),
        value,
        created_by: "agent1".to_string(),
        created_at: 0,
        modified_by: None,
        modified_at: None,
        access_count: 0,
        tags: vec![],
    };

    // This should NOT panic
    let view = MemoryEntryView::from_entry(&entry);
    println!("Preview: {}", view.value_preview);
}

fn main() {
    test_utf8_truncation_panic();
}
