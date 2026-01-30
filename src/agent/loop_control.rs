#[derive(Debug, Clone)]
pub enum AgentState {
    Planning,
    Executing { step: usize },
    ErrorRecovery { error: String },
    #[allow(dead_code)]
    Completed,
    Failed { reason: String },
}

pub struct AgentLoop {
    state: AgentState,
    max_iterations: usize,
    current_step: usize,
    iteration: usize,
}

impl AgentLoop {
    pub fn new(max_iterations: usize) -> Self {
        Self {
            state: AgentState::Planning,
            max_iterations,
            current_step: 0,
            iteration: 0,
        }
    }

    pub fn next_state(&mut self) -> Option<AgentState> {
        if self.iteration >= self.max_iterations {
            return Some(AgentState::Failed { 
                reason: "Max iterations exceeded".to_string() 
            });
        }
        self.iteration += 1;
        Some(self.state.clone())
    }

    pub fn set_state(&mut self, state: AgentState) {
        self.state = state;
    }

    pub fn increment_step(&mut self) {
        self.current_step += 1;
        self.state = AgentState::Executing { step: self.current_step };
    }

    pub fn current_step(&self) -> usize {
        self.current_step
    }
}
