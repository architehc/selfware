#!/usr/bin/env python3
"""Unit and integration tests for scripts/phi_tmux_game.py and scripts/phi_tmux_arena.sh."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent.parent
REPO_ROOT = SCRIPTS_DIR.parent
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

import phi_tmux_game as game  # noqa: E402


class PhiTmuxGameTests(unittest.TestCase):
    def test_all_ten_visemes_reachable(self) -> None:
        samples = {
            "rest": "...",
            "mbp": "module",
            "fv": "fox",
            "l_th": "laser",
            "wq": "quick",
            "ai": "cairn",
            "e": "test",
            "o": "code",
            "u": "rust",
            "etc": "sync",
        }
        seen = {viseme: game.word_to_viseme(word) for viseme, word in samples.items()}
        for expected, actual in seen.items():
            self.assertEqual(expected, actual, f"word_to_viseme mismatch for {expected}")
            self.assertIn(actual, game.VISEME_MOUTH_GLYPHS)

        frames = game.text_to_viseme_frames("module fox laser quick cairn test code rust sync")
        self.assertGreaterEqual(len(frames), 10)
        self.assertEqual(frames[-1][0], "rest")

    def test_read_workspace_activity_no_capture_and_valid_capture(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            ws = Path(tmp).resolve()
            now_ms = int(time.time() * 1000)

            empty = game.read_workspace_activity(ws, now_ms=now_ms)
            self.assertEqual(empty["status"], "unavailable")
            self.assertEqual(empty["reason"], "no_capture")

            act_dir = ws / ".selfware" / "phi" / "activity"
            act_dir.mkdir(parents=True)
            capture = {
                "schema_version": 1,
                "workspace_root": str(ws),
                "session_id": "sess-alpha",
                "agent_id": "agent-1",
                "task_id": "task-1",
                "recorded_at_ms": now_ms - 1000,
                "phase": "running",
                "observation_truncated": False,
                "evidence": {
                    "outstanding": 1,
                    "unreviewed_lines": 40,
                    "untested_lines": 20,
                    "unknown_size_obligations": 0,
                    "unattributed": [],
                    "possible_unrecorded_mutations": 0,
                    "observations": [
                        {"kind": "run_finished", "outcome": "Passed"},
                    ],
                },
            }
            (act_dir / "agent1.json").write_text(json.dumps(capture), encoding="utf-8")

            res = game.read_workspace_activity(ws, now_ms=now_ms)
            self.assertEqual(res["status"], "available")
            self.assertEqual(len(res["agents"]), 1)
            agent = res["agents"][0]
            self.assertNotEqual(agent["agent_id"], "agent-1")
            self.assertEqual(agent["fox_posture"]["emotion"], "skeptical")
            self.assertEqual(agent["evidence"]["unreviewed_lines"], 40)
            self.assertEqual(agent["evidence"]["untested_lines"], 20)

    def test_read_workspace_activity_rejects_symlinks_and_hardlinks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            ws = Path(tmp).resolve()
            now_ms = int(time.time() * 1000)
            phi_dir = ws / ".selfware" / "phi"
            phi_dir.mkdir(parents=True)
            outside = ws / "outside_dir"
            outside.mkdir()
            os.symlink(outside, phi_dir / "activity")

            symlink_dir_res = game.read_workspace_activity(ws, now_ms=now_ms)
            self.assertEqual(symlink_dir_res["status"], "unavailable")
            self.assertEqual(symlink_dir_res["reason"], "unsafe_capture")

            os.unlink(phi_dir / "activity")
            act_dir = phi_dir / "activity"
            act_dir.mkdir()

            target_file = ws / "secret.json"
            target_file.write_text("{}", encoding="utf-8")
            os.link(target_file, act_dir / "hardlinked.json")

            hardlink_res = game.read_workspace_activity(ws, now_ms=now_ms)
            self.assertEqual(hardlink_res["status"], "incomplete")
            self.assertEqual(hardlink_res["reason"], "invalid_capture")

    def test_stale_and_future_captures_handled_honestly(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            ws = Path(tmp).resolve()
            now_ms = int(time.time() * 1000)
            act_dir = ws / ".selfware" / "phi" / "activity"
            act_dir.mkdir(parents=True)

            stale_capture = {
                "schema_version": 1,
                "workspace_root": str(ws),
                "session_id": "sess-1",
                "agent_id": "agent-stale",
                "task_id": "task-1",
                "recorded_at_ms": now_ms - (game.STALE_MS + 10_000),
                "phase": "running",
                "evidence": {"observations": []},
            }
            future_capture = {
                "schema_version": 1,
                "workspace_root": str(ws),
                "session_id": "sess-1",
                "agent_id": "agent-future",
                "task_id": "task-2",
                "recorded_at_ms": now_ms + 30_000,
                "phase": "running",
                "evidence": {"observations": []},
            }
            (act_dir / "stale.json").write_text(json.dumps(stale_capture), encoding="utf-8")
            (act_dir / "future.json").write_text(json.dumps(future_capture), encoding="utf-8")

            res = game.read_workspace_activity(ws, now_ms=now_ms)
            by_status = {a["status"]: a["reason"] for a in res["agents"]}
            self.assertEqual(by_status.get("stale"), "capture_expired")
            self.assertEqual(by_status.get("incomplete"), "capture_in_future")

    def test_debt_boss_hp_drain_and_xp_progression(self) -> None:
        high_debt_activity = {
            "status": "available",
            "reason": None,
            "agents": [
                {
                    "status": "available",
                    "phase": "running",
                    "evidence": {
                        "unreviewed_lines": 80,
                        "untested_lines": 35,
                        "outstanding": 2,
                        "failed_runs": 1,
                        "unknown_runs": 0,
                        "unknown_size_obligations": 0,
                        "unattributed_mutations": 0,
                    },
                }
            ],
        }
        boss1 = game.compute_debt_boss_state(high_debt_activity, peak_hp=100, xp=0, streak=1.0)
        # 80*1 + 35*2 + 2*25 + 1*50 = 250 HP
        self.assertEqual(boss1.current_hp, 250)
        self.assertEqual(boss1.max_hp, 250)
        self.assertIn("Regression Hydra", boss1.boss_name)
        self.assertEqual(boss1.trust_state, "observed_signals")

        cleared_activity = {
            "status": "available",
            "reason": None,
            "agents": [
                {
                    "status": "available",
                    "phase": "completed",
                    "evidence": {
                        "unreviewed_lines": 0,
                        "untested_lines": 0,
                        "outstanding": 0,
                        "failed_runs": 0,
                        "unknown_runs": 0,
                        "unknown_size_obligations": 0,
                        "unattributed_mutations": 0,
                    },
                }
            ],
        }
        boss2 = game.compute_debt_boss_state(
            cleared_activity,
            peak_hp=boss1.max_hp,
            xp=boss1.xp,
            streak=boss1.streak,
        )
        self.assertEqual(boss2.current_hp, 0)
        self.assertEqual(boss2.xp, 250)
        self.assertGreaterEqual(boss2.level, 3)
        self.assertGreater(boss2.streak, 1.0)
        self.assertIn("Arena Clear", boss2.boss_name)

    def test_render_cockpit_frame_and_cli_modes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            ws = Path(tmp).resolve()
            (ws / "src").mkdir()
            (ws / "src" / "lib.rs").write_text("pub fn demo_target() {}\n", encoding="utf-8")

            app = game.PhiTmuxArenaApp(ws)
            app.refresh()
            app.handle_intent("hunt", "Hunt target")
            frame = app.step_frame()
            self.assertIn("PHI FOX TMUX COCKPIT & DEBT SLAYER", frame)
            self.assertIn("TRUST: OBSERVED_SIGNALS", frame)
            self.assertIn("src/lib.rs:1", frame)

            proc = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPTS_DIR / "phi_tmux_game.py"),
                    "--workspace",
                    str(ws),
                    "--intent",
                    "check_debt",
                    "--json",
                ],
                capture_output=True,
                text=True,
                check=True,
                timeout=10,
            )
            payload = json.loads(proc.stdout)
            self.assertEqual(payload["boss"]["trust_state"], "observed_signals")
            self.assertTrue(len(payload["chat_turns"]) >= 2)

    def test_tmux_arena_launcher_dry_run(self) -> None:
        arena_sh = SCRIPTS_DIR / "phi_tmux_arena.sh"
        proc = subprocess.run(
            ["bash", str(arena_sh), "--dry-run", "--session", "test-phi-cockpit"],
            capture_output=True,
            text=True,
            check=True,
            timeout=10,
        )
        self.assertIn("session_name=test-phi-cockpit", proc.stdout)
        self.assertIn("phi_tmux_game.py", proc.stdout)
        self.assertIn("observed_signals", proc.stdout)

    def test_boss_battle_drills_achievements_and_profile_persistence(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            ws = Path(tmp).resolve()
            (ws / "src").mkdir()
            (ws / "src" / "alpha.rs").write_text("pub fn alpha_sentinel_gate() {}\n", encoding="utf-8")
            (ws / "src" / "beta.rs").write_text("pub struct BetaLedger {}\n", encoding="utf-8")
            (ws / "src" / "gamma.rs").write_text("pub enum GammaOutcome { Observed }\n", encoding="utf-8")

            app = game.PhiTmuxArenaApp(ws)
            app.refresh()
            for _ in range(3):
                drill = app.start_boss_drill()
                hit = app.answer_boss_drill(drill["correct_index"])
                self.assertTrue(hit)

            self.assertEqual(app.profile["drills_completed"], 3)
            self.assertGreaterEqual(app.profile["xp"], 105)
            self.assertIn("first_laser_lock", app.profile["achievements"])
            self.assertIn("debt_slayer_i", app.profile["achievements"])
            self.assertIn("golden_spiral", app.profile["achievements"])
            self.assertIn("drill_master", app.profile["achievements"])
            self.assertIsNotNone(app.code_preview)

            reloaded = game.load_arena_profile(ws)
            self.assertEqual(reloaded["drills_completed"], 3)

            # Reject path traversal outside workspace_root
            self.assertIsNone(game.inspect_citation_preview(ws, "../outside.rs:1"))

            # Reject symlinked arena_profile.json
            target = ws / ".selfware" / "phi" / "arena_profile.json"
            target.unlink()
            outside = ws / "outside.json"
            outside.write_text("{}", encoding="utf-8")
            os.symlink(outside, target)
            self.assertFalse(game.save_arena_profile(ws, app.profile))
            self.assertEqual(game.load_arena_profile(ws)["xp"], 0)


if __name__ == "__main__":
    unittest.main()
