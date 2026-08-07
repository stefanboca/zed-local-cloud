#!/usr/bin/env python3
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlsplit


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        request = urlsplit(self.path)
        if request.path != "/extensions" and not request.path.startswith(
            "/extensions/"
        ):
            self.send_error(404)
            return

        body = json.dumps(
            {
                "authorization": self.headers.get("Authorization"),
                "cookie": self.headers.get("Cookie"),
                "host": self.headers.get("Host"),
                "path": request.path,
                "query": parse_qs(request.query),
                "source": "fake-extension-upstream",
            },
            separators=(",", ":"),
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, _format, *_args):
        pass


ThreadingHTTPServer(("127.0.0.1", 18081), Handler).serve_forever()
