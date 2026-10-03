//! Unit tests for `agent::review_shards`: sharding math, the scheduler's
//! concurrency cap and retry/unread accounting, answer parsing, finding
//! verification, and mock-endpoint end-to-end reviews.

use super::*;
use crate::agent::ReviewPhase;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn meta(path: &str, tokens: usize) -> SliceMeta {
    SliceMeta {
        path: path.to_string(),
        tokens,
    }
}

// ---------------------------------------------------------------- sharding

#[test]
fn line_ranges_are_split_by_measured_tokens() {
    // 10 lines of 30 tokens, budget 100: 3 lines per range.
    let costs = vec![30; 10];
    assert_eq!(
        split_line_ranges(&costs, 1, 100),
        vec![(1, 3), (4, 6), (7, 9), (10, 10)]
    );
    // Offsets: the ranges keep absolute line numbers.
    assert_eq!(
        split_line_ranges(&costs[..4], 41, 60),
        vec![(41, 42), (43, 44)]
    );
    // A single line over budget is its own range, never dropped.
    assert_eq!(
        split_line_ranges(&[5, 500, 5], 1, 100),
        vec![(1, 1), (2, 2), (3, 3)]
    );
    assert!(split_line_ranges(&[], 1, 100).is_empty());
    // Every line is covered exactly once.
    let costs: Vec<usize> = (0..997).map(|i| 1 + (i * 7) % 23).collect();
    let ranges = split_line_ranges(&costs, 1, 250);
    let mut next = 1;
    for &(a, b) in &ranges {
        assert_eq!(a, next);
        assert!(b >= a);
        let used: usize = costs[a - 1..b].iter().sum();
        assert!(used <= 250 || a == b, "{a}-{b} uses {used}");
        next = b + 1;
    }
    assert_eq!(next, 998);
}

#[test]
fn shards_respect_the_budget_and_group_by_directory() {
    let slices = vec![
        meta("src/a/one.rs", 400),
        meta("src/b/x.rs", 300),
        meta("src/a/two.rs", 400),
        meta("src/b/y.rs", 300),
        meta("src/c/big.rs", 900),
        meta("top.rs", 50),
    ];
    let shards = pack_shards(&slices, 1_000);
    // Every slice exactly once.
    let mut all: Vec<usize> = shards.iter().flatten().copied().collect();
    all.sort_unstable();
    assert_eq!(all, (0..slices.len()).collect::<Vec<_>>());
    for shard in &shards {
        let used: usize = shard.iter().map(|&i| slices[i].tokens).sum();
        assert!(used <= 1_000 || shard.len() == 1, "{shard:?} uses {used}");
    }
    // src/a's two files travel together, ahead of src/b (plan order of
    // first appearance).
    assert_eq!(shards[0], vec![0, 2]);
    assert_eq!(shards[1], vec![1, 3]);
    // One slice over budget is still delivered, alone.
    let one = pack_shards(&[meta("huge.rs", 5_000)], 1_000);
    assert_eq!(one, vec![vec![0]]);
}

#[test]
fn shard_budget_is_clamped_to_the_context_window() {
    // qwen38: 163,840 window, 12,288 output → the configured 32k holds.
    assert_eq!(effective_shard_tokens(32_000, 163_840, 12_288), 32_000);
    // A 32k window cannot hold a 32k shard plus its answer.
    let small = effective_shard_tokens(32_000, 32_768, 12_288);
    assert!(
        small < 32_000 && small + 12_288 + 4_096 <= 32_768,
        "{small}"
    );
    // Never below the floor.
    assert_eq!(effective_shard_tokens(32_000, 8_000, 12_288), 2_000);
}

#[test]
fn a_small_scope_is_spread_over_the_parallelism() {
    // 30k tokens, 6 slots: 8k shards (the floor), not one 30k shard.
    assert_eq!(balanced_shard_tokens(30_000, 6, 32_000), 8_000);
    // 120k over 6 slots: 20k each.
    assert_eq!(balanced_shard_tokens(120_000, 6, 32_000), 20_000);
    // A large scope keeps the full budget.
    assert_eq!(balanced_shard_tokens(3_000_000, 6, 32_000), 32_000);
    // A budget below the floor (small context window) still binds.
    assert_eq!(balanced_shard_tokens(30_000, 6, 4_000), 4_000);
}

#[test]
fn shards_stop_starting_while_the_synthesis_still_fits() {
    // The live 2 h run: 396 s left when the phase ended — too late.
    // 7,200 s budget: reserve 900 s synthesis + the longest shard (600 s).
    assert!(wall_dispatch_stop(1_600, 7_200, 60, 600).is_none());
    let why = wall_dispatch_stop(1_499, 7_200, 60, 600).expect("stop");
    assert!(
        why.contains("1499s of the wall budget left < 1500s"),
        "{why}"
    );
    // A slower measured answer raises the reserve.
    assert!(wall_dispatch_stop(1_600, 7_200, 1_200, 600).is_some());
    // A small budget keeps a third for the answer, not 900 s: the first
    // wave of a 1,800 s slugify review still starts.
    assert!(wall_dispatch_stop(1_800, 1_800, 30, 0).is_none());
    assert!(wall_dispatch_stop(599, 1_800, 30, 0).is_some());
}

