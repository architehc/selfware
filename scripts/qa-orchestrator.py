#!/usr/bin/env python3
"""
Selfware QA Orchestrator

Coordinates quality assurance across multiple programming languages
for agentic code generation workflows.

Usage:
    python qa-orchestrator.py --action run --language rust --config qa-schema.yaml
    python qa-orchestrator.py --action aggregate --reports-dir reports/ --output unified.json
    python qa-orchestrator.py --action feedback --report unified.json --output feedback.json
"""

import argparse
import asyncio
import json
import logging
import os
import subprocess
import sys
import time
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any, Optional

import yaml

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger("qa-orchestrator")


class StageStatus(Enum):
    """Status of a QA stage."""
    PENDING = "pending"
    RUNNING = "running"
    PASSED = "passed"
    FAILED = "failed"
    SKIPPED = "skipped"
    ERROR = "error"


@dataclass
class StageResult:
    """Result of a QA stage execution."""
    name: str
    status: StageStatus
    duration_ms: int = 0
    error_message: Optional[str] = None
    output: dict = field(default_factory=dict)


@dataclass
class QualityReport:
    """Unified quality report for generated code."""
    report_version: str = "1.0"
    timestamp: str = field(default_factory=lambda: time.strftime("%Y-%m-%dT%H:%M:%SZ"))
    project: str = ""
    languages: list = field(default_factory=list)
    stages: list = field(default_factory=list)
    quality_score: float = 0.0
    grade: str = "F"
    passed: bool = False
    summary: dict = field(default_factory=dict)


class QualityScorer:
    """Calculate quality scores for generated code."""
    
    WEIGHTS = {
        "syntax": 0.10,
        "format": 0.05,
        "lint": 0.15,
        "typecheck": 0.10,
        "test": 0.30,
        "coverage": 0.20,
        "security": 0.10,
    }
    
    GRADE_THRESHOLDS = {
        "S": 95,
        "A": 90,
        "B": 80,
        "C": 70,
        "D": 60,
        "F": 0,
    }
    
    def calculate(self, results: dict[str, StageResult]) -> dict[str, Any]:
        """Calculate overall quality score from stage results."""
        scores = {}
        
        # Syntax score (binary)
        syntax = results.get("syntax")
        scores["syntax"] = 100 if syntax and syntax.status == StageStatus.PASSED else 0
        
        # Format score (binary)
        fmt = results.get("format")
        scores["format"] = 100 if fmt and fmt.status == StageStatus.PASSED else 0
        
        # Lint score (error count based)
        lint = results.get("lint")
        if lint and lint.status == StageStatus.PASSED:
            scores["lint"] = 100
        elif lint:
            error_count = lint.output.get("error_count", 5)
            scores["lint"] = max(0, 100 - error_count * 5)
        else:
            scores["lint"] = 0
        
        # Type check score (binary)
        typecheck = results.get("typecheck")
        scores["typecheck"] = 100 if typecheck and typecheck.status == StageStatus.PASSED else 0
        
        # Test score (pass rate)
        test = results.get("test")
        if test and test.output:
            total = test.output.get("total", 0)
            passed = test.output.get("passed", 0)
            scores["test"] = (passed / total * 100) if total > 0 else 0
        else:
            scores["test"] = 0
        
        # Coverage score (min threshold 80%)
        coverage = results.get("coverage")
        if coverage and coverage.output:
            cov_pct = coverage.output.get("percentage", 0)
            scores["coverage"] = min(100, (cov_pct / 80) * 100) if cov_pct < 80 else 100
        else:
            scores["coverage"] = 0
        
        # Security score (vulnerability based)
        security = results.get("security")
        if security and security.output:
            critical = security.output.get("critical", 0)
            high = security.output.get("high", 0)
            medium = security.output.get("medium", 0)
            scores["security"] = max(0, 100 - critical * 50 - high * 20 - medium * 5)
        else:
            scores["security"] = 100
        
        # Weighted total
        total = sum(scores[k] * self.WEIGHTS[k] for k in self.WEIGHTS if k in scores)
        
        return {
            "overall": round(total, 1),
            "breakdown": scores,
            "grade": self._grade(total),
            "passed": total >= 80 and scores.get("security", 100) >= 70
        }
    
    def _grade(self, score: float) -> str:
        """Convert score to letter grade."""
        for grade, threshold in sorted(self.GRADE_THRESHOLDS.items(), key=lambda x: -x[1]):
            if score >= threshold:
                return grade
        return "F"


