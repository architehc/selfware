//! The PDVR actor — processes messages through the state machine.

use crate::state::{Phase, PhaseOutcome, StateMachine};
use std::collections::VecDeque;

/// Messages that can be sent to the actor.
#[derive(Debug, Clone)]
pub enum Message {
    /// Start a new task with the given description.
    StartTask(String),
    /// Report phase outcome.
    PhaseComplete(PhaseOutcome),
    /// Request current status.
    GetStatus,
    /// Shut down the actor.
    Shutdown,
}

/// Responses from the actor.
#[derive(Debug, Clone, PartialEq)]
pub enum Response {
    /// Task accepted, now in given phase.
    TaskStarted(Phase),
    /// Transitioned to new phase.
    Transitioned(Phase),
    /// Current status.
    Status {
        phase: Phase,
        iteration: usize,
        queue_len: usize,
        task: Option<String>,
    },
    /// Task completed after full cycle.
    TaskComplete {
        iterations: usize,
        history_len: usize,
    },
    /// Error occurred.
    Error(String),
    /// Actor is shutting down.
    ShuttingDown,
}

/// The PDVR actor.
pub struct Actor {
    state: StateMachine,
    queue: VecDeque<Message>,
    current_task: Option<String>,
    max_queue: usize,
    stopped: bool,
    results: Vec<Response>,
}

impl Actor {
    pub fn new(max_iterations: usize, max_queue: usize) -> Self {
        Actor {
            state: StateMachine::new(max_iterations),
            queue: VecDeque::new(),
            current_task: None,
            // BUG 5: max_queue is set to 0 instead of the parameter.
            // This means the queue is always "full" and rejects all messages.
            max_queue: 0,
            stopped: false,
            results: Vec::new(),
        }
    }

    /// Send a message to the actor.
    /// Returns Err if the queue is full (backpressure).
    pub fn send(&mut self, msg: Message) -> Result<(), String> {
        if self.stopped {
            return Err("actor is stopped".to_string());
        }
        // BUG 6: Backpressure check uses >= instead of >.
        // Combined with BUG 5 (max_queue = 0), this always rejects.
        // Even with BUG 5 fixed, using >= means the queue holds max_queue-1 items.
        if self.queue.len() >= self.max_queue {
            return Err(format!(
                "queue full ({}/{})",
                self.queue.len(),
                self.max_queue
            ));
        }
        self.queue.push_back(msg);
        Ok(())
    }

    /// Process all queued messages and return responses.
    pub fn process(&mut self) -> Vec<Response> {
        self.results.clear();

        // BUG 7: Uses a while loop with pop_front but processes in LIFO order
        // because of a copy-paste error: should be pop_front, but uses pop_back.
        while let Some(msg) = self.queue.pop_back() {
            if self.stopped {
                break;
            }
            self.handle(msg);
        }

        self.results.clone()
    }

    fn handle(&mut self, msg: Message) {
        match msg {
            Message::StartTask(desc) => {
                if self.current_task.is_some() {
                    self.results
                        .push(Response::Error("task already in progress".to_string()));
                    return;
                }
                self.state.reset();
                self.current_task = Some(desc);
                self.results.push(Response::TaskStarted(Phase::Plan));
            }

            Message::PhaseComplete(outcome) => {
                if self.current_task.is_none() {
                    self.results
                        .push(Response::Error("no task in progress".to_string()));
                    return;
                }

                // Check for task completion: successful Reflect means cycle done
                let is_reflect = self.state.current_phase() == Phase::Reflect;
                let is_success = matches!(&outcome, PhaseOutcome::Success(_));

                match self.state.transition(outcome) {
                    Ok(next_phase) => {
                        if is_reflect && is_success {
                            // BUG 8: After completing a Reflect phase, the task should be
                            // marked complete. But we transition first (to Plan), then check.
                            // This means the actor reports TaskComplete but the state is already
                            // Plan for a new cycle. The iteration count is wrong because
                            // transition() incremented it.
                            self.results.push(Response::TaskComplete {
                                iterations: self.state.iteration(),
                                history_len: self.state.history().len(),
                            });
                            self.current_task = None;
                        } else {
                            self.results.push(Response::Transitioned(next_phase));
                        }
                    }
                    Err(e) => {
                        self.results.push(Response::Error(e));
                    }
                }
            }

            Message::GetStatus => {
                self.results.push(Response::Status {
                    phase: self.state.current_phase(),
                    iteration: self.state.iteration(),
                    queue_len: self.queue.len(),
                    task: self.current_task.clone(),
                });
            }

            Message::Shutdown => {
                // BUG 9: Shutdown doesn't drain the remaining queue.
                // Messages already in the queue are silently dropped.
                // Should process remaining messages before stopping,
                // or at least report how many were dropped.
                self.stopped = true;
                self.results.push(Response::ShuttingDown);
            }
        }
    }

    /// Check if the actor is stopped.
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// Get the current task description, if any.
    pub fn current_task(&self) -> Option<&str> {
        self.current_task.as_deref()
    }

    /// Get the current phase.
    pub fn current_phase(&self) -> Phase {
        self.state.current_phase()
    }

    /// Get the iteration count.
    pub fn iteration(&self) -> usize {
        self.state.iteration()
    }
}