// --------------------------------------------------------------- scheduler

#[tokio::test]
async fn scheduler_never_exceeds_the_parallelism_cap() {
    let now = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let totals = schedule_shards(
        20,
        4,
        Duration::from_millis(5),
        || Dispatch::Go,
        |_i, _attempt| {
            let now = Arc::clone(&now);
            let peak = Arc::clone(&peak);
            async move {
                let n = now.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(n, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(15)).await;
                now.fetch_sub(1, Ordering::SeqCst);
                Ok::<(), String>(())
            }
        },
        |_, _, _| {},
        |_| None,
    )
    .await;
    assert_eq!(totals.succeeded.len(), 20);
    assert_eq!(peak.load(Ordering::SeqCst), 4);
    assert_eq!(totals.peak_in_flight, 4);
    assert!(totals.failed.is_empty() && totals.not_run.is_empty());
}

#[tokio::test]
async fn a_failed_shard_is_retried_once_then_reported_unread() {
    // Shard 1 fails once then succeeds; shard 2 fails twice.
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut seen = Vec::new();
    let totals = schedule_shards(
        4,
        2,
        Duration::from_millis(5),
        || Dispatch::Go,
        |i, attempt| {
            let calls = Arc::clone(&calls);
            async move {
                calls.lock().unwrap().push((i, attempt));
                match (i, attempt) {
                    (1, 0) | (2, _) => Err(format!("shard {i} attempt {attempt} failed")),
                    _ => Ok(i),
                }
            }
        },
        |i, attempt, result| seen.push((i, attempt, result.is_ok())),
        |_| None,
    )
    .await;
    let mut ok = totals.succeeded.clone();
    ok.sort_unstable();
    assert_eq!(ok, vec![0, 1, 3]);
    assert_eq!(totals.failed, vec![2]);
    assert_eq!(totals.retried, 2);
    assert!(totals.not_run.is_empty());
    // Exactly one retry each, with attempt = 1.
    let calls = calls.lock().unwrap();
    assert_eq!(calls.iter().filter(|c| c.0 == 2).count(), 2);
    assert!(calls.contains(&(1, 1)) && calls.contains(&(2, 1)));
    assert_eq!(seen.len(), 6);
}

#[tokio::test]
async fn a_budget_stop_leaves_the_rest_not_run() {
    let dispatched = AtomicUsize::new(0);
    let totals = schedule_shards(
        10,
        2,
        Duration::from_millis(5),
        || {
            if dispatched.fetch_add(1, Ordering::SeqCst) >= 3 {
                Dispatch::Stop("token budget".to_string())
            } else {
                Dispatch::Go
            }
        },
        |_i, _a| async { Ok::<(), String>(()) },
        |_, _, _| {},
        |_| None,
    )
    .await;
    assert_eq!(totals.succeeded.len(), 3);
    assert_eq!(totals.not_run, (3..10).collect::<Vec<_>>());
    assert_eq!(totals.stopped.as_deref(), Some("token budget"));
}

#[tokio::test]
async fn an_abort_drops_calls_in_flight() {
    let aborted = std::sync::atomic::AtomicBool::new(false);
    let started = Instant::now();
    let totals = schedule_shards(
        3,
        3,
        Duration::from_millis(10),
        || {
            if aborted.load(Ordering::SeqCst) {
                Dispatch::Abort("cancelled".to_string())
            } else {
                aborted.store(
                    started.elapsed() > Duration::from_millis(30),
                    Ordering::SeqCst,
                );
                Dispatch::Go
            }
        },
        |_i, _a| async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            Ok::<(), String>(())
        },
        |_, _, _| {},
        |_| None,
    )
    .await;
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(totals.succeeded.is_empty());
    assert_eq!(totals.not_run, vec![0, 1, 2]);
    assert_eq!(totals.stopped.as_deref(), Some("cancelled"));
}

// --------------------------------------------------------- circuit breaker

