import http.server
import socketserver
import time


class H(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.0"

    def do_GET(self):
        if self.path == "/set":
            self.send_response(200)
            self.send_header("Set-Cookie", "sid=abc; Path=/")
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"set")
        elif self.path == "/slow":
            time.sleep(2)
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"slow")
        elif self.path == "/redirect":
            self.send_response(302)
            self.send_header("Location", "/set")
            self.end_headers()
            self.wfile.write(b"")
        else:
            body = ("cookie=" + (self.headers.get("Cookie") or "") +
                    ";xtest=" + (self.headers.get("X-Test") or "")).encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(body)
        self.close_connection = True

    def do_POST(self):
        n = int(self.headers.get("Content-Length") or 0)
        data = self.rfile.read(n)
        body = ("len=%d hex=%s" % (len(data), data.hex())).encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.end_headers()
        self.wfile.write(body)
        self.close_connection = True

    def log_message(self, *a):
        pass


with socketserver.TCPServer(("127.0.0.1", 18081), H) as s:
    s.serve_forever()
