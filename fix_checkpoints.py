import re

with open('src/session/checkpoint.rs', 'r') as f:
    content = f.read()

delta_structs = """
/// Represents the delta/diff between two checkpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointDelta {
    pub task_id: String,
    pub base_version: u32,
    pub target_version: u32,
    
    // Updates
    pub updated_at: DateTime<Utc>,
    pub status: Option<TaskStatus>,
    pub current_step: Option<usize>,
    pub current_iteration: Option<usize>,
    
    // Context additions (we only append messages in the context window)
    pub new_messages: Vec<Message>,
    pub new_memory_entries: Vec<MemoryEntry>,
    pub new_tool_calls: Vec<ToolCallLog>,
    pub new_errors: Vec<ErrorLog>,
    
    pub updated_tokens: Option<usize>,
    pub git_checkpoint: Option<GitCheckpointInfo>,
}

impl TaskCheckpoint {
    /// Computes a differential payload to reduce disk IO during saves
    pub fn compute_delta(&self, base: &TaskCheckpoint) -> Option<CheckpointDelta> {
        if self.task_id != base.task_id {
            return None;
        }

        Some(CheckpointDelta {
            task_id: self.task_id.clone(),
            base_version: base.version,
            target_version: self.version,
            updated_at: self.updated_at,
            status: if self.status != base.status { Some(self.status.clone()) } else { None },
            current_step: if self.current_step != base.current_step { Some(self.current_step) } else { None },
            current_iteration: if self.current_iteration != base.current_iteration { Some(self.current_iteration) } else { None },
            
            // Only capture the new elements added to vectors
            new_messages: if self.messages.len() > base.messages.len() {
                self.messages[base.messages.len()..].to_vec()
            } else {
                Vec::new()
            },
            new_memory_entries: if self.memory_entries.len() > base.memory_entries.len() {
                self.memory_entries[base.memory_entries.len()..].to_vec()
            } else {
                Vec::new()
            },
            new_tool_calls: if self.tool_calls.len() > base.tool_calls.len() {
                self.tool_calls[base.tool_calls.len()..].to_vec()
            } else {
                Vec::new()
            },
            new_errors: if self.errors.len() > base.errors.len() {
                self.errors[base.errors.len()..].to_vec()
            } else {
                Vec::new()
            },
            
            updated_tokens: if self.estimated_tokens != base.estimated_tokens { Some(self.estimated_tokens) } else { None },
            git_checkpoint: if self.git_checkpoint != base.git_checkpoint { self.git_checkpoint.clone() } else { None },
        })
    }

    /// Applies a delta to an existing checkpoint to hydrate the full state
    pub fn apply_delta(&mut self, delta: &CheckpointDelta) -> Result<()> {
        if self.task_id != delta.task_id {
            return Err(anyhow::anyhow!("Delta task ID mismatch"));
        }
        
        self.version = delta.target_version;
        self.updated_at = delta.updated_at;
        
        if let Some(ref status) = delta.status {
            self.status = status.clone();
        }
        if let Some(step) = delta.current_step {
            self.current_step = step;
        }
        if let Some(iter) = delta.current_iteration {
            self.current_iteration = iter;
        }
        
        self.messages.extend(delta.new_messages.clone());
        self.memory_entries.extend(delta.new_memory_entries.clone());
        self.tool_calls.extend(delta.new_tool_calls.clone());
        self.errors.extend(delta.new_errors.clone());
        
        if let Some(tokens) = delta.updated_tokens {
            self.estimated_tokens = tokens;
        }
        if let Some(ref git) = delta.git_checkpoint {
            self.git_checkpoint = Some(git.clone());
        }
        
        Ok(())
    }
}
"""

content = content.replace('pub struct TaskCheckpoint {', delta_structs + '\npub struct TaskCheckpoint {')

with open('src/session/checkpoint.rs', 'w') as f:
    f.write(content)