/// The 0.9.6 gate failure, at the scheduler: an endpoint that never
/// produces a usable answer. Without the breaker every one of 162 shards
/// ran twice (324 calls); with it the phase stops after N = 12 failed
/// attempts (6 in parallel) plus the calls still in flight, credits
/// nothing, and leaves every shard for the main loop.
#[tokio::test]
async fn an_endpoint_that_never_answers_trips_the_breaker_early() {
    let calls = AtomicUsize::new(0);
    let mut breaker = ShardBreaker::new(6);
    let totals = schedule_shards(
        162,
        6,
        Duration::from_millis(5),
        || Dispatch::Go,
        |_i, _a| {
            calls.fetch_add(1, Ordering::SeqCst);
            async {
                tokio::task::yield_now().await;
                Err::<(), _>(ShardError::new(
                    ShardFailure::Format,
                    "answer is not the JSON object asked for: \"done\"",
                ))
            }
        },
        |_, _, _| {},
        |r: &Result<(), ShardError>| breaker.observe(r.as_ref().map(|_| ()).map_err(|e| e.kind)),
    )
    .await;
    let calls = calls.load(Ordering::SeqCst);
    assert!(
        (12..=12 + 5).contains(&calls),
        "{calls} calls (N = 12, at most 5 more in flight)"
    );
    let why = totals.tripped.as_deref().expect("tripped");
    assert!(
        why.contains("the first 12 shard attempts all failed"),
        "{why}"
    );
    assert!(totals.succeeded.is_empty());
    assert_eq!(
        totals.failed.len() + totals.not_run.len(),
        162,
        "every shard is accounted for"
    );
    assert!(totals.not_run.len() >= 162 - 17, "{totals:?}");
    assert!(totals.stopped.is_none());
}

/// A transient fault — the whole first wave fails at once — whose retries
/// and the following shards succeed never trips the breaker: everything is
/// read.
#[tokio::test]
async fn a_transient_fault_that_recovers_never_trips() {
    let calls = AtomicUsize::new(0);
    let mut breaker = ShardBreaker::new(6);
    let totals = schedule_shards(
        40,
        6,
        Duration::from_millis(5),
        || Dispatch::Go,
        |_i, _a| {
            let k = calls.fetch_add(1, Ordering::SeqCst);
            async move {
                if k < 6 {
                    Err(ShardError::new(ShardFailure::Transport, "HTTP 503"))
                } else {
                    Ok(())
                }
            }
        },
        |_, _, _| {},
        |r: &Result<(), ShardError>| breaker.observe(r.as_ref().map(|_| ()).map_err(|e| e.kind)),
    )
    .await;
    assert!(totals.tripped.is_none(), "{totals:?}");
    assert_eq!(totals.succeeded.len(), 40);
    assert_eq!(totals.retried, 6);
    assert_eq!(breaker.failure_counts(), (0, 6, 0, 0));
}

/// Reserve cuts are the harness's own doing: a phase whose every call is
/// cut at the reserve never trips the breaker (the same number of format
/// failures would, at the 4th).
#[tokio::test(start_paused = true)]
async fn reserve_cuts_never_trip_the_breaker() {
    let cut = Some(tokio::time::Instant::now() + Duration::from_secs(100));
    let mut breaker = ShardBreaker::new(2);
    let totals = schedule_shards(
        12,
        2,
        Duration::from_secs(10_000),
        || Dispatch::Go,
        |_i, _a| {
            within_reserve(cut, async {
                tokio::time::sleep(Duration::from_secs(3_000)).await;
                Ok::<(), ShardError>(())
            })
        },
        |_, _, _| {},
        |r: &Result<(), ShardError>| breaker.observe(r.as_ref().map(|_| ()).map_err(|e| e.kind)),
    )
    .await;
    assert!(totals.tripped.is_none(), "{totals:?}");
    assert_eq!(totals.failed.len(), 12);
    assert_eq!(breaker.failure_counts(), (0, 0, 0, 24));
    let mut format = ShardBreaker::new(2);
    let first = (1..=24).find(|_| format.observe(Err(ShardFailure::Format)).is_some());
    assert_eq!(first, Some(4));
}

// ------------------------------------------------------ answers + findings

#[test]
fn shard_answers_are_parsed_from_bare_fenced_or_wrapped_json() {
    let bare = r#"{"files":[{"path":"a.py","summary":"math"}],"findings":[]}"#;
    assert_eq!(parse_shard_answer(bare).unwrap().files.len(), 1);
    let fenced = format!("Here you go:\n```json\n{bare}\n```\n");
    assert_eq!(parse_shard_answer(&fenced).unwrap().files[0].path, "a.py");
    let wrapped = format!("<think>{{not json}}</think>Result: {bare} done.");
    assert!(parse_shard_answer(&wrapped).is_some());
    let string_line =
        r#"{"findings":[{"path":"a.py","line":"12","title":"t","evidence_quote":"q"}]}"#;
    assert_eq!(
        parse_shard_answer(string_line).unwrap().findings[0].line_number(),
        Some(12)
    );
    assert!(parse_shard_answer("I found no problems.").is_none());
    assert!(parse_shard_answer("[1, 2]").is_none());
}

fn lines(text: &str, first: usize) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .map(|(i, l)| (first + i, l.to_string()))
        .collect()
}

