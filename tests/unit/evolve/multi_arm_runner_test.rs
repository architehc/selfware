use super::*;
use std::path::PathBuf;

fn arm(id: &str, name: &str, source: &str) -> OptimizationArm {
    OptimizationArm {
        arm_id: id.to_string(),
        name: name.to_string(),
        description: String::new(),
        target_file: "src/algo.rs".to_string(),
        proposed_source: source.to_string(),
    }
}

const VALID: &str = "pub fn optimized() -> usize { 42 }";

fn measured(passed: usize, failed: usize, samples: &[u64]) -> ArmMeasurement {
    ArmMeasurement {
        compiled: true,
        tests_passed: passed,
        tests_failed: failed,
        bench_samples_ns: samples.to_vec(),
        ..Default::default()
    }
}

fn baseline() -> ArmMeasurement {
    measured(10, 0, &[100, 105, 110])
}

fn runner(k: usize) -> MultiArmEvolutionRunner {
    MultiArmEvolutionRunner::new(k, PathBuf::from("/tmp/sw-evolve-arms"))
}

#[test]
fn test_multi_arm_consensus_reached() {
    let runner = runner(2);
    assert_eq!(runner.consensus_threshold(), 2);
    assert_eq!(runner.scratch_root(), Path::new("/tmp/sw-evolve-arms"));

    let arms = vec![
        arm("arm-1", "Loop Unrolling", VALID),
        arm("arm-2", "SIMD Vectorization", VALID),
        arm("arm-3", "Lookup Table", VALID),
        arm("arm-4", "Broken Build", VALID),
    ];
    let report = runner.evaluate_arms(&baseline(), &arms, |a| {
        Ok(match a.arm_id.as_str() {
            "arm-1" => measured(10, 0, &[80, 82, 85]),
            "arm-2" => measured(10, 0, &[60, 61, 62]),
            "arm-3" => measured(9, 1, &[30, 31, 32]),
            "arm-4" => ArmMeasurement {
                compiled: false,
                error: Some("cargo check: 2 error(s)".into()),
                ..Default::default()
            },
            _ => unreachable!(),
        })
    });

    assert_eq!(report.total_arms, 4);
    assert_eq!(report.passing_arms, 2);
    assert!(report.consensus_met && report.fitness_measured);
    let winner = report.winning_arm.as_ref().expect("winning arm must exist");
    assert_eq!(winner.arm_id, "arm-2");
    assert_eq!(winner.median_ns, Some(61));
    let delta = winner.fitness_delta.unwrap();
    assert!((delta - (105.0 - 61.0) / 105.0).abs() < 1e-9);

    // A failing test is a test regression of a build that DID compile.
    let e3 = &report.evaluations[2];
    assert!(e3.compile_success && !e3.passes_consensus_gate);
    assert_eq!(e3.test_regressions, 2, "one extra failure + one lost pass");
    assert!(e3.verdict.contains("regression"));
    // A compile failure is reported as such, with no test numbers.
    let e4 = &report.evaluations[3];
    assert!(!e4.compile_success);
    assert!(e4.verdict.contains("did not compile"));

    let md = report.to_pr_markdown();
    assert!(md.contains("Replicate consensus met (2 of 4 arms passed; 2 required)"));
    assert!(md.contains("SIMD Vectorization"));
    assert!(md.contains("41.90% lower median wall time"), "{md}");
    assert!(
        md.contains("| `arm-3` | Lookup Table | yes | yes | 9/10 | 2 |"),
        "{md}"
    );
    assert!(
        md.contains("| `arm-4` | Broken Build | yes | no | not run | - |"),
        "{md}"
    );
}

#[test]
fn test_multi_arm_consensus_failed_rollback() {
    let runner = runner(2);
    let arms = vec![
        arm("arm-1", "Syntax Error Arm", "fn broken {"),
        arm("arm-2", "Regressing Arm", "pub fn f() {}"),
    ];
    let report = runner.evaluate_arms(&baseline(), &arms, |a| match a.arm_id.as_str() {
        "arm-2" => Ok(measured(5, 5, &[50, 50, 50])),
        _ => unreachable!("syntax error arm should not be measured"),
    });
    assert_eq!(report.passing_arms, 0);
    assert!(!report.consensus_met && report.winning_arm.is_none());
    let syntax = &report.evaluations[0];
    assert!(!syntax.syntax_valid);
    assert!(syntax.syntax_error.as_ref().unwrap().line.is_some());

    let md = report.to_pr_markdown();
    assert!(md.contains("Replicate consensus was NOT met (0 of 2 arms passed; 2 required)"));
    assert!(
        md.contains("| `arm-1` | Syntax Error Arm | no | not run | not run | - |"),
        "{md}"
    );
}

