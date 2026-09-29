//! Conformance of `agent::review_shards` with formal/ReviewBounds.lean:
//! the real scheduler against the exported shard state machine (RV4), the
//! synthesis reserve under simulated time with arbitrary call durations
//! (RV5), and finding verification against the model's decision (RV6).

use super::*;
use proptest::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

// ------------------------------------------------ RV4: shard state machine

/// `formal/review_shard_table.json`: `(state, event) → next`; every other
/// pair is refused.
fn shard_table() -> HashMap<(String, String), String> {
    let text = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/formal/review_shard_table.json"
    ));
    let rows: Vec<[String; 3]> = serde_json::from_str(text).expect("review_shard_table.json");
    let mut map = HashMap::new();
    for [s, e, t] in rows {
        assert!(map.insert((s, e), t).is_none(), "duplicate row");
    }
    map
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Outcome {
    Ok,
    Err,
    /// Never returns; the gate aborts while it is in flight.
    Hang,
}

type CallFuture = Pin<Box<dyn Future<Output = Result<(), String>>>>;

/// Run the real `schedule_shards` with scripted call outcomes, a gate
/// that stops after `stop_after` dispatches (or aborts while a call
/// hangs), and a breaker that trips at the `trip_after`-th completed
/// attempt. Returns the totals and every shard's observed events; checks
/// that nothing is dispatched once the breaker tripped (RV7
/// `tripped_starts_nothing`).
fn run_scenario(
    rt: &tokio::runtime::Runtime,
    cap: usize,
    outcomes: &[[Outcome; 2]],
    stop_after: Option<usize>,
    trip_after: Option<usize>,
) -> (ScheduleTotals, Vec<Vec<&'static str>>) {
    let n = outcomes.len();
    let events: Rc<RefCell<Vec<Vec<&'static str>>>> = Rc::new(RefCell::new(vec![Vec::new(); n]));
    let tripped = Rc::new(Cell::new(false));
    let completed = Cell::new(0usize);
    let hanging = Rc::new(Cell::new(0usize));
    let goes = Cell::new(0usize);
    let gate = || {
        if hanging.get() > 0 {
            return Dispatch::Abort("a call hangs".to_string());
        }
        if stop_after.is_some_and(|k| goes.get() >= k) {
            return Dispatch::Stop("budget".to_string());
        }
        goes.set(goes.get() + 1);
        Dispatch::Go
    };
    let call = |i: usize, attempt: u8| -> CallFuture {
        assert!(
            !tripped.get(),
            "shard {i} attempt {attempt} dispatched after the breaker tripped"
        );
        let ev = Rc::clone(&events);
        ev.borrow_mut()[i].push("dispatch");
        // Checked here, not only after the run: a scheduler that re-queues
        // without end would otherwise spin forever instead of failing.
        let dispatched = ev.borrow()[i].iter().filter(|e| **e == "dispatch").count();
        assert!(
            dispatched <= 2,
            "shard {i} dispatched a third time (attempt {attempt}): {:?}",
            ev.borrow()[i]
        );
        let outcome = outcomes[i][usize::from(attempt).min(1)];
        let hanging = Rc::clone(&hanging);
        Box::pin(async move {
            match outcome {
                Outcome::Ok => Ok(()),
                Outcome::Err => Err("scripted failure".to_string()),
                Outcome::Hang => {
                    hanging.set(hanging.get() + 1);
                    std::future::pending::<()>().await;
                    unreachable!()
                }
            }
        })
    };
    let done = |i: usize, _attempt: u8, r: &Result<(), String>| {
        events.borrow_mut()[i].push(if r.is_ok() { "ok" } else { "err" });
    };
    let breaker = |_: &Result<(), String>| {
        completed.set(completed.get() + 1);
        (trip_after == Some(completed.get())).then(|| {
            tripped.set(true);
            "breaker".to_string()
        })
    };
    let totals = rt.block_on(schedule_shards(
        n,
        cap,
        Duration::from_millis(1),
        gate,
        call,
        done,
        breaker,
    ));
    assert_eq!(totals.tripped.is_some(), tripped.get());
    let events = events.borrow().clone();
    (totals, events)
}

