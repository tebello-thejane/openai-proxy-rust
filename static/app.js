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

function createDetailsSection(className, summaryText, preContent, downloadBtn) {
  const details = document.createElement('details');
  details.className = className;

  const summary = document.createElement('summary');
  summary.textContent = summaryText;
  details.appendChild(summary);

  const content = document.createElement('div');
  content.className = 'detail-content';

  if (downloadBtn) {
    content.appendChild(downloadBtn);
  }

  const pre = document.createElement('pre');
  pre.textContent = preContent;
  content.appendChild(pre);

  details.appendChild(content);
  return details;
}

function createTxCard(tx) {
  const req = tx.request || {};
  const res = tx.response || {};
  const txId = tx.id || '';
  const curlCmd = generateCurl(tx);

  const card = document.createElement('div');
  card.className = 'tx-card';
  card.dataset.txId = txId;

  // Header row
  const header = document.createElement('div');
  header.className = 'tx-header';

  const tsSpan = document.createElement('span');
  tsSpan.className = 'ts';
  tsSpan.textContent = fmtTs(tx.timestamp);

  const methodSpan = document.createElement('span');
  methodSpan.className = 'method';
  methodSpan.textContent = req.method || '?';

  const statusSpan = document.createElement('span');
  statusSpan.className = statusClass(res.status);
  statusSpan.textContent = String(res.status || '?');

  const latencySpan = document.createElement('span');
  latencySpan.className = 'latency';
  latencySpan.textContent = res.latency_ms != null ? res.latency_ms + ' ms' : '';

  header.appendChild(tsSpan);
  header.appendChild(methodSpan);
  header.appendChild(statusSpan);
  header.appendChild(latencySpan);
  card.appendChild(header);

  // Response details
  const respBtn = document.createElement('button');
  respBtn.className = 'download-md';
  respBtn.textContent = '⬇ Response.md';
  respBtn.addEventListener('click', (e) => { e.stopPropagation(); downloadResponse(tx); });

  card.appendChild(createDetailsSection(
    'response-details',
    'Response body',
    JSON.stringify(res.body, null, 2),
    respBtn
  ));

  // Request details
  const reqBtn = document.createElement('button');
  reqBtn.className = 'download-md';
  reqBtn.textContent = '⬇ Conversation.md';
  reqBtn.addEventListener('click', (e) => { e.stopPropagation(); downloadConversation(tx); });

  card.appendChild(createDetailsSection(
    'request-details',
    'Request body',
    JSON.stringify(req.body, null, 2),
    reqBtn
  ));

  // Curl details (no detail-content wrapper needed)
  const curlDetails = document.createElement('details');
  curlDetails.className = 'curl-details';

  const curlSummary = document.createElement('summary');
  curlSummary.textContent = 'Replay with curl (downstream)';
  curlDetails.appendChild(curlSummary);

  const curlPre = document.createElement('pre');
  curlPre.textContent = curlCmd;
  curlDetails.appendChild(curlPre);

  card.appendChild(curlDetails);

  return card;
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
    el.innerHTML = '';
    for (const tx of txs) {
      el.appendChild(createTxCard(tx));
    }
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

  const card = createTxCard(tx);
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
