//! Multi-language coding benchmark — tests LLM on Rust, Python, JavaScript, Go tasks.
//!
//! Run with: cargo run --features bench-harness --example multilang_bench

use selfware::api::types::Message;
use selfware::bench_harness::*;

fn rust_tasks() -> Vec<BenchTask> {
    vec![
        BenchTask {
            id: "rust-fibonacci".into(),
            description: "Rust: iterative fibonacci".into(),
            messages: vec![
                Message::system("You are a Rust expert. Output ONLY code, no explanation."),
                Message::user("Write a Rust function `fn fibonacci(n: u64) -> u64` that returns the nth fibonacci number iteratively. Include tests."),
            ],
            evaluator: Box::new(KeywordEvaluator::new(vec![
                "fn fibonacci".into(), "u64".into(), "test".into(),
            ])),
        },
        BenchTask {
            id: "rust-binary-search".into(),
            description: "Rust: generic binary search".into(),
            messages: vec![
                Message::system("You are a Rust expert. Output ONLY code, no explanation."),
                Message::user("Write a Rust function `fn binary_search<T: Ord>(slice: &[T], target: &T) -> Option<usize>` that returns the index of target in a sorted slice. Include tests."),
            ],
            evaluator: Box::new(KeywordEvaluator::new(vec![
                "fn binary_search".into(), "Option<usize>".into(), "Ord".into(),
            ])),
        },
        BenchTask {
            id: "rust-lru-cache".into(),
            description: "Rust: LRU cache".into(),
            messages: vec![
                Message::system("You are a Rust expert. Output ONLY code, no explanation."),
                Message::user("Implement an LRU cache in Rust with `fn new(capacity: usize)`, `fn get(&mut self, key: &str) -> Option<&str>`, and `fn put(&mut self, key: String, value: String)`. Include tests."),
            ],
            evaluator: Box::new(KeywordEvaluator::new(vec![
                "struct".into(), "fn get".into(), "fn put".into(), "capacity".into(),
            ])),
        },
    ]
}

fn python_tasks() -> Vec<BenchTask> {
    vec![
        BenchTask {
            id: "python-merge-sort".into(),
            description: "Python: merge sort".into(),
            messages: vec![
                Message::system("You are a Python expert. Output ONLY code, no explanation."),
                Message::user("Write a Python function `def merge_sort(arr: list) -> list` that sorts using merge sort. Include pytest tests."),
            ],
            evaluator: Box::new(KeywordEvaluator::new(vec![
                "def merge_sort".into(), "def test".into(), "merge".into(),
            ])),
        },
        BenchTask {
            id: "python-decorator".into(),
            description: "Python: retry decorator".into(),
            messages: vec![
                Message::system("You are a Python expert. Output ONLY code, no explanation."),
                Message::user("Write a Python decorator `@retry(max_attempts=3, delay=1.0)` that retries a function on exception. Include tests."),
            ],
            evaluator: Box::new(KeywordEvaluator::new(vec![
                "def retry".into(), "max_attempts".into(), "def test".into(),
            ])),
        },
        BenchTask {
            id: "python-async-queue".into(),
            description: "Python: async task queue".into(),
            messages: vec![
                Message::system("You are a Python expert. Output ONLY code, no explanation."),
                Message::user("Write an async task queue in Python using asyncio with `async def enqueue(task)`, `async def process()`, and `async def wait_all()`. Include tests."),
            ],
            evaluator: Box::new(KeywordEvaluator::new(vec![
                "async def".into(), "asyncio".into(), "queue".into(),
            ])),
        },
    ]
}

fn javascript_tasks() -> Vec<BenchTask> {
    vec![
        BenchTask {
            id: "js-debounce".into(),
            description: "JS: debounce function".into(),
            messages: vec![
                Message::system("You are a JavaScript expert. Output ONLY code, no explanation."),
                Message::user("Write a JavaScript debounce function: `function debounce(fn, delay)` that returns a debounced version. Include Jest tests."),
            ],
            evaluator: Box::new(KeywordEvaluator::new(vec![
                "function debounce".into(), "setTimeout".into(), "test".into(),
            ])),
        },
        BenchTask {
            id: "js-promise-pool".into(),
            description: "JS: promise pool".into(),
            messages: vec![
                Message::system("You are a JavaScript expert. Output ONLY code, no explanation."),
                Message::user("Write a JavaScript class `PromisePool` that limits concurrent promises: `constructor(concurrency)`, `async add(promiseFn)`, `async drain()`. Include tests."),
            ],
            evaluator: Box::new(KeywordEvaluator::new(vec![
                "class PromisePool".into(), "async".into(), "concurrency".into(),
            ])),
        },
        BenchTask {
            id: "ts-type-safe-event".into(),
            description: "TS: type-safe event emitter".into(),
            messages: vec![
                Message::system("You are a TypeScript expert. Output ONLY code, no explanation."),
                Message::user("Write a TypeScript type-safe event emitter: `class TypedEmitter<Events>` with `on<K extends keyof Events>(event: K, handler: Events[K])` and `emit<K>(event: K, ...args)`. Include tests."),
            ],
            evaluator: Box::new(KeywordEvaluator::new(vec![
                "class TypedEmitter".into(), "keyof".into(), "extends".into(),
            ])),
        },
    ]
}

