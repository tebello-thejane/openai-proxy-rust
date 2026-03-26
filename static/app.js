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
  // Extract date-time and timezone from ISO 8601 (e.g., 2026-03-25T20:34:09+00:00 or 2026-03-25T20:34:09.638351441Z)
  const match = ts.match(/^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2})(?:\.\d+)?([+-]\d{2}:\d{2}|Z)$/);
  if (match) {
    const [, datetime, tz] = match;
    return datetime.replace('T', ' ') + ' ' + tz;
  }
  return ts.replace('T', ' ').replace(/(\.\d+)?(Z|[+-].*)$/, '').trim();
}

function statusClass(code) {
  return (code >= 200 && code < 300) ? 'status-ok' : 'status-err';
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

async function downloadConversation(tx) {
  const txId = tx.id;
  if (!txId) { setStatus('No transaction ID'); return; }
  try {
    const r = await fetch(`/api/transactions/${txId}/conversation`);
    if (!r.ok) {
      const msg = await r.text();
      setStatus(msg || 'Failed to download');
      return;
    }
    const md = await r.text();
    const timestamp = tx.timestamp ? new Date(tx.timestamp).toISOString().replace(/[:.]/g, '-') : 'unknown';
    downloadMarkdown(md, `conversation_${timestamp}.md`);
  } catch(e) {
    setStatus('Error: ' + e.message);
  }
}

async function downloadResponse(tx) {
  const txId = tx.id;
  if (!txId) { setStatus('No transaction ID'); return; }
  try {
    const r = await fetch(`/api/transactions/${txId}/response`);
    if (!r.ok) {
      const msg = await r.text();
      setStatus(msg || 'Failed to download');
      return;
    }
    const md = await r.text();
    const timestamp = tx.timestamp ? new Date(tx.timestamp).toISOString().replace(/[:.]/g, '-') : 'unknown';
    downloadMarkdown(md, `response_${timestamp}.md`);
  } catch(e) {
    setStatus('Error: ' + e.message);
  }
}


function createTxCard(tx) {
  // Handle both full transaction (from WebSocket) and summary (from API)
  const txId = tx.id || '';
  const method = tx.method || tx.request?.method || '?';
  const status = tx.status ?? tx.response?.status;
  const latencyMs = tx.latency_ms ?? tx.response?.latency_ms;
  const timestamp = tx.timestamp;

  const card = document.createElement('div');
  card.className = 'tx-card';
  card.dataset.txId = txId;
  card.dataset.loaded = 'false';
  // Pre-cache if this is a full transaction (from WebSocket)
  if (tx.request && tx.response) {
    card._cachedTx = tx;
  }

  // Header row
  const header = document.createElement('div');
  header.className = 'tx-header';

  const tsSpan = document.createElement('span');
  tsSpan.className = 'ts';
  tsSpan.textContent = fmtTs(timestamp);

  const methodSpan = document.createElement('span');
  methodSpan.className = 'method';
  methodSpan.textContent = method;

  const statusSpan = document.createElement('span');
  statusSpan.className = statusClass(status);
  statusSpan.textContent = String(status || '?');

  const latencySpan = document.createElement('span');
  latencySpan.className = 'latency';
  latencySpan.textContent = latencyMs != null ? latencyMs + ' ms' : '';

  header.appendChild(tsSpan);
  header.appendChild(methodSpan);
  header.appendChild(statusSpan);
  header.appendChild(latencySpan);
  card.appendChild(header);

  // Helper to create lazy-loaded details section
  function createLazyDetailsSection(className, summaryText, placeholderText) {
    const details = document.createElement('details');
    details.className = className;

    const sum = document.createElement('summary');
    sum.textContent = summaryText;
    details.appendChild(sum);

    const content = document.createElement('div');
    content.className = 'detail-content';

    const placeholder = document.createElement('p');
    placeholder.className = 'detail-placeholder';
    placeholder.textContent = placeholderText;
    placeholder.style.color = '#64748b';
    content.appendChild(placeholder);

    details.appendChild(content);

    // Fetch full transaction when opened
    details.addEventListener('toggle', async function() {
      if (this.open && !this.dataset.loaded) {
        const card = this.closest('.tx-card');
        const isResponse = className === 'response-details';

        // Check if already cached in another section
        const cached = card?._cachedTx;
        if (cached) {
          this.dataset.loaded = 'true';
          const req = cached.request || {};
          const res = cached.response || {};

          content.innerHTML = '';

          const btn = document.createElement('button');
          btn.className = 'download-md';
          btn.textContent = isResponse ? '⬇ Response.md' : '⬇ Conversation.md';
          btn.addEventListener('click', (e) => {
            e.stopPropagation();
            if (isResponse) downloadResponse(cached);
            else downloadConversation(cached);
          });
          content.appendChild(btn);

          const pre = document.createElement('pre');
          pre.textContent = JSON.stringify(
            isResponse ? res.body : req.body,
            null, 2
          );
          content.appendChild(pre);
          pre.style.cursor = 'pointer';
          pre.addEventListener('click', () => copyToClipboard(
            pre.textContent,
            isResponse ? 'response body' : 'request body'
          ));
          return;
        }

        placeholder.textContent = 'Loading...';
        try {
          const r = await fetch('/api/transactions/' + txId);
          const fullTx = await r.json();

          if (fullTx && fullTx.id) {
            this.dataset.loaded = 'true';
            if (card) {
              card.dataset.loaded = 'true';
              card._cachedTx = fullTx;
            }
            const req = fullTx.request || {};
            const res = fullTx.response || {};

            // Clear placeholder
            content.innerHTML = '';

            // Add download button and content
            const btn = document.createElement('button');
            btn.className = 'download-md';
            btn.textContent = isResponse ? '⬇ Response.md' : '⬇ Conversation.md';
            btn.addEventListener('click', (e) => {
              e.stopPropagation();
              if (isResponse) downloadResponse(fullTx);
              else downloadConversation(fullTx);
            });
            content.appendChild(btn);

            const pre = document.createElement('pre');
            pre.textContent = JSON.stringify(
              isResponse ? res.body : req.body,
              null, 2
            );
            content.appendChild(pre);
            pre.style.cursor = 'pointer';
            pre.addEventListener('click', () => copyToClipboard(
              pre.textContent,
              isResponse ? 'response body' : 'request body'
            ));
          }
        } catch(e) {
          placeholder.textContent = 'Failed to load: ' + e.message;
        }
      }
    });

    return details;
  }

  // Response details (closed by default)
  card.appendChild(createLazyDetailsSection(
    'response-details',
    'Response body',
    'Click to load...'
  ));

  // Request details (closed by default)
  card.appendChild(createLazyDetailsSection(
    'request-details',
    'Request body',
    'Click to load...'
  ));

  return card;
}

// Load existing transactions on page load via HTTP (summary only)
async function loadTransactions() {
  try {
    const r = await fetch('/api/transactions/summary');
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
      updateStatsFromTransaction(tx);
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

// Metrics functions with time window support
let lastDashboardStats = null;
let currentTimeWindow = '1h';

// Time window labels for display
const timeWindowLabels = {
  '1m': '1 minute',
  '5m': '5 minutes',
  '15m': '15 minutes',
  '1h': '1 hour',
  '6h': '6 hours',
  '12h': '12 hours',
  '24h': '24 hours'
};

function getCurrentTimeWindow() {
  const select = document.getElementById('time-window');
  return select ? select.value : '1h';
}

function onTimeWindowChange() {
  currentTimeWindow = getCurrentTimeWindow();
  loadDashboardStats();
  updateLabels();
}

function updateLabels() {
  const windowLabel = timeWindowLabels[currentTimeWindow] || 'selected window';
  const requestsLabel = document.getElementById('label-requests');
  const costLabel = document.getElementById('label-cost');

  if (requestsLabel) requestsLabel.textContent = `Requests (${windowLabel})`;
  if (costLabel) costLabel.textContent = `Cost (${windowLabel})`;
}

async function loadDashboardStats() {
  try {
    const window = getCurrentTimeWindow();
    const r = await fetch(`/api/metrics/dashboard/v2?window=${window}`);
    const stats = await r.json();
    lastDashboardStats = stats;
    renderStats(stats);
  } catch(e) {
    console.error('Failed to load dashboard stats:', e);
  }
}

function renderStats(stats) {
  if (!stats) return;

  // Handle v2 format: { current: { window, stats }, per_model }
  const current = stats.current?.stats || stats.last_hour || {};

  document.getElementById('stat-requests').textContent = current.requests !== undefined ? current.requests : '-';
  document.getElementById('stat-latency').textContent = current.avg_latency_ms !== undefined
    ? Math.round(current.avg_latency_ms) + ' ms'
    : '-';
  document.getElementById('stat-errors').textContent = current.error_rate !== undefined
    ? current.error_rate.toFixed(1) + '%'
    : '-';
  document.getElementById('stat-cost').textContent = current.cost !== undefined
    ? '$' + current.cost.toFixed(4)
    : '-';
}

function updateStatsFromTransaction(tx) {
  // Optimistic update on WebSocket message
  if (!lastDashboardStats) return;

  const res = tx.response || {};
  const status = res.status || 0;
  const latency = res.latency_ms || 0;

  // Get current stats (handle both v1 and v2 formats)
  const current = lastDashboardStats.current?.stats || lastDashboardStats.last_hour || {};

  // Update current window stats
  current.requests = (current.requests || 0) + 1;
  const totalLatency = (current.avg_latency_ms || 0) * (current.requests - 1) + latency;
  current.avg_latency_ms = totalLatency / current.requests;

  if (status >= 400) {
    const errors = (current.error_rate || 0) * (current.requests - 1) / 100 + 1;
    current.error_rate = (errors / current.requests) * 100;
  }

  // Also update legacy format if present
  if (lastDashboardStats.last_hour) {
    lastDashboardStats.last_hour.requests = (lastDashboardStats.last_hour.requests || 0) + 1;
  }
  if (lastDashboardStats.today) {
    lastDashboardStats.today.requests = (lastDashboardStats.today.requests || 0) + 1;
  }

  renderStats(lastDashboardStats);
}

// Initialize: load existing transactions and metrics, then connect WebSocket for live updates
currentTimeWindow = getCurrentTimeWindow();
updateLabels();
loadTransactions();
loadDashboardStats();
connectWebSocket();

// Refresh metrics every 10 seconds
setInterval(() => {
  loadDashboardStats();
}, 10000);
