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
