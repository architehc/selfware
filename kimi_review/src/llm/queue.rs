//! Request queue for LLM inference

use super::{SamplingParams, TokenOutput};
use crate::error::LLMError;
use crate::Priority;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Inference request queue
pub struct InferenceQueue {
    high_priority: VecDeque<InferenceRequest>,
    normal_priority: VecDeque<InferenceRequest>,
    low_priority: VecDeque<InferenceRequest>,
    in_progress: std::collections::HashMap<String, InProgressRequest>,
    max_concurrent: usize,
}

/// Inference request
#[derive(Debug, Clone)]
pub struct InferenceRequest {
    pub id: String,
    pub prompt: String,
    pub params: SamplingParams,
    pub priority: Priority,
    pub deadline: Option<Instant>,
    pub estimated_tokens: usize,
    pub checkpoint_on_completion: bool,
    pub submitted_at: Instant,
}

/// In-progress request
#[derive(Debug, Clone)]
pub struct InProgressRequest {
    pub request: InferenceRequest,
    pub started_at: Instant,
}

impl InferenceQueue {
    /// Create a new inference queue
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            high_priority: VecDeque::new(),
            normal_priority: VecDeque::new(),
            low_priority: VecDeque::new(),
            in_progress: std::collections::HashMap::new(),
            max_concurrent,
        }
    }
    
    /// Enqueue a request
    pub fn enqueue(&mut self, request: InferenceRequest) {
        match request.priority {
            Priority::Critical | Priority::High => {
                self.high_priority.push_back(request);
            }
            Priority::Normal => {
                self.normal_priority.push_back(request);
            }
            Priority::Low | Priority::Background => {
                self.low_priority.push_back(request);
            }
        }
    }
    
    /// Get the next request to process
    pub fn next(&mut self) -> Option<InferenceRequest> {
        // Check deadlines first
        for queue in [&mut self.high_priority, &mut self.normal_priority] {
            if let Some(pos) = queue.iter().position(|r| {
                r.deadline.map(|d| d < Instant::now()).unwrap_or(false)
            }) {
                return queue.remove(pos);
            }
        }
        
        // Normal priority ordering
        self.high_priority.pop_front()
            .or_else(|| self.normal_priority.pop_front())
            .or_else(|| self.low_priority.pop_front())
    }
    
    /// Mark request as in-progress
    pub fn start_request(&mut self, request: InferenceRequest) {
        let id = request.id.clone();
        self.in_progress.insert(id, InProgressRequest {
            request,
            started_at: Instant::now(),
        });
    }
    
    /// Mark request as complete
    pub fn complete_request(&mut self, request_id: &str) -> Option<InProgressRequest> {
        self.in_progress.remove(request_id)
    }
    
    /// Get number of in-progress requests
    pub fn in_progress_count(&self) -> usize {
        self.in_progress.len()
    }
    
    /// Check if can accept more requests
    pub fn can_accept(&self) -> bool {
        self.in_progress.len() < self.max_concurrent
    }
    
    /// Get queue lengths
    pub fn queue_lengths(&self) -> QueueLengths {
        QueueLengths {
            high: self.high_priority.len(),
            normal: self.normal_priority.len(),
            low: self.low_priority.len(),
            in_progress: self.in_progress.len(),
        }
    }
    
    /// Preempt low-priority requests for critical ones
    pub fn preempt_if_needed(&mut self, critical_request: &InferenceRequest) -> Option<InProgressRequest> {
        if self.in_progress.len() < self.max_concurrent {
            return None;
        }
        
        // Find lowest priority in-progress request
        if let Some((id, _)) = self.in_progress
            .iter()
            .min_by_key(|(_, r)| r.request.priority)
        {
            if self.in_progress[id].request.priority > critical_request.priority {
                return self.in_progress.remove(id);
            }
        }
        
        None
    }
    
    /// Clear all queues
    pub fn clear(&mut self) {
        self.high_priority.clear();
        self.normal_priority.clear();
        self.low_priority.clear();
        self.in_progress.clear();
    }
}

/// Queue lengths
#[derive(Debug, Clone, Copy, Default)]
pub struct QueueLengths {
    pub high: usize,
    pub normal: usize,
    pub low: usize,
    pub in_progress: usize,
}

impl InferenceRequest {
    /// Create a new inference request
    pub fn new(prompt: String, params: SamplingParams, priority: Priority) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            prompt,
            params,
            priority,
            deadline: None,
            estimated_tokens: 0,
            checkpoint_on_completion: false,
            submitted_at: Instant::now(),
        }
    }
    
    /// Set deadline
    pub fn with_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }
    
    /// Set estimated tokens
    pub fn with_estimated_tokens(mut self, tokens: usize) -> Self {
        self.estimated_tokens = tokens;
        self
    }
    
    /// Enable checkpoint on completion
    pub fn with_checkpoint(mut self) -> Self {
        self.checkpoint_on_completion = true;
        self
    }
    
    /// Get wait time
    pub fn wait_time(&self) -> Duration {
        self.submitted_at.elapsed()
    }
}