#[test]
fn fitness_is_not_inflated_by_the_number_of_tests() {
    // The WIP scored throughput × (tests + 1): adding tests "improved" an arm.
    let arms = vec![
        arm("more-tests", "Adds 40 tests, same speed", VALID),
        arm("faster", "Faster, same tests", VALID),
    ];
    let report = runner(1).evaluate_arms(&baseline(), &arms, |a| {
        Ok(match a.arm_id.as_str() {
            "more-tests" => measured(50, 0, &[100, 105, 110]),
            _ => measured(10, 0, &[70, 71, 72]),
        })
    });
    let more = &report.evaluations[0];
    assert!(!more.passes_consensus_gate);
    assert_eq!(more.fitness_delta, Some(0.0));
    assert_eq!(report.winning_arm.unwrap().arm_id, "faster");
}

#[test]
fn overlapping_samples_do_not_pass_on_a_better_median_alone() {
    let report = runner(1).evaluate_arms(&baseline(), &[arm("a", "noisy", VALID)], |_| {
        Ok(measured(10, 0, &[90, 95, 120]))
    });
    let e = &report.evaluations[0];
    assert!(e.fitness_delta.unwrap() > 0.0);
    assert!(!e.separated_from_baseline && !e.passes_consensus_gate);
    assert!(e.verdict.contains("within noise"));
}

#[test]
fn no_benchmark_means_no_fitness_and_no_promotion() {
    let base = measured(10, 0, &[]);
    let report = runner(1).evaluate_arms(&base, &[arm("a", "untimed", VALID)], |_| {
        Ok(measured(10, 0, &[]))
    });
    assert!(!report.fitness_measured && !report.consensus_met);
    assert_eq!(report.evaluations[0].fitness_delta, None);
    let md = report.to_pr_markdown();
    assert!(md.contains("No benchmark command was configured"));
    assert!(!md.contains("NaN") && !md.contains("inf"), "{md}");
}

#[test]
fn a_zero_baseline_median_never_divides_by_zero() {
    let base = measured(10, 0, &[0, 0, 0]);
    let report = runner(1).evaluate_arms(&base, &[arm("a", "x", VALID)], |_| {
        Ok(measured(10, 0, &[0, 0, 0]))
    });
    let e = &report.evaluations[0];
    assert_eq!(e.fitness_delta, None);
    assert!(!e.passes_consensus_gate);
    let md = report.to_pr_markdown();
    assert!(!md.contains("NaN") && !md.contains("inf"), "{md}");
}

#[test]
fn threshold_zero_never_promotes_and_ties_pick_a_winner_without_panicking() {
    let arms = vec![arm("a", "a", VALID), arm("b", "b", VALID)];
    let same = |_: &OptimizationArm| Ok(measured(10, 0, &[50, 50, 50]));
    let zero = runner(0).evaluate_arms(&baseline(), &arms, same);
    assert_eq!(zero.passing_arms, 2);
    assert!(!zero.consensus_met, "E4 requires k > 0");
    let tie = runner(2).evaluate_arms(&baseline(), &arms, same);
    assert!(tie.winning_arm.is_some());
}

#[test]
fn unsafe_or_protected_targets_are_rejected_before_measuring() {
    let mut protected = arm("p", "edits the safety module", VALID);
    protected.target_file = "src/safety/checker/mod.rs".into();
    let mut escape = arm("e", "leaves the tree", VALID);
    escape.target_file = "../outside.rs".into();
    let mut bad_id = arm("../x", "bad id", VALID);
    bad_id.target_file = "src/algo.rs".into();
    let report = runner(1).evaluate_arms(&baseline(), &[protected, escape, bad_id], |_| {
        unreachable!("rejected arms are never measured")
    });
    let reasons: Vec<&str> = report
        .evaluations
        .iter()
        .map(|e| e.rejected.as_deref().unwrap())
        .collect();
    assert!(reasons[0].contains("protected"));
    assert!(reasons[1].contains("project-relative"));
    assert!(reasons[2].contains("arm id"));
}

