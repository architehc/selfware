#!/bin/bash
# Development Test Runner - End-to-end development tasks with visual feedback
# Tests selfware on real projects: games, websites, simulations

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
SELFWARE_BIN="$PROJECT_DIR/target/release/selfware"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
TEST_DIR="$PROJECT_DIR/parallel_dev_tests/run_$TIMESTAMP"

# Ensure binary exists
if [[ ! -f "$SELFWARE_BIN" ]]; then
    echo "Error: selfware binary not found. Building..."
    cd "$PROJECT_DIR" && cargo build --release
fi

# Create test directories
mkdir -p "$TEST_DIR"/{flappy_bird,portfolio_website,tqec_sim,rust_game}
mkdir -p "$TEST_DIR"/screenshots/{flappy_bird,portfolio_website,tqec_sim,rust_game}

echo "=========================================="
echo "  Selfware Development Test Runner"
echo "=========================================="
echo "Test Directory: $TEST_DIR"
echo "Timestamp: $TIMESTAMP"
echo ""

# Define 4 comprehensive development tasks
# Task 1: Flappy Bird game in HTML5 Canvas with visual iteration
task_flappy_bird() {
    local instance=$1
    local workdir="$TEST_DIR/flappy_bird/instance_$instance"
    mkdir -p "$workdir"
    
    local prompt="Create a complete Flappy Bird game in a single HTML file with:
- HTML5 Canvas for rendering
- JavaScript game loop with physics (gravity, jump)
- Pipe obstacles that spawn and scroll
- Collision detection
- Score tracking
- Game over and restart functionality
- Clean visual design with bird sprite and pipe graphics

Requirements:
1. Create index.html with embedded CSS and JS
2. Make it playable in a browser
3. Add visual polish (colors, animations)
4. Ensure the game is fully functional end-to-end

Work iteratively: create the file, then improve it based on what would make it better."

    echo "[$instance] Starting Flappy Bird game development..."
    "$SELFWARE_BIN" \
        --config "$PROJECT_DIR/selfware.toml" \
        --mode yolo \
        --workdir "$workdir" \
        run "$prompt" > "$workdir/selfware.log" 2>&1 &
    
    echo $! > "$workdir/pid"
    echo "  Started (PID: $!)"
}

# Task 2: Portfolio website with modern design
task_portfolio() {
    local instance=$1
    local workdir="$TEST_DIR/portfolio_website/instance_$instance"
    mkdir -p "$workdir"
    
    local prompt="Create a modern, responsive portfolio website with:
- index.html with semantic HTML5 structure
- CSS with modern styling (flexbox/grid, animations)
- Sections: Hero, About, Projects, Skills, Contact
- Mobile-responsive design
- Smooth scrolling navigation
- Dark/light mode toggle
- Professional typography and spacing

Requirements:
1. Create a complete, standalone website
2. Use modern CSS techniques
3. Add interactive elements with vanilla JS
4. Ensure it looks professional and polished
5. Test responsiveness conceptually

Build this as a complete portfolio site that could be deployed."

    echo "[$instance] Starting Portfolio website development..."
    "$SELFWARE_BIN" \
        --config "$PROJECT_DIR/selfware.toml" \
        --mode yolo \
        --workdir "$workdir" \
        run "$prompt" > "$workdir/selfware.log" 2>&1 &
    
    echo $! > "$workdir/pid"
    echo "  Started (PID: $!)"
}

# Task 3: TQEC (Topological Quantum Error Correction) simulator in Rust
task_tqec() {
    local instance=$1
    local workdir="$TEST_DIR/tqec_sim/instance_$instance"
    mkdir -p "$workdir"
    
    local prompt="Create a TQEC (Topological Quantum Error Correction) simulator in Rust:

Core components:
1. Surface code lattice representation (2D grid of qubits)
2. Stabilizer measurement simulation (X and Z type)
3. Error injection with configurable error rates
4. Syndrome extraction and decoding
5. Visualization output (ASCII or SVG representation)

Specific requirements:
- Create Cargo.toml with necessary dependencies
- Implement lattice.rs: Grid structure for surface code
- Implement stabilizer.rs: Measurement operators
- Implement errors.rs: Error models (bit-flip, phase-flip, depolarizing)
- Implement decoder.rs: Minimum Weight Perfect Matching decoder
- Implement main.rs: CLI to run simulations
- Add examples/ showing usage

Focus on:
- Clean, idiomatic Rust code
- Efficient data structures for large lattices
- Comprehensive error handling
- Documentation and tests

This should be a functional quantum error correction simulation library."

    echo "[$instance] Starting TQEC simulator development..."
    "$SELFWARE_BIN" \
        --config "$PROJECT_DIR/selfware.toml" \
        --mode yolo \
        --workdir "$workdir" \
        run "$prompt" > "$workdir/selfware.log" 2>&1 &
    
    echo $! > "$workdir/pid"
    echo "  Started (PID: $!)"
}

