#!/usr/bin/env python3
"""
Selfware Mascot Assistant: "Phi the Fox" (Φ)
Local Web Launcher & Development Server

Starts a high-performance HTTP server serving the Phi God-Mode IDE at:
http://localhost:8765/phi/index.html
"""

import http.server
import socketserver
import os
import sys
import webbrowser
import argparse

PORT = 8765
WEB_ROOT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "src", "evolve", "web")

BANNER = r"""
  /\_/\   ╔═════════════════════════════════════════════════════════╗
 ( o.o )  ║  SELFWARE MASCOT ASSISTANT: PHI THE FOX (Φ)           ║
  > ^ <   ║  God-Mode AGI Interface & English Viseme Lip-Sync       ║
 /  |  \  ╚═════════════════════════════════════════════════════════╝
(  / \  )   Port: http://localhost:8765/phi/index.html
"""

class CustomHTTPRequestHandler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=WEB_ROOT, **kwargs)

    def end_headers(self):
        # Enable CORS and caching headers for high-performance asset loading
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Cache-Control', 'no-cache, must-revalidate')
        super().end_headers()

def main():
    parser = argparse.ArgumentParser(description="Launch Selfware Phi Mascot Assistant")
    parser.add_argument("--port", type=int, default=PORT, help="Port to bind (default: 8765)")
    parser.add_argument("--no-browser", action="store_true", help="Do not open browser automatically")
    args = parser.parse_args()

    port = args.port
    print(BANNER)
    print(f"Serving Selfware Evolve & Phi Workspace from: {WEB_ROOT}")
    print(f"Direct Phi URL: http://localhost:{port}/phi/index.html")

    socketserver.TCPServer.allow_reuse_address = True
    with socketserver.TCPServer(("", port), CustomHTTPRequestHandler) as httpd:
        if not args.no_browser:
            try:
                webbrowser.open(f"http://localhost:{port}/phi/index.html")
            except Exception:
                pass
        print(f"Server active. Press Ctrl+C to terminate.")
        try:
            httpd.serve_forever()
        except KeyboardInterrupt:
            print("\nShutting down Phi server.")

if __name__ == "__main__":
    main()
