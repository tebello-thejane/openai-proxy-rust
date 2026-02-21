use axum::response::Html;
use axum::Json;
use serde_json::Value;
use tokio::fs;

//language=xhtml
const HTML_CONTENT: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>OpenAI Proxy Dashboard</title>
  <style>
    *, *::before, *::after { box-sizing: border-box; }
    body {
      font-family: 'Segoe UI', system-ui, sans-serif;
      background: #0f1117;
      color: #e2e8f0;
      margin: 0;
      padding: 24px;
    }
    h1 { font-size: 1.5rem; margin-bottom: 20px; color: #a78bfa; }
    h2 { font-size: 1rem; color: #94a3b8; text-transform: uppercase; letter-spacing: 0.08em; margin: 24px 0 12px; }
    .controls {
      display: flex;
      align-items: center;
      gap: 10px;
      flex-wrap: wrap;
    }
    input[type="text"] {
      background: #1e2130;
      border: 1px solid #334155;
      color: #e2e8f0;
      border-radius: 6px;
      padding: 8px 12px;
      font-size: 0.9rem;
      width: 300px;
    }
    input[type="text"]::placeholder { color: #64748b; }
    button {
      background: #6d28d9;
      color: #fff;
      border: none;
      border-radius: 6px;
      padding: 8px 16px;
      font-size: 0.9rem;
      cursor: pointer;
      transition: background 0.15s;
    }
    button:hover { background: #7c3aed; }
    #status {
      margin-top: 12px;
      padding: 10px 14px;
      background: #1e2130;
      border-left: 3px solid #6d28d9;
      border-radius: 0 6px 6px 0;
      font-size: 0.875rem;
      min-height: 38px;
      color: #94a3b8;
      white-space: pre-wrap;
      word-break: break-all;
    }
    #transactions { display: flex; flex-direction: column; gap: 10px; }
    .tx-card {
      background: #1e2130;
      border: 1px solid #334155;
      border-radius: 8px;
      overflow: hidden;
    }
    .tx-header {
      display: flex;
      align-items: center;
      gap: 12px;
      padding: 10px 14px;
      font-size: 0.85rem;
      font-family: 'Cascadia Code', 'Fira Code', monospace;
    }
    .ts { color: #64748b; }
    .method { color: #38bdf8; font-weight: 600; }
    .status-ok  { color: #34d399; font-weight: 600; }
    .status-err { color: #f87171; font-weight: 600; }
    .latency { color: #94a3b8; }
    details { border-top: 1px solid #334155; }
    summary {
      padding: 6px 14px;
      font-size: 0.8rem;
      color: #64748b;
      cursor: pointer;
      user-select: none;
    }
    summary:hover { color: #94a3b8; }
    pre {
      margin: 0;
      padding: 12px 14px;
      background: #131620;
      font-size: 0.78rem;
      overflow-x: auto;
      color: #a5b4fc;
    }
    .hint { font-size: 0.78rem; color: #475569; margin-top: 8px; }
  </style>
</head>
<body>
  <h1>OpenAI Proxy Dashboard</h1>

  <div class="controls">
    <input type="text" id="apiKey" placeholder="OpenAI API key (sk-…)" />
    <button onclick="pingTest()">Ping /test</button>
    <button onclick="sendTestMessage()">Send Test Message</button>
  </div>
  <div id="status">Ready.</div>

  <h2>Transaction Log</h2>
  <div id="transactions"><p style="color:#475569">Loading…</p></div>
  <p class="hint">Auto-refreshes every 3 s</p>

  <script>
    function setStatus(text) {
      document.getElementById('status').textContent = text;
    }

    async function pingTest() {
      setStatus('Pinging…');
      try {
        const r = await fetch('/test');
        const t = await r.text();
        setStatus(t);
      } catch(e) {
        setStatus('Error: ' + e.message);
      }
    }

    async function sendTestMessage() {
      const key = document.getElementById('apiKey').value.trim();
      if (!key) { setStatus('Please enter an API key first.'); return; }
      setStatus('Sending test message…');
      try {
        const r = await fetch('/v1/chat/completions', {
          method: 'POST',
          headers: {
            'Authorization': 'Bearer ' + key,
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({
            model: 'gpt-4o-mini',
            messages: [{ role: 'user', content: 'Say "hi" in one word.' }],
          }),
        });
        const json = await r.json();
        setStatus('Status ' + r.status + '\n' + JSON.stringify(json, null, 2));
      } catch(e) {
        setStatus('Error: ' + e.message);
      }
    }

    function fmtTs(ts) {
      if (!ts) return '?';
      const d = new Date(ts);
      return d.toLocaleString();
    }

    function statusClass(code) {
      return (code >= 200 && code < 300) ? 'status-ok' : 'status-err';
    }

    async function refreshTransactions() {
      try {
        const r = await fetch('/api/transactions');
        const txs = await r.json();
        const el = document.getElementById('transactions');
        if (!txs.length) { el.innerHTML = '<p style="color:#475569">No transactions yet.</p>'; return; }
        el.innerHTML = txs.map(tx => {
          const req = tx.request || {};
          const res = tx.response || {};
          const sc = statusClass(res.status);
          return `
            <div class="tx-card">
              <div class="tx-header">
                <span class="ts">${fmtTs(tx.timestamp)}</span>
                <span class="method">${req.method || '?'}</span>
                <span class="${sc}">${res.status || '?'}</span>
                <span class="latency">${res.latency_ms != null ? res.latency_ms + ' ms' : ''}</span>
              </div>
              <details>
                <summary>Response body</summary>
                <pre>${JSON.stringify(res.body, null, 2)}</pre>
              </details>
              <details>
                <summary>Request body</summary>
                <pre>${JSON.stringify(req.body, null, 2)}</pre>
              </details>
            </div>`;
        }).join('');
      } catch(e) {
        // silently ignore refresh errors
      }
    }

    refreshTransactions();
    setInterval(refreshTransactions, 3000);
  </script>
</body>
</html>
"#;

pub async fn dashboard() -> Html<&'static str> {
    Html(HTML_CONTENT)
}

pub async fn list_transactions() -> Json<Vec<Value>> {
    let mut entries = Vec::new();

    if let Ok(mut dir) = fs::read_dir("log").await {
        while let Ok(Some(entry)) = dir.next_entry().await {
            let path = entry.path();
            let is_json = path.extension().and_then(|e| e.to_str()) == Some("json");
            if !is_json {
                continue;
            }
            if let Ok(contents) = fs::read_to_string(&path).await {
                if let Ok(val) = serde_json::from_str::<Value>(&contents) {
                    entries.push(val);
                }
            }
        }
    }

    // Sort newest-first by the "timestamp" string field (ISO 8601 — lexicographic sort works)
    entries.sort_by(|a, b| {
        let ta = a.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        let tb = b.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        tb.cmp(ta)
    });

    Json(entries)
}