#[test]
fn a_finding_is_verified_only_when_its_quote_is_on_the_cited_lines() {
    let src = lines(
        "def ratio(x, n):\n    return x / n\n\ndef show(v):\n    print(v)\n    return None\n",
        1,
    );
    assert_eq!(
        verify_quote(&src, 2, "return x / n"),
        QuoteVerdict::Verified
    );
    // Whitespace and a copied `N<TAB>` prefix do not matter.
    assert_eq!(
        verify_quote(&src, 2, "2\t    return   x / n"),
        QuoteVerdict::Verified
    );
    // One line off: the citation is corrected to where the quote is.
    assert_eq!(
        verify_quote(&src, 3, "return x / n"),
        QuoteVerdict::Relocated(2)
    );
    // A multi-line quote spans the cited line.
    assert_eq!(
        verify_quote(&src, 5, "def show(v):\n    print(v)"),
        QuoteVerdict::Verified
    );
    // Multi-line quote starting at the cited line.
    assert_eq!(
        verify_quote(&src, 4, "def show(v):\n    print(v)"),
        QuoteVerdict::Verified
    );
    // Found, but elsewhere in the slice: the line is corrected.
    let long = lines(
        &(1..=40)
            .map(|i| format!("let v{i} = compute({i});"))
            .collect::<Vec<_>>()
            .join("\n"),
        100,
    );
    assert_eq!(
        verify_quote(&long, 105, "let v30 = compute(30);"),
        QuoteVerdict::Relocated(129)
    );
    // A quote that is not in the file is rejected.
    assert!(matches!(
        verify_quote(&src, 2, "return x // n if n else 0"),
        QuoteVerdict::Unverified(_)
    ));
    // Too short to prove anything.
    assert!(matches!(
        verify_quote(&src, 2, " x "),
        QuoteVerdict::Unverified(_)
    ));
}

#[test]
fn verify_answer_records_verified_and_names_the_rest() {
    let content = "1\tdef ratio(x, n):\n2\t    return x / n\n";
    let slice = Slice {
        path: "pkg/a.py".to_string(),
        start: 1,
        end: 2,
        total_lines: 2,
        tokens: 10,
        content: content.to_string(),
        notes: Vec::new(),
        args: serde_json::json!({}),
        payload: String::new(),
    };
    let answer: ShardAnswer = serde_json::from_str(
        r#"{"files":[{"path":"a.py","summary":"ratio helper"},{"path":"nope.py","summary":"x"}],
            "findings":[
              {"path":"pkg/a.py","line":2,"severity":"critical","title":"division by zero when n is 0","evidence_quote":"return x / n"},
              {"path":"./pkg/a.py","line":2,"severity":"low","title":"invented problem here","evidence_quote":"return x * n"},
              {"path":"pkg/other.py","line":1,"severity":"high","title":"file not in shard","evidence_quote":"import os"}
            ]}"#,
    )
    .unwrap();
    let out = verify_answer(&[&slice], &answer);
    assert_eq!(
        out.notes,
        vec![("pkg/a.py".to_string(), "ratio helper".to_string())]
    );
    assert_eq!(out.verified.len(), 1);
    assert!(
        out.verified[0].starts_with("FINDING: pkg/a.py:2 `return x / n` — [high] division by zero"),
        "{}",
        out.verified[0]
    );
    assert_eq!(out.unverified.len(), 2);
    assert!(
        out.unverified[0].contains("not found"),
        "{:?}",
        out.unverified
    );
    assert!(
        out.unverified[1].contains("did not read"),
        "{:?}",
        out.unverified
    );
    // Unverified claims carry no `path:line` an answer could copy as a
    // citation.
    assert!(
        out.unverified
            .iter()
            .all(|u| !u.contains("a.py:2") && !u.contains("other.py:1")),
        "{:?}",
        out.unverified
    );
}

// ------------------------------------------------------------ end to end

fn review_fixture(root: &std::path::Path) {
    std::fs::create_dir_all(root.join("pkg")).unwrap();
    std::fs::write(
        root.join("pkg/__init__.py"),
        "from .a import ratio\nfrom .b import last\n",
    )
    .unwrap();
    // Planted: division by zero (line 2), off-by-one index (line 3).
    std::fs::write(
        root.join("pkg/a.py"),
        "def ratio(x, n):\n    return x / n\n",
    )
    .unwrap();
    std::fs::write(
        root.join("pkg/b.py"),
        "def last(items):\n    # the last element\n    return items[len(items)]\n",
    )
    .unwrap();
}

fn shard_config(url: &str) -> crate::config::Config {
    let mut config = crate::test_support::mock_agent_config(&format!("{url}/v1"));
    config.review.shard_reading = true;
    config.review.shard_parallelism = 3;
    config
}