#[test]
fn the_real_scheduler_follows_the_lean_shard_machine() {
    let table = shard_table();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let all = [Outcome::Ok, Outcome::Err, Outcome::Hang];
    let mut rows_seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut scenarios = 0;
    for n in 1..=3usize {
        // Every outcome for every (shard, attempt).
        for code in 0..3usize.pow(2 * n as u32) {
            let mut c = code;
            let outcomes: Vec<[Outcome; 2]> = (0..n)
                .map(|_| {
                    let a = all[c % 3];
                    c /= 3;
                    let b = all[c % 3];
                    c /= 3;
                    [a, b]
                })
                .collect();
            for cap in 1..=2usize {
                for (stop_after, trip_after) in [None, Some(0), Some(1), Some(3)]
                    .into_iter()
                    .flat_map(|s| [None, Some(1), Some(2)].map(|t| (s, t)))
                {
                    scenarios += 1;
                    let (totals, events) =
                        run_scenario(&rt, cap, &outcomes, stop_after, trip_after);
                    let case = format!(
                        "n={n} cap={cap} stop={stop_after:?} trip={trip_after:?} {outcomes:?}"
                    );
                    assert!(
                        totals.peak_in_flight <= cap,
                        "{case}: peak {}",
                        totals.peak_in_flight
                    );
                    for (i, observed) in events.iter().enumerate() {
                        // Fold the observed events through the model; the
                        // end of the phase adds the implied abort / sweep.
                        let mut state = "queued0".to_string();
                        let mut step = |state: &mut String, event: &str| {
                            let key = (state.clone(), event.to_string());
                            let next = table.get(&key).unwrap_or_else(|| {
                                panic!(
                                    "{case}: shard {i}: {event} refused in {state} ({observed:?})"
                                )
                            });
                            rows_seen.insert(key);
                            *state = next.clone();
                        };
                        for e in observed {
                            step(&mut state, e);
                        }
                        if state.starts_with("flying") {
                            step(&mut state, "abort");
                        } else if state.starts_with("queued") {
                            step(&mut state, "sweep");
                        }
                        let dispatches = observed.iter().filter(|e| **e == "dispatch").count();
                        assert!(
                            dispatches <= 2,
                            "{case}: shard {i} dispatched {dispatches}×"
                        );
                        let in_succeeded = totals.succeeded.contains(&i);
                        let in_failed = totals.failed.contains(&i);
                        let in_not_run = totals.not_run.contains(&i);
                        assert_eq!(
                            usize::from(in_succeeded)
                                + usize::from(in_failed)
                                + usize::from(in_not_run),
                            1,
                            "{case}: shard {i} is in exactly one outcome list"
                        );
                        let rust = if in_succeeded {
                            "succeeded"
                        } else if in_failed {
                            "failed"
                        } else {
                            "not_run"
                        };
                        assert_eq!(rust, state, "{case}: shard {i} ({observed:?})");
                        // Credit (the done callback's Ok) iff succeeded.
                        assert_eq!(observed.contains(&"ok"), in_succeeded, "{case}: shard {i}");
                    }
                    let retried = events
                        .iter()
                        .filter(|ev| {
                            ev.iter().filter(|e| **e == "err").count() >= 1
                                && ev.iter().filter(|e| **e == "dispatch").count() == 2
                        })
                        .count();
                    assert!(totals.retried >= retried, "{case}");
                }
            }
        }
    }
    assert!(scenarios > 15_000);
    let all_rows: BTreeSet<(String, String)> = table.keys().cloned().collect();
    assert_eq!(
        rows_seen, all_rows,
        "every transition of the model was exercised"
    );
}

// ------------------------------------------------ RV7: the circuit breaker

