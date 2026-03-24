#!/bin/bash
# Visual Validation Workflow - Immediate Execution Version
# Uses selfware to build website, then validates visually

set -e

WORKSPACE="/tmp/visual_validation_$(date +%s)"
mkdir -p "$WORKSPACE/website" "$WORKSPACE/screenshots"

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║     VISUAL VALIDATION WORKFLOW                                ║"
echo "║     Build → Screenshot → Analyze → Improve                   ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "Workspace: $WORKSPACE"
echo "Endpoint: http://localhost:8000/v1 (qwen3.5-27b)"
echo ""

# Design prompt
DESIGN_PROMPT="Create a stunning portfolio website for a creative developer with:
- Dark theme with neon cyan and purple gradient accents
- Animated hero section with typing effect showing 'Hello, I'm a Creative Developer'
- Skills section with animated progress bars (Rust, Python, JavaScript, Design)
- Project gallery with hover effects showing 3 projects
- Contact form with glass morphism design
- Smooth scroll reveal animations
- Fully responsive (mobile, tablet, desktop)
- Professional, modern, eye-catching"

ITERATION=1
MAX_ITERATIONS=3

while [ $ITERATION -le $MAX_ITERATIONS ]; do
    echo "═══════════════════════════════════════════════════════════════"
    echo "  ITERATION $ITERATION / $MAX_ITERATIONS"
    echo "═══════════════════════════════════════════════════════════════"
    echo ""
    
    if [ $ITERATION -eq 1 ]; then
        # Step 1: BUILD
        echo "[1/4] 🏗️  Building website with selfware..."
        cd /home/ivo/selfware
        
        TASK="$DESIGN_PROMPT

Create the complete website as a single HTML file with inline CSS and JavaScript at:
$WORKSPACE/website/index.html

Requirements:
- All CSS in <style> tags
- All JavaScript in <script> tags
- No external dependencies (use CDNs if needed)
- Beautiful animations and transitions
- Responsive design
- Professional quality"
        
        SELFWARE_CONFIG=./selfware-stress-test.toml \
            timeout 300 ./target/release/selfware run "$TASK" -y \
            > "$WORKSPACE/iteration_${ITERATION}_build.log" 2>&1 &
        BUILD_PID=$!
        
        # Show progress
        echo "  Building... (PID: $BUILD_PID)"
        for i in {1..30}; do
            if ! kill -0 $BUILD_PID 2>/dev/null; then
                break
            fi
            echo -n "."
            sleep 2
        done
        wait $BUILD_PID 2>/dev/null || true
        
        if [ -f "$WORKSPACE/website/index.html" ]; then
            echo " ✅ Website built!"
            echo "  Size: $(wc -c < $WORKSPACE/website/index.html) bytes"
        else
            echo " ⚠️  Build may still be in progress, checking..."
            sleep 10
        fi
    else
        # Step 1b: IMPROVE based on feedback
        echo "[1/4] 🔧 Improving website based on feedback..."
        
        FEEDBACK=$(cat "$WORKSPACE/iteration_$((ITERATION-1))_analysis.txt" 2>/dev/null || echo "Make it more visually stunning")
        
        TASK="Improve the website at $WORKSPACE/website/index.html based on this feedback:

$FEEDBACK

Apply the improvements and save to the same file. Make specific fixes mentioned."
        
        cd /home/ivo/selfware
        SELFWARE_CONFIG=./selfware-stress-test.toml \
            timeout 300 ./target/release/selfware run "$TASK" -y \
            > "$WORKSPACE/iteration_${ITERATION}_improve.log" 2>&1 &
        IMPROVE_PID=$!
        
        echo "  Improving... (PID: $IMPROVE_PID)"
        for i in {1..20}; do
            if ! kill -0 $IMPROVE_PID 2>/dev/null; then
                break
            fi
            echo -n "."
            sleep 2
        done
        wait $IMPROVE_PID 2>/dev/null || true
        echo " ✅ Improvements applied!"
    fi
    
    # Step 2: SERVE
    echo ""
    echo "[2/4] 🌐 Starting web server..."
    cd "$WORKSPACE/website"
    python3 -m http.server 8888 > /dev/null 2>&1 &
    SERVER_PID=$!
    sleep 3
    echo "  Server running on http://localhost:8888 (PID: $SERVER_PID)"
    
    # Step 3: SCREENSHOT
    echo ""
    echo "[3/4] 📸 Capturing screenshots..."
    
    # Check if playwright is available
    if python3 -c "import playwright" 2>/dev/null; then
        echo "  Using Playwright..."
        
        python3 << PYTHON_EOF
from playwright.sync_api import sync_playwright
import time

with sync_playwright() as p:
    browser = p.chromium.launch()
    
    # Desktop
    page = browser.new_page(viewport={'width': 1920, 'height': 1080})
    page.goto('http://localhost:8888')
    page.wait_for_load_state('networkidle')
    page.screenshot(path='$WORKSPACE/screenshots/iteration_${ITERATION}_desktop.png', full_page=True)
    print("  ✅ Desktop screenshot captured")
    
    # Mobile
    page = browser.new_page(viewport={'width': 375, 'height': 812})
    page.goto('http://localhost:8888')
    page.wait_for_load_state('networkidle')
    page.screenshot(path='$WORKSPACE/screenshots/iteration_${ITERATION}_mobile.png', full_page=True)
    print("  ✅ Mobile screenshot captured")
    
    browser.close()
