#!/usr/bin/env python3
"""
Advanced Selfware Development Test Runner
Runs parallel development tasks with visual feedback loop
"""

import os
import sys
import json
import time
import subprocess
import signal
from pathlib import Path
from datetime import datetime
from dataclasses import dataclass
from typing import Dict, List, Optional
import threading
import queue

# Configuration
DEFAULT_ENDPOINT = "http://localhost:8000/v1"
MAX_ITERATIONS = 50
STEP_TIMEOUT = 300
SCREENSHOT_INTERVAL = 60

TASKS = {
    "flappy_bird": {
        "name": "Flappy Bird Game",
        "prompt": """Create a complete, playable Flappy Bird game in a single HTML file.

REQUIREMENTS:
1. HTML5 Canvas-based game with smooth 60fps animation
2. Game physics: gravity, jump on space/click, pipe collision
3. Procedurally generated pipes with gaps
4. Score tracking (localStorage for high score)
5. Game states: menu, playing, game over with restart
6. Visual polish: bird animation, background parallax, particle effects
7. Mobile touch support

DELIVERABLE: index.html (self-contained, playable in browser)

Work iteratively: first make it work, then make it polished."""
    },
    "portfolio": {
        "name": "Portfolio Website", 
        "prompt": """Create a stunning, modern portfolio website for a software developer.

REQUIREMENTS:
1. Single-page with smooth scroll navigation
2. Sections: Hero (animated intro), About, Projects (3+ cards), Skills, Contact
3. Modern CSS: Grid, Flexbox, CSS variables for theming
4. Dark/Light mode toggle with persistence
5. Responsive: Mobile-first design
6. Animations: Fade-in on scroll, hover effects
7. Professional typography

DELIVERABLES: index.html, styles.css, script.js

Design: Clean, minimalist, accent color #6366f1 (indigo)"""
    },
    "tqec_sim": {
        "name": "TQEC Quantum Simulator",
        "prompt": """Create a TQEC (Topological Quantum Error Correction) simulator in Rust.

BACKGROUND: TQEC uses surface codes to protect quantum information.

REQUIREMENTS:
1. Surface Code Lattice: 2D grid of data and ancilla qubits
2. Stabilizer Operations: X-stabilizers and Z-stabilizers
3. Error Models: Bit-flip, phase-flip, depolarizing noise
4. Decoder: Minimum Weight Perfect Matching or Union-Find
5. Output: CLI tool with ASCII/SVG visualization

DELIVERABLES:
- Cargo.toml
- src/lattice.rs, src/stabilizer.rs, src/errors.rs
- src/decoder.rs, src/visualizer.rs, src/main.rs
- examples/basic.rs

Quality: Production-ready Rust crate."""
    },
    "rust_game": {
        "name": "Space Shooter Game",
        "prompt": """Create a Space Shooter game in Rust using macroquad.

REQUIREMENTS:
1. Core Gameplay: Player ship, enemies, shooting, collision
2. Entities: Player, 2-3 enemy types, bullets, power-ups
3. Visual Polish: Starfield background, particles, screen shake
4. UI: Score, lives, wave number

DELIVERABLES:
- Cargo.toml with macroquad
- src/main.rs, src/player.rs, src/enemy.rs
- src/bullet.rs, src/collision.rs, src/particle.rs

Must compile with 'cargo run' and run at 60fps."""
    }
}


@dataclass
class Instance:
    task: str
    instance_id: int
    workdir: Path
    pid: Optional[int] = None
    status: str = "pending"
    
    def __post_init__(self):
        self.workdir.mkdir(parents=True, exist_ok=True)
        (self.workdir / "screenshots").mkdir(exist_ok=True)