#[test]
fn arms_files_written_for_the_wip_format_still_load() {
    let json = r#"[{"arm_id":"a","name":"n","description":"d","target_file":"src/x.rs","proposed_patch":"fn f() {}"}]"#;
    let arms: Vec<OptimizationArm> = serde_json::from_str(json).unwrap();
    assert_eq!(arms[0].proposed_source, "fn f() {}");
}

/// Conformance with formal/EvolutionBounds.lean E4: an arm passes iff it
/// has fitnessDelta > 0 and 0 test regressions (the runner additionally
/// requires the samples to separate from the baseline's, which only makes
/// passing stricter), and consensus_met ⇒ at least one passing arm
/// (`consensus_requires_passing_arms`).
#[test]
fn scoring_conforms_to_evolution_bounds_e4() {
    let sample_sets: [&[u64]; 4] = [
        &[50, 55, 60],
        &[100, 105, 110],
        &[150, 160, 170],
        &[95, 104, 130],
    ];
    let test_sets = [(10, 0), (9, 1), (12, 0), (10, 2)];
    let mut arms = Vec::new();
    let mut table = Vec::new();
    for (i, s) in sample_sets.iter().enumerate() {
        for (j, t) in test_sets.iter().enumerate() {
            let id = format!("a{i}{j}");
            arms.push(arm(&id, &id, VALID));
            table.push((id, measured(t.0, t.1, s)));
        }
    }
    for k in 0..=arms.len() + 1 {
        let report = runner(k).evaluate_arms(&baseline(), &arms, |a| {
            Ok(table
                .iter()
                .find(|(id, _)| *id == a.arm_id)
                .unwrap()
                .1
                .clone())
        });
        for e in &report.evaluations {
            let lean_passing = e.fitness_delta.is_some_and(|d| d > 0.0) && e.test_regressions == 0;
            assert!(!e.passes_consensus_gate || lean_passing, "{}", e.arm_id);
        }
        let passing = report
            .evaluations
            .iter()
            .filter(|e| e.passes_consensus_gate)
            .count();
        assert_eq!(report.passing_arms, passing);
        assert_eq!(report.consensus_met, k > 0 && passing >= k);
        if report.consensus_met {
            assert!(report.passing_arms > 0);
            assert!(report.winning_arm.as_ref().unwrap().passes_consensus_gate);
        }
    }
}

#[test]
fn median_is_the_lower_middle_sample() {
    assert_eq!(median_ns(&[]), None);
    assert_eq!(median_ns(&[3, 1, 2]), Some(2));
    assert_eq!(median_ns(&[4, 1, 3, 2]), Some(2));
}

// --- quarantined end-to-end run (real cargo, everything in a TempDir) ---

#[cfg(unix)]
mod quarantined {
    use super::*;
    use crate::safety::quarantine::{HostToolchain, QuarantineOptions, RegistryMode};
    use std::ffi::OsString;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    fn lib_rs(work_ms: u64, test_ok: bool) -> String {
        format!(
            "pub const WORK_MS: u64 = {work_ms};\n\
             pub fn work() {{ std::thread::sleep(std::time::Duration::from_millis(WORK_MS)); }}\n\
             pub fn take(v: &String) -> usize {{ v.len() }}\n\
             #[test]\nfn work_is_bounded() {{ assert!({}); }}\n",
            if test_ok {
                "WORK_MS < 1000"
            } else {
                "WORK_MS > 1000"
            }
        )
    }

