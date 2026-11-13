"""mock_judge.py: a stdlib System One server for tests and demos (not a judge: it never reads the text).

    from mock_judge import MockJudge
    with MockJudge({"urgent": 0.41, "team": {"billing": 0.48, "technical": 0.44}}, model="example-judge", key="k") as judge:
        answer = probbit.evaluate(request, judge=judge.url, auth_env="JUDGE_KEY")   # JUDGE_KEY=k in the environment

It answers any POST with the System One response shape of TypeSafe's OpenAPI 0.2.0 (https://api.typesafe.ai/openapi.json)
and the Workers AI clef output schema (https://developers.cloudflare.com/workers-ai/models/clef/schema-output.json):
{"model", "answers": {id: {"type": "noul", "noul"} | {"type": "choice", "choice", "confidence", "probabilities"} |
{"type": "score", "score", "confidence", "legend", "probabilities"}}, "usage": {"input_tokens", "output_tokens"}}. The
probabilities come from the table given at start (noul: P(true); choice / score: {option or level: p}); a question without a
row gets uniform odds. `confidence` is the chosen option's probability and the token counts are made up (neither vendor
publishes how it computes them). envelope=True wraps the reply as Workers AI's REST API does ({"result", "success", "errors",
"messages"}); key= requires `Authorization: Bearer <key>` (401 otherwise); a request without `questions` gets 422 with
TypeSafe's validation-error shape. `requests` records (path, headers, body) of every call.
"""
import http.server, json, threading


def answer(question, probs=None):
    """The vendor answer object for one question, from probabilities per option (noul: P(true)); None = uniform."""
    t = question["type"]
    if t == "noul":
        return {"type": "noul", "noul": float(probs) if isinstance(probs, (int, float)) else 0.5}
    opts = list(question["criteria"]) if t == "choice" else [str(level) for level in range(len(question["criteria"]))]
    p = {o: float(probs.get(o, 0.0)) for o in opts} if isinstance(probs, dict) else {o: 1.0 / len(opts) for o in opts}
    top = max(opts, key=lambda o: p[o])
    if t == "choice":
        return {"type": "choice", "choice": top, "confidence": p[top], "probabilities": p}
    return {"type": "score", "score": round(sum(int(o) * p[o] for o in opts), 6), "confidence": p[top],
            "legend": {str(level): d for level, d in enumerate(question["criteria"])}, "probabilities": p}


class MockJudge:
    def __init__(self, table=None, model="mock-judge", key=None, envelope=False):
        self.table, self.model, self.key, self.envelope, self.requests = table or {}, model, key, envelope, []

    def reply(self, body):
        qs = body["questions"]
        out = {"model": self.model, "answers": {k: answer(q, self.table.get(k)) for k, q in qs.items()},
               "usage": {"input_tokens": len(json.dumps(body)) // 4, "output_tokens": len(qs)}}
        return {"result": out, "success": True, "errors": [], "messages": []} if self.envelope else out

    def start(self):
        judge = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                raw = self.rfile.read(int(self.headers.get("Content-Length") or 0))
                judge.requests.append((self.path, dict(self.headers), raw.decode("utf-8")))
                if judge.key is not None and self.headers.get("Authorization") != "Bearer " + judge.key:
                    return self.send(401, {"detail": "Not authenticated"})
                try:
                    body = json.loads(raw.decode("utf-8"))
                except ValueError:
                    return self.send(422, {"detail": [{"loc": ["body"], "msg": "JSON decode error", "type": "json_invalid"}]})
                if not isinstance(body, dict) or not isinstance(body.get("questions"), dict):
                    return self.send(422, {"detail": [{"loc": ["body", "questions"], "msg": "Field required", "type": "missing"}]})
                self.send(200, judge.reply(body))

            def send(self, code, doc):
                data = json.dumps(doc).encode("utf-8")
                self.send_response(code); self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(data)))
                self.end_headers(); self.wfile.write(data)

            def log_message(self, *args):
                pass

        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.server.server_address[1]}"
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True); self.thread.start()
        return self

    def stop(self):
        self.server.shutdown(); self.server.server_close(); self.thread.join()

    def __enter__(self):
        return self.start()

    def __exit__(self, *exc):
        self.stop()