# Task 4: Rust game using macroquad or similar lightweight framework
task_rust_game() {
    local instance=$1
    local workdir="$TEST_DIR/rust_game/instance_$instance"
    mkdir -p "$workdir"
    
    local prompt="Create a simple but complete game in Rust using macroquad (or ggez if preferred):

Game concept: Space Shooter
- Player ship at bottom of screen
- Enemies spawning from top
- Shooting mechanics
- Collision detection
- Score and lives system
- Game over and restart

Requirements:
1. Create Cargo.toml with macroquad dependency
2. Implement main.rs with game loop
3. Add player.rs - player ship logic
4. Add enemy.rs - enemy behavior and spawning
5. Add bullet.rs - projectile system
6. Add collision.rs - hit detection
7. Add assets/ folder with simple sprites (or procedural graphics)

Focus on:
- Smooth 60 FPS gameplay
- Clean separation of concerns
- Input handling (WASD + Space or mouse)
- Visual polish with particle effects
- Sound effects (if possible)

This should compile and run with 'cargo run'."

    echo "[$instance] Starting Rust game development..."
    "$SELFWARE_BIN" \
        --config "$PROJECT_DIR/selfware.toml" \
        --mode yolo \
        --workdir "$workdir" \
        run "$prompt" > "$workdir/selfware.log" 2>&1 &
    
    echo $! > "$workdir/pid"
    echo "  Started (PID: $!)"
}

# Screenshot and review function (runs periodically)
screenshot_and_review() {
    local task=$1
    local instance=$2
    local workdir="$TEST_DIR/$task/instance_$instance"
    
    # Install playwright if needed for screenshots
    if ! command -v playwright &> /dev/null; then
        npm install -g @playwright/test 2>/dev/null || true
        npx playwright install chromium 2>/dev/null || true
    fi
    
    # Take screenshots of HTML files if they exist
    if [[ -f "$workdir/index.html" ]]; then
        local screenshot_file="$TEST_DIR/screenshots/$task/instance_$instance/$(date +%H%M%S).png"
        mkdir -p "$(dirname "$screenshot_file")"
        
        # Use playwright to screenshot
        node "$SCRIPT_DIR/playwright-bridge.js" screenshot \
            "file://$workdir/index.html" \
            "$screenshot_file" 2>/dev/null || true
    fi
}

# Launch all 16 instances (4 tasks x 4 instances)
echo "Launching 16 development instances..."
echo ""

for i in 1 2 3 4; do
    task_flappy_bird $i &
    task_portfolio $i &
    task_tqec $i &
    task_rust_game $i &
done

wait
echo ""
echo "All 16 instances launched!"

# Create helper scripts
cat > "$TEST_DIR/monitor.sh" << 'EOF'
#!/bin/bash
TEST_DIR="$(dirname "$0")"
echo "=========================================="
echo "  Development Test Monitor"
echo "=========================================="
echo ""

for task in flappy_bird portfolio_website tqec_sim rust_game; do
    echo "=== $task ==="
    for i in 1 2 3 4; do
        workdir="$TEST_DIR/$task/instance_$i"
        pid_file="$workdir/pid"
        
        if [[ -f "$pid_file" ]]; then
            pid=$(cat "$pid_file" 2>/dev/null)
            if kill -0 "$pid" 2>/dev/null; then
                # Count files created
                file_count=$(find "$workdir" -type f 2>/dev/null | wc -l)
                echo "  instance_$i: RUNNING (PID: $pid, Files: $file_count)"
            else
                echo "  instance_$i: FINISHED"
            fi
        else
            echo "  instance_$i: NOT STARTED"
        fi
    done
    echo ""