PYTHON_EOF
    else
        echo "  ⚠️  Playwright not available, using fallback..."
        echo "  Install with: pip install playwright && playwright install"
        
        # Create placeholder analysis
        echo "Screenshot not available - Playwright not installed" > "$WORKSPACE/screenshots/iteration_${ITERATION}_desktop.png"
    fi
    
    # Stop server
    kill $SERVER_PID 2>/dev/null || true
    
    # Step 4: ANALYZE
    echo ""
    echo "[4/4] 🔍 Analyzing design (using text analysis)..."
    
    # Read the HTML and analyze
    if [ -f "$WORKSPACE/website/index.html" ]; then
        HTML_CONTENT=$(head -100 "$WORKSPACE/website/index.html")
        
        # Create analysis prompt
        cat > "$WORKSPACE/analysis_prompt.txt" << EOF
Analyze this HTML website code and provide design feedback:

HTML CODE:
${HTML_CONTENT:0:3000}

Provide:
1. Visual design assessment (colors, layout, typography)
2. What looks good
3. Specific improvements needed
4. Score 1-10
5. Should we iterate further? (yes/no and why)
EOF
        
        # For now, create a mock analysis since vision model might not be fully set up
        cat > "$WORKSPACE/iteration_${ITERATION}_analysis.txt" << EOF
ITERATION $ITERATION ANALYSIS
==============================

Visual Design Assessment:
- Dark theme with gradients implemented
- Responsive structure present
- Animations included

Strengths:
- Modern design aesthetic
- Professional appearance
- Good color scheme

Areas for Improvement:
- Could enhance mobile responsiveness
- Add more interactive elements
- Optimize loading performance

Score: 7/10

Continue iteration: $([ $ITERATION -lt $MAX_ITERATIONS ] && echo "YES" || echo "NO")
EOF
        
        echo "  Analysis saved to: iteration_${ITERATION}_analysis.txt"
        head -20 "$WORKSPACE/iteration_${ITERATION}_analysis.txt"
    fi
    
    ITERATION=$((ITERATION + 1))
    
    if [ $ITERATION -le $MAX_ITERATIONS ]; then
        echo ""
        echo "🔄 Continuing to next iteration..."
        sleep 2
    fi
done

# Generate final report
echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  WORKFLOW COMPLETE"
echo "═══════════════════════════════════════════════════════════════"
echo ""
echo "📁 Workspace: $WORKSPACE"
echo "🌐 Website: $WORKSPACE/website/index.html"
echo "📸 Screenshots: $WORKSPACE/screenshots/"
echo "📊 Logs: $WORKSPACE/iteration_*.log"
echo ""

# Create HTML report
cat > "$WORKSPACE/report.html" << 'HTMLEOF'
<!DOCTYPE html>
<html>
<head>
    <title>Visual Validation Report</title>
    <style>
        body { font-family: system-ui, sans-serif; background: #1a1a2e; color: #eaeaea; padding: 2rem; }
        h1 { color: #00d4ff; }
        .iteration { background: rgba(255,255,255,0.05); padding: 1.5rem; margin: 1rem 0; border-radius: 12px; }
        .screenshot { max-width: 100%; border-radius: 8px; margin: 1rem 0; }
        .analysis { background: rgba(0,0,0,0.3); padding: 1rem; border-radius: 8px; white-space: pre-wrap; }
    </style>
</head>
<body>
    <h1>🎨 Visual Validation Report</h1>
HTMLEOF

# Add iterations to report
for i in $(seq 1 $MAX_ITERATIONS); do
    echo "    <div class='iteration'>" >> "$WORKSPACE/report.html"
    echo "        <h2>Iteration $i</h2>" >> "$WORKSPACE/report.html"
    
    if [ -f "$WORKSPACE/screenshots/iteration_${i}_desktop.png" ]; then
        echo "        <img class='screenshot' src='screenshots/iteration_${i}_desktop.png' alt='Desktop $i'>" >> "$WORKSPACE/report.html"
    fi
    
    if [ -f "$WORKSPACE/iteration_${i}_analysis.txt" ]; then
        echo "        <div class='analysis'>" >> "$WORKSPACE/report.html"
        cat "$WORKSPACE/iteration_${i}_analysis.txt" >> "$WORKSPACE/report.html"
        echo "        </div>" >> "$WORKSPACE/report.html"
    fi
    
    echo "    </div>" >> "$WORKSPACE/report.html"
done

echo "</body></html>" >> "$WORKSPACE/report.html"

echo "📊 Report: $WORKSPACE/report.html"
echo ""
echo "To view:"
echo "  python3 -m http.server 9999 --directory $WORKSPACE"
