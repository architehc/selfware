use actor_pdvr::actor::{Actor, Message, Response};
use actor_pdvr::state::PhaseOutcome;

fn main() {
    let mut actor = Actor::new(10, 32);

    actor.send(Message::StartTask("example task".to_string())).unwrap();
    let responses = actor.process();
    for r in &responses {
        println!("{:?}", r);
    }

    // Run through one full PDVR cycle
    for phase_name in &["Plan", "Do", "Verify", "Reflect"] {
        actor
            .send(Message::PhaseComplete(PhaseOutcome::Success(
                format!("{} done", phase_name),
            )))
            .unwrap();
        let responses = actor.process();
        for r in &responses {
            println!("{:?}", r);
        }
    }
}