/// One shard over a 3-file fixture with two planted bugs: the verified
/// finding is recorded, the line-corrected one is recorded at the real
/// line, the invented one is rejected; every file is credited, and the main
/// agent starts in synthesis and answers without reading anything itself.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn a_review_is_read_by_shards_and_findings_are_verified() {
    use crate::testing::mock_api::MockLlmServer;
    let dir = tempfile::tempdir().unwrap();
    review_fixture(dir.path());
    let _cwd = crate::test_support::CwdGuard::enter(dir.path());
    let shard = r#"{"files":[
        {"path":"pkg/__init__.py","summary":"re-exports ratio and last"},
        {"path":"pkg/a.py","summary":"ratio(x, n) divides x by n"},
        {"path":"pkg/b.py","summary":"last(items) returns the last element"}],
      "findings":[
        {"path":"pkg/a.py","line":2,"severity":"high","title":"ratio raises ZeroDivisionError when n is 0","evidence_quote":"return x / n"},
        {"path":"pkg/b.py","line":1,"severity":"high","title":"last indexes one past the end (IndexError)","evidence_quote":"return items[len(items)]"},
        {"path":"pkg/b.py","line":2,"severity":"low","title":"a made-up issue in a comment","evidence_quote":"items.pop()"}]}"#;
    let final_review = "Final review.\n- pkg/a.py:2 — [high] ratio divides by n without a zero \
                        check.\n- pkg/b.py:3 — [high] items[len(items)] is one past the end.";
    let server = MockLlmServer::builder()
        .with_response(shard)
        .with_default_response(crate::testing::mock_api::MockResponse::Text(
            final_review.to_string(),
        ))
        .build()
        .await;
    let mut agent = Agent::new(shard_config(server.url())).await.unwrap();
    let result = agent
        .run_task("review this repository for bugs, cite file:line")
        .await;
    let bodies = server.captured_request_bodies().await;
    server.stop().await;
    assert!(result.is_ok(), "{:?}", result.err());

    // The shard request carried the numbered file content, not the history.
    assert!(
        bodies[0].contains("=== FILE \\\"pkg/a.py\\\" (whole file, 2 lines) ==="),
        "{}",
        &bodies[0][..bodies[0].len().min(2000)]
    );
    assert!(bodies[0].contains("<review_file path=\\\"pkg/a.py\\\">"));
    assert!(bodies[0].contains("2\\t    return x / n"));
    assert!(!bodies[0].contains("\"tools\""));

    let coverage = agent.review_coverage().expect("review session");
    assert!(coverage.complete, "{coverage:?}");
    assert_eq!((coverage.read_files, coverage.relevant_files), (3, 3));
    // The two verified shard findings (the accepted answer's cited lines
    // are recorded as well).
    assert!(coverage.findings_recorded >= 2, "{coverage:?}");
    let shards = coverage.shards.clone().expect("shard report");
    assert_eq!((shards.shards, shards.succeeded, shards.failed), (1, 1, 0));
    assert_eq!(shards.findings_verified, 2);
    assert_eq!(shards.findings_relocated, 1);
    assert_eq!(shards.findings_unverified, 1);
    assert_eq!(shards.files_read, 3);
    assert!(
        coverage.line.contains("review shards: 1 of 1 succeeded"),
        "{}",
        coverage.line
    );

    // The relocated finding is recorded at the line the quote is on, and
    // the context note hands notes + the rejected finding to the agent.
    let note = agent
        .messages
        .iter()
        .map(|m| m.content.text().to_string())
        .find(|t| t.contains("kind=review_shards"))
        .expect("shard context note");
    assert!(
        note.contains("pkg/a.py: ratio(x, n) divides x by n"),
        "{note}"
    );
    assert!(note.contains("a made-up issue"), "{note}");
    // Follow-up reads stay narrow: the note and every synthesis status say
    // the shard-read lines count and must not be re-read.
    assert!(note.contains("Do NOT file_read them again"), "{note}");
    assert_eq!(agent.review_phase(), Some(ReviewPhase::Synthesis));
    let status = agent.review_turn_note().unwrap();
    assert!(
        status.contains("pkg/b.py:3 `return items[len(items)]` — [high] last indexes"),
        "{status}"
    );
    assert!(status.contains("Do NOT file_read them again"), "{status}");
    // The main agent answered without reading anything itself.
    assert!(
        !agent
            .messages
            .iter()
            .any(|m| m.content.text().contains("\"total_lines\"")),
        "the main agent ran file_read"
    );
    assert!(agent.last_assistant_response.contains("Final review"));
    // The answer's plain citations name the lines the shard verified by
    // quote (b.py:3 after the line correction), in files unchanged since:
    // content-verified, and reported as the shard's credit.
    let grounding = agent.grounding_status().expect("grounding");
    assert_eq!(
        (
            grounding.total,
            grounding.verified,
            grounding.shard_verified
        ),
        (2, 2, 2),
        "{grounding:?}"
    );
}