class DevelopmentRunner:
    def __init__(self, test_dir: Path, endpoint: str = DEFAULT_ENDPOINT):
        self.test_dir = test_dir
        self.endpoint = endpoint
        self.instances: List[Instance] = []
        self.config_file = test_dir / "selfware-dev.toml"
        self.screenshot_queue = queue.Queue()
        self.running = False
        
    def check_endpoint(self) -> bool:
        """Check if LLM endpoint is available"""
        import urllib.request
        try:
            req = urllib.request.Request(
                f"{self.endpoint}/models",
                method="GET",
                headers={"Accept": "application/json"}
            )
            with urllib.request.urlopen(req, timeout=5) as resp:
                return resp.status == 200
        except Exception as e:
            print(f"❌ Endpoint check failed: {e}")
            return False
    
    def create_config(self):
        """Create specialized config for development tests"""
        config_content = f"""# Development test configuration
endpoint = "{self.endpoint}"
model = "qwen3.5-27b"
max_tokens = 81920
temperature = 0.7

[extra_body]
top_p = 0.95
top_k = 20
min_p = 0.0

context_length = 1048576

[concurrency]
max_parallel_requests = 16
max_retries = 5
timeout_secs = 300

[safety]
allowed_paths = ["{self.test_dir}/**", "./**", "~/**"]
denied_paths = ["**/.env", "**/secrets/**", "**/.ssh/**"]

[agent]
streaming = true
max_iterations = {MAX_ITERATIONS}
step_timeout_secs = {STEP_TIMEOUT}
native_function_calling = true
token_budget = 900000
token_safety_margin = 100000

[continuous_work]
enabled = true
checkpoint_interval_tools = 10
checkpoint_interval_secs = 300
auto_recovery = true
max_recovery_attempts = 5

[retry]
max_retries = 10
base_delay_ms = 2000
max_delay_ms = 120000
"""
        self.config_file.write_text(config_content)
        print(f"✅ Config created: {self.config_file}")
    
    def launch_instance(self, instance: Instance) -> bool:
        """Launch a single selfware instance"""
        task_info = TASKS[instance.task]
        prompt = task_info["prompt"]
        
        # Add instance-specific variation
        instance_prompt = f"{prompt}\n\nINSTANCE {instance.instance_id}/4: Try a unique approach or variation."
        
        cmd = [
            str(self.selfware_bin),
            "--config", str(self.config_file),
            "--mode", "yolo",
            "--workdir", str(instance.workdir),
            "run", instance_prompt
        ]
        
        log_file = instance.workdir / "selfware.log"
        
        try:
            with open(log_file, "w") as f:
                proc = subprocess.Popen(
                    cmd,
                    stdout=f,
                    stderr=subprocess.STDOUT,
                    start_new_session=True
                )
            instance.pid = proc.pid
            instance.status = "running"
            
            # Save PID
            (instance.workdir / "pid").write_text(str(proc.pid))
            return True
            
        except Exception as e:
            instance.status = f"error: {e}"
            return False
    
    def check_instance_status(self, instance: Instance) -> str:
        """Check if instance is still running"""
        if instance.pid is None:
            return "not_started"
        
        try:
            os.kill(instance.pid, 0)
            return "running"
        except ProcessLookupError:
            return "finished"
        except Exception as e:
            return f"error: {e}"
    
    def capture_screenshot(self, instance: Instance) -> Optional[Path]:
        """Capture screenshot of HTML file if exists"""
        html_file = instance.workdir / "index.html"
        if not html_file.exists():
            return None
        
        screenshot_dir = instance.workdir / "screenshots"
        screenshot_dir.mkdir(exist_ok=True)
        
        timestamp = datetime.now().strftime("%H%M%S")
        screenshot_file = screenshot_dir / f"screenshot_{timestamp}.png"
        
        # Try using playwright
        try:
            subprocess.run(
                [
                    "npx", "playwright", "screenshot",
                    "--browser=chromium",
                    "--viewport-size=1280,720",
                    f"file://{html_file.absolute()}",
                    str(screenshot_file)
                ],
                capture_output=True,
                timeout=30
            )
            if screenshot_file.exists():
                return screenshot_file
        except Exception:
            pass
        
        return None
    
    def screenshot_worker(self):
        """Background worker for screenshot capture"""
        while self.running:
            for instance in self.instances:
                if instance.status == "running":
                    self.capture_screenshot(instance)
            time.sleep(SCREENSHOT_INTERVAL)
    
    def generate_report(self):
        """Generate comprehensive report"""
        report_file = self.test_dir / "RESULTS.md"
        
        lines = [
            "# Selfware Development Test Results",
            "",
            f"**Generated:** {datetime.now().isoformat()}",
            f"**Endpoint:** {self.endpoint}",
            "",
        ]
        
        for task_key, task_info in TASKS.items():
            lines.append(f"## {task_info['name']}")
            lines.append("")
            
            for i in range(1, 5):
                instance_dir = self.test_dir / task_key / f"instance_{i}"
                lines.append(f"### Instance {i}")
                lines.append("")
                
                # Count files
                files = list(instance_dir.rglob("*")) if instance_dir.exists() else []
                code_files = [f for f in files if f.suffix in ['.html', '.css', '.js', '.rs', '.toml']]
                
                lines.append(f"**Files Created:** {len(code_files)}")
                lines.append("")
                
                if code_files:
                    lines.append("```")
                    for f in sorted(code_files):
                        size = f.stat().st_size if f.exists() else 0
                        rel_path = f.relative_to(instance_dir)
                        lines.append(f"{size:>8} B  {rel_path}")
                    lines.append("```")
                    lines.append("")
                
                # Screenshots
                screenshot_dir = instance_dir / "screenshots"
                if screenshot_dir.exists():
                    screenshots = list(screenshot_dir.glob("*.png"))
                    lines.append(f"**Screenshots:** {len(screenshots)}")
                    lines.append("")
                
                # Log excerpt
                log_file = instance_dir / "selfware.log"
                if log_file.exists():
                    lines.append("**Last Activity:**")
                    lines.append("```")
                    try:
                        log_content = log_file.read_text().splitlines()
                        lines.extend(log_content[-20:])
                    except Exception as e:
                        lines.append(f"Error reading log: {e}")
                    lines.append("```")
                    lines.append("")
        
        report_file.write_text("\n".join(lines))
        print(f"\n📊 Report generated: {report_file}")
        return report_file
    
    def run(self):
        """Main execution"""
        print("=" * 50)
        print("  Advanced Development Test Runner")
        print("=" * 50)
        print()
        
        # Find selfware binary
        self.selfware_bin = Path(__file__).parent.parent / "target" / "release" / "selfware"
        if not self.selfware_bin.exists():
            print("❌ Selfware binary not found!")
            print(f"   Expected: {self.selfware_bin}")
            print("   Run: cargo build --release")
            return False
        
        print(f"✅ Selfware binary: {self.selfware_bin}")
        
        # Check endpoint
        if not self.check_endpoint():
            print("\n⚠️  WARNING: LLM endpoint not available!")
            print(f"   Expected: {self.endpoint}")
            print("\nPlease start your vLLM/sglang server:")
            print("   vllm serve Qwen/Qwen3.5-27B-FP8 --port 8000")
            print()
            response = input("Continue anyway? (y/n): ")
            if response.lower() != 'y':
                return False
        
        # Setup
        self.test_dir.mkdir(parents=True, exist_ok=True)
        self.create_config()
        
        # Create instances
        for task_key in TASKS.keys():
            for i in range(1, 5):
                instance = Instance(
                    task=task_key,
                    instance_id=i,
                    workdir=self.test_dir / task_key / f"instance_{i}"
                )
                self.instances.append(instance)
        
        # Launch all instances
        print(f"\n🚀 Launching {len(self.instances)} instances...")
        for instance in self.instances:
            if self.launch_instance(instance):
                print(f"  ✅ {instance.task} instance_{instance.instance_id} (PID: {instance.pid})")
            else:
                print(f"  ❌ {instance.task} instance_{instance.instance_id} FAILED")
            time.sleep(0.5)  # Stagger launches
        
        # Start screenshot worker
        self.running = True
        screenshot_thread = threading.Thread(target=self.screenshot_worker)
        screenshot_thread.daemon = True
        screenshot_thread.start()
        
        print("\n" + "=" * 50)
        print("  All instances launched!")
        print("=" * 50)
        print(f"\n📁 Test Directory: {self.test_dir}")
        print("\nMonitoring commands:")
        print(f"  python {__file__} --monitor {self.test_dir}")
        print(f"  python {__file__} --report {self.test_dir}")
        print(f"  python {__file__} --stop {self.test_dir}")
        print()
        
        return True
    
    def monitor(self):
        """Monitor running instances"""
        print("Monitoring instances... (Ctrl+C to stop)")
        try:
            while True:
                os.system('clear' if os.name != 'nt' else 'cls')
                print(f"=== Development Test Monitor ===")
                print(f"Time: {datetime.now().isoformat()}")
                print()
                
                for task_key in TASKS.keys():
                    print(f"=== {TASKS[task_key]['name']} ===")
                    for i in range(1, 5):
                        instance_dir = self.test_dir / task_key / f"instance_{i}"
                        pid_file = instance_dir / "pid"
                        
                        status = "⚪ unknown"
                        if pid_file.exists():
                            try:
                                pid = int(pid_file.read_text().strip())
                                os.kill(pid, 0)
                                status = "🟢 running"
                            except ProcessLookupError:
                                status = "✅ finished"
                            except:
                                status = "❌ error"
                        
                        # Count deliverables
                        html = len(list(instance_dir.glob("*.html"))) if instance_dir.exists() else 0
                        rs = len(list((instance_dir / "src").glob("*.rs"))) if (instance_dir / "src").exists() else 0
                        cargo = 1 if (instance_dir / "Cargo.toml").exists() else 0
                        
                        print(f"  instance_{i}: {status:12} HTML:{html} RS:{rs} Cargo:{cargo}")
                    print()
                
                time.sleep(10)
                
        except KeyboardInterrupt:
            print("\nMonitoring stopped.")
    
    def stop(self):
        """Stop all instances"""
        print("Stopping all instances...")
        self.running = False
        
        stopped = 0
        for task_key in TASKS.keys():
            for i in range(1, 5):
                instance_dir = self.test_dir / task_key / f"instance_{i}"
                pid_file = instance_dir / "pid"
                
                if pid_file.exists():
                    try:
                        pid = int(pid_file.read_text().strip())
                        os.kill(pid, signal.SIGTERM)
                        stopped += 1
                        print(f"  Stopped {task_key} instance_{i}")
                    except Exception as e:
                        print(f"  Error stopping {task_key} instance_{i}: {e}")
        
        print(f"\nStopped {stopped} instances.")


