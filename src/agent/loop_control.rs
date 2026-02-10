#[derive(Debug, Clone)]
pub enum AgentState {
    Planning,
    Executing {
        step: usize,
    },
    ErrorRecovery {
        error: String,
    },
    Completed,
    Failed {
        reason: String,
    },
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
                reason: "Max iterations exceeded".to_string(),
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
        self.state = AgentState::Executing {
            step: self.current_step,
        };
    }

    pub fn current_step(&self) -> usize {
        self.current_step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_loop_new() {
        let loop_ctrl = AgentLoop::new(100);
        assert_eq!(loop_ctrl.max_iterations, 100);
        assert_eq!(loop_ctrl.current_step, 0);
        assert_eq!(loop_ctrl.iteration, 0);
    }

    #[test]
    fn test_agent_loop_initial_state_is_planning() {
        let mut loop_ctrl = AgentLoop::new(100);
        let state = loop_ctrl.next_state();
        assert!(matches!(state, Some(AgentState::Planning)));
    }

    #[test]
    fn test_agent_loop_set_state() {
        let mut loop_ctrl = AgentLoop::new(100);
        loop_ctrl.set_state(AgentState::Executing { step: 0 });
        let state = loop_ctrl.next_state();
        assert!(matches!(state, Some(AgentState::Executing { step: 0 })));
    }

    #[test]
    fn test_agent_loop_increment_step() {
        let mut loop_ctrl = AgentLoop::new(100);
        assert_eq!(loop_ctrl.current_step(), 0);
        loop_ctrl.increment_step();
        assert_eq!(loop_ctrl.current_step(), 1);
        loop_ctrl.increment_step();
        assert_eq!(loop_ctrl.current_step(), 2);
    }

    #[test]
    fn test_agent_loop_max_iterations_exceeded() {
        let mut loop_ctrl = AgentLoop::new(3);

        // First 3 iterations should work
        assert!(loop_ctrl.next_state().is_some());
        assert!(loop_ctrl.next_state().is_some());
        assert!(loop_ctrl.next_state().is_some());

        // 4th should fail
        let state = loop_ctrl.next_state();
        assert!(
            matches!(state, Some(AgentState::Failed { reason }) if reason == "Max iterations exceeded")
        );
    }

    #[test]
    fn test_agent_state_error_recovery() {
        let mut loop_ctrl = AgentLoop::new(100);
        loop_ctrl.set_state(AgentState::ErrorRecovery {
            error: "Test error".to_string(),
        });

        let state = loop_ctrl.next_state();
        match state {
            Some(AgentState::ErrorRecovery { error }) => {
                assert_eq!(error, "Test error");
            }
            _ => panic!("Expected ErrorRecovery state"),
        }
    }

    #[test]
    fn test_agent_state_failed() {
        let mut loop_ctrl = AgentLoop::new(100);
        loop_ctrl.set_state(AgentState::Failed {
            reason: "Something went wrong".to_string(),
        });

        let state = loop_ctrl.next_state();
        match state {
            Some(AgentState::Failed { reason }) => {
                assert_eq!(reason, "Something went wrong");
            }
            _ => panic!("Expected Failed state"),
        }
    }

    #[test]
    fn test_executing_state_tracks_step() {
        let state = AgentState::Executing { step: 5 };
        match state {
            AgentState::Executing { step } => assert_eq!(step, 5),
            _ => panic!("Expected Executing state"),
        }
    }

    #[test]
    fn test_increment_step_updates_state() {
        let mut loop_ctrl = AgentLoop::new(100);
        loop_ctrl.increment_step();

        // After increment, state should be Executing with current step
        match &loop_ctrl.state {
            AgentState::Executing { step } => assert_eq!(*step, 1),
            _ => panic!("Expected Executing state after increment"),
        }
    }
}