/// `formal/review_breaker_table.json`: `(parallelism, name, history, first
/// trip)` — the history as `o` (succeeded), `x` (failed), `c` (reserve
/// cut); the first trip is the 1-based attempt at which the breaker trips,
/// or `none`.
fn breaker_table() -> Vec<[String; 4]> {
    let text = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/formal/review_breaker_table.json"
    ));
    serde_json::from_str(text).expect("review_breaker_table.json")
}

/// The real `ShardBreaker` trips at exactly the attempt the Lean model
/// does, for every exported history — the recorded healthy core reviews
/// (never), an always-failing endpoint (at N), cuts (never counted), a
/// mid-run collapse (12 of the last 16) — whatever kind each failure is.
#[test]
fn the_breaker_trips_where_the_lean_model_does() {
    let rows = breaker_table();
    assert!(rows.len() >= 60, "{} rows", rows.len());
    let kinds = [
        ShardFailure::Format,
        ShardFailure::Transport,
        ShardFailure::Timeout,
    ];
    let mut tripped_rows = 0;
    for [cap, name, history, expected] in &rows {
        let cap: usize = cap.parse().unwrap();
        // The same history with every failure one kind, then mixed.
        for variant in 0..=kinds.len() {
            let mut breaker = ShardBreaker::new(cap);
            let mut first = None;
            for (k, c) in history.chars().enumerate() {
                let outcome = match c {
                    'o' => Ok(()),
                    'x' => Err(kinds.get(variant).copied().unwrap_or(kinds[k % 3])),
                    'c' => Err(ShardFailure::ReserveCut),
                    other => panic!("history symbol {other:?}"),
                };
                if breaker.observe(outcome).is_some() {
                    assert!(first.is_none(), "{name}: tripped twice");
                    first = Some(k + 1);
                }
            }
            let got = first.map_or_else(|| "none".to_string(), |k| k.to_string());
            assert_eq!(
                &got, expected,
                "{name} at parallelism {cap} (variant {variant})"
            );
            assert_eq!(breaker.tripped().is_some(), first.is_some(), "{name}");
        }
        if expected != "none" {
            tripped_rows += 1;
        }
    }
    assert!(tripped_rows > 0 && tripped_rows < rows.len());
    // The live runs are in the table and never trip.
    assert!(rows
        .iter()
        .any(|r| r[1].starts_with("core_long_") && r[3] == "none"));
    assert!(rows
        .iter()
        .filter(|r| r[1].starts_with("core_long_"))
        .all(|r| r[3] == "none"));
}

/// The breaker's reason names what failed, and a trip is announced once.
#[test]
fn the_trip_reason_names_the_failure_kind() {
    let mut b = ShardBreaker::new(2);
    let mut reasons = Vec::new();
    for _ in 0..10 {
        reasons.extend(b.observe(Err(ShardFailure::Format)));
    }
    assert_eq!(reasons.len(), 1, "{reasons:?}");
    assert!(
        reasons[0].contains("the first 4 shard attempts all failed")
            && reasons[0].contains("4 answer(s) not in the JSON shard format")
            && reasons[0].contains("does not follow the shard answer format"),
        "{}",
        reasons[0]
    );
    let mut b = ShardBreaker::new(6);
    for _ in 0..40 {
        b.observe(Ok(()));
    }
    let why = (0..12)
        .find_map(|i| {
            b.observe(Err(if i % 2 == 0 {
                ShardFailure::Transport
            } else {
                ShardFailure::Timeout
            }))
        })
        .expect("12 of the last 16 failed");
    assert!(
        why.contains("12 of the last 16 shard attempts failed")
            && why.contains("6 request error(s)")
            && why.contains("6 call(s) over the time cap")
            && why.contains("the endpoint is failing"),
        "{why}"
    );
    assert_eq!(b.failure_counts(), (0, 6, 6, 0));
}

// ---------------------------------------------- RV5: the synthesis reserve