done
EOF
chmod +x "$TEST_DIR/monitor.sh"

cat > "$TEST_DIR/summary.sh" << 'EOF'
#!/bin/bash
TEST_DIR="$(dirname "$0")"
SUMMARY_FILE="$TEST_DIR/RESULTS.md"

echo "# Development Test Results" > "$SUMMARY_FILE"
echo "" >> "$SUMMARY_FILE"
echo "**Date:** $(date)" >> "$SUMMARY_FILE"
echo "" >> "$SUMMARY_FILE"

for task in flappy_bird portfolio_website tqec_sim rust_game; do
    echo "## $task" >> "$SUMMARY_FILE"
    echo "" >> "$SUMMARY_FILE"
    
    for i in 1 2 3 4; do
        workdir="$TEST_DIR/$task/instance_$i"
        echo "### Instance $i" >> "$SUMMARY_FILE"
        echo "" >> "$SUMMARY_FILE"
        
        # List all created files
        echo "**Files created:**" >> "$SUMMARY_FILE"
        echo '```' >> "$SUMMARY_FILE"
        find "$workdir" -type f -name "*.html" -o -name "*.css" -o -name "*.js" -o -name "*.rs" -o -name "*.toml" 2>/dev/null | head -20 >> "$SUMMARY_FILE" || echo "(none)" >> "$SUMMARY_FILE"
        echo '```' >> "$SUMMARY_FILE"
        echo "" >> "$SUMMARY_FILE"
        
        # Check for specific deliverables
        if [[ -f "$workdir/index.html" ]]; then
            echo "- ✅ index.html exists" >> "$SUMMARY_FILE"
            lines=$(wc -l < "$workdir/index.html" 2>/dev/null)
            echo "  - Lines: $lines" >> "$SUMMARY_FILE"
        fi
        
        if [[ -f "$workdir/Cargo.toml" ]]; then
            echo "- ✅ Cargo.toml exists (Rust project)" >> "$SUMMARY_FILE"
        fi
        
        if [[ -d "$workdir/src" ]]; then
            rs_files=$(find "$workdir/src" -name "*.rs" 2>/dev/null | wc -l)
            echo "- ✅ src/ directory with $rs_files Rust files" >> "$SUMMARY_FILE"
        fi
        
        # Last 20 lines of log
        if [[ -f "$workdir/selfware.log" ]]; then
            echo "" >> "$SUMMARY_FILE"
            echo "**Last log entries:**" >> "$SUMMARY_FILE"
            echo '```' >> "$SUMMARY_FILE"
            tail -20 "$workdir/selfware.log" 2>/dev/null >> "$SUMMARY_FILE"
            echo '```' >> "$SUMMARY_FILE"
        fi
        
        echo "" >> "$SUMMARY_FILE"
    done
done

echo "Results saved to: $SUMMARY_FILE"
cat "$SUMMARY_FILE"
EOF
chmod +x "$TEST_DIR/summary.sh"

cat > "$TEST_DIR/stop.sh" << 'EOF'
#!/bin/bash
TEST_DIR="$(dirname "$0")"
echo "Stopping all instances..."

for task in flappy_bird portfolio_website tqec_sim rust_game; do
    for i in 1 2 3 4; do
        pid_file="$TEST_DIR/$task/instance_$i/pid"
        if [[ -f "$pid_file" ]]; then
            pid=$(cat "$pid_file" 2>/dev/null)
            if kill -0 "$pid" 2>/dev/null; then
                echo "  Stopping $task instance $i (PID: $pid)"
                kill "$pid" 2>/dev/null || true
            fi
        fi
    done
done

echo "All instances stopped."
EOF
chmod +x "$TEST_DIR/stop.sh"

echo ""
echo "=========================================="
echo "  Development Test Runner Complete!"
echo "=========================================="
echo ""
echo "Test Directory: $TEST_DIR"
echo ""
echo "Commands:"
echo "  $TEST_DIR/monitor.sh  - Check status"
echo "  $TEST_DIR/summary.sh  - Generate results"
echo "  $TEST_DIR/stop.sh     - Stop all instances"
echo ""
echo "Monitor progress with:"
echo "  watch -n 30 $TEST_DIR/monitor.sh"
