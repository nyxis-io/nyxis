#!/usr/bin/env python3
"""
Usage: python3 server.py [port]

Drop-in replacement for `python3 -m http.server` that sets COOP/COEP headers
so SharedArrayBuffer is available in the browser. Default port: 8000.
"""
import sys
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer


class COOPCOEPRequestHandler(SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        self.send_header("Cross-Origin-Resource-Policy", "same-origin")
        self.send_header("Cache-Control", "no-store")
        super().end_headers()


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8000
    server = ThreadingHTTPServer(("", port), COOPCOEPRequestHandler)
    print(f"Serving on http://localhost:{port} (COOP/COEP enabled, SAB should work)")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down.")
        server.server_close()


if __name__ == "__main__":
    main()
