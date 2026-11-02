"""A provider on loopback that replays recorded replies in order.

The offline arm has no credential and no bill, so the run asks this instead: it
serves the `*.json` files in its directory, one per request, in filename order,
and never reaches the network. The bodies are written to the Anthropic wire
format rather than captured from it — the environment this bench was built in had
no key — which is the same honesty `crates/evals/fixtures` keeps and the same
reason: what is replayed is our loop and our product, not a model.

Streaming is answered when the request asked for it: one `data:` frame holding
the whole body, exactly as `crates/harness` writes one, so a body reassembles to
itself and no reader has to know which way it arrived.

    python3 mock_provider.py RECORDED_DIR     # prints the base URL, then serves
"""
import http.server
import json
import pathlib
import socketserver
import sys
import threading


def replies(directory):
    """The recorded bodies, in the order the run is meant to receive them."""
    found = sorted(pathlib.Path(directory).glob("*.json"))
    if not found:
        raise SystemExit(f"mock: {directory} holds no recorded reply")
    return [path.read_text() for path in found]


def frame(body):
    """One whole body as one SSE frame, every line a `data:` line."""
    lines = "".join(f"data: {line}\n" for line in body.splitlines())
    return (lines + "\n").encode()


class Handler(http.server.BaseHTTPRequestHandler):
    bodies = []
    served = 0
    lock = threading.Lock()

    def do_POST(self):  # noqa: N802 - the name http.server dispatches on
        length = int(self.headers.get("Content-Length") or 0)
        asked = self.rfile.read(length)
        with Handler.lock:
            body = Handler.bodies[min(Handler.served, len(Handler.bodies) - 1)]
            Handler.served += 1
        try:
            streaming = bool(json.loads(asked or b"{}").get("stream"))
        except json.JSONDecodeError:
            streaming = False
        if streaming:
            payload = frame(body)
            kind = "text/event-stream"
        else:
            payload = body.encode()
            kind = "application/json"
        self.send_response(200)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, *args):  # the bench's own output is the record
        pass


def main(argv):
    if len(argv) != 1:
        raise SystemExit("usage: mock_provider.py RECORDED_DIR")
    Handler.bodies = replies(argv[0])
    with socketserver.TCPServer(("127.0.0.1", 0), Handler) as server:
        print(f"http://127.0.0.1:{server.server_address[1]}", flush=True)
        server.serve_forever()


if __name__ == "__main__":
    main(sys.argv[1:])
