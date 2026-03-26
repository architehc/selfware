#!/bin/bash
# Production deployment script for selfware

echo "🚀 SELFWARE PRODUCTION DEPLOYMENT"
echo "=================================="
echo ""

# 1. Create production config
cat > /home/ivo/selfware/selfware-production.toml << 'EOF'
# Production Configuration
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
auto_scale = true
min_instances = 4
max_instances = 16
cache_enabled = true
cache_ttl_secs = 3600

[monitoring]
metrics_enabled = true
metrics_port = 9090
log_level = "info"
EOF

echo "✅ Production config created"

# 2. Create systemd service
sudo tee /etc/systemd/system/selfware.service > /dev/null << 'EOF'
[Unit]
Description=Selfware AI Workshop
After=docker.service vllm.service
Requires=docker.service

[Service]
Type=simple
User=ivo
WorkingDirectory=/home/ivo/selfware
Environment="SELFWARE_CONFIG=/home/ivo/selfware/selfware-production.toml"
Environment="RUST_LOG=info"
ExecStartPre=-/usr/bin/docker pull selfware:latest
ExecStart=/usr/bin/docker run --rm \
    --name selfware-prod \
    --network host \
    --gpus all \
    -v /home/ivo/selfware:/workspace \
    -v /home/ivo/selfware/selfware-production.toml:/root/.config/selfware/config.toml:ro \
    selfware:latest dashboard
ExecStop=/usr/bin/docker stop -t 30 selfware-prod
ExecStopPost=-/usr/bin/docker rm -f selfware-prod
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

echo "✅ Systemd service created"

# 3. Enable and start
echo ""
echo "To start production service:"
echo "  sudo systemctl daemon-reload"
echo "  sudo systemctl enable selfware"
echo "  sudo systemctl start selfware"
echo ""
echo "To check status:"
echo "  sudo systemctl status selfware"
echo "  sudo journalctl -u selfware -f"
