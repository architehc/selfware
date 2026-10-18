#!/usr/bin/env python3
"""phi_tmux_game.py — Phi Fox Tmux Cockpit, Debt Slayer Game HUD & Terminal Bot.

Brings Phi the Fox (Φ) into tmux / terminal sessions with:
  1. Animated ANSI + Braille Phi Fox sprite with real-time mood/posture
     transitions driven by `.selfware/phi/activity` and `PhiPolicy`.
  2. Gamified "Verification Debt / Boss HP" bar & Grounded XP streak counter
     that drains Boss HP as unreviewed/untested lines and failed runs are cleared.
  3. 10-viseme terminal mouth animation (`rest`, `mbp`, `etc`, `ai`, `e`, `o`,
     `u`, `fv`, `l_th`, `wq`) synchronized with bot replies.
  4. Interactive hotkeys (`[r]` What's red, `[d]` Debt check, `[h]` Hunt target,
     `[g]` God mode, `[c]` Chat with `/api/phi/chat`, `[o]` Jump tmux pane to
     cited `path:line`, `[q]` Quit).

Honest status contract (AGENTS.md Rule 3):
  - Captures from `.selfware/phi/activity` are reported as `observed_signals`.
  - Absence of captures (`no_capture`) or stale captures (`stale`) never renders
    as "verified" or "all tests passing".
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import select
import shutil
import stat
import subprocess
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable, Dict, List, Optional, Tuple

MAX_SCAN = 256
MAX_AGENTS = 64
MAX_FILE_BYTES = 256 * 1024
MAX_TOTAL_BYTES = 4 * 1024 * 1024
STALE_MS = 5 * 60 * 1000
RETENTION_SECS = 24 * 60 * 60

VISEME_MOUTH_GLYPHS: Dict[str, str] = {
    "rest": "───",
    "mbp": "═══",
    "etc": "─▫─",
    "ai": "◁◯▷",
    "e": "◁═▷",
    "o": "(◯)",
    "u": "(▫)",
    "fv": "─▿─",
    "l_th": "─△─",
    "wq": "∘▫∘",
}

VISEME_SEQUENCE: Tuple[str, ...] = (
    "rest", "mbp", "etc", "ai", "e", "o", "u", "fv", "l_th", "wq"
)

EMOTION_THEMES: Dict[str, Dict[str, str]] = {
    "curious": {"color": "\033[38;5;214m", "badge": "CURIOUS · EARS PERKED", "eyes": "o . o", "ears": "/\\_/\\"},
    "working": {"color": "\033[38;5;220m", "badge": "WORKING · TRACKING RUN", "eyes": "◉ . ◉", "ears": "/\\_/\\"},
    "analytical": {"color": "\033[38;5;117m", "badge": "ANALYTICAL · LASER LOCK", "eyes": "◈ _ ◉", "ears": "/\\_/\\"},
    "skeptical": {"color": "\033[38;5;208m", "badge": "SKEPTICAL · DEBT RISING", "eyes": "¬ . o", "ears": "/\\_/˜"},
    "guard": {"color": "\033[38;5;178m", "badge": "GUARD · UNCERTAIN EVIDENCE", "eyes": "Ѻ . Ѻ", "ears": "/\\_/\\"},
    "error": {"color": "\033[38;5;203m", "badge": "ALERT · RED CHECKS OBSERVED", "eyes": "> . <", "ears": "\\_/_/"},
    "idle": {"color": "\033[38;5;180m", "badge": "IDLE · CALM WATCH", "eyes": "- . -", "ears": "/\\_/\\"},
    "sleep": {"color": "\033[38;5;103m", "badge": "SLEEP · QUIET PERCH", "eyes": "u . u", "ears": "v\\_/v"},
    "god_mode": {"color": "\033[38;5;51m", "badge": "✧ GOD MODE · GOLDEN SPIRAL ✧", "eyes": "✦ . ✦", "ears": "/\\varphi/\\"},
}

BRAILLE_SPARKS: Tuple[str, ...] = ("⠁", "⠂", "⠄", "⡀", "⢀", "⠠", "⠐", "⠈", "✦", "✧", "φ")


@dataclass
class PolicyConfig:
    mode: str = "advise"
    debt_gate_lines: int = 250
    bulk_generation_lines: int = 500
    capitulation_threshold: int = 2
    interrupt_budget: int = 3


@dataclass
class DebtBossState:
    boss_name: str
    current_hp: int
    max_hp: int
    debt_ratio: float
    level: int
    xp: int
    streak: float
    status_label: str
    trust_state: str
    quests: List[str] = field(default_factory=list)


def word_to_viseme(word: str) -> str:
    cleaned = re.sub(r"[^a-zA-Z]", "", word).lower()
    if not cleaned:
        return "rest"
    if cleaned.startswith(("m", "b", "p")):
        return "mbp"
    if cleaned.startswith(("f", "v")):
        return "fv"
    if cleaned.startswith(("l", "th")):
        return "l_th"
    if cleaned.startswith(("w", "qu", "q")):
        return "wq"
    for ch in cleaned:
        if ch == "a":
            return "ai"
        if ch == "e":
            return "e"
        if ch == "o":
            return "o"
        if ch == "u":
            return "u"
    return "etc"


def text_to_viseme_frames(text: str, max_frames: int = 24) -> List[Tuple[str, str]]:
    words = [w for w in re.split(r"\s+", text.strip()) if w]
    if not words:
        return [("rest", "")]
    frames: List[Tuple[str, str]] = []
    for word in words[: max(1, max_frames - 1)]:
        frames.append((word_to_viseme(word), word))
    frames.append(("rest", ""))
    return frames


def _opaque_id(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()[:24]


def read_workspace_activity(workspace_root: Path, now_ms: Optional[int] = None) -> Dict[str, Any]:
    """Read `.selfware/phi/activity` with symlink/hardlink/size guards matching phi_activity.rs."""
    now = int(now_ms if now_ms is not None else time.time() * 1000)
    try:
        canon_root = workspace_root.resolve(strict=True)
    except Exception:
        return {
            "status": "unavailable",
            "reason": "workspace_unavailable",
            "observed_at_ms": now,
            "agents": [],
            "truncated": False,
        }

    rel_parts = (".selfware", "phi", "activity")
    curr = canon_root
    for part in rel_parts:
        curr = curr / part
        try:
            st = os.lstat(curr)
        except FileNotFoundError:
            return {
                "status": "unavailable",
                "reason": "no_capture",
                "observed_at_ms": now,
                "agents": [],
                "truncated": False,
            }
        except OSError:
            return {
                "status": "unavailable",
                "reason": "unsafe_capture",
                "observed_at_ms": now,
                "agents": [],
                "truncated": False,
            }
        if stat.S_ISLNK(st.st_mode) or not stat.S_ISDIR(st.st_mode):
            return {
                "status": "unavailable",
                "reason": "unsafe_capture",
                "observed_at_ms": now,
                "agents": [],
                "truncated": False,
            }

    try:
        entries = list(os.scandir(curr))
    except OSError:
        return {
            "status": "unavailable",
            "reason": "capture_read_failed",
            "observed_at_ms": now,
            "agents": [],
            "truncated": False,
        }

    json_candidates: List[Tuple[float, str]] = []
    rejected = False
    truncated = False
    now_sec = time.time()

    for entry in entries:
        name = entry.name
        if name.startswith(".tmp-") or not name.endswith(".json"):
            continue
        try:
            st = entry.stat(follow_symlinks=False)
        except OSError:
            rejected = True
            continue
        json_candidates.append((st.st_mtime, name))

    if len(json_candidates) > MAX_SCAN:
        truncated = True
    json_candidates.sort(key=lambda item: item[0], reverse=True)
    json_candidates = json_candidates[:MAX_SCAN]
    json_candidates = [
        (mtime, name)
        for (mtime, name) in json_candidates
        if (now_sec - mtime) <= RETENTION_SECS
    ]

    agents: List[Dict[str, Any]] = []
    total_bytes = 0
    open_flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)

    for _, name in json_candidates:
        file_path = curr / name
        try:
            fd = os.open(file_path, open_flags)
        except FileNotFoundError:
            continue
        except OSError:
            rejected = True
            continue
        try:
            st = os.fstat(fd)
            if st.st_nlink == 0:
                continue
            if not stat.S_ISREG(st.st_mode) or st.st_nlink != 1 or st.st_size > MAX_FILE_BYTES:
                rejected = True
                continue
            if total_bytes >= MAX_TOTAL_BYTES:
                truncated = True
                break
            raw = os.read(fd, min(MAX_FILE_BYTES + 1, MAX_TOTAL_BYTES - total_bytes))
            total_bytes += len(raw)
            if len(raw) > MAX_FILE_BYTES:
                rejected = True
                continue
            capture = json.loads(raw.decode("utf-8"))
        except Exception:
            rejected = True
            continue
        finally:
            os.close(fd)

        if not isinstance(capture, dict) or capture.get("schema_version") != 1:
            rejected = True
            continue
        cap_root = capture.get("workspace_root")
        agent_id = capture.get("agent_id", "")
        task_id = capture.get("task_id", "")
        session_id = capture.get("session_id", "")
        if (
            not isinstance(cap_root, str)
            or Path(cap_root) != canon_root
            or any(not isinstance(x, str) or not (1 <= len(x) <= 128) for x in (agent_id, task_id, session_id))
        ):
            rejected = True
            continue
        agents.append(_summarize_capture(capture, now))

    agents.sort(key=lambda row: int(row.get("recorded_at_ms", 0)), reverse=True)
    seen_agents = set()
    deduped = []
    for row in agents:
        aid = row["agent_id"]
        if aid not in seen_agents:
            seen_agents.add(aid)
            deduped.append(row)
    agents = deduped
    if len(agents) > MAX_AGENTS:
        agents = agents[:MAX_AGENTS]
        truncated = True

    if rejected or truncated:
        status = "incomplete"
        reason: Optional[str] = "capture_limit" if truncated else "invalid_capture"
    elif not agents:
        status = "unavailable"
        reason = "no_capture"
    else:
        status = "available"
        reason = None

    return {
        "status": status,
        "reason": reason,
        "observed_at_ms": now,
        "agents": agents,
        "truncated": truncated,
    }


def _summarize_capture(capture: Dict[str, Any], now_ms: int) -> Dict[str, Any]:
    recorded_at = int(capture.get("recorded_at_ms", 0))
    age = max(0, now_ms - recorded_at)
    phase = str(capture.get("phase", "running")).lower()
    if phase not in ("running", "completed", "failed", "partial", "abandoned"):
        phase = "incomplete"
    obs_truncated = bool(capture.get("observation_truncated", False))
    ev = capture.get("evidence")
    status = "available"
    reason: Optional[str] = None
    evidence_summary: Optional[Dict[str, Any]] = None
    outstanding = unreviewed = untested = unknown_obl = passed = failed = 0

    if isinstance(ev, dict):
        obs = ev.get("observations") if isinstance(ev.get("observations"), list) else []
        runs = [r for r in obs if isinstance(r, dict) and r.get("kind") in ("run_finished", "opaque_run")]
        passed = sum(1 for r in runs if r.get("outcome") == "Passed")
        failed = sum(1 for r in runs if r.get("outcome") == "Failed")
        outstanding = int(ev.get("outstanding", 0))
        unreviewed = int(ev.get("unreviewed_lines", 0))
        untested = int(ev.get("untested_lines", 0))
        unknown_obl = int(ev.get("unknown_size_obligations", 0))
        unattributed = len(ev.get("unattributed", [])) if isinstance(ev.get("unattributed"), list) else 0
        unrecorded = int(ev.get("possible_unrecorded_mutations", 0))
        if unknown_obl > 0 or unattributed > 0 or unrecorded > 0 or obs_truncated or len(runs) != passed + failed:
            status = "incomplete"
            reason = "uncertain_evidence"
        latest_run = None
        if not obs_truncated and runs:
            last_out = runs[-1].get("outcome")
            if last_out == "Passed":
                latest_run = "passed"
            elif last_out == "Failed":
                latest_run = "failed"
        evidence_summary = {
            "outstanding": outstanding,
            "unreviewed_lines": unreviewed,
            "untested_lines": untested,
            "unknown_size_obligations": unknown_obl,
            "unattributed_mutations": unattributed,
            "possible_unrecorded_mutations": unrecorded,
            "observed_runs": len(runs),
            "passed_runs": passed,
            "failed_runs": failed,
            "unknown_runs": len(runs) - passed - failed,
            "latest_run": latest_run,
        }
    else:
        status = "incomplete"
        reason = "evidence_unavailable"

    if age > STALE_MS:
        status = "stale"
        reason = "capture_expired"
    if recorded_at > now_ms + 5000:
        status = "incomplete"
        reason = "capture_in_future"

    emotion = derive_posture_emotion(
        phase=phase,
        status=status,
        failed_runs=failed,
        unreviewed_lines=unreviewed,
        untested_lines=untested,
        outstanding=outstanding,
        unknown_obligations=unknown_obl,
        latest_run=evidence_summary.get("latest_run") if evidence_summary else None,
    )
    debt_lines = max(unreviewed, untested)
    debt_ratio = min(1.0, debt_lines / 250.0) if debt_lines > 0 else 0.0

    return {
        "agent_id": _opaque_id(f"{capture['session_id']}\0{capture['agent_id']}"),
        "task_id": _opaque_id(capture["task_id"]),
        "session_id": _opaque_id(capture["session_id"]),
        "status": status,
        "reason": reason,
        "recorded_at_ms": recorded_at,
        "age_ms": age,
        "phase": phase,
        "evidence": evidence_summary,
        "fox_posture": {
            "emotion": emotion,
            "debt_ratio": round(debt_ratio, 3),
            "channel": "ambient",
        },
    }


def derive_posture_emotion(
    *,
    phase: str,
    status: str,
    failed_runs: int,
    unreviewed_lines: int,
    untested_lines: int,
    outstanding: int,
    unknown_obligations: int = 0,
    latest_run: Optional[str] = None,
) -> str:
    """Derive Phi Fox emotion honestly from observed evidence (never claiming green)."""
    if status == "stale":
        return "idle"
    if phase == "failed" or latest_run == "failed" or (failed_runs > 0 and latest_run != "passed"):
        return "error"
    if status == "incomplete" or unknown_obligations > 0:
        return "guard"
    if unreviewed_lines > 0 or untested_lines > 0 or outstanding > 0:
        return "skeptical"
    if phase == "running":
        return "working"
    return "idle"


def compute_debt_boss_state(
    activity: Dict[str, Any],
    *,
    peak_hp: int = 0,
    xp: int = 0,
    streak: float = 1.0,
) -> DebtBossState:
    """Compute gamified Debt Boss HP & quests from real workspace activity."""
    status = activity.get("status", "unavailable")
    agents = [a for a in activity.get("agents", []) if isinstance(a, dict) and a.get("status") != "stale"]

    if status == "unavailable" or not agents:
        reason = activity.get("reason") or "no_capture"
        return DebtBossState(
            boss_name="Dormant Fog (No Live Agent Captures)",
            current_hp=0,
            max_hp=max(100, peak_hp),
            debt_ratio=0.0,
            level=max(1, 1 + xp // 100),
            xp=xp,
            streak=streak,
            status_label=f"OBSERVATION ABSENT ({reason}) — NOT VERIFIED",
            trust_state="observed_signals",
            quests=[
                "Press [r] to probe workspace gates & status",
                "Press [h] to launch a Debt Slayer training drill",
                "Press [c] to chat with Phi Fox Bot",
            ],
        )

    unreviewed = 0
    untested = 0
    outstanding = 0
    failed_runs = 0
    unknown_runs = 0
    unknown_obl = 0
    running = 0

    for agent in agents:
        if agent.get("phase") == "running":
            running += 1
        ev = agent.get("evidence")
        if isinstance(ev, dict):
            unreviewed += int(ev.get("unreviewed_lines", 0))
            untested += int(ev.get("untested_lines", 0))
            outstanding += int(ev.get("outstanding", 0))
            failed_runs += int(ev.get("failed_runs", 0))
            unknown_runs += int(ev.get("unknown_runs", 0))
            unknown_obl += int(ev.get("unknown_size_obligations", 0)) + int(ev.get("unattributed_mutations", 0))
        else:
            unknown_obl += 1

    raw_hp = (
        unreviewed * 1
        + untested * 2
        + outstanding * 25
        + failed_runs * 50
        + unknown_runs * 20
        + unknown_obl * 40
    )
    max_hp = max(100, peak_hp, raw_hp)
    cleared = max(0, max_hp - raw_hp)
    new_xp = xp + cleared
    new_streak = round(min(3.0, 1.0 + (cleared / 100.0) * 0.25), 2) if raw_hp < max_hp else streak

    quests: List[str] = []
    if failed_runs > 0:
        boss_name = "Regression Hydra (Failing Command Runs)"
        quests.append(f"Slay {failed_runs} failing command run(s) (+{failed_runs * 50} XP)")
    elif unknown_obl > 0 or unknown_runs > 0:
        boss_name = "Phantom Specter (Unbounded / Uncertain Mutations)"
        quests.append(f"Audit {unknown_obl + unknown_runs} uncertain mutation(s) (+40 XP)")
    elif unreviewed > 0 or untested > 0:
        boss_name = "Boilerplate Golem (Unreviewed & Untested Diff)"
        if unreviewed > 0:
            quests.append(f"Review {unreviewed} unreviewed line(s) (+{unreviewed} XP)")
        if untested > 0:
            quests.append(f"Cover {untested} untested line(s) (+{untested * 2} XP)")
    elif outstanding > 0:
        boss_name = "Sentinel Ward (Outstanding Obligations)"
        quests.append(f"Resolve {outstanding} verification obligation(s) (+{outstanding * 25} XP)")
    else:
        boss_name = "Arena Clear (0 Observed Debt Obligations)"
        quests.append("All observed agent obligations cleared — ready for next mission")

    status_label = (
        f"{len(agents)} agent(s) observed ({running} running) · "
        f"failed={failed_runs} · unreviewed={unreviewed} · untested={untested}"
    )

    return DebtBossState(
        boss_name=boss_name,
        current_hp=raw_hp,
        max_hp=max_hp,
        debt_ratio=min(1.0, raw_hp / float(max(1, max_hp))),
        level=max(1, 1 + new_xp // 100),
        xp=new_xp,
        streak=new_streak,
        status_label=status_label,
        trust_state="observed_signals",
        quests=quests,
    )


def render_hp_bar(current_hp: int, max_hp: int, width: int = 24) -> str:
    safe_max = max(1, max_hp)
    ratio = max(0.0, min(1.0, current_hp / float(safe_max)))
    filled = int(round(ratio * width))
    empty = max(0, width - filled)
    color = "\033[38;5;203m" if ratio > 0.6 else "\033[38;5;214m" if ratio > 0.2 else "\033[38;5;114m"
    reset = "\033[0m"
    return f"{color}[{'█' * filled}{'░' * empty}]{reset} {current_hp}/{safe_max} HP"


def render_fox_sprite(
    emotion: str,
    frame_tick: int,
    *,
    viseme: str = "rest",
    god_mode: bool = False,
    laser_target: Optional[str] = None,
) -> List[str]:
    """Render an 8-line animated ANSI + Braille Phi Fox sprite."""
    effective_emotion = "god_mode" if god_mode else (emotion if emotion in EMOTION_THEMES else "curious")
    theme = EMOTION_THEMES[effective_emotion]
    color = theme["color"]
    reset = "\033[0m"
    gold = "\033[38;5;220m"
    cyan = "\033[38;5;117m"

    spark_a = BRAILLE_SPARKS[frame_tick % len(BRAILLE_SPARKS)]
    spark_b = BRAILLE_SPARKS[(frame_tick + 3) % len(BRAILLE_SPARKS)]
    blink = (frame_tick % 19) == 0 and effective_emotion not in ("error", "sleep")
    eyes = "- . -" if blink else theme["eyes"]
    mouth = VISEME_MOUTH_GLYPHS.get(viseme, VISEME_MOUTH_GLYPHS["rest"])
    tail_wave = ("╭─φ─╮", "╭─✦─╮", "╰─φ─╯", "╭─φ─╰")[frame_tick % 4]

    halo = f"{cyan}✧ ✦ φ ✦ ✧{reset}" if god_mode else f"{gold}{spark_a}   φ   {spark_b}{reset}"
    laser_line = (
        f"  {cyan}═══╬══► 🎯 {laser_target}{reset}"
        if laser_target
        else f"  {color}{theme['badge']}{reset}"
    )

    return [
        f"      {halo}",
        f"     {color}{theme['ears']}{reset}    {laser_line}",
        f"    {color}( {eyes} ){reset}   {gold}Viseme:{reset} [{viseme.upper():^4}]",
        f"    {color}(  {mouth}  ){reset}   {gold}Tail:{reset}   {tail_wave}",
        f"   {color}╭─┴─────┴─╮{reset}",
        f"   {color}│  ╭───╮  │{reset} {color}{ tail_wave }{reset}",
        f"   {color}╰──┴───┴──╯{reset}",
    ]


def render_cockpit_frame(
    *,
    workspace_root: Path,
    activity: Dict[str, Any],
    boss: DebtBossState,
    emotion: str,
    frame_tick: int,
    viseme: str = "rest",
    god_mode: bool = False,
    laser_target: Optional[str] = None,
    chat_turns: Optional[List[Dict[str, str]]] = None,
    input_buffer: Optional[str] = None,
    width: int = 76,
) -> str:
    """Render a complete, flicker-free ANSI frame for the tmux Phi Fox HUD pane."""
    bold = "\033[1m"
    dim = "\033[2m"
    gold = "\033[38;5;214m"
    cyan = "\033[38;5;117m"
    emerald = "\033[38;5;114m"
    reset = "\033[0m"

    rule = "─" * max(40, width - 2)
    lines: List[str] = []
    lines.append(f"{gold}╭{rule}╮{reset}")
    title = f" φ SELFWARE · PHI FOX TMUX COCKPIT & DEBT SLAYER "
    badge = f"[TRUST: {boss.trust_state.upper()}]"
    lines.append(f"{gold}│{reset}{bold}{title}{reset}{cyan}{badge:>24}{reset} {gold}│{reset}")
    lines.append(f"{gold}├{rule}┤{reset}")

    for sprite_line in render_fox_sprite(
        emotion,
        frame_tick,
        viseme=viseme,
        god_mode=god_mode,
        laser_target=laser_target,
    ):
        lines.append(f"  {sprite_line}")

    lines.append(f"{gold}├{rule}┤{reset}")
    lines.append(
        f"  {bold}⚔ BOSS:{reset} {boss.boss_name[:52]} "
        f"{emerald}Lv.{boss.level} · {boss.xp} XP ({boss.streak:.2f}x φ){reset}"
    )
    lines.append(f"  {bold}♥ DEBT:{reset} {render_hp_bar(boss.current_hp, boss.max_hp)}")
    lines.append(f"  {dim}📡 {boss.status_label}{reset}")

    lines.append(f"{gold}├{rule}┤{reset}")
    lines.append(f"  {bold}📜 ACTIVE QUESTS & VERIFICATION OBLIGATIONS:{reset}")
    for q in boss.quests[:3]:
        lines.append(f"   • {q}")

    lines.append(f"{gold}├{rule}┤{reset}")
    lines.append(f"  {bold}💬 PHI FOX BOT CHANNEL:{reset}")
    turns = (chat_turns or [])[-3:]
    if not turns:
        lines.append(
            f"   {cyan}Phi:{reset} Ready on perch. Press {bold}[r]{reset} What's red, "
            f"{bold}[d]{reset} Debt check, {bold}[h]{reset} Hunt, or {bold}[c]{reset} Chat."
        )
    else:
        for turn in turns:
            role = "You" if turn.get("role") == "user" else "Phi"
            role_color = cyan if role == "Phi" else gold
            text = turn.get("content", "").replace("\n", " ")
            cite = turn.get("citation")
            cite_suffix = f" {emerald}[🎯 {cite}]{reset}" if cite else ""
            lines.append(f"   {role_color}{role}:{reset} {text[:110]}{cite_suffix}")

    lines.append(f"{gold}├{rule}┤{reset}")
    if input_buffer is not None:
        lines.append(f"  {bold}{cyan}Ask Phi > {reset}{input_buffer}█")
    else:
        lines.append(
            f"  {bold}Keys:{reset} {gold}[r]{reset} Red?  {gold}[d]{reset} Debt  "
            f"{gold}[h]{reset} Hunt  {gold}[g]{reset} GodMode  {gold}[c]{reset} Chat  "
            f"{gold}[o]{reset} Open Cite  {gold}[q]{reset} Quit"
        )
    lines.append(f"{gold}╰{rule}╯{reset}")
    return "\n".join(lines)


def query_phi_bot_server(
    server_url: str,
    message: str,
    intent: str = "chat",
    timeout: float = 5.0,
    session_token: Optional[str] = None,
) -> Optional[Dict[str, Any]]:
    """Query a running local EvolveServer `/api/workspace` + `/api/phi/chat`."""
    base = server_url.rstrip("/")
    try:
        token = session_token
        if not token:
            ws_req = urllib.request.Request(f"{base}/api/workspace", headers={"Host": "127.0.0.1"})
            with urllib.request.urlopen(ws_req, timeout=timeout) as resp:
                ws = json.loads(resp.read().decode("utf-8"))
            token = ws.get("session_token")
        if not token:
            return None
        payload = json.dumps({"message": message, "intent": intent, "mode": "local_signals"}).encode("utf-8")
        chat_req = urllib.request.Request(
            f"{base}/api/phi/chat",
            data=payload,
            headers={
                "Content-Type": "application/json",
                "Host": "127.0.0.1",
                "x-selfware-session": token,
            },
            method="POST",
        )
        with urllib.request.urlopen(chat_req, timeout=timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except Exception:
        return None


def scan_workspace_target(workspace_root: Path) -> Optional[str]:
    """Find a concrete `path:line` landmark in `src/` for Phi's optical laser."""
    src_dir = workspace_root / "src"
    if not src_dir.is_dir():
        return None
    for candidate in sorted(src_dir.rglob("*.rs"))[:16]:
        try:
            rel = candidate.relative_to(workspace_root)
            with open(candidate, "r", encoding="utf-8", errors="ignore") as f:
                for idx, line in enumerate(f, start=1):
                    stripped = line.strip()
                    if stripped.startswith(("pub fn ", "pub async fn ", "pub struct ")):
                        return f"{rel}:{idx}"
        except OSError:
            continue
    return None