def main():
    import argparse
    
    parser = argparse.ArgumentParser(description="Selfware Development Test Runner")
    parser.add_argument("--run", action="store_true", help="Start new test run")
    parser.add_argument("--monitor", metavar="DIR", help="Monitor existing test directory")
    parser.add_argument("--report", metavar="DIR", help="Generate report for test directory")
    parser.add_argument("--stop", metavar="DIR", help="Stop all instances in directory")
    parser.add_argument("--endpoint", default=DEFAULT_ENDPOINT, help="LLM endpoint URL")
    
    args = parser.parse_args()
    
    if args.monitor:
        runner = DevelopmentRunner(Path(args.monitor))
        runner.monitor()
    elif args.report:
        runner = DevelopmentRunner(Path(args.report))
        report = runner.generate_report()
        print(report.read_text())
    elif args.stop:
        runner = DevelopmentRunner(Path(args.stop))
        runner.stop()
    elif args.run:
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        test_dir = Path(__file__).parent.parent / "parallel_dev_tests" / f"run_{timestamp}"
        runner = DevelopmentRunner(test_dir, args.endpoint)
        runner.run()
    else:
        # Default: run new test
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        test_dir = Path(__file__).parent.parent / "parallel_dev_tests" / f"run_{timestamp}"
        runner = DevelopmentRunner(test_dir, args.endpoint)
        runner.run()


if __name__ == "__main__":
    main()
