"""Transparent proxy: ftl -> this -> llama-server. Logs per-request timing to a JSONL file.

usage: python3 timing_proxy.py <listen_port> <upstream e.g. http://127.0.0.1:8080> <log.jsonl>
"""
import http.client, json, os, sys, time, urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT, UP, LOG = int(sys.argv[1]), urllib.parse.urlparse(sys.argv[2]), sys.argv[3]

class H(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, *a): pass
    def _proxy(self, method):
        n = int(self.headers.get("Content-Length", 0)); body = self.rfile.read(n) if n else None
        req = {}
        try: req = json.loads(body) if body else {}
        except Exception: pass
        if os.environ.get("MODEL_OVERRIDE") and isinstance(req, dict) and "model" in req:
            req["model"] = os.environ["MODEL_OVERRIDE"]; body = json.dumps(req).encode()
        cap = int(os.environ.get("MAX_TOKENS", "0"))
        if cap and isinstance(req, dict) and req.get("stream"):
            req["max_tokens"] = min(int(req.get("max_tokens") or cap), cap)
            req.pop("max_completion_tokens", None)
            if os.environ.get("TEMPERATURE"): req["temperature"] = float(os.environ["TEMPERATURE"])
            if os.environ.get("SAMPLING"): req.update(json.loads(os.environ["SAMPLING"]))
            body = json.dumps(req).encode()
        t0 = time.time()
        conn = http.client.HTTPConnection(UP.hostname, UP.port, timeout=3600)
        hdrs = {k: v for k, v in self.headers.items() if k.lower() not in ("host", "accept-encoding", "content-length")}
        if body is not None: hdrs["Content-Length"] = str(len(body))
        conn.request(method, self.path, body=body, headers=hdrs)
        resp = conn.getresponse()
        self.send_response(resp.status)
        for k, v in resp.getheaders():
            if k.lower() not in ("transfer-encoding", "content-length", "connection"): self.send_header(k, v)
        self.send_header("Transfer-Encoding", "chunked"); self.send_header("Connection", "close"); self.end_headers()
        ttft = None; last = None; buf = b""; timings = usage = None
        while True:
            chunk = resp.read1(65536) if hasattr(resp, "read1") else resp.read(65536)
            if not chunk: break
            if ttft is None and b'"content"' in chunk or (ttft is None and b'"reasoning_content"' in chunk): ttft = time.time() - t0
            buf += chunk
            self.wfile.write(b"%x\r\n" % len(chunk) + chunk + b"\r\n"); self.wfile.flush()
        self.wfile.write(b"0\r\n\r\n"); self.wfile.flush()
        total = time.time() - t0
        for line in buf.decode("utf-8", "replace").splitlines():
            line = line[6:] if line.startswith("data: ") else line
            try: d = json.loads(line)
            except Exception: continue
            timings = d.get("timings", timings); usage = d.get("usage", usage)
        rec = {"ts": t0, "path": self.path, "stream": req.get("stream"), "temperature": req.get("temperature"), "top_p": req.get("top_p"), "top_k": req.get("top_k"), "max_tokens": req.get("max_tokens"), "n_tools": len(req.get("tools") or []),
               "n_messages": len(req.get("messages") or []), "req_bytes": n, "status": resp.status,
               "ttft_s": ttft, "total_s": round(total, 3), "usage": usage, "timings": timings}
        with open(LOG, "a") as f: f.write(json.dumps(rec) + "\n")
        self.close_connection = True
    def do_POST(self): self._proxy("POST")
    def do_GET(self): self._proxy("GET")

ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