/// Longest call so far, recorded when a call ends (as `LongestGuard`).
struct Longest<'a>(&'a AtomicU64, tokio::time::Instant);

impl Drop for Longest<'_> {
    fn drop(&mut self) {
        self.0
            .fetch_max(self.1.elapsed().as_secs(), Ordering::Relaxed);
    }
}

/// Simulate the shard phase on tokio's paused clock with the real
/// scheduler, the real dispatch rule and the real reserve cut. Returns
/// `(remaining before, reserve, remaining after, succeeded)`.
fn simulate_phase(
    max_wall: u64,
    elapsed0: u64,
    forecast: u64,
    parallelism: usize,
    calls: &[(u64, bool)],
    shards: usize,
    cut_enabled: bool,
) -> (u64, u64, u64, usize) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap();
    rt.block_on(async {
        let base = tokio::time::Instant::now();
        let remaining = || max_wall.saturating_sub(elapsed0 + base.elapsed().as_secs());
        let remaining0 = remaining();
        let reserve = synthesis_reserve_secs(max_wall, forecast);
        let cut = cut_enabled.then(|| reserve_cut(base, remaining0, reserve));
        let longest = AtomicU64::new(0);
        let longest_ref = &longest;
        let gate = || match wall_dispatch_stop(
            remaining(),
            max_wall,
            forecast,
            longest_ref.load(Ordering::Relaxed),
        ) {
            Some(why) => Dispatch::Stop(why),
            None => Dispatch::Go,
        };
        let call = |i: usize, attempt: u8| {
            let (secs, ok) = calls[(2 * i + usize::from(attempt)) % calls.len()];
            within_reserve(cut, async move {
                let _longest = Longest(longest_ref, tokio::time::Instant::now());
                tokio::time::sleep(Duration::from_secs(secs)).await;
                if ok {
                    Ok(())
                } else {
                    Err("failed".to_string())
                }
            })
        };
        let totals = schedule_shards(
            shards,
            parallelism,
            // Only an abort is checked on a tick; none happens here.
            Duration::from_secs(30),
            gate,
            call,
            |_, _, _: &Result<(), String>| {},
            |_: &Result<(), String>| None,
        )
        .await;
        (remaining0, reserve, remaining(), totals.succeeded.len())
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// RV5 (`reserve_by_cut`): whatever each shard call takes — up to 50
    /// minutes, far past anything measured before it — the phase ends with
    /// at least the synthesis reserve left, or used no time at all.
    #[test]
    fn the_reserve_holds_for_any_call_durations(
        max_wall in 300u64..9_000,
        elapsed0 in 0u64..9_000,
        forecast in 0u64..1_500,
        parallelism in 1usize..6,
        shards in 1usize..12,
        calls in prop::collection::vec((1u64..3_000, any::<bool>()), 1..24),
    ) {
        let (before, reserve, after, _) =
            simulate_phase(max_wall, elapsed0, forecast, parallelism, &calls, shards, true);
        if before >= reserve {
            prop_assert!(after >= reserve, "{after}s left < reserve {reserve}s (had {before}s)");
        } else {
            prop_assert_eq!(after, before, "no time used when the reserve was already reached");
        }
    }
}

/// `dispatch_rule_alone_can_eat_reserve`: before any shard returned, the
/// measured longest call is 0, so a first-wave call running to the 1,200 s
/// side-call cap took the reserve of an 1,800 s review. With the cut the
/// reserve holds and the shard is reported as not read.
#[test]
fn a_first_wave_call_at_its_cap_is_cut_at_the_reserve() {
    let calls = [(1_200, true)];
    let (before, reserve, after, succeeded) = simulate_phase(1_800, 5, 0, 1, &calls, 1, false);
    assert_eq!((before, reserve), (1_795, 600));
    assert!(after < reserve, "the dispatch rule alone: {after}s left");
    assert_eq!(succeeded, 1);
    let (_, reserve, after, succeeded) = simulate_phase(1_800, 5, 0, 1, &calls, 1, true);
    assert!(after >= reserve, "with the cut: {after}s left");
    assert_eq!(succeeded, 0, "the cut shard is not credited");
}

#[test]
fn the_cut_keeps_one_second_for_the_clock_floor() {
    let now = tokio::time::Instant::now();
    assert_eq!(reserve_cut(now, 1_000, 600), now + Duration::from_secs(399));
    assert_eq!(reserve_cut(now, 600, 600), now);
    assert_eq!(reserve_cut(now, 10, 600), now);
    assert_eq!(synthesis_reserve_secs(7_200, 60), 900);
    assert_eq!(synthesis_reserve_secs(1_800, 60), 600);
    assert_eq!(synthesis_reserve_secs(7_200, 1_500), 1_500);
}

// -------------------------------------------- RV6: findings verification

/// The Lean model's `verify` over the matcher's candidate starts: nearest
/// candidate (first on a tie); verified at the cited line when the quote
/// starts there, or spans it and the cited line was delivered; else
/// relocated when near or unique; else unverified.
fn reference_verdict(lines: &[(usize, String)], cited: usize, quote: &[String]) -> QuoteVerdict {
    let n = lines.len();
    let matches_at = |idx: usize| -> bool {
        let mut pos = idx;
        for (k, ql) in quote.iter().enumerate() {
            let end = if k == 0 { pos + 1 } else { pos + 4 };
            match (pos..end.min(n)).find(|&j| lines[j].1 == *ql) {
                Some(j) => pos = j + 1,
                None => return false,
            }
        }
        true
    };
    let cands: Vec<usize> = (0..n)
        .filter(|&i| matches_at(i))
        .map(|i| lines[i].0)
        .collect();
    let mut nearest: Option<usize> = None;
    for &c in &cands {
        if nearest.is_none_or(|b| c.abs_diff(cited) < b.abs_diff(cited)) {
            nearest = Some(c);
        }
    }
    let Some(at) = nearest else {
        return QuoteVerdict::Unverified("");
    };
    let delivered = lines.iter().any(|(l, _)| *l == cited);
    if at == cited || (at <= cited && cited < at + quote.len() && delivered) {
        QuoteVerdict::Verified
    } else if at.abs_diff(cited) <= 30 || cands.len() == 1 {
        QuoteVerdict::Relocated(at)
    } else {
        QuoteVerdict::Unverified("")
    }
}

fn same_verdict(a: &QuoteVerdict, b: &QuoteVerdict) -> bool {
    match (a, b) {
        (QuoteVerdict::Unverified(_), QuoteVerdict::Unverified(_)) => true,
        _ => a == b,
    }
}

fn code_line(k: usize) -> String {
    format!("let v{k} = w;")
}

/// One file delivered as one or two slices (line numbers with a gap).
fn slices_strategy() -> impl Strategy<Value = Vec<(usize, String)>> {
    (
        1usize..40,
        prop::collection::vec(0usize..5, 1..12),
        prop::option::of((1usize..30, prop::collection::vec(0usize..5, 1..12))),
    )
        .prop_map(|(start, first, second)| {
            let mut out: Vec<(usize, String)> = first
                .iter()
                .enumerate()
                .map(|(i, &k)| (start + i, code_line(k)))
                .collect();
            if let Some((gap, ks)) = second {
                let s2 = start + first.len() + gap;
                out.extend(ks.iter().enumerate().map(|(i, &k)| (s2 + i, code_line(k))));
            }
            out
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_048))]

    /// RV6 against the real `verify_quote`: the Lean decision exactly, and
    /// its guarantee — a recorded line is a delivered line inside the span
    /// the quote matched; no match, no finding.
    #[test]
    fn verify_quote_is_the_lean_decision(
        lines in slices_strategy(),
        pick in 0usize..30,
        len in 1usize..4,
        absent in prop::option::of(0usize..8),
        cited in 1usize..90,
    ) {
        let quote: Vec<String> = match absent {
            Some(k) => vec![code_line(k)],
            None => {
                let s = pick % lines.len();
                lines[s..(s + len).min(lines.len())].iter().map(|(_, t)| t.clone()).collect()
            }
        };
        let got = verify_quote(&lines, cited, &quote.join("\n"));
        let want = reference_verdict(&lines, cited, &quote);
        prop_assert!(same_verdict(&got, &want), "got {:?}, model {:?}", got, want);
        let delivered = |l: usize| lines.iter().any(|(n, _)| *n == l);
        match got {
            QuoteVerdict::Verified => prop_assert!(delivered(cited)),
            QuoteVerdict::Relocated(at) => prop_assert!(delivered(at)),
            QuoteVerdict::Unverified(_) => {}
        }
    }
}

