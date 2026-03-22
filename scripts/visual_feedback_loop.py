#!/usr/bin/env python3
"""
Visual Feedback Loop for Selfware Development Tests

Captures screenshots of web projects and uses vision-capable models
to review and provide feedback for iterative improvement.
"""

import os
import sys
import time
import json
import base64
import requests
from pathlib import Path
from datetime import datetime
from typing import Optional, Dict, List
import subprocess


class VisualFeedbackLoop:
    """Manages screenshot capture and vision-based review"""
    
    def __init__(
        self,
        test_dir: Path,
        endpoint: str = "http://localhost:8000/v1",
        screenshot_interval: int = 120,  # 2 minutes
        review_interval: int = 300,      # 5 minutes
    ):
        self.test_dir = Path(test_dir)
        self.endpoint = endpoint
        self.screenshot_interval = screenshot_interval
        self.review_interval = review_interval
        self.running = False
        
        # Track reviewed screenshots to avoid duplicates
        self.reviewed_screenshots: set = set()
        
    def check_playwright(self) -> bool:
        """Check if playwright is available for screenshots"""
        try:
            result = subprocess.run(
                ["npx", "playwright", "--version"],
                capture_output=True,
                timeout=10
            )
            return result.returncode == 0
        except:
            return False
    
    def capture_screenshot(self, html_file: Path, output_file: Path) -> bool:
        """Capture screenshot of HTML file using playwright"""
        try:
            result = subprocess.run(
                [
                    "npx", "playwright", "screenshot",
                    "--browser=chromium",
                    "--viewport-size=1280,720",
                    "--full-page",
                    f"file://{html_file.absolute()}",
                    str(output_file)
                ],
                capture_output=True,
                timeout=30
            )
            return result.returncode == 0 and output_file.exists()
        except Exception as e:
            print(f"Screenshot error: {e}")
            return False
    
    def encode_image(self, image_path: Path) -> str:
        """Encode image to base64 for API"""
        with open(image_path, "rb") as f:
            return base64.b64encode(f.read()).decode("utf-8")
    
    def review_with_vision(self, screenshot_path: Path, task_type: str) -> Optional[str]:
        """Review screenshot using vision-capable model"""
        try:
            base64_image = self.encode_image(screenshot_path)
            
            # Determine review criteria based on task type
            if task_type == "flappy_bird":
                review_prompt = """You are reviewing a Flappy Bird game implementation.
                
Analyze this screenshot and provide feedback:
1. Is the game visually complete (bird, pipes, background visible)?
2. Are there any obvious visual bugs or layout issues?
3. Does it look playable?
4. What improvements would you suggest?

Be specific and actionable. If the game is not visible or broken, state exactly what's wrong."""
            
            elif task_type == "portfolio":
                review_prompt = """You are reviewing a portfolio website.

Analyze this screenshot and provide feedback:
1. Visual design quality (0-10)
2. Layout issues or broken elements
3. Responsiveness concerns (based on viewport)
4. Typography and color scheme
5. Specific improvements needed

Be constructive and specific about what to fix."""
            else:
                return None
            
            # Call vision model API
            response = requests.post(
                f"{self.endpoint}/chat/completions",
                headers={"Content-Type": "application/json"},
                json={
                    "model": "qwen3.5-27b",  # Adjust based on your model
                    "messages": [
                        {
                            "role": "user",
                            "content": [
                                {"type": "text", "text": review_prompt},
                                {
                                    "type": "image_url",
                                    "image_url": {
                                        "url": f"data:image/png;base64,{base64_image}"
                                    }
                                }
                            ]
                        }
                    ],
                    "max_tokens": 1000,
                    "temperature": 0.7
                },
                timeout=60
            )
            
            if response.status_code == 200:
                result = response.json()
                feedback = result["choices"][0]["message"]["content"]
                return feedback
            else:
                print(f"API error: {response.status_code} - {response.text}")
                return None
                
        except Exception as e:
            print(f"Review error: {e}")
            return None
    
    def save_feedback(self, instance_dir: Path, screenshot_path: Path, feedback: str):
        """Save review feedback for instance"""
        feedback_file = instance_dir / "visual_feedback.jsonl"
        
        entry = {
            "timestamp": datetime.now().isoformat(),
            "screenshot": str(screenshot_path.name),
            "feedback": feedback
        }
        
        with open(feedback_file, "a") as f:
            f.write(json.dumps(entry) + "\n")
    
    def scan_and_capture(self):
        """Scan all instances and capture screenshots"""
        tasks = ["flappy_bird", "portfolio_website"]
        
        for task in tasks:
            for i in range(1, 5):
                instance_dir = self.test_dir / task / f"instance_{i}"
                html_file = instance_dir / "index.html"
                
                if not html_file.exists():
                    continue
                
                # Create screenshot filename
                timestamp = datetime.now().strftime("%H%M%S")
                screenshot_dir = instance_dir / "screenshots"
                screenshot_dir.mkdir(exist_ok=True)
                screenshot_path = screenshot_dir / f"screenshot_{timestamp}.png"
                
                # Capture screenshot
                if self.capture_screenshot(html_file, screenshot_path):
                    print(f"📸 Captured: {task}/instance_{i} at {timestamp}")
                    
                    # Queue for review if new
                    if screenshot_path not in self.reviewed_screenshots:
                        self.reviewed_screenshots.add(screenshot_path)
                        
                        # Immediate review for new screenshots
                        feedback = self.review_with_vision(screenshot_path, task)
                        if feedback:
                            self.save_feedback(instance_dir, screenshot_path, feedback)
                            print(f"   💬 Review: {feedback[:100]}...")
    
    def provide_feedback_to_selfware(self, instance_dir: Path, feedback: str):
        """Write feedback in a format selfware can read"""
        feedback_path = instance_dir / "REVIEW_FEEDBACK.md"
        
        content = f"""# Visual Review Feedback

**Generated:** {datetime.now().isoformat()}

## Reviewer Comments

{feedback}

## Action Items

Please address the feedback above in your next iteration.
"""
        
        feedback_path.write_text(content)
    
    def run(self):
        """Main loop for visual feedback"""
        print("=" * 50)
        print("  Visual Feedback Loop")
        print("=" * 50)
        print(f"Test Directory: {self.test_dir}")
        print(f"Screenshot Interval: {self.screenshot_interval}s")
        print(f"Review Interval: {self.review_interval}s")
        print("")
        
        if not self.check_playwright():
            print("⚠️  Playwright not available!")
            print("Install with: npm install -g @playwright/test")
            print("Then: npx playwright install chromium")
            return
        
        print("✅ Playwright available")
        
        # Check endpoint
        try:
            response = requests.get(f"{self.endpoint}/models", timeout=5)
            if response.status_code == 200:
                print("✅ LLM endpoint available")
            else:
                print("⚠️  LLM endpoint check failed")
        except Exception as e:
            print(f"⚠️  Cannot reach LLM endpoint: {e}")
            print("Screenshots will be captured but not reviewed")
        
        print("")
        print("Starting visual feedback loop...")
        print("Press Ctrl+C to stop")
        print("")
        
        self.running = True
        last_screenshot = 0
        
        try:
            while self.running:
                current_time = time.time()
                
                # Capture screenshots
                if current_time - last_screenshot >= self.screenshot_interval:
                    self.scan_and_capture()
                    last_screenshot = current_time
                
                time.sleep(10)
                
        except KeyboardInterrupt:
            print("\n\nStopping visual feedback loop...")
            self.running = False
    
    def generate_summary(self):
        """Generate summary of all visual feedback"""
        summary_file = self.test_dir / "VISUAL_REVIEW_SUMMARY.md"
        
        lines = [
            "# Visual Review Summary",
            "",
            f"**Generated:** {datetime.now().isoformat()}",
            "",
        ]
        
        for task in ["flappy_bird", "portfolio_website"]:
            lines.append(f"## {task.replace('_', ' ').title()}")
            lines.append("")
            
            for i in range(1, 5):
                instance_dir = self.test_dir / task / f"instance_{i}"
                feedback_file = instance_dir / "visual_feedback.jsonl"
                
                lines.append(f"### Instance {i}")
                lines.append("")
                
                if feedback_file.exists():
                    entries = []
                    with open(feedback_file) as f:
                        for line in f:
                            try:
                                entries.append(json.loads(line))
                            except:
                                pass
                    
                    if entries:
                        # Show latest feedback
                        latest = entries[-1]
                        lines.append(f"**Latest Review ({latest['timestamp']}):**")
                        lines.append("")
                        lines.append(latest['feedback'])
                        lines.append("")
                        lines.append(f"*Total reviews: {len(entries)}*")
                    else:
                        lines.append("No reviews recorded")
                else:
                    lines.append("No feedback file")
                
                lines.append("")
        
        summary_file.write_text("\n".join(lines))
        print(f"Summary saved: {summary_file}")
        return summary_file


def main():
    import argparse
    
    parser = argparse.ArgumentParser(description="Visual Feedback Loop for Selfware")
    parser.add_argument("test_dir", help="Test directory to monitor")
    parser.add_argument("--endpoint", default="http://localhost:8000/v1", help="LLM endpoint")
    parser.add_argument("--screenshot-interval", type=int, default=120, help="Seconds between screenshots")
    parser.add_argument("--summary", action="store_true", help="Generate summary only")
    
    args = parser.parse_args()
    
    vfl = VisualFeedbackLoop(
        test_dir=Path(args.test_dir),
        endpoint=args.endpoint,
        screenshot_interval=args.screenshot_interval
    )
    
    if args.summary:
        vfl.generate_summary()
    else:
        vfl.run()


if __name__ == "__main__":
    main()
