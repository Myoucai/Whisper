import http.server
import socketserver

class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/set":
            self.send_response(200)
            self.send_header("Set-Cookie", "sid=abc; Path=/")
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"set")
        else:
            body = ("cookie=" + (self.headers.get("Cookie") or "") +
                    ";xtest=" + (self.headers.get("X-Test") or "")).encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(body)
    def log_message(self, *a):
        pass

with socketserver.TCPServer(("127.0.0.1", 18081), H) as s:
    s.serve_forever()
