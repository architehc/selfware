#!/usr/bin/env python3
"""
Simple HTTP server to serve the Weather Appointments website.
Usage: python server.py [port]
Default port: 8080
"""

import http.server
import socketserver
import os
import sys
from urllib.parse import urlparse

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
DIRECTORY = os.path.dirname(os.path.abspath(__file__)) or '.'

class WeatherAppHandler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=DIRECTORY, **kwargs)
    
    def end_headers(self):
        # Add CORS headers for API access if needed
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Cache-Control', 'no-cache, no-store, must-revalidate')
        self.send_header('Pragma', 'no-cache')
        self.send_header('Expires', '0')
        super().end_headers()

def run_server(port):
    os.chdir(DIRECTORY)
    
    with socketserver.TCPServer(("", port), WeatherAppHandler) as httpd:
        print(f"☀️  Weather Appointments Server")
        print(f"━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━")
        print(f"🌐  Server running at: http://localhost:{port}")
        print(f"📁  Serving from: {DIRECTORY}")
        print(f"🔌  Press Ctrl+C to stop the server")
        print(f"━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━")
        
        try:
            httpd.serve_forever()
        except KeyboardInterrupt:
            print("\n👋 Server stopped!")
            sys.exit(0)

if __name__ == "__main__":
    run_server(PORT)