def jump_tmux_to_citation(
    citation: str,
    tmux_session: Optional[str] = None,
    editor_pane: Optional[str] = None,
) -> bool:
    """If inside tmux, display a navigation status banner for `path:line`."""
    if not (os.environ.get("TMUX") or tmux_session) or not shutil.which("tmux"):
        return False
    try:
        cmd = ["tmux", "display-message"]
        if tmux_session and editor_pane:
            cmd.extend(["-t", f"{tmux_session}:{editor_pane}"])
        cmd.append(f"🦊 Phi Laser locked on {citation}")
        subprocess.run(
            cmd,
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=2,
        )
        return True
    except Exception:
        return False


class PhiTmuxArenaApp:
    def __init__(
        self,
        workspace_root: Path,
        server_url: str = "http://127.0.0.1:8888",
        fps: float = 10.0,
        session_token: Optional[str] = None,
        tmux_session: Optional[str] = None,
        editor_pane: Optional[str] = None,
    ) -> None:
        self.workspace_root = workspace_root
        self.server_url = server_url
        self.fps = max(1.0, min(30.0, fps))
        self.session_token = session_token
        self.tmux_session = tmux_session
        self.editor_pane = editor_pane
        self.frame_tick = 0
        self.god_mode = False
        self.laser_target: Optional[str] = None
        self.peak_hp = 100
        self.xp = 0
        self.streak = 1.0
        self.chat_turns: List[Dict[str, str]] = []
        self.viseme_queue: List[Tuple[str, str]] = []
        self.input_buffer: Optional[str] = None
        self.activity: Dict[str, Any] = {}
        self.boss: DebtBossState = compute_debt_boss_state({"status": "unavailable", "reason": "no_capture", "agents": []})
        self.emotion = "curious"
        self.last_poll = 0.0

    def refresh(self, now_ms: Optional[int] = None) -> None:
        self.activity = read_workspace_activity(self.workspace_root, now_ms=now_ms)
        self.boss = compute_debt_boss_state(
            self.activity,
            peak_hp=self.peak_hp,
            xp=self.xp,
            streak=self.streak,
        )
        self.peak_hp = max(self.peak_hp, self.boss.current_hp)
        self.xp = self.boss.xp
        self.streak = self.boss.streak

        agents = [a for a in self.activity.get("agents", []) if isinstance(a, dict) and a.get("status") != "stale"]
        if agents:
            posture = agents[0].get("fox_posture") or {}
            self.emotion = posture.get("emotion", "curious")
        elif not self.viseme_queue:
            self.emotion = "curious"

    def speak(self, reply: str, emotion: str = "analytical", citation: Optional[str] = None) -> None:
        self.emotion = emotion
        if citation:
            self.laser_target = citation
        self.viseme_queue = text_to_viseme_frames(reply, max_frames=20)
        self.chat_turns.append({"role": "assistant", "content": reply, "citation": citation or ""})

    def handle_intent(self, intent: str, user_prompt: str) -> Dict[str, Any]:
        self.chat_turns.append({"role": "user", "content": user_prompt})
        server_reply = query_phi_bot_server(
            self.server_url,
            user_prompt,
            intent=intent,
            timeout=1.5,
            session_token=self.session_token,
        )
        if server_reply and isinstance(server_reply.get("reply"), str):
            cites = server_reply.get("citations") or []
            first_cite = None
            if cites and isinstance(cites[0], dict):
                first_cite = f"{cites[0].get('path')}:{cites[0].get('start_line', 1)}"
            emotion = str(server_reply.get("emotion") or "analytical")
            self.speak(server_reply["reply"], emotion=emotion, citation=first_cite)
            return server_reply

        # Deterministic local fallback (honest observed_signals status)
        self.refresh()
        landmark = scan_workspace_target(self.workspace_root)
        if intent == "whats_red":
            if self.boss.current_hp > 0:
                reply = f"Observed signals: {self.boss.status_label}. Debt Boss HP is {self.boss.current_hp}/{self.boss.max_hp}."
                emotion = "error" if "failed=0" not in self.boss.status_label else "skeptical"
            else:
                reply = (
                    f"{self.boss.status_label}. Note: absence of failed captures does not claim unrun tests passed."
                )
                emotion = "curious"
        elif intent in ("check_debt", "audit_unreviewed"):
            quest_summary = "; ".join(self.boss.quests[:2])
            reply = f"Boss: {self.boss.boss_name} ({self.boss.current_hp} HP). Next: {quest_summary}."
            emotion = "skeptical" if self.boss.current_hp > 0 else "analytical"
        elif intent == "hunt":
            target = landmark or "src/lib.rs:1"
            self.laser_target = target
            reply = f"Optical laser locked onto `{target}`. Press [o] to ping tmux or inspect unreviewed lines."
            emotion = "analytical"
        else:
            target = landmark or "src/lib.rs:1"
            reply = f"Anchored to local workspace (`{target}`). {self.boss.status_label}."
            emotion = "analytical"

        self.speak(reply, emotion=emotion, citation=landmark)
        return {
            "reply": reply,
            "emotion": emotion,
            "trust_state": "observed_signals",
            "citation": landmark,
        }

    def step_frame(self) -> str:
        now = time.time()
        if now - self.last_poll >= 1.5 or not self.activity:
            self.refresh()
            self.last_poll = now
        self.frame_tick += 1
        current_viseme = "rest"
        if self.viseme_queue:
            current_viseme, _ = self.viseme_queue.pop(0)
        return render_cockpit_frame(
            workspace_root=self.workspace_root,
            activity=self.activity,
            boss=self.boss,
            emotion=self.emotion,
            frame_tick=self.frame_tick,
            viseme=current_viseme,
            god_mode=self.god_mode,
            laser_target=self.laser_target,
            chat_turns=self.chat_turns,
            input_buffer=self.input_buffer,
        )


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Phi Fox Tmux Cockpit & Debt Slayer Game HUD")
    parser.add_argument("--workspace", default=".", help="Workspace root directory")
    parser.add_argument("--server", default="http://127.0.0.1:8888", help="Local EvolveServer base URL")
    parser.add_argument("--session-token", default=None, help="Optional X-Selfware-Session token")
    parser.add_argument("--tmux-session", default=None, help="Optional tmux session name")
    parser.add_argument("--editor-pane", default="0.0", help="Optional tmux target pane for [o] open citation")
    parser.add_argument("--once", action="store_true", help="Render a single frame and exit")
    parser.add_argument("--json", action="store_true", help="Print activity + Debt Boss JSON state and exit")
    parser.add_argument("--intent", choices=["whats_red", "check_debt", "hunt", "chat"], help="Run one bot intent")
    parser.add_argument("--prompt", default="", help="Prompt message when --intent is provided")
    parser.add_argument("--fps", type=float, default=8.0, help="Animation refresh rate")
    args = parser.parse_args(argv)

    ws_root = Path(args.workspace).resolve()
    app = PhiTmuxArenaApp(
        ws_root,
        server_url=args.server,
        fps=args.fps,
        session_token=args.session_token,
        tmux_session=args.tmux_session,
        editor_pane=args.editor_pane,
    )
    app.refresh()

    if args.intent:
        default_prompts = {
            "whats_red": "What checks or agent runs are red?",
            "check_debt": "Check verification debt and Boss HP.",
            "hunt": "Lock laser on the next code target.",
            "chat": args.prompt or "Status report",
        }
        app.handle_intent(args.intent, args.prompt or default_prompts[args.intent])

    if args.json:
        print(
            json.dumps(
                {
                    "activity": app.activity,
                    "boss": {
                        "boss_name": app.boss.boss_name,
                        "current_hp": app.boss.current_hp,
                        "max_hp": app.boss.max_hp,
                        "debt_ratio": app.boss.debt_ratio,
                        "level": app.boss.level,
                        "xp": app.boss.xp,
                        "streak": app.boss.streak,
                        "trust_state": app.boss.trust_state,
                        "quests": app.boss.quests,
                    },
                    "emotion": app.emotion,
                    "laser_target": app.laser_target,
                    "chat_turns": app.chat_turns,
                },
                indent=2,
            )
        )
        return 0

    if args.once or not sys.stdin.isatty():
        print(app.step_frame())
        return 0

    import termios
    import tty

    fd = sys.stdin.fileno()
    old_settings = termios.tcgetattr(fd)
    try:
        tty.setcbreak(fd)
        sys.stdout.write("\033[?25l\033[2J")
        sys.stdout.flush()
        interval = 1.0 / app.fps
        while True:
            frame = app.step_frame()
            sys.stdout.write("\033[H" + frame + "\033[J\n")
            sys.stdout.flush()

            rlist, _, _ = select.select([sys.stdin], [], [], interval)
            if not rlist:
                continue
            ch = sys.stdin.read(1)
            if app.input_buffer is not None:
                if ch in ("\n", "\r"):
                    prompt = app.input_buffer.strip()
                    app.input_buffer = None
                    if prompt:
                        app.handle_intent("chat", prompt)
                elif ch == "\x1b":
                    app.input_buffer = None
                elif ch in ("\x7f", "\b"):
                    app.input_buffer = app.input_buffer[:-1]
                elif ch.isprintable():
                    app.input_buffer += ch
                continue

            if ch in ("q", "Q", "\x03"):
                break
            if ch in ("r", "R"):
                app.handle_intent("whats_red", "What's red?")
            elif ch in ("d", "D"):
                app.handle_intent("check_debt", "Check agent debt")
            elif ch in ("h", "H"):
                app.handle_intent("hunt", "Hunt next target")
            elif ch in ("g", "G"):
                app.god_mode = not app.god_mode
                app.speak(
                    "✧ Celestial God Mode engaged." if app.god_mode else "Returning to quiet amber glow.",
                    emotion="god_mode" if app.god_mode else "curious",
                )
            elif ch in ("o", "O") and app.laser_target:
                jump_tmux_to_citation(app.laser_target)
            elif ch in ("c", "C", "/"):
                app.input_buffer = ""
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old_settings)
        sys.stdout.write("\033[?25h\n")
        sys.stdout.flush()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
