#!/usr/bin/env python3
import http.server
import socketserver
import threading

class MyHandler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header('Content-type', 'text/html')
        self.end_headers()
        response = f"""
        <html>
        <head><title>Test Server</title></head>
        <body>
        <h1>Hello from Local Service!</h1>
        <p>Path: {self.path}</p>
        <p>Headers: {dict(self.headers)}</p>
        </body>
        </html>
        """
        self.wfile.write(response.encode())

def start_server(port):
    with socketserver.TCPServer(("", port), MyHandler) as httpd:
        print(f"Serving at port {port}")
        httpd.serve_forever()

if __name__ == "__main__":
    start_server(8080) 