/// A shard whose answer is unusable twice credits nothing: its files stay
/// unread, the gate sends the main agent to read them with file_read, and
/// only that delivered content completes the coverage.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn a_failed_shard_credits_nothing_and_the_agent_reads_instead() {
    use crate::testing::mock_api::MockLlmServer;
    let dir = tempfile::tempdir().unwrap();
    review_fixture(dir.path());
    let _cwd = crate::test_support::CwdGuard::enter(dir.path());
    let read = |p: &str| {
        format!(
            "<tool>\n<name>file_read</name>\n<arguments>{{\"path\":\"{p}\"}}</arguments>\n</tool>"
        )
    };
    let server = MockLlmServer::builder()
        .with_response("I reviewed the files and they look fine.")
        .with_response("Still no JSON, sorry.")
        .with_response(read("pkg/__init__.py"))
        .with_response(read("pkg/a.py"))
        .with_response(read("pkg/b.py"))
        .with_response("Final review. pkg/a.py:2 — division by zero when n is 0.")
        .build()
        .await;
    let mut agent = Agent::new(shard_config(server.url())).await.unwrap();
    let result = agent
        .run_task("review this repository for bugs, cite file:line")
        .await;
    let bodies = server.captured_request_bodies().await;
    server.stop().await;
    assert!(result.is_ok(), "{:?}", result.err());
    // The retry went with thinking off; the first attempt did not.
    assert!(bodies.len() >= 2);
    assert!(!bodies[0].contains("DO NOT use <think> blocks"));
    assert!(bodies[1].contains("DO NOT use <think> blocks"));
    // No workload table: the endpoint is not known to take the switch.
    assert!(!bodies[1].contains("enable_thinking"));
    let coverage = agent.review_coverage().expect("review session");
    let shards = coverage.shards.clone().expect("shard report");
    assert_eq!((shards.succeeded, shards.failed, shards.retried), (0, 1, 1));
    assert_eq!(shards.files_read, 0);
    assert!(
        shards.line.contains("failed twice (files left unread)"),
        "{}",
        shards.line
    );
    let note = agent
        .messages
        .iter()
        .map(|m| m.content.text().to_string())
        .find(|t| t.contains("kind=review_shards"))
        .expect("shard context note");
    assert!(note.contains("Still unread (3)"), "{note}");
    // Coverage came from the agent's own file_read results only.
    assert!(coverage.complete, "{coverage:?}");
}

/// A file larger than the shard budget is read as line-range shards, run in
/// parallel; together they credit every line.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn a_large_file_is_read_as_parallel_range_shards() {
    use crate::testing::mock_api::MockLlmServer;
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("pkg")).unwrap();
    let big: String = (1..=900)
        .map(|i| {
            format!("def f{i}(value_{i}, other_{i}):\n    return value_{i} + other_{i} * {i}\n")
        })
        .collect();
    std::fs::write(dir.path().join("pkg/big.py"), &big).unwrap();
    let _cwd = crate::test_support::CwdGuard::enter(dir.path());
    // Every request gets the same answer: a valid shard answer, and (with
    // its `path:line`) an acceptable final review for the main turn.
    let answer = r#"{"files":[{"path":"pkg/big.py","summary":"many small adders, see pkg/big.py:2"}],"findings":[]}"#;
    let server = MockLlmServer::builder()
        .with_default_response(crate::testing::mock_api::MockResponse::Text(
            answer.to_string(),
        ))
        .build()
        .await;
    let mut config = shard_config(server.url());
    config.review.shard_tokens = 2_000;
    let mut agent = Agent::new(config).await.unwrap();
    let result = agent
        .run_task("review this repository for bugs, cite file:line")
        .await;
    server.stop().await;
    assert!(result.is_ok(), "{:?}", result.err());
    let coverage = agent.review_coverage().expect("review session");
    let shards = coverage.shards.clone().expect("shard report");
    assert!(shards.slices >= 3, "{shards:?}");
    assert_eq!(shards.shards, shards.succeeded, "{shards:?}");
    assert!(shards.peak_in_flight >= 2, "{shards:?}");
    assert!(shards.peak_in_flight <= 3, "{shards:?}");
    assert!(coverage.complete, "{coverage:?}");
    assert_eq!(coverage.read_lines, 1_800);
}

/// The shard phase runs once per task: a continued segment (auto-continue,
/// resume) keeps the session and never sends the plan to shards again. A
/// later main request that carries no file content neither retracts nor
/// re-counts what the shards delivered.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn the_shard_phase_runs_once_and_main_requests_keep_its_coverage() {
    use crate::testing::mock_api::MockLlmServer;
    let dir = tempfile::tempdir().unwrap();
    review_fixture(dir.path());
    let _cwd = crate::test_support::CwdGuard::enter(dir.path());
    let answer = r#"{"files":[{"path":"pkg/a.py","summary":"ratio"}],"findings":[]}"#;
    let server = MockLlmServer::builder()
        .with_default_response(crate::testing::mock_api::MockResponse::Text(
            answer.to_string(),
        ))
        .build()
        .await;
    let mut agent = Agent::new(shard_config(server.url())).await.unwrap();
    agent.current_task_context = "review this repository for bugs".to_string();
    agent.classify_task_policy();
    agent.begin_review_session().await;
    agent.run_review_shard_phase().await;
    let sent = server.captured_request_bodies().await.len();
    assert_eq!(sent, 1, "one shard for the fixture");
    let before = agent.review_coverage().unwrap();
    assert!(before.complete, "{before:?}");

    // A continued segment: same session, no second shard phase.
    agent.continue_review_session().await;
    agent.run_review_shard_phase().await;
    assert_eq!(server.captured_request_bodies().await.len(), sent);

    // A main request without any file_read result: the turn note is built
    // and its (empty) delivered reads committed; shard coverage stays.
    let pending = agent.review_reads_delivered(&agent.messages.clone());
    let _ = agent.review_turn_note_for(&agent.messages.clone());
    agent.review_commit_reads(&pending);
    let after = agent.review_coverage().unwrap();
    assert_eq!(
        (after.read_files, after.read_lines),
        (before.read_files, before.read_lines)
    );
    server.stop().await;
}