class QAPipeline:
    """Quality Assurance Pipeline for a single language."""
    
    def __init__(self, config: dict, language: str, working_dir: str):
        self.config = config
        self.language = language
        self.working_dir = Path(working_dir)
        self.results: dict[str, StageResult] = {}
    
    async def run_stage(self, stage_config: dict) -> StageResult:
        """Run a single QA stage."""
        stage_name = stage_config["name"]
        start_time = time.time()
        
        logger.info(f"Running stage: {stage_name} for {self.language}")
        
        # Get tools for this language
        tools = stage_config.get("tools", {}).get(self.language, [])
        if not tools:
            logger.warning(f"No tools configured for {stage_name} / {self.language}")
            return StageResult(
                name=stage_name,
                status=StageStatus.SKIPPED,
                output={"message": "No tools configured"}
            )
        
        # Run each tool
        all_passed = True
        combined_output = {}
        
        for tool in tools:
            cmd = tool["command"] if isinstance(tool, dict) else tool
            env = tool.get("env", {}) if isinstance(tool, dict) else {}
            
            try:
                result = await self._run_command(cmd, env)
                combined_output[cmd] = result
                
                if result["returncode"] != 0:
                    all_passed = False
                    
            except Exception as e:
                logger.error(f"Tool failed: {cmd} - {e}")
                all_passed = False
                combined_output[cmd] = {"error": str(e)}
        
        duration_ms = int((time.time() - start_time) * 1000)
        
        status = StageStatus.PASSED if all_passed else StageStatus.FAILED
        
        return StageResult(
            name=stage_name,
            status=status,
            duration_ms=duration_ms,
            output=combined_output
        )
    
    async def _run_command(self, cmd: str, env: dict) -> dict:
        """Run a shell command and return results."""
        full_env = {**os.environ, **env}
        
        process = await asyncio.create_subprocess_shell(
            cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd=self.working_dir,
            env=full_env
        )
        
        stdout, stderr = await process.communicate()
        
        return {
            "returncode": process.returncode,
            "stdout": stdout.decode().strip(),
            "stderr": stderr.decode().strip(),
        }
    
    async def run(self) -> list[StageResult]:
        """Run all configured stages."""
        stages = self.config.get("stages", [])
        results = []
        
        for stage_config in stages:
            if not stage_config.get("required", True):
                logger.info(f"Skipping optional stage: {stage_config['name']}")
                continue
            
            result = await self.run_stage(stage_config)
            results.append(result)
            self.results[result.name] = result
            
            # Fail fast if configured
            if stage_config.get("fail_fast", False) and result.status == StageStatus.FAILED:
                logger.error(f"Stage {result.name} failed, stopping pipeline")
                break
        
        return results


