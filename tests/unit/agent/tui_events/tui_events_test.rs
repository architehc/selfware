use super::*;

#[tokio::test]
async fn broadcast_emitter_fans_out() {
    let (tx, _) = tokio::sync::broadcast::channel::<AgentEvent>(16);
    let emitter = BroadcastEmitter::new(tx.clone());

    let mut rx1 = tx.subscribe();
    let mut rx2 = tx.subscribe();

    emitter.emit(AgentEvent::Started);

    let r1 = tokio::time::timeout(std::time::Duration::from_secs(2), rx1.recv())
        .await
        .unwrap()
        .unwrap();
    let r2 = tokio::time::timeout(std::time::Duration::from_secs(2), rx2.recv())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(r1, AgentEvent::Started);
    assert_eq!(r2, AgentEvent::Started);
}

#[tokio::test]
async fn broadcast_emitter_no_receivers_is_ok() {
    // Emitting with zero receivers should not panic — the Result is
    // silently dropped.
    let (tx, _) = tokio::sync::broadcast::channel::<AgentEvent>(16);
    let emitter = BroadcastEmitter::new(tx);
    emitter.emit(AgentEvent::Started);
}