/// Measured before (8282adc2, live): with coverage complete from the shard
/// reads, the main agent still re-read every file whole. The first broad
/// re-read of a shard-read file gets the shard note and the file's recorded
/// findings instead of the content; a narrow confirmation read and a
/// repeated request are delivered; nothing is credited for the note.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn a_broad_reread_of_a_shard_read_file_is_withheld_once() {
    use crate::testing::mock_api::MockLlmServer;
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("pkg")).unwrap();
    let body: String = (1..=100)
        .map(|i| format!("def f{i}(x):\n    return x + {i}\n"))
        .collect();
    std::fs::write(dir.path().join("pkg/big.py"), &body).unwrap();
    let _cwd = crate::test_support::CwdGuard::enter(dir.path());
    let read = |args: &str| {
        format!("<tool>\n<name>file_read</name>\n<arguments>{args}</arguments>\n</tool>")
    };
    let shard = r#"{"files":[{"path":"pkg/big.py","summary":"one hundred adders"}],
      "findings":[{"path":"pkg/big.py","line":4,"severity":"low","title":"f2 adds a constant","evidence_quote":"return x + 2"}]}"#;
    let server = MockLlmServer::builder()
        .with_response(shard)
        .with_response(read(r#"{"path":"pkg/big.py"}"#))
        .with_response(read(r#"{"path":"pkg/big.py","line_range":[1,30]}"#))
        .with_response(read(r#"{"path":"pkg/big.py"}"#))
        .with_default_response(crate::testing::mock_api::MockResponse::Text(
            "Final review: pkg/big.py:4 `return x + 2` — [low] f2 adds a constant.".to_string(),
        ))
        .build()
        .await;
    let mut agent = Agent::new(shard_config(server.url())).await.unwrap();
    let result = agent
        .run_task("review this repository for bugs, cite file:line")
        .await;
    server.stop().await;
    assert!(result.is_ok(), "{:?}", result.err());
    let texts: Vec<String> = agent
        .messages
        .iter()
        .map(|m| m.content.text().to_string())
        .collect();
    let withheld: Vec<&String> = texts
        .iter()
        .filter(|t| t.contains("review_shards_already_read"))
        .collect();
    assert_eq!(withheld.len(), 1, "exactly the first broad re-read");
    assert!(
        withheld[0].contains("one hundred adders"),
        "{}",
        withheld[0]
    );
    assert!(withheld[0].contains("pkg/big.py:4"), "{}", withheld[0]);
    // The narrow read and the repeated whole read were delivered.
    let delivered = texts
        .iter()
        .filter(|t| t.contains("return x + 1") && t.contains("lines_returned"))
        .count()
        + texts
            .iter()
            .filter(|t| t.contains("return x + 100") && !t.contains("review_shards_already_read"))
            .count();
    assert!(delivered >= 2, "{texts:#?}");
    let coverage = agent.review_coverage().unwrap();
    assert!(coverage.complete);
    assert_eq!(coverage.shards.unwrap().rereads_withheld, 1);
}

/// `n` modules of ~1,200 measured tokens each: one shard per file at the
/// 2,000-token shard budget.
fn many_module_fixture(root: &std::path::Path, n: usize) -> Vec<String> {
    std::fs::create_dir_all(root.join("pkg")).unwrap();
    (0..n)
        .map(|k| {
            let body: String = (1..=48)
                .map(|i| format!("def f{k}_{i}(value, other):\n    return value + other * {i}\n"))
                .collect();
            let rel = format!("pkg/m{k}.py");
            std::fs::write(root.join(&rel), body).unwrap();
            rel
        })
        .collect()
}

fn breaker_config(url: &str) -> crate::config::Config {
    let mut config = shard_config(url);
    config.review.shard_parallelism = 2;
    config.review.shard_tokens = 2_000;
    config
}

/// The 0.9.6 gate failure, end to end: the endpoint answers "done" to
/// every shard. The breaker stops the shard phase after N = 4 failed
/// attempts (2 in parallel) instead of running all 10 shards twice; the
/// main agent reads the plan itself with file_read and coverage counts
/// only what it delivered.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn an_endpoint_that_cannot_answer_shards_trips_and_the_main_loop_reads() {
    use crate::testing::mock_api::{MockLlmServer, MockResponse};
    let dir = tempfile::tempdir().unwrap();
    let files = many_module_fixture(dir.path(), 10);
    let _cwd = crate::test_support::CwdGuard::enter(dir.path());
    let reads: String = files
        .iter()
        .map(|p| {
            format!(
                "<tool>\n<name>file_read</name>\n<arguments>{{\"path\":\"{p}\"}}</arguments>\n</tool>\n"
            )
        })
        .collect();
    let server = MockLlmServer::builder()
        .with_route(
            "You are one reader in a parallel code review",
            Vec::new(),
            MockResponse::Text("done".to_string()),
        )
        .with_response(reads)
        .with_default_response(MockResponse::Text(
            "Final review: pkg/m0.py:2 `return value + other * 1` — no defects found in the \
             adders."
                .to_string(),
        ))
        .build()
        .await;
    let mut agent = Agent::new(breaker_config(server.url())).await.unwrap();
    let result = agent
        .run_task("review this repository for bugs, cite file:line")
        .await;
    let bodies = server.captured_request_bodies().await;
    server.stop().await;
    assert!(result.is_ok(), "{:?}", result.err());
    let shard_calls = bodies
        .iter()
        .filter(|b| b.contains("You are one reader in a parallel code review"))
        .count();
    assert!(
        (4..=5).contains(&shard_calls),
        "{shard_calls} shard calls (N = 4, at most 1 more in flight; 20 without the breaker)"
    );
    let coverage = agent.review_coverage().expect("review session");
    let shards = coverage.shards.clone().expect("shard report");
    assert_eq!(shards.shards, 10, "{shards:?}");
    assert_eq!(shards.succeeded, 0);
    assert_eq!(shards.files_read, 0, "a tripped phase credits nothing");
    assert!(shards.failed_format >= 4, "{shards:?}");
    let why = shards.tripped.as_deref().expect("tripped");
    assert!(
        why.contains("the first 4 shard attempts all failed")
            && why.contains("does not follow the shard answer format"),
        "{why}"
    );
    assert!(
        shards.line.contains(&format!(
            "shard reading stopped after {}/10 shards",
            shards.started
        )),
        "{}",
        shards.line
    );
    let json = serde_json::to_value(&coverage).unwrap();
    assert!(json["shards"]["tripped"].is_string(), "{json}");
    let note = agent
        .messages
        .iter()
        .map(|m| m.content.text().to_string())
        .find(|t| t.contains("kind=review_shards"))
        .expect("shard context note");
    assert!(note.contains("Shard reading was stopped early"), "{note}");
    assert!(note.contains("Still unread (10)"), "{note}");
    // The main loop read everything itself; coverage is complete from its
    // own file_read results only.
    assert!(coverage.complete, "{coverage:?}");
    assert_eq!((coverage.read_files, coverage.relevant_files), (10, 10));
}

/// An endpoint that fails the first few shard requests and then recovers
/// never trips the breaker: the failed shards are retried and every file
/// is read by the shards.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn an_endpoint_that_recovers_never_trips_the_breaker() {
    use crate::testing::mock_api::{MockLlmServer, MockResponse};
    let dir = tempfile::tempdir().unwrap();
    many_module_fixture(dir.path(), 10);
    let _cwd = crate::test_support::CwdGuard::enter(dir.path());
    let answer = r#"{"files":[],"findings":[]}"#;
    let bad = || MockResponse::Error {
        status: 400,
        body: r#"{"error":{"message":"transient"}}"#.to_string(),
    };
    let server = MockLlmServer::builder()
        .with_route(
            "You are one reader in a parallel code review",
            vec![bad(), bad(), bad()],
            MockResponse::Text(answer.to_string()),
        )
        .with_default_response(MockResponse::Text(
            "Final review: pkg/m0.py:2 `return value + other * 1` — no defects found.".to_string(),
        ))
        .build()
        .await;
    let mut agent = Agent::new(breaker_config(server.url())).await.unwrap();
    let result = agent
        .run_task("review this repository for bugs, cite file:line")
        .await;
    server.stop().await;
    assert!(result.is_ok(), "{:?}", result.err());
    let coverage = agent.review_coverage().expect("review session");
    let shards = coverage.shards.clone().expect("shard report");
    assert!(shards.tripped.is_none(), "{shards:?}");
    assert_eq!((shards.shards, shards.succeeded), (10, 10), "{shards:?}");
    assert!(shards.retried >= 1, "{shards:?}");
    assert!(shards.failed_transport >= 1, "{shards:?}");
    assert_eq!(shards.failed_format, 0, "{shards:?}");
    assert!(
        !shards.line.contains("shard reading stopped"),
        "{}",
        shards.line
    );
    assert!(coverage.complete, "{coverage:?}");
}