class QAOrchestrator:
    """Main QA Orchestrator for multi-language projects."""
    
    def __init__(self, config_path: str):
        self.config = self._load_config(config_path)
        self.scorer = QualityScorer()
    
    def _load_config(self, path: str) -> dict:
        """Load QA configuration from YAML file."""
        with open(path, 'r') as f:
            # Load all documents, use first qa_profile found
            for doc in yaml.safe_load_all(f):
                if doc and "qa_profile" in doc:
                    return doc["qa_profile"]
        raise ValueError("No qa_profile found in config file")
    
    async def run_language(self, language: str, working_dir: str) -> list[StageResult]:
        """Run QA pipeline for a specific language."""
        pipeline = QAPipeline(self.config, language, working_dir)
        return await pipeline.run()
    
    def aggregate_reports(
        self,
        languages: list[str],
        reports_dir: str,
        output_path: str
    ) -> QualityReport:
        """Aggregate individual language reports into unified report."""
        report = QualityReport(
            project="generated-code",
            languages=languages
        )
        
        reports_path = Path(reports_dir)
        all_results: dict[str, StageResult] = {}
        
        # Load individual reports
        for lang in languages:
            lang_report_path = reports_path / f"{lang}-report.json"
            if lang_report_path.exists():
                with open(lang_report_path) as f:
                    data = json.load(f)
                    # Convert to StageResult objects
                    for stage_data in data.get("stages", []):
                        stage = StageResult(
                            name=stage_data["name"],
                            status=StageStatus(stage_data["status"]),
                            duration_ms=stage_data.get("duration_ms", 0),
                            output=stage_data.get("output", {})
                        )
                        all_results[stage.name] = stage
                        report.stages.append(stage_data)
        
        # Calculate quality score
        score_data = self.scorer.calculate(all_results)
        report.quality_score = score_data["overall"]
        report.grade = score_data["grade"]
        report.passed = score_data["passed"]
        
        # Generate summary
        report.summary = {
            "total_stages": len(report.stages),
            "passed_stages": sum(1 for s in report.stages if s.get("status") == "passed"),
            "failed_stages": sum(1 for s in report.stages if s.get("status") == "failed"),
        }
        
        # Save report
        with open(output_path, 'w') as f:
            json.dump(report.__dict__, f, indent=2, default=str)
        
        logger.info(f"Unified report saved to {output_path}")
        logger.info(f"Quality Score: {report.quality_score}/100 (Grade: {report.grade})")
        
        return report
    
    def generate_feedback(
        self,
        report_path: str,
        output_path: str
    ) -> dict:
        """Generate feedback for agentic improvement."""
        with open(report_path) as f:
            report = json.load(f)
        
        feedback = {
            "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ"),
            "quality_score": report.get("quality_score", 0),
            "passed": report.get("passed", False),
            "recommendations": [],
            "issues": [],
            "retry_suggested": False
        }
        
        # Analyze stages for issues
        for stage in report.get("stages", []):
            if stage.get("status") != "passed":
                feedback["issues"].append({
                    "stage": stage["name"],
                    "status": stage["status"],
                    "details": stage.get("output", {})
                })
                feedback["retry_suggested"] = True
        
        # Generate recommendations based on score breakdown
        score_breakdown = report.get("score_breakdown", {})
        
        if score_breakdown.get("coverage", 100) < 80:
            feedback["recommendations"].append(
                "Increase test coverage to at least 80%"
            )
        
        if score_breakdown.get("lint", 100) < 100:
            feedback["recommendations"].append(
                "Fix linting errors to ensure code quality"
            )
        
        if score_breakdown.get("security", 100) < 100:
            feedback["recommendations"].append(
                "Address security vulnerabilities immediately"
            )
        
        # Save feedback
        with open(output_path, 'w') as f:
            json.dump(feedback, f, indent=2)
        
        logger.info(f"Feedback saved to {output_path}")
        
        return feedback


def main():
    parser = argparse.ArgumentParser(description="Selfware QA Orchestrator")
    parser.add_argument(
        "--action",
        choices=["run", "aggregate", "feedback"],
        required=True,
        help="Action to perform"
    )
    parser.add_argument("--config", help="Path to QA config YAML")
    parser.add_argument("--language", help="Language to run QA for")
    parser.add_argument("--working-dir", default=".", help="Working directory")
    parser.add_argument("--languages", help="JSON array of languages")
    parser.add_argument("--reports-dir", help="Directory containing reports")
    parser.add_argument("--report", help="Path to unified report")
    parser.add_argument("--output", help="Output file path")
    parser.add_argument("--profile", default="standard", help="QA profile to use")
    
    args = parser.parse_args()
    
    if args.action == "run":
        if not args.config or not args.language:
            parser.error("--config and --language required for 'run' action")
        
        orchestrator = QAOrchestrator(args.config)
        results = asyncio.run(orchestrator.run_language(
            args.language,
            args.working_dir
        ))
        
        # Output results as JSON
        output = {
            "language": args.language,
            "stages": [
                {
                    "name": r.name,
                    "status": r.status.value,
                    "duration_ms": r.duration_ms,
                    "output": r.output
                }
                for r in results
            ]
        }
        
        if args.output:
            with open(args.output, 'w') as f:
                json.dump(output, f, indent=2)
        else:
            print(json.dumps(output, indent=2))
    
    elif args.action == "aggregate":
        if not args.config or not args.languages:
            parser.error("--config and --languages required for 'aggregate' action")
        
        orchestrator = QAOrchestrator(args.config)
        languages = json.loads(args.languages)
        
        report = orchestrator.aggregate_reports(
            languages,
            args.reports_dir or "reports/",
            args.output or "unified-report.json"
        )
        
        print(json.dumps(report.__dict__, indent=2, default=str))
    
    elif args.action == "feedback":
        if not args.report:
            parser.error("--report required for 'feedback' action")
        
        orchestrator = QAOrchestrator(args.config or "selfware-qa-schema.yaml")
        feedback = orchestrator.generate_feedback(
            args.report,
            args.output or "feedback.json"
        )
        
        print(json.dumps(feedback, indent=2))


if __name__ == "__main__":
    main()
