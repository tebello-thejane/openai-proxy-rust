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

function generateCurl(tx) {
  const req = tx.request || {};
  const downstreamUrl = req.downstream_url || 'https://api.openai.com/v1/chat/completions';
  const method = req.method || 'POST';
  const headers = req.headers || {};
  // Filter proxy-specific headers
  const filteredHeaders = {};
  for (const [k, v] of Object.entries(headers)) {
    const lowerK = k.toLowerCase();
    if (!['host', 'content-length', 'x-forwarded-for', 'x-forwarded-host', 'x-forwarded-proto', 'origin', 'referer', 'sec-fetch-*'].includes(lowerK)) {
      filteredHeaders[k] = v;
    }
  }
  const headersStr = Object.entries(filteredHeaders).map(([k, v]) => `-H "${k}: ${v.replace(/"/g, '\\"')}"`).join(' \\\n  ');
  const jsonBody = req.body ? JSON.stringify(req.body) : null;
  const bodyStr = jsonBody ? ` \\\n  --data-raw '${jsonBody}'` : '';
  return `curl -X ${method}${bodyStr.length ? ' \\\n  ' + headersStr + bodyStr : ' ' + headersStr} \\\n  "${downstreamUrl}"`;
}

async function refreshTransactions() {
  // Capture current open states
  const openStates = {};
  document.querySelectorAll('.tx-card').forEach(card => {
    const txId = card.dataset.txId;
    if (txId) {
      openStates[txId] = [];
      const responseDetail = card.querySelector('.response-details');
      const requestDetail = card.querySelector('.request-details');
      const curlDetail = card.querySelector('.curl-details');
      if (responseDetail && responseDetail.open) openStates[txId].push('response');
      if (requestDetail && requestDetail.open) openStates[txId].push('request');
      if (curlDetail && curlDetail.open) openStates[txId].push('curl');
    }
  });

  try {
    const r = await fetch('/api/transactions');
    const txs = await r.json();
    const el = document.getElementById('transactions');
    if (!txs.length) {
      el.innerHTML = '<p style="color:#475569">No transactions yet.</p>';
      return;
    }
    el.innerHTML = txs.map(tx => {
      const req = tx.request || {};
      const res = tx.response || {};
      const sc = statusClass(res.status);
      const txId = tx.id || '';
      const curlCmd = generateCurl(tx);
      return `
        <div class="tx-card" data-tx-id="${txId}">
          <div class="tx-header">
            <span class="ts">${fmtTs(tx.timestamp)}</span>
            <span class="method">${req.method || '?'}</span>
            <span class="${sc}">${res.status || '?'}</span>
            <span class="latency">${res.latency_ms != null ? res.latency_ms + ' ms' : ''}</span>
          </div>
          <details class="response-details">
            <summary>Response body</summary>
            <pre>${JSON.stringify(res.body, null, 2)}</pre>
          </details>
          <details class="request-details">
            <summary>Request body</summary>
            <pre>${JSON.stringify(req.body, null, 2)}</pre>
          </details>
          <details class="curl-details">
            <summary>Replay with curl (downstream)</summary>
            <pre>${curlCmd}</pre>
          </details>
        </div>`;
    }).join('');

    // Restore open states
    setTimeout(() => {
      Object.entries(openStates).forEach(([txId, sections]) => {
        const card = document.querySelector(`.tx-card[data-tx-id="${txId}"]`);
        if (card) {
          if (sections.includes('response')) {
            const detail = card.querySelector('.response-details');
            if (detail) detail.open = true;
          }
          if (sections.includes('request')) {
            const detail = card.querySelector('.request-details');
            if (detail) detail.open = true;
          }
          if (sections.includes('curl')) {
            const detail = card.querySelector('.curl-details');
            if (detail) detail.open = true;
          }
        }
      });
    }, 0);
  } catch(e) {
    // silently ignore refresh errors
  }
}

refreshTransactions();
setInterval(refreshTransactions, 3000);
