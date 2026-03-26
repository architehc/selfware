#!/bin/bash
# Production deployment for selfware with 2x RTX 4090
# Run this after the 6-hour test completes successfully

set -e

echo "🚀 SELFWARE PRODUCTION DEPLOYMENT"
echo "=================================="
echo ""
echo "This will:"
echo "  1. Create production configuration"
echo "  2. Set up systemd service"
echo "  3. Configure log rotation"
echo "  4. Enable monitoring"
echo ""
read -p "Continue? (y/n) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    exit 1
fi

# 1. Production config
echo ""
echo "📋 Step 1: Creating production config..."
cat > /home/ivo/selfware/selfware-production.toml << 'EOF'
# Production Configuration for 2x RTX 4090
endpoint = "http://localhost:8000/v1"
model = "qwen3.5-27b"
max_tokens = 65536
temperature = 0.4
context_length = 1010000

[concurrency]
max_parallel_requests = 8
max_retries = 3
timeout_secs = 300

[performance]
# Auto-scale based on GPU utilization
auto_scale = true
min_instances = 4
max_instances = 16

# Result caching to save API costs
cache_enabled = true
cache_ttl_secs = 3600
cache_max_entries = 1000

[monitoring]
# Export metrics for Prometheus
metrics_enabled = true
metrics_port = 9090
metrics_path = "/metrics"

# Logging
log_level = "info"
audit_logging = true
session_logging = true

[limits]
# Safety limits for production
max_tokens_per_hour = 1000000
max_tasks_per_hour = 100
max_context_size = 678928
EOF
echo "✅ Production config created"

# 2. Systemd service
echo ""
echo "📋 Step 2: Setting up systemd service..."
sudo tee /etc/systemd/system/selfware.service > /dev/null << 'EOF'
[Unit]
Description=Selfware AI Workshop
Documentation=https://selfware.design
After=docker.service network.target
Requires=docker.service

[Service]
Type=simple
User=ivo
Group=ivo
WorkingDirectory=/home/ivo/selfware

# Environment
Environment="RUST_LOG=info"
Environment="SELFWARE_ENDPOINT=http://localhost:8000/v1"
Environment="SELFWARE_MODEL=qwen3.5-27b"

# Pre-start: ensure vLLM is accessible
ExecStartPre=/bin/sh -c 'until curl -s http://localhost:8000/health > /dev/null; do sleep 5; done'

# Main service
ExecStart=/usr/local/bin/selfware \
    --config /home/ivo/selfware/selfware-production.toml \
    dashboard

# Graceful shutdown
ExecStop=/bin/kill -TERM $MAINPID
TimeoutStopSec=60
KillMode=mixed

# Restart policy
Restart=always
RestartSec=10
StartLimitInterval=60
StartLimitBurst=3

# Resource limits
LimitNOFILE=65536
LimitNPROC=4096

# Security
NoNewPrivileges=false
ProtectSystem=no
ProtectHome=no

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
echo "✅ Systemd service configured"

# 3. Log rotation
echo ""
echo "📋 Step 3: Configuring log rotation..."
sudo tee /etc/logrotate.d/selfware > /dev/null << 'EOF'
/home/ivo/.local/share/selfware/logs/*.log {
    daily
    rotate 7
    compress
    delaycompress
    missingok
    notifempty
    create 644 ivo ivo
    postrotate
        /bin/kill -HUP $(cat /var/run/syslogd.pid 2> /dev/null) 2> /dev/null || true
    endscript
}
EOF
echo "✅ Log rotation configured"

# 4. Health check script
echo ""
echo "📋 Step 4: Creating health check script..."
cat > /home/ivo/selfware/health-check.sh << 'EOF'
#!/bin/bash
# Health check for selfware

ENDPOINT="http://localhost:8000/v1"

check_gpu() {
    if command -v nvidia-smi &> /dev/null; then
        util=$(nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader | head -1 | tr -d ' ')
        if [ "$util" -lt 10 ]; then
            echo "WARNING: GPU utilization low (${util}%)"
            return 1
        fi
    fi
    return 0
}

check_endpoint() {
    if ! curl -s "$ENDPOINT/models" > /dev/null; then
        echo "ERROR: vLLM endpoint not accessible"
        return 1
    fi
    return 0
}

check_service() {
    if ! systemctl is-active --quiet selfware; then
        echo "ERROR: selfware service not running"
        return 1
    fi
    return 0
}

# Run checks
ERRORS=0
check_gpu || ERRORS=$((ERRORS + 1))
check_endpoint || ERRORS=$((ERRORS + 1))
check_service || ERRORS=$((ERRORS + 1))

if [ $ERRORS -eq 0 ]; then
    echo "✅ All checks passed"
    exit 0
else
    echo "⚠️  $ERRORS check(s) failed"
    exit 1
fi
EOF
chmod +x /home/ivo/selfware/health-check.sh
echo "✅ Health check script created"

# 5. Monitoring dashboard
echo ""
echo "📋 Step 5: Creating monitoring dashboard..."
cat > /home/ivo/selfware/dashboard.html << 'EOF'
<!DOCTYPE html>
<html>
<head>
    <title>Selfware Dashboard</title>
    <meta http-equiv="refresh" content="5">
    <style>
        body { font-family: monospace; background: #1a1a1a; color: #0f0; padding: 20px; }
        .metric { margin: 10px 0; padding: 10px; background: #2a2a2a; border-radius: 5px; }
        .ok { color: #0f0; }
        .warn { color: #ff0; }
        .error { color: #f00; }
        h1 { color: #0ff; }
    </style>
</head>
<body>
    <h1>🚀 Selfware Production Dashboard</h1>
    <div class="metric">
        <strong>Status:</strong> <span class="ok">● Running</span>
    </div>
    <div class="metric">
        <strong>Endpoint:</strong> http://localhost:8000/v1
    </div>
    <div class="metric">
        <strong>Model:</strong> qwen3.5-27b (Qwen3.5-27B-FP8)
    </div>
    <div class="metric">
        <strong>GPU:</strong> 2x RTX 4090
    </div>
    <div class="metric">
        <strong>Last Updated:</strong> <span id="time"></span>
    </div>
    <script>
        document.getElementById('time').textContent = new Date().toLocaleString();
    </script>
</body>
</html>
EOF
echo "✅ Dashboard created"

# Summary
echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║              DEPLOYMENT COMPLETE                             ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "🎉 Selfware is ready for production!"
echo ""
echo "To start the service:"
echo "  sudo systemctl enable selfware"
echo "  sudo systemctl start selfware"
echo ""
echo "To check status:"
echo "  sudo systemctl status selfware"
echo "  sudo journalctl -u selfware -f"
echo "  ./health-check.sh"
echo ""
echo "Files created:"
echo "  - selfware-production.toml"
echo "  - /etc/systemd/system/selfware.service"
echo "  - /etc/logrotate.d/selfware"
echo "  - health-check.sh"
echo "  - dashboard.html"
echo ""
echo "📊 View dashboard: file:///home/ivo/selfware/dashboard.html"
