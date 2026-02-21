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
  const authHeader = headers.authorization || headers.Authorization || '';
  const contentType = headers['content-type'] || headers['Content-Type'] || 'application/json';
  const bodyJson = JSON.stringify(req.body || {});
  let curl = `curl -X ${method} \\\n`;
  curl += `  -H "Content-Type: ${contentType}" \\\n`;
  if (authHeader) {
    curl += `  -H "Authorization: ${authHeader}" \\\n`;
  }
  curl += `  -d '${bodyJson}' \\\n`;
  curl += `  "${downstreamUrl}"`;
  return curl;
}

async function copyToClipboard(text, label) {
  try {
    await navigator.clipboard.writeText(text);
    const oldStatus = document.getElementById('status').textContent;
    setStatus(`✓ Copied ${label} to clipboard`);
    setTimeout(() => setStatus(oldStatus), 2000);
  } catch(e) {
    setStatus('Failed to copy: ' + e.message);
  }
}

function sanitizeContent(content) {
  return content.replace(/\\n/g, '\n');
}

function conversationToMarkdown(messages) {
  let md = '';
  for (const msg of messages) {
    const role = msg.role || 'unknown';
    const content = sanitizeContent(msg.content || '');
    const displayRole = role === 'system' ? 'System'
                      : role === 'user' ? 'User'
                      : role === 'assistant' ? 'Assistant'
                      : role.charAt(0).toUpperCase() + role.slice(1);
    md += `=== ${displayRole} ===\n${content}\n\n`;
  }
  return md.trim();
}

function responseToMarkdown(content) {
  return `=== Assistant ===\n${sanitizeContent(content)}`;
}

function downloadMarkdown(content, filename) {
  const blob = new Blob([content], { type: 'text/markdown' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
  const oldStatus = document.getElementById('status').textContent;
  setStatus(`✓ Downloaded ${filename}`);
  setTimeout(() => setStatus(oldStatus), 2000);
}

function downloadConversation(tx) {
  const messages = tx.request?.body?.messages || [];
  if (!messages.length) { setStatus('No messages to download'); return; }
  const md = conversationToMarkdown(messages);
  const timestamp = tx.timestamp ? new Date(tx.timestamp).toISOString().replace(/[:.]/g, '-') : 'unknown';
  downloadMarkdown(md, `conversation_${timestamp}.md`);
}

function downloadResponse(tx) {
  const choices = tx.response?.body?.choices || [];
  if (!choices.length) { setStatus('No response to download'); return; }
  const content = choices[0]?.message?.content || '';
  if (!content) { setStatus('Response has no content'); return; }
  const md = responseToMarkdown(content);
  const timestamp = tx.timestamp ? new Date(tx.timestamp).toISOString().replace(/[:.]/g, '-') : 'unknown';
  downloadMarkdown(md, `response_${timestamp}.md`);
}

function attachCopyHandlersToCard(card) {
  card.querySelectorAll('.response-details pre, .request-details pre, .curl-details pre').forEach(pre => {
    if (!pre.dataset.copyAttached) {
      pre.style.cursor = 'pointer';
      const label = pre.closest('.response-details') ? 'response body'
                  : pre.closest('.request-details') ? 'request body'
                  : 'curl command';
      pre.addEventListener('click', () => copyToClipboard(pre.textContent, label));
      pre.dataset.copyAttached = 'true';
    }
  });
  card.querySelectorAll('.download-md').forEach(btn => {
    btn.addEventListener('click', (e) => e.stopPropagation());
  });
}

function renderTxCard(tx) {
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
        <div class="detail-content">
          <button class="download-md" onclick='downloadResponse(${JSON.stringify(tx).replace(/'/g, "&#39;")})'>⬇ Response.md</button>
          <pre>${JSON.stringify(res.body, null, 2)}</pre>
        </div>
      </details>
      <details class="request-details">
        <summary>Request body</summary>
        <div class="detail-content">
          <button class="download-md" onclick='downloadConversation(${JSON.stringify(tx).replace(/'/g, "&#39;")})'>⬇ Conversation.md</button>
          <pre>${JSON.stringify(req.body, null, 2)}</pre>
        </div>
      </details>
      <details class="curl-details">
        <summary>Replay with curl (downstream)</summary>
        <pre>${curlCmd}</pre>
      </details>
    </div>`;
}

// Load existing transactions on page load via HTTP
async function loadTransactions() {
  try {
    const r = await fetch('/api/transactions');
    const txs = await r.json();
    const el = document.getElementById('transactions');
    if (!txs.length) {
      el.innerHTML = '<p style="color:#475569">No transactions yet.</p>';
      return;
    }
    el.innerHTML = txs.map(tx => renderTxCard(tx)).join('');
    el.querySelectorAll('.tx-card').forEach(card => attachCopyHandlersToCard(card));
  } catch(e) {
    // silently ignore load errors
  }
}

// Prepend a single new transaction card at the top (no re-render of existing cards)
function prependTransaction(tx) {
  const el = document.getElementById('transactions');
  // Remove "No transactions yet" placeholder if present
  const placeholder = el.querySelector('p');
  if (placeholder) placeholder.remove();

  const html = renderTxCard(tx);
  const temp = document.createElement('div');
  temp.innerHTML = html;
  const card = temp.firstElementChild;
  el.prepend(card);
  attachCopyHandlersToCard(card);
}

// WebSocket connection with auto-reconnect
function connectWebSocket() {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const wsUrl = `${protocol}//${window.location.host}/ws`;
  const ws = new WebSocket(wsUrl);

  ws.onopen = () => {
    console.log('WebSocket connected');
    setStatus('Connected (live updates)');
  };

  ws.onmessage = (event) => {
    try {
      const tx = JSON.parse(event.data);
      prependTransaction(tx);
    } catch(e) {
      console.error('Failed to parse WS message:', e);
    }
  };

  ws.onclose = () => {
    console.log('WebSocket disconnected, reconnecting in 3s…');
    setStatus('Disconnected. Reconnecting…');
    setTimeout(connectWebSocket, 3000);
  };

  ws.onerror = (err) => {
    console.error('WebSocket error:', err);
    ws.close();
  };
}

// Initialize: load existing transactions, then connect WebSocket for live updates
loadTransactions();
connectWebSocket();
