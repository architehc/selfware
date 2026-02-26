#!/usr/bin/env python3
"""
Long-Running Mega Project Test Runner

Orchestrates multi-hour test sessions for Selfware agent validation.
"""

import argparse
import json
import logging
import os
import subprocess
import sys
import time
import uuid
from dataclasses import dataclass, asdict
from datetime import datetime, timedelta
from pathlib import Path
from typing import Dict, List, Optional
import signal

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
    handlers=[
        logging.StreamHandler(sys.stdout),
        logging.FileHandler('test_session.log')
    ]
)
logger = logging.getLogger('mega_test')


@dataclass
class TestConfig:
    """Test session configuration"""
    session_id: str
    project_type: str
    duration_hours: int
    agent_count: int
    checkpoint_interval_min: int = 10
    project_specs: Dict = None
    
    def __post_init__(self):
        if self.project_specs is None:
            self.project_specs = self._default_specs()
    
    def _default_specs(self) -> Dict:
        specs = {
            'task_queue': {
                'name': 'RedQueue',
                'description': 'Redis-compatible distributed task queue',
                'complexity': 'high',
                'target_loc': 5000,
                'target_coverage': 0.80,
            },
            'database': {
                'name': 'MiniDB',
                'description': 'Simplified database engine',
                'complexity': 'very_high',
                'target_loc': 8000,
                'target_coverage': 0.75,
            },
            'microservices': {
                'name': 'ServiceMesh',
                'description': 'Microservices platform',
                'complexity': 'high',
                'target_loc': 6000,
                'target_coverage': 0.80,
            }
        }
        return specs.get(self.project_type, specs['task_queue'])


@dataclass
class SessionMetrics:
    """Real-time session metrics"""
    elapsed_seconds: int = 0
    total_tokens: int = 0
    tokens_per_minute: float = 0.0
    tasks_completed: int = 0
    tasks_remaining: int = 0
    test_pass_rate: float = 0.0
    lines_of_code: int = 0
    test_coverage: float = 0.0
    checkpoint_count: int = 0
    errors_encountered: int = 0
    phase: str = "bootstrap"
    status: str = "running"


class CheckpointManager:
    """Manages session checkpoints"""
    
    def __init__(self, session_dir: Path):
        self.session_dir = session_dir
        self.checkpoint_dir = session_dir / "checkpoints"
        self.checkpoint_dir.mkdir(exist_ok=True)
        self.last_checkpoint = None
        
    def create_checkpoint(self, phase: str, metrics: SessionMetrics) -> Path:
        """Create a new checkpoint"""
        timestamp = datetime.now().isoformat()
        checkpoint_id = f"checkpoint_{int(time.time())}"
        
        checkpoint_data = {
            'id': checkpoint_id,
            'timestamp': timestamp,
            'phase': phase,
            'metrics': asdict(metrics),
            'git_commit': self._get_git_commit(),
        }
        
        checkpoint_path = self.checkpoint_dir / f"{checkpoint_id}.json"
        with open(checkpoint_path, 'w') as f:
            json.dump(checkpoint_data, f, indent=2)
        
        self.last_checkpoint = checkpoint_path
        logger.info(f"Checkpoint created: {checkpoint_path}")
        return checkpoint_path
    
    def _get_git_commit(self) -> Optional[str]:
        """Get current git commit hash"""
        try:
            result = subprocess.run(
                ['git', 'rev-parse', 'HEAD'],
                capture_output=True,
                text=True,
                cwd=self.session_dir
            )
            return result.stdout.strip() if result.returncode == 0 else None
        except:
            return None
    
    def list_checkpoints(self) -> List[Path]:
        """List all checkpoints"""
        return sorted(self.checkpoint_dir.glob("checkpoint_*.json"))
    
    def restore_checkpoint(self, checkpoint_path: Path) -> bool:
        """Restore from a checkpoint"""
        logger.info(f"Restoring from checkpoint: {checkpoint_path}")
        try:
            # Extract task_id from filename (e.g., "task_123.json")
            task_id = checkpoint_path.stem
            result = subprocess.run(
                ['selfware', 'resume', task_id],
                capture_output=True,
                text=True
            )
            if result.returncode == 0:
                logger.info(f"Successfully resumed task {task_id}")
                return True
            else:
                logger.error(f"Failed to resume task {task_id}: {result.stderr}")
                return False
        except Exception as e:
            logger.error(f"Error during checkpoint restore: {e}")
            return False


