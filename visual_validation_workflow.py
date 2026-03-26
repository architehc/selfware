#!/usr/bin/env python3
"""
Visual Validation Workflow for Selfware
Uses 2x RTX 4090 endpoint with multi-modal capabilities

Pipeline:
1. Design & Build Website (Agent)
2. Serve Website Locally
3. Capture Screenshots (Multi-device/resolution)
4. Vision Analysis (Qwen3.5 Vision)
5. Feedback & Improvements Loop
6. Pipeline Editor for on-the-fly changes
"""

import asyncio
import aiohttp
import json
import subprocess
import time
import os
import tempfile
import shutil
from pathlib import Path
from datetime import datetime
from typing import List, Dict, Optional, Tuple
import base64

# Configuration
ENDPOINT = "http://localhost:8000/v1"
MODEL = "qwen3.5-27b"
WORKSPACE = "/tmp/visual_validation_workspace"
SCREENSHOT_DIR = f"{WORKSPACE}/screenshots"
ITERATION = 0
MAX_ITERATIONS = 5

class VisualValidationPipeline:
    def __init__(self):
        self.workspace = Path(WORKSPACE)
        self.workspace.mkdir(parents=True, exist_ok=True)
        self.screenshot_dir = Path(SCREENSHOT_DIR)
        self.screenshot_dir.mkdir(exist_ok=True)
        self.iteration = 0
        self.feedback_history = []
        
    async def call_vision_model(self, prompt: str, image_path: Optional[str] = None) -> Dict:
        """Call the multi-modal endpoint with optional image."""
        
        messages = [{"role": "user", "content": prompt}]
        
        if image_path and os.path.exists(image_path):
            # Encode image to base64
            with open(image_path, "rb") as f:
                image_data = base64.b64encode(f.read()).decode('utf-8')
            
            # Multi-modal message format
            messages = [{
                "role": "user",
                "content": [
                    {"type": "text", "text": prompt},
                    {"type": "image_url", "image_url": {"url": f"data:image/png;base64,{image_data}"}}
                ]
            }]
        
        payload = {
            "model": MODEL,
            "messages": messages,
            "max_tokens": 2048,
            "temperature": 0.6,
        }
        
        async with aiohttp.ClientSession() as session:
            async with session.post(f"{ENDPOINT}/chat/completions", json=payload) as resp:
                if resp.status == 200:
                    data = await resp.json()
                    return {
                        "success": True,
                        "content": data["choices"][0]["message"].get("content", ""),
                        "reasoning": data["choices"][0]["message"].get("reasoning", "")
                    }
                else:
                    return {"success": False, "error": await resp.text()}
    
    async def run_selfware_agent(self, task: str, config_file: str = "selfware-stress-test.toml") -> str:
        """Run a selfware agent with the given task."""
        
        cmd = [
            "./target/release/selfware",
            "-c", config_file,
            "run", task,
            "-y"
        ]
        
        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd="/home/ivo/selfware"
        )
        
        stdout, stderr = await proc.communicate()
        return stdout.decode() + stderr.decode()
    
    def serve_website(self, port: int = 8080) -> subprocess.Popen:
        """Start a local HTTP server for the website."""
        
        website_dir = self.workspace / "website"
        website_dir.mkdir(exist_ok=True)
        
        # Create index.html if it doesn't exist
        if not (website_dir / "index.html").exists():
            (website_dir / "index.html").write_text("""
<!DOCTYPE html>
<html>
<head><title>Placeholder</title></head>
<body><h1>Website Under Construction</h1></body>
</html>
""")
        
        # Start Python HTTP server
        proc = subprocess.Popen(
            ["python3", "-m", "http.server", str(port)],
            cwd=str(website_dir),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE
        )
        
        time.sleep(2)  # Wait for server to start
        return proc
    
    async def capture_screenshot(self, url: str, filename: str, 
                                  width: int = 1920, height: int = 1080) -> str:
        """Capture screenshot using Playwright or similar."""
        
        screenshot_path = self.screenshot_dir / filename
        
        # Try playwright first
        try:
            playwright_script = f"""
from playwright.sync_api import sync_playwright
with sync_playwright() as p:
    browser = p.chromium.launch()
    page = browser.new_page(viewport={{'width': {width}, 'height': {height}}})
    page.goto('{url}')
    page.wait_for_load_state('networkidle')
    page.screenshot(path='{screenshot_path}', full_page=True)
    browser.close()
"""
            with tempfile.NamedTemporaryFile(mode='w', suffix='.py', delete=False) as f:
                f.write(playwright_script)
                script_path = f.name
            
            result = subprocess.run(
                ['python3', script_path],
                capture_output=True,
                text=True,
                timeout=30
            )
            
            os.unlink(script_path)
            
            if result.returncode == 0:
                return str(screenshot_path)
            else:
                print(f"Playwright error: {result.stderr}")
                
        except Exception as e:
            print(f"Playwright failed: {e}")
        
        # Fallback: Use selenium if playwright not available
        try:
            from selenium import webdriver
            from selenium.webdriver.chrome.options import Options
            from selenium.webdriver.chrome.service import Service
            
            chrome_options = Options()
            chrome_options.add_argument('--headless')
            chrome_options.add_argument(f'--window-size={width},{height}')
            
            driver = webdriver.Chrome(options=chrome_options)
            driver.get(url)
            time.sleep(3)  # Wait for load
            driver.save_screenshot(str(screenshot_path))
            driver.quit()
            
            return str(screenshot_path)
            
        except Exception as e:
            print(f"Selenium fallback failed: {e}")
            return ""
    
    async def analyze_design(self, screenshot_path: str) -> Dict:
        """Analyze website design using vision model."""
        
        if not os.path.exists(screenshot_path):
            return {"success": False, "error": "Screenshot not found"}
        
        prompt = """Analyze this website screenshot and provide detailed feedback on:
1. Visual Design (colors, typography, spacing, layout)
2. User Experience (clarity, navigation, call-to-actions)
3. Responsive Design (if visible)
4. Issues or bugs visible
5. Specific improvements needed

Rate each category 1-10 and provide actionable suggestions."""
        
        result = await self.call_vision_model(prompt, screenshot_path)
        return result
    
    async def generate_improvements(self, design_analysis: str, 
                                     current_code: str,
                                     iteration: int) -> str:
        """Generate code improvements based on analysis."""
        
        prompt = f"""Based on the design analysis feedback, generate improved HTML/CSS/JS code.

ITERATION: {iteration}

DESIGN ANALYSIS FEEDBACK:
{design_analysis}

CURRENT CODE:
```html
{current_code}
```

Generate the complete improved index.html file. Include:
- All CSS inline or in style tags
- All JavaScript inline or in script tags
- Responsive design fixes
- Visual improvements based on feedback
- Dark/light mode toggle if appropriate

Return ONLY the complete HTML file content, ready to save as index.html."""

        result = await self.call_vision_model(prompt)
        return result.get("content", "")
    
    async def run_pipeline_iteration(self, design_prompt: str) -> Dict:
        """Run one iteration of the pipeline."""
        
        self.iteration += 1
        print(f"\n{'='*60}")
        print(f"ITERATION {self.iteration}/{MAX_ITERATIONS}")
        print(f"{'='*60}\n")
        
        # Step 1: Build/Improve Website
        print("[1/5] Building website...")
        
        if self.iteration == 1:
            # First iteration - build from scratch
            task = f"""Create a beautiful, modern website in workspace/website/index.html.
            
Requirements:
{design_prompt}

The website should be:
- Fully self-contained (CSS in style tags, JS in script tags)
- Responsive (mobile, tablet, desktop)
- Visually stunning with animations
- Professional quality

Create the file at: /tmp/visual_validation_workspace/website/index.html"""
            
            output = await self.run_selfware_agent(task)
            print(f"Build output: {output[:500]}...")
        else:
            # Subsequent iterations - improve based on feedback
            current_code = ""
            website_file = self.workspace / "website" / "index.html"
            if website_file.exists():
                current_code = website_file.read_text()
            
            if self.feedback_history:
                last_feedback = self.feedback_history[-1]
                improved_code = await self.generate_improvements(
                    last_feedback.get("content", ""),
                    current_code,
                    self.iteration
                )
                
                # Extract HTML from response
                if "```html" in improved_code:
                    html_content = improved_code.split("```html")[1].split("```")[0]
                elif "<!DOCTYPE" in improved_code:
                    html_content = improved_code[improved_code.find("<!DOCTYPE"):]
                else:
                    html_content = improved_code
                
                website_file.write_text(html_content)
                print("Website updated with improvements")
        
        # Step 2: Serve Website
        print("[2/5] Starting local server...")
        server = self.serve_website(port=8080 + self.iteration)
        url = f"http://localhost:{8080 + self.iteration}"
        
        # Step 3: Capture Screenshots (Multiple viewports)
        print("[3/5] Capturing screenshots...")
        screenshots = []
        
        # Desktop
        desktop_ss = await self.capture_screenshot(
            url, f"iteration_{self.iteration}_desktop.png", 1920, 1080
        )
        if desktop_ss:
            screenshots.append(("Desktop", desktop_ss))
        
        # Mobile
        mobile_ss = await self.capture_screenshot(
            url, f"iteration_{self.iteration}_mobile.png", 375, 812
        )
        if mobile_ss:
            screenshots.append(("Mobile", mobile_ss))
        
        # Step 4: Vision Analysis
        print("[4/5] Analyzing design...")
        analysis_results = []
        
        for device, screenshot in screenshots:
            print(f"  Analyzing {device} screenshot...")
            analysis = await self.analyze_design(screenshot)
            analysis["device"] = device
            analysis["screenshot"] = screenshot
            analysis_results.append(analysis)
        
        # Step 5: Decision
        print("[5/5] Evaluating...")
        
        should_continue = False
        for result in analysis_results:
            if result.get("success"):
                content = result.get("content", "").lower()
                # Check if improvements are still needed
                if any(word in content for word in ["improve", "fix", "issue", "bug", "problem"]):
                    should_continue = True
                    self.feedback_history.append(result)
                    print(f"  {result['device']}: Improvements needed")
                else:
                    print(f"  {result['device']}: Looks good!")
        
        # Cleanup server
        server.terminate()
        
        return {
            "iteration": self.iteration,
            "screenshots": screenshots,
            "analysis": analysis_results,
            "should_continue": should_continue and self.iteration < MAX_ITERATIONS,
            "url": url
        }
    
    async def run_full_pipeline(self, design_prompt: str):
        """Run the complete iterative pipeline."""
        
        print("╔════════════════════════════════════════════════════════════╗")
        print("║     VISUAL VALIDATION PIPELINE                            ║")
        print("║     Multi-Modal Website Builder + Validator               ║")
        print("╚════════════════════════════════════════════════════════════╝")
        print(f"\nEndpoint: {ENDPOINT}")
        print(f"Model: {MODEL}")
        print(f"Max Iterations: {MAX_ITERATIONS}\n")
        
        results = []
        
        while True:
            result = await self.run_pipeline_iteration(design_prompt)
            results.append(result)
            
            if not result["should_continue"]:
                print(f"\n✅ Pipeline complete after {self.iteration} iterations!")
                break
            
            print(f"\n🔄 Continuing to iteration {self.iteration + 1}...")
            await asyncio.sleep(2)
        
        # Generate final report
        await self.generate_report(results)
        
        return results
    
    async def generate_report(self, results: List[Dict]):
        """Generate final HTML report with all iterations."""
        
        report_html = """<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Visual Validation Report</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
            color: #eaeaea;
            min-height: 100vh;
            padding: 2rem;
        }
        .header {
            text-align: center;
            margin-bottom: 3rem;
        }
        .header h1 {
            font-size: 2.5rem;
            background: linear-gradient(135deg, #00d4ff, #7b2cbf);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            margin-bottom: 0.5rem;
        }
        .iteration {
            background: rgba(255,255,255,0.05);
            border-radius: 16px;
            padding: 2rem;
            margin-bottom: 2rem;
            border: 1px solid rgba(255,255,255,0.1);
        }
        .iteration-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 1.5rem;
        }
        .iteration-number {
            font-size: 1.5rem;
            color: #00d4ff;
        }
        .screenshots {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(400px, 1fr));
            gap: 1.5rem;
            margin-bottom: 1.5rem;
        }
        .screenshot-card {
            background: rgba(0,0,0,0.3);
            border-radius: 12px;
            overflow: hidden;
        }
        .screenshot-card img {
            width: 100%;
            height: auto;
            display: block;
        }
        .screenshot-label {
            padding: 1rem;
            font-weight: 600;
            color: #00d4ff;
        }
        .analysis {
            background: rgba(0,0,0,0.2);
            border-radius: 8px;
            padding: 1.5rem;
            white-space: pre-wrap;
            font-family: 'Monaco', 'Menlo', monospace;
            font-size: 0.9rem;
            line-height: 1.6;
            max-height: 400px;
            overflow-y: auto;
        }
        .final-website {
            text-align: center;
            margin-top: 3rem;
            padding: 3rem;
            background: linear-gradient(135deg, rgba(0,212,255,0.1), rgba(123,44,191,0.1));
            border-radius: 20px;
            border: 2px solid rgba(0,212,255,0.3);
        }
        .final-website a {
            display: inline-block;
            padding: 1rem 2rem;
            background: linear-gradient(135deg, #00d4ff, #7b2cbf);
            color: white;
            text-decoration: none;
            border-radius: 8px;
            font-weight: 600;
            margin-top: 1rem;
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>🎨 Visual Validation Report</h1>
        <p>Multi-Modal Website Builder Results</p>
    </div>
"""
        
        for result in results:
            iteration = result["iteration"]
            report_html += f"""
    <div class="iteration">
        <div class="iteration-header">
            <h2 class="iteration-number">Iteration {iteration}</h2>
        </div>
        <div class="screenshots">
"""
            
            for device, screenshot in result["screenshots"]:
                screenshot_rel = os.path.relpath(screenshot, WORKSPACE)
                report_html += f"""
            <div class="screenshot-card">
                <img src="{screenshot_rel}" alt="{device} Screenshot">
                <div class="screenshot-label">{device}</div>
            </div>
"""
            
            report_html += "</div>\n<div class=\"analysis\">\n"
            
            for analysis in result["analysis"]:
                if analysis.get("success"):
                    report_html += f"<h4>{analysis['device']} Analysis:</h4>\n"
                    report_html += f"<p>{analysis['content']}</p>\n\n"
            
            report_html += "</div>\n</div>\n"
        
        # Add final website link
        if results:
            final_url = results[-1].get("url", "")
            report_html += f"""
    <div class="final-website">
        <h2>🚀 Final Website</h2>
        <p>View the completed website</p>
        <a href="{final_url}" target="_blank">Open Website</a>
    </div>
"""
        
        report_html += "</body>\n</html>"
        
        report_path = self.workspace / "report.html"
        report_path.write_text(report_html)
        print(f"\n📊 Report generated: {report_path}")


async def main():
    """Main entry point."""
    
    # Default design prompt
    design_prompt = """
Create a modern portfolio website for a software developer with:
- Dark theme with neon accents (cyan, purple)
- Hero section with animated typing effect
- Skills section with animated progress bars
- Project showcase with hover effects
- Contact form
- Smooth scroll animations
- Professional, clean design
"""
    
    # Or use command line argument
    import sys
    if len(sys.argv) > 1:
        design_prompt = sys.argv[1]
    
    pipeline = VisualValidationPipeline()
    results = await pipeline.run_full_pipeline(design_prompt)
    
    print(f"\n✅ Pipeline complete!")
    print(f"📁 Workspace: {WORKSPACE}")
    print(f"📊 Report: {WORKSPACE}/report.html")
    print(f"🌐 Website: {results[-1]['url'] if results else 'N/A'}")


if __name__ == "__main__":
    asyncio.run(main())