fn go_tasks() -> Vec<BenchTask> {
    vec![
        BenchTask {
            id: "go-concurrent-map".into(),
            description: "Go: concurrent safe map".into(),
            messages: vec![
                Message::system("You are a Go expert. Output ONLY code, no explanation."),
                Message::user("Write a Go concurrent-safe generic map: `type SafeMap[K comparable, V any] struct` with `Get(key K) (V, bool)`, `Set(key K, value V)`, `Delete(key K)`. Include tests."),
            ],
            evaluator: Box::new(KeywordEvaluator::new(vec![
                "sync.RWMutex".into(), "func".into(), "SafeMap".into(),
            ])),
        },
        BenchTask {
            id: "go-worker-pool".into(),
            description: "Go: worker pool".into(),
            messages: vec![
                Message::system("You are a Go expert. Output ONLY code, no explanation."),
                Message::user("Write a Go worker pool: `func NewPool(workers int) *Pool`, `func (p *Pool) Submit(task func())`, `func (p *Pool) Wait()`. Use channels. Include tests."),
            ],
            evaluator: Box::new(KeywordEvaluator::new(vec![
                "chan".into(), "func NewPool".into(), "goroutine".into(),
            ]).with_threshold(0.4)),
        },
        BenchTask {
            id: "go-http-middleware".into(),
            description: "Go: HTTP middleware chain".into(),
            messages: vec![
                Message::system("You are a Go expert. Output ONLY code, no explanation."),
                Message::user("Write a Go HTTP middleware chain: `type Middleware func(http.Handler) http.Handler`, `func Chain(middlewares ...Middleware) Middleware`, and example logging/auth middlewares. Include tests."),
            ],
            evaluator: Box::new(KeywordEvaluator::new(vec![
                "Middleware".into(), "http.Handler".into(), "func Chain".into(),
            ])),
        },
    ]
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info,selfware::bench_harness=info")
        .init();

    let endpoint = std::env::var("SELFWARE_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:8000/v1".to_string());
    let model = std::env::var("SELFWARE_MODEL")
        .unwrap_or_else(|_| "qwen3.5-27b".to_string());
    let concurrent: usize = std::env::var("SELFWARE_CONCURRENT")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(12);

    let config = HarnessConfig {
        endpoint: endpoint.clone(),
        model: model.clone(),
        max_concurrent: concurrent,
        max_tokens: 2048,
        temperature: 0.3,
        timeout_secs: 120,
        output_dir: "bench_results/multilang".into(),
        extra_body: serde_json::json!({"chat_template_kwargs": {"enable_thinking": false}}),
    };

    let runner = HarnessRunner::new(config)?;

    let mut all_tasks = Vec::new();
    all_tasks.extend(rust_tasks());
    all_tasks.extend(python_tasks());
    all_tasks.extend(javascript_tasks());
    all_tasks.extend(go_tasks());

    eprintln!("\n=== Multi-Language Coding Benchmark ===");
    eprintln!("Endpoint: {endpoint}");
    eprintln!("Model: {model}");
    eprintln!("Tasks: {} (Rust={}, Python={}, JS/TS={}, Go={})",
        all_tasks.len(), 3, 3, 3, 3);
    eprintln!("Concurrent: {concurrent}");
    eprintln!();

    let report = runner.run(all_tasks).await?;

    // Group results by language
    let mut by_lang: std::collections::HashMap<&str, Vec<&StreamResult>> = std::collections::HashMap::new();
    for r in &report.results {
        let lang = if r.task_id.starts_with("rust") { "Rust" }
            else if r.task_id.starts_with("python") { "Python" }
            else if r.task_id.starts_with("js") || r.task_id.starts_with("ts") { "JS/TS" }
            else { "Go" };
        by_lang.entry(lang).or_default().push(r);
    }

    eprintln!("\n{}", "=".repeat(60));
    eprintln!("MULTI-LANGUAGE RESULTS — {model}");
    eprintln!("{}", "=".repeat(60));

    for lang in &["Rust", "Python", "JS/TS", "Go"] {
        if let Some(results) = by_lang.get(lang) {
            let passed = results.iter().filter(|r| r.success).count();
            let avg_score: f64 = results.iter()
                .filter_map(|r| r.eval.as_ref().map(|e| e.score))
                .sum::<f64>() / results.len() as f64;
            eprintln!("\n  {} — {}/{} passed, {:.0}% avg", lang, passed, results.len(), avg_score * 100.0);
            for r in results {
                let score = r.eval.as_ref().map(|e| format!("{:.0}%", e.score * 100.0)).unwrap_or("ERR".into());
                let status = if r.success { "+" } else { "-" };
                eprintln!("    {} {} {:>6} {:>6}tok {:>6.1}s",
                    status, r.task_id, score, r.completion_tokens, r.latency_ms as f64 / 1000.0);
            }
        }
    }

    eprintln!("\n  Overall: {}/{} passed, {:.0}% avg, {:.0} tok/s, {:.1}s",
        report.tasks_passed, report.tasks_total,
        report.avg_score * 100.0, report.tokens_per_sec, report.total_duration_secs);
    eprintln!("{}", "=".repeat(60));

    report.write_to_dir(std::path::Path::new("bench_results/multilang"))?;

    Ok(())
}