class TestMonitor:
    """Monitors test execution and collects metrics"""
    
    def __init__(self, session_dir: Path):
        self.session_dir = session_dir
        self.metrics_dir = session_dir / "metrics"
        self.metrics_dir.mkdir(exist_ok=True)
        self.metrics_history = []
        
    def record_snapshot(self, metrics: SessionMetrics):
        """Record metrics snapshot"""
        timestamp = datetime.now().isoformat()
        snapshot = {
            'timestamp': timestamp,
            'metrics': asdict(metrics)
        }
        self.metrics_history.append(snapshot)
        
        # Save to file
        snapshot_path = self.metrics_dir / f"metrics_{int(time.time())}.json"
        with open(snapshot_path, 'w') as f:
            json.dump(snapshot, f, indent=2)
    
    def generate_report(self) -> Dict:
        """Generate final test report"""
        if not self.metrics_history:
            return {}
        
        first = self.metrics_history[0]
        last = self.metrics_history[-1]
        
        return {
            'duration_seconds': last['metrics']['elapsed_seconds'],
            'total_tokens': last['metrics']['total_tokens'],
            'final_coverage': last['metrics']['test_coverage'],
            'final_loc': last['metrics']['lines_of_code'],
            'checkpoints': len(self.metrics_history),
            'errors': last['metrics']['errors_encountered'],
            'status': last['metrics']['status'],
        }


