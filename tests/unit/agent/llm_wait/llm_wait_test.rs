use super::*;
use crate::agent::progress::render_event_kv;

#[derive(Debug)]
enum Chunk {
    Reasoning(&'static str),
    Content(&'static str),
}

/// Mirror of the `chat_streaming` receive loop: consume chunks, and on every
/// heartbeat classify the phase and record `(elapsed_secs, phase, tokens, source)`.
async fn drive(
    mut rx: mpsc::Receiver<Chunk>,
    mut ticker: LlmWaitTicker,
) -> Vec<(u64, String, usize, String)> {
    let mut content = String::new();
    let mut reasoning = String::new();
    let mut ticks = Vec::new();
    loop {
        match recv_or_tick(&mut rx, &ticker).await {
            RecvOrTick::Item(Some(Chunk::Reasoning(t))) => reasoning.push_str(t),
            RecvOrTick::Item(Some(Chunk::Content(t))) => content.push_str(t),
            RecvOrTick::Item(None) => break,
            RecvOrTick::Tick => {
                let phase = LlmWaitPhase::classify(&content, &reasoning, 0, false);
                let (n, src) = tokens_so_far(None, &content, &reasoning);
                match ticker.fire(phase, n, src) {
                    ProgressEvent::LlmWaiting {
                        elapsed_secs,
                        phase,
                        tokens_so_far,
                        tokens_source,
                    } => ticks.push((elapsed_secs, phase, tokens_so_far, tokens_source)),
                    other => panic!("unexpected event {other:?}"),
                }
            }
        }
    }
    ticks
}

#[tokio::test(start_paused = true)]
async fn slow_stream_emits_heartbeat_every_15s_with_phase_progression() {
    let (tx, rx) = mpsc::channel(8);
    let ticker = LlmWaitTicker::start();
    tokio::spawn(async move {
        // 40 s of silent prefill, reasoning until 70 s, then the answer.
        tokio::time::sleep(Duration::from_secs(40)).await;
        tx.send(Chunk::Reasoning("let me think about this carefully"))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(30)).await;
        tx.send(Chunk::Content("The answer is 42.")).await.unwrap();
        tokio::time::sleep(Duration::from_secs(10)).await;
        // dropping tx closes the stream at 80 s
    });
    let ticks = drive(rx, ticker).await;
    let summary: Vec<(u64, &str)> = ticks.iter().map(|t| (t.0, t.1.as_str())).collect();
    assert_eq!(
        summary,
        vec![
            (15, "prefill"),
            (30, "prefill"),
            (45, "reasoning"),
            (60, "reasoning"),
            (75, "streaming"),
        ],
        "one heartbeat per 15 s, phase follows what has streamed"
    );
    assert_eq!(ticks[0].2, 0, "no tokens during prefill");
    assert!(ticks[2].2 > 0, "reasoning tokens are counted");
    assert!(ticks[4].2 > ticks[2].2, "token count grows");
    assert!(ticks.iter().all(|t| t.3 == "estimate"));
}

#[tokio::test(start_paused = true)]
async fn fast_call_emits_no_heartbeat() {
    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        tx.send(Chunk::Content("quick")).await.unwrap();
    });
    assert!(drive(rx, LlmWaitTicker::start()).await.is_empty());
}

#[tokio::test(start_paused = true)]
async fn continuous_stream_does_not_starve_the_heartbeat() {
    // A chunk every 100 ms for 40 s: the receiver is always ready, yet the
    // heartbeat must still fire at 15 s and 30 s.
    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(async move {
        for _ in 0..400 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if tx.send(Chunk::Reasoning("tok ")).await.is_err() {
                return;
            }
        }
    });
    let ticks = drive(rx, LlmWaitTicker::start()).await;
    let secs: Vec<u64> = ticks.iter().map(|t| t.0).collect();
    assert_eq!(secs, vec![15, 30]);
    assert!(ticks.iter().all(|t| t.1 == "reasoning"));
}

#[tokio::test(start_paused = true)]
async fn missed_ticks_are_skipped_not_replayed() {
    let mut ticker = LlmWaitTicker::start();
    tokio::time::advance(Duration::from_secs(47)).await;
    let ev = ticker.fire(LlmWaitPhase::Prefill, 0, LlmWaitTokenSource::Estimate);
    assert!(matches!(
        ev,
        ProgressEvent::LlmWaiting {
            elapsed_secs: 47,
            ..
        }
    ));
    // Next due at 60 s, not 30 s (no burst of catch-up ticks).
    assert_eq!(
        ticker.next_due() - tokio::time::Instant::now(),
        Duration::from_secs(13)
    );
}

#[tokio::test(start_paused = true)]
async fn nonstreaming_call_reports_awaiting_response_ticks() {
    let mut events = Vec::new();
    let out = await_with_ticks(
        async {
            tokio::time::sleep(Duration::from_secs(50)).await;
            7
        },
        LlmWaitTicker::start(),
        |ev| events.push(ev),
    )
    .await;
    assert_eq!(out, 7);
    let rendered: Vec<String> = events.iter().map(render_event_kv).collect();
    assert_eq!(
        rendered,
        vec![
            "kind=llm_waiting elapsed=15s phase=awaiting_response tokens_so_far=0 tokens_source=none",
            "kind=llm_waiting elapsed=30s phase=awaiting_response tokens_so_far=0 tokens_source=none",
            "kind=llm_waiting elapsed=45s phase=awaiting_response tokens_so_far=0 tokens_source=none",
        ]
    );
}

#[test]
fn reported_usage_wins_over_estimate() {
    assert_eq!(
        tokens_so_far(Some(812), "abc", "def"),
        (812, LlmWaitTokenSource::Usage)
    );
    assert_eq!(
        tokens_so_far(None, "", ""),
        (0, LlmWaitTokenSource::Estimate)
    );
}

#[test]
fn classify_phases() {
    assert_eq!(
        LlmWaitPhase::classify("", "", 0, false),
        LlmWaitPhase::Prefill
    );
    assert_eq!(
        LlmWaitPhase::classify("", "hmm", 0, false),
        LlmWaitPhase::Reasoning
    );
    assert_eq!(
        LlmWaitPhase::classify("x", "hmm", 0, false),
        LlmWaitPhase::Streaming
    );
    assert_eq!(
        LlmWaitPhase::classify("", "", 1, false),
        LlmWaitPhase::Streaming
    );
    assert_eq!(
        LlmWaitPhase::classify("<think>", "", 0, true),
        LlmWaitPhase::Reasoning
    );
}

#[test]
fn spinner_status_shows_phase_and_elapsed() {
    let ev = ProgressEvent::LlmWaiting {
        elapsed_secs: 45,
        phase: "prefill".into(),
        tokens_so_far: 0,
        tokens_source: "estimate".into(),
    };
    assert_eq!(
        spinner_status("Thinking", &ev, true).as_deref(),
        Some("Thinking — prefill 45s")
    );
    assert_eq!(
        spinner_status("Thinking", &ev, false).as_deref(),
        Some("Thinking — prefill")
    );
    let ev2 = ProgressEvent::LlmWaiting {
        elapsed_secs: 90,
        phase: "reasoning".into(),
        tokens_so_far: 1200,
        tokens_source: "usage".into(),
    };
    assert_eq!(
        spinner_status("Thinking", &ev2, true).as_deref(),
        Some("Thinking — reasoning 90s, 1200 tokens")
    );
    assert!(spinner_status("x", &ProgressEvent::LlmRequestSent { tokens: 1 }, true).is_none());
}
