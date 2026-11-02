import http.server
import os
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
        elif self.path == "/setpath":
            self.send_response(200)
            self.send_header("Set-Cookie", "p=1; Path=/echo")
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"setpath")
        elif self.path == "/setsecure":
            self.send_response(200)
            self.send_header("Set-Cookie", "s=1; Secure; Path=/")
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"setsecure")
        elif self.path == "/setdomainbad":
            self.send_response(200)
            self.send_header("Set-Cookie", "d=1; Domain=example.com")
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"setdomainbad")
        elif self.path == "/setdomaingood":
            self.send_response(200)
            self.send_header("Set-Cookie", "d=1; Domain=127.0.0.1; HttpOnly")
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"setdomaingood")
        elif self.path == "/setmax":
            self.send_response(200)
            self.send_header("Set-Cookie", "m=1; Max-Age=3600; Path=/")
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"setmax")
        elif self.path == "/delmax":
            self.send_response(200)
            self.send_header("Set-Cookie", "m=0; Max-Age=0; Path=/")
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"delmax")
        elif self.path == "/setexpired":
            self.send_response(200)
            self.send_header("Set-Cookie", "e=1; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Path=/")
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"setexpired")
        elif self.path == "/loop":
            self.send_response(302)
            self.send_header("Location", "/loop")
            self.end_headers()
            self.wfile.write(b"")
        elif self.path == "/cross":
            port = self.server.server_address[1]
            self.send_response(302)
            self.send_header("Location", "http://localhost:%d/set" % port)
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


with socketserver.TCPServer(("127.0.0.1", int(os.environ.get("HTTP6_PORT", "18081"))), H) as s:
    s.serve_forever()