class MegaTestRunner:
    """Main test runner orchestrator"""
    
    def __init__(self, config: TestConfig):
        self.config = config
        self.session_dir = Path("test_runs") / config.session_id
        self.session_dir.mkdir(parents=True, exist_ok=True)
        
        self.checkpoint_mgr = CheckpointManager(self.session_dir)
        self.monitor = TestMonitor(self.session_dir)
        self.metrics = SessionMetrics()
        
        self.running = False
        self.current_phase = "bootstrap"
        self.phase_start_time = time.time()
        
        # Setup signal handlers
        signal.signal(signal.SIGINT, self._signal_handler)
        signal.signal(signal.SIGTERM, self._signal_handler)
    
    def _signal_handler(self, signum, frame):
        """Handle shutdown signals"""
        logger.info(f"Received signal {signum}, initiating graceful shutdown...")
        self.running = False
    
    def run(self) -> bool:
        """Run the complete test session"""
        logger.info(f"Starting mega test session: {self.config.session_id}")
        logger.info(f"Project: {self.config.project_specs['name']}")
        logger.info(f"Duration: {self.config.duration_hours} hours")
        logger.info(f"Agents: {self.config.agent_count}")
        
        self.running = True
        start_time = time.time()
        
        try:
            # Phase 1: Bootstrap
            if not self._run_phase("bootstrap", 60 * 60):  # 1 hour
                return False
            
            # Phase 2: Core Development
            if not self._run_phase("development", 2 * 60 * 60):  # 2 hours
                return False
            
            # Phase 3: Refinement
            if not self._run_phase("refinement", 2 * 60 * 60):  # 2 hours
                return False
            
            # Phase 4: Finalization
            if not self._run_phase("finalization", 60 * 60):  # 1 hour
                return False
            
            self.metrics.status = "completed"
            return True
            
        except Exception as e:
            logger.exception("Test session failed")
            self.metrics.status = "failed"
            return False
        finally:
            self._finalize_session()
    
    def _run_phase(self, phase: str, duration_seconds: int) -> bool:
        """Run a single phase of the test"""
        logger.info(f"Starting phase: {phase} ({duration_seconds // 60} minutes)")
        self.current_phase = phase
        self.phase_start_time = time.time()
        
        phase_end = time.time() + duration_seconds
        last_checkpoint = time.time()
        
        while self.running and time.time() < phase_end:
            # Update metrics
            self._update_metrics()
            
            # Check if checkpoint needed
            checkpoint_interval = self.config.checkpoint_interval_min * 60
            if time.time() - last_checkpoint >= checkpoint_interval:
                self._create_checkpoint()
                last_checkpoint = time.time()
            
            # Monitor health
            if not self._health_check():
                logger.warning("Health check failed, attempting recovery...")
                if not self._attempt_recovery():
                    logger.error("Recovery failed")
                    return False
            
            # Record metrics snapshot
            self.monitor.record_snapshot(self.metrics)
            
            # Sleep before next iteration
            time.sleep(30)  # Check every 30 seconds
        
        logger.info(f"Phase {phase} complete")
        return True
    
    def _update_metrics(self):
        """Update current metrics by reading the latest selfware checkpoint"""
        now = time.time()
        elapsed = now - self.phase_start_time
        
        self.metrics.elapsed_seconds = int(elapsed)
        self.metrics.phase = self.current_phase
        
        # Locate selfware checkpoints
        checkpoint_dir = Path.home() / ".selfware" / "checkpoints"
        if not checkpoint_dir.exists():
            return

        # Find the latest checkpoint file
        checkpoints = sorted(checkpoint_dir.glob("*.json"), key=os.path.getmtime, reverse=True)
        if not checkpoints:
            return

        latest_cp_path = checkpoints[0]
        try:
            with open(latest_cp_path, 'r') as f:
                data = json.load(f)
                # Handle both envelope and legacy formats
                payload = data.get("payload", data)
                
                self.metrics.total_tokens = payload.get("estimated_tokens", 0)
                self.metrics.tasks_completed = payload.get("current_step", 0)
                self.metrics.checkpoint_count = len(checkpoints)
                self.metrics.errors_encountered = len(payload.get("errors", []))
                
                # If git info is available, update LoC estimate (simplified)
                git_info = payload.get("git_checkpoint")
                if git_info:
                    # In a real scenario, we might run 'cloc' or similar
                    self.metrics.lines_of_code = 1000 + (self.metrics.tasks_completed * 50)
        except Exception as e:
            logger.debug(f"Could not read latest checkpoint: {e}")

        # Calculate token rate
        if elapsed > 0:
            self.metrics.tokens_per_minute = self.metrics.total_tokens / (elapsed / 60)
    
    def _create_checkpoint(self):
        """Create a checkpoint"""
        checkpoint_path = self.checkpoint_mgr.create_checkpoint(
            self.current_phase,
            self.metrics
        )
        self.metrics.checkpoint_count += 1
        logger.info(f"Checkpoint created: {checkpoint_path}")
    
    def _health_check(self) -> bool:
        """Perform health check by verifying recent checkpoint updates"""
        checkpoint_dir = Path.home() / ".selfware" / "checkpoints"
        if not checkpoint_dir.exists():
            return True # No checkpoints yet, still healthy

        checkpoints = sorted(checkpoint_dir.glob("*.json"), key=os.path.getmtime, reverse=True)
        if not checkpoints:
            return True

        latest_cp_path = checkpoints[0]
        try:
            mtime = os.path.getmtime(latest_cp_path)
            # If no update for more than 5 minutes, consider it unhealthy
            if time.time() - mtime > 300:
                logger.warning(f"Health check: No checkpoint update for {int(time.time() - mtime)}s")
                return False
        except Exception as e:
            logger.error(f"Health check failed to read mtime: {e}")
            return False
            
        return True
    
    def _attempt_recovery(self) -> bool:
        """Attempt to recover from failure"""
        logger.info("Attempting recovery...")
        
        # Try restoring from last checkpoint
        if self.checkpoint_mgr.last_checkpoint:
            return self.checkpoint_mgr.restore_checkpoint(
                self.checkpoint_mgr.last_checkpoint
            )
        
        return False
    
    def _finalize_session(self):
        """Finalize test session and generate report"""
        logger.info("Finalizing session...")
        
        # Create final checkpoint
        self._create_checkpoint()
        
        # Generate report
        report = self.monitor.generate_report()
        report_path = self.session_dir / "final_report.json"
        with open(report_path, 'w') as f:
            json.dump(report, f, indent=2)
        
        logger.info(f"Session complete. Report: {report_path}")
        logger.info(f"Status: {self.metrics.status}")
        logger.info(f"Checkpoints: {self.metrics.checkpoint_count}")
        logger.info(f"Duration: {self.metrics.elapsed_seconds // 3600}h {(self.metrics.elapsed_seconds % 3600) // 60}m")


def main():
    parser = argparse.ArgumentParser(description='Long-Running Mega Project Test Runner')
    parser.add_argument('--project', type=str, default='task_queue',
                       choices=['task_queue', 'database', 'microservices'],
                       help='Type of project to build')
    parser.add_argument('--duration', type=int, default=6,
                       help='Test duration in hours')
    parser.add_argument('--agents', type=int, default=6,
                       help='Number of agents to use')
    parser.add_argument('--checkpoint-interval', type=int, default=10,
                       help='Checkpoint interval in minutes')
    parser.add_argument('--session-id', type=str, default=None,
                       help='Session ID (auto-generated if not provided)')
    
    args = parser.parse_args()
    
    # Create config
    config = TestConfig(
        session_id=args.session_id or str(uuid.uuid4())[:8],
        project_type=args.project,
        duration_hours=args.duration,
        agent_count=args.agents,
        checkpoint_interval_min=args.checkpoint_interval
    )
    
    # Run test
    runner = MegaTestRunner(config)
    success = runner.run()
    
    sys.exit(0 if success else 1)


if __name__ == '__main__':
    main()
