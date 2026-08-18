"""A small flight-search site for the browser example.

Deliberately renders its content with JavaScript from JSON endpoints. A page that
is only server-rendered HTML could be exercised with an HTTP client; this one
cannot, which keeps the example honest about driving a real browser.

Standard library only. Deterministic: the same request always gets the same bytes.
"""

import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

FLIGHTS = [
    {"id": "FL-101", "price": 412, "airline": "Kestrel", "stops": 1, "seats": 0},
    {"id": "FL-203", "price": 680, "airline": "Northwind", "stops": 0, "seats": 4},
    {"id": "FL-311", "price": 744, "airline": "Kestrel", "stops": 2, "seats": 2},
    {"id": "FL-455", "price": 915, "airline": "Northwind", "stops": 0, "seats": 9},
]
BY_ID = {f["id"]: f for f in FLIGHTS}

INDEX = """<!doctype html>
<meta charset="utf-8"><title>Flights</title>
<h1>Flights</h1>
<table id="results"><tbody id="rows"></tbody></table>
<script>
fetch('/api/flights').then(r => r.json()).then(flights => {
  document.getElementById('rows').innerHTML = flights.map(f => `
    <tr class="flight" data-id="${f.id}" data-price="${f.price}">
      <td class="id">${f.id}</td>
      <td class="price">${f.price}</td>
      <td><a id="open-${f.id}" href="/flight/${f.id}">details</a></td>
    </tr>`).join('');
  document.body.dataset.ready = 'true';
});
</script>"""

DETAIL = """<!doctype html>
<meta charset="utf-8"><title>Flight %(id)s</title>
<h1 id="flight-id">%(id)s</h1>
<div id="detail">loading</div>
<script>
fetch('/api/flight/%(id)s').then(r => r.json()).then(f => {
  document.getElementById('detail').innerHTML =
    `<span id="price">${f.price}</span> <span id="seats">${f.seats}</span>` +
    `<button id="book" onclick="location.href='/book/%(id)s'">Book</button>`;
  document.body.dataset.ready = 'true';
});
</script>"""

BOOKED = """<!doctype html>
<meta charset="utf-8"><title>Booking %(id)s</title>
<h1>Booking</h1>
<div id="outcome">loading</div>
<script>
fetch('/api/book/%(id)s').then(r => r.json()).then(b => {
  document.getElementById('outcome').textContent = b.status;
  document.body.dataset.ready = 'true';
});
</script>"""


# Deliberately not reproducible: the HTML is byte-identical every time, but the text
# the browser ends up showing is not, because it comes from the clock rather than from
# the response. Re-driving this page can never reproduce the recorded observation,
# which is exactly the boundary the adapter has to be honest about.
VOLATILE = """<!doctype html>
<meta charset="utf-8"><title>Volatile</title>
<h1>Volatile</h1>
<div id="stamp">pending</div>
<script>
document.getElementById('stamp').textContent = 'rendered at ' + Date.now();
document.body.dataset.ready = 'true';
</script>"""


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass  # quiet: the example's output should be the agent's, not the server's

    def _send(self, body, content_type):
        payload = body.encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_GET(self):
        path = self.path
        if path == "/":
            return self._send(INDEX, "text/html; charset=utf-8")
        if path == "/volatile":
            return self._send(VOLATILE, "text/html; charset=utf-8")
        if path == "/api/flights":
            return self._send(json.dumps(FLIGHTS), "application/json")
        for prefix, page in (("/flight/", DETAIL), ("/book/", BOOKED)):
            if path.startswith(prefix):
                flight_id = path[len(prefix):]
                if flight_id not in BY_ID:
                    return self._send("<h1>unknown flight</h1>", "text/html")
                return self._send(page % {"id": flight_id}, "text/html; charset=utf-8")
        if path.startswith("/api/flight/"):
            flight = BY_ID.get(path.rsplit("/", 1)[-1])
            return self._send(json.dumps(flight or {}), "application/json")
        if path.startswith("/api/book/"):
            flight = BY_ID.get(path.rsplit("/", 1)[-1])
            status = "confirmed" if flight and flight["seats"] > 0 else "sold out"
            return self._send(json.dumps({"status": status}), "application/json")
        self.send_error(404)


def serve(port):
    return ThreadingHTTPServer(("127.0.0.1", port), Handler)


if __name__ == "__main__":
    import sys

    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8099
    server = serve(port)
    print(f"serving on http://127.0.0.1:{port}")
    server.serve_forever()