    /// Everything in `tmp`: the project repo, a stand-in host home, the
    /// scratch root. The only real host path is the toolchain sysroot.
    fn setup(tmp: &Path) -> (PathBuf, HostToolchain) {
        let repo = tmp.join("repo");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::create_dir_all(repo.join("examples")).unwrap();
        std::fs::write(
            repo.join("Cargo.toml"),
            "[package]\nname = \"cand\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
        )
        .unwrap();
        std::fs::write(repo.join("src/lib.rs"), lib_rs(400, true)).unwrap();
        std::fs::write(
            repo.join("examples/bench.rs"),
            "fn main() { cand::work(); }\n",
        )
        .unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["add", "-A"]);
        git(
            &repo,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                "base",
            ],
        );
        let fake_home = tmp.join("host-home");
        std::fs::create_dir_all(fake_home.join(".cargo")).unwrap();
        let out = Command::new("rustc")
            .args(["--print", "sysroot"])
            .output()
            .unwrap();
        let sysroot = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
        let host = HostToolchain {
            home: fake_home.clone(),
            cargo_home: fake_home.join(".cargo"),
            sysroot,
            parent_env: vec![
                ("PATH".into(), std::env::var_os("PATH").unwrap_or_default()),
                ("HOME".into(), OsString::from(&fake_home)),
                ("SELFWARE_TEST_CANARY".into(), "canary-not-a-secret".into()),
            ],
        };
        (repo, host)
    }

    #[tokio::test]
    async fn arms_are_built_tested_and_benchmarked_in_quarantined_snapshots() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, host) = setup(tmp.path());
        let scratch = tmp.path().join("scratch");
        let lib_arm = |id: &str, source: String| OptimizationArm {
            arm_id: id.into(),
            name: id.into(),
            description: String::new(),
            target_file: "src/lib.rs".into(),
            proposed_source: source,
        };
        // Needs rustc's machine-applicable `&` to compile.
        let needs_repair =
            lib_rs(200, true) + "pub fn uses_take() -> usize { let s = String::new(); take(s) }\n";
        let arms = vec![
            lib_arm("fast", lib_rs(20, true)),
            lib_arm("repaired", needs_repair),
            lib_arm("regresses", lib_rs(20, false)),
            lib_arm("syntax", "pub fn broken( {".into()),
        ];
        let config = MultiArmConfig {
            consensus_threshold: 2,
            replicates: 2,
            bench_command: Some(
                ["cargo", "run", "--quiet", "--offline", "--example", "bench"]
                    .map(String::from)
                    .to_vec(),
            ),
            parallel: 2,
            step_timeout: Duration::from_secs(300),
            quarantine: QuarantineOptions {
                registry: RegistryMode::Empty,
                sandbox: false,
            },
            ..Default::default()
        };
        let report = MultiArmEvolutionRunner::new(2, &scratch)
            .run_quarantined(&repo, "HEAD", &arms, &host, &config)
            .await
            .unwrap();
        let by_id = |id: &str| report.evaluations.iter().find(|e| e.arm_id == id).unwrap();

        assert!(report.baseline.measurement.compiled);
        assert_eq!(report.baseline.measurement.tests_passed, 1);
        assert_eq!(report.baseline.measurement.bench_samples_ns.len(), 2);

        let fast = by_id("fast");
        assert!(fast.passes_consensus_gate, "{}", fast.verdict);
        assert_eq!(fast.bench_samples_ns.len(), 2);

        let repaired = by_id("repaired");
        assert!(repaired.compile_success, "{:?}", repaired.error);
        assert_eq!(repaired.repair_rounds, 1);
        assert!(repaired.diff.as_deref().unwrap().contains("take(&s)"));
        assert!(repaired.passes_consensus_gate, "{}", repaired.verdict);

        let regresses = by_id("regresses");
        assert!(regresses.compile_success && !regresses.passes_consensus_gate);
        assert_eq!(regresses.test_fail_count, 1);

        assert!(!by_id("syntax").syntax_valid);

        assert!(report.consensus_met);
        assert_eq!(report.winning_arm.as_ref().unwrap().arm_id, "fast");
        assert!(report
            .isolation
            .as_ref()
            .unwrap()
            .filesystem
            .contains("NOT sandboxed"));

        // The operator's repository and stand-in home were not touched, and
        // the run directory was removed.
        assert!(!repo.join("target").exists());
        let status = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&repo)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(status.stdout.is_empty());
        assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 0);
        assert_eq!(
            std::fs::read_dir(&host.home).unwrap().count(),
            1,
            "only .cargo"
        );
    }
}