/// The regression behind `old_rule_cited_an_unread_line`: two slices of one
/// file in one shard (lines 1-10 and 50-60); a two-line quote of lines 10
/// and 50 cited at 11 was "verified" at line 11, which no shard read.
#[test]
fn a_quote_across_two_slices_does_not_verify_an_unread_line() {
    let lines: Vec<(usize, String)> = (1..=10)
        .chain(50..=60)
        .map(|n| (n, format!("let v{n} = w({n});")))
        .collect();
    let quote = "let v10 = w(10);\nlet v50 = w(50);";
    assert_eq!(verify_quote(&lines, 11, quote), QuoteVerdict::Relocated(10));
    assert_eq!(verify_quote(&lines, 10, quote), QuoteVerdict::Verified);
}

fn slice(path: &str, lines: &[(usize, String)]) -> Slice {
    let content: String = lines.iter().map(|(n, t)| format!("{n}\t{t}\n")).collect();
    Slice {
        path: path.to_string(),
        start: lines.first().map(|l| l.0).unwrap_or(1),
        end: lines.last().map(|l| l.0).unwrap_or(1),
        total_lines: 200,
        tokens: 0,
        content,
        notes: Vec::new(),
        args: serde_json::json!({}),
        payload: String::new(),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// RV6 at the answer level: `verify_answer` records a `FINDING:` only
    /// for a verdict the model records, at the model's line; every other
    /// claim goes to the unverified list, which carries no `path:line`.
    #[test]
    fn only_verified_findings_are_recorded(
        lines in slices_strategy(),
        findings in prop::collection::vec((1usize..90, 0usize..8, 0usize..3), 0..6),
    ) {
        let s = slice("src/m.rs", &lines);
        let refs = vec![&s];
        let answer = ShardAnswer {
            files: Vec::new(),
            findings: findings
                .iter()
                .map(|&(line, k, path)| RawFinding {
                    path: ["src/m.rs", "m.rs", "src/other.rs"][path].to_string(),
                    line: serde_json::json!(line),
                    severity: "high".to_string(),
                    title: format!("defect {k}"),
                    evidence_quote: code_line(k),
                })
                .collect(),
        };
        let outcome = verify_answer(&refs, &answer);
        let mut want = Vec::new();
        for &(line, k, path) in &findings {
            if path == 2 {
                continue; // a file this shard did not read
            }
            let at = match reference_verdict(&lines, line, &[code_line(k)]) {
                QuoteVerdict::Verified => line,
                QuoteVerdict::Relocated(at) => at,
                QuoteVerdict::Unverified(_) => continue,
            };
            want.push(format!("FINDING: src/m.rs:{at} `{}` — [high] defect {k}", code_line(k)));
        }
        prop_assert_eq!(&outcome.verified, &want);
        prop_assert_eq!(outcome.verified.len() + outcome.unverified.len(), findings.len());
        for u in &outcome.unverified {
            prop_assert!(!u.contains("src/m.rs:") && !u.contains("src/other.rs:"), "{}", u);
        }
    }
}
