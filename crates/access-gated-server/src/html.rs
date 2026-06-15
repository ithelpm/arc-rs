pub fn storefront() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>arc-x402 Marketplace</title>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { background: #0f172a; color: #e2e8f0; font-family: system-ui, sans-serif; padding: 2rem; }
  h1 { font-size: 1.75rem; font-weight: 700; margin-bottom: 0.5rem; }
  .subtitle { color: #94a3b8; margin-bottom: 2rem; }
  nav a { color: #f59e0b; text-decoration: none; margin-right: 1.5rem; }
  nav { margin-bottom: 2rem; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 1.5rem; }
  .card { background: #1e293b; border-radius: 0.75rem; padding: 1.5rem; }
  .card h2 { font-size: 1.1rem; margin-bottom: 0.5rem; }
  .card p { color: #94a3b8; font-size: 0.9rem; margin-bottom: 1rem; }
  .price-row { display: flex; gap: 1rem; margin-bottom: 1rem; }
  .price { color: #f59e0b; font-weight: 600; font-size: 0.85rem; }
  input[type=text] { width: 100%; background: #0f172a; border: 1px solid #334155; border-radius: 0.5rem; color: #e2e8f0; padding: 0.5rem 0.75rem; font-size: 0.9rem; margin-bottom: 0.75rem; }
  button { background: #f59e0b; color: #0f172a; border: none; border-radius: 0.5rem; padding: 0.5rem 1.25rem; font-weight: 600; cursor: pointer; font-size: 0.9rem; }
  button:hover { background: #d97706; }
  .result { margin-top: 1rem; background: #0f172a; border-radius: 0.5rem; padding: 1rem; font-size: 0.8rem; }
  pre { white-space: pre-wrap; word-break: break-all; }
  .ok { color: #4ade80; }
  .err { color: #f87171; }
  .cmd { background: #1e293b; border: 1px solid #334155; border-radius: 0.375rem; padding: 0.25rem 0.5rem; font-family: monospace; color: #f59e0b; }
  #loading { color: #94a3b8; }
</style>
</head>
<body>
<nav>
  <a href="/">Marketplace</a>
  <a href="/stats">Stats</a>
</nav>
<h1>arc-x402 Marketplace</h1>
<p class="subtitle">x402-gated content on Arc testnet. Pay per view or stream by the chunk.</p>
<div id="loading">Loading items...</div>
<div id="grid" class="grid" style="display:none"></div>
<script>
async function loadItems() {
  const res = await fetch('/items');
  const items = await res.json();
  const grid = document.getElementById('grid');
  const loading = document.getElementById('loading');
  loading.style.display = 'none';
  grid.style.display = '';
  if (!items.length) {
    grid.innerHTML = '<p style="color:#94a3b8">No items listed yet.</p>';
    return;
  }
  grid.innerHTML = items.map(item => `
    <div class="card">
      <h2>${esc(item.title)}</h2>
      <p>${esc(item.description || 'No description')}</p>
      <div class="price-row">
        <span class="price">Buy: ${(item.buy_price_atomic/1e6).toFixed(2)} USDC</span>
        ${item.chunk_price_atomic ? `<span class="price">Chunk: ${(item.chunk_price_atomic/1e6).toFixed(4)} USDC</span>` : ''}
      </div>
      <input type="text" id="w-${esc(item.item_id)}" placeholder="0x wallet address">
      <button onclick="checkAccess('${esc(item.item_id)}')">Check Access</button>
      <div id="r-${esc(item.item_id)}" class="result" style="display:none"></div>
    </div>
  `).join('');
}

function esc(s) { return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;'); }

async function checkAccess(itemId) {
  const wallet = document.getElementById('w-' + itemId).value.trim();
  const resultEl = document.getElementById('r-' + itemId);
  resultEl.style.display = 'block';
  resultEl.innerHTML = 'Checking...';

  const url = '/content/' + encodeURIComponent(itemId) + (wallet ? '?wallet=' + encodeURIComponent(wallet) : '');
  try {
    const res = await fetch(url);
    if (res.ok) {
      const data = await res.json();
      resultEl.innerHTML = '<div class="ok">Access granted!</div><pre>' + esc(JSON.stringify(data, null, 2)) + '</pre>';
    } else if (res.status === 402) {
      const payReq = res.headers.get('payment-required');
      resultEl.innerHTML = '<div class="err">Payment required (402)</div>' +
        '<p style="margin-top:0.75rem;color:#94a3b8;font-size:0.8rem">Pay with Circle CLI:</p>' +
        '<p style="margin-top:0.5rem"><span class="cmd">circle services pay ' + location.origin + '/content/' + esc(itemId) + ' --wallet ' + esc(wallet || '&lt;your-wallet&gt;') + '</span></p>' +
        (payReq ? '<pre style="margin-top:0.75rem;color:#94a3b8">' + esc(payReq) + '</pre>' : '');
    } else {
      const text = await res.text();
      resultEl.innerHTML = '<div class="err">Error ' + res.status + ': ' + esc(text) + '</div>';
    }
  } catch(e) {
    resultEl.innerHTML = '<div class="err">Network error: ' + esc(e.message) + '</div>';
  }
}

loadItems();
</script>
</body>
</html>"#.to_string()
}

pub fn stats_page() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>arc-x402 Stats</title>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { background: #0f172a; color: #e2e8f0; font-family: system-ui, sans-serif; padding: 2rem; }
  h1 { font-size: 1.75rem; font-weight: 700; margin-bottom: 0.5rem; }
  .subtitle { color: #94a3b8; margin-bottom: 2rem; }
  nav a { color: #f59e0b; text-decoration: none; margin-right: 1.5rem; }
  nav { margin-bottom: 2rem; }
  .stat-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 1rem; margin-bottom: 2rem; }
  .stat-card { background: #1e293b; border-radius: 0.75rem; padding: 1.5rem; }
  .stat-label { color: #94a3b8; font-size: 0.85rem; margin-bottom: 0.5rem; }
  .stat-value { font-size: 2rem; font-weight: 700; color: #f59e0b; }
  table { width: 100%; border-collapse: collapse; background: #1e293b; border-radius: 0.75rem; overflow: hidden; }
  th { background: #0f172a; padding: 0.75rem 1rem; text-align: left; font-size: 0.85rem; color: #94a3b8; }
  td { padding: 0.75rem 1rem; border-top: 1px solid #334155; font-size: 0.9rem; }
  h2 { margin-bottom: 1rem; }
  .refresh { color: #94a3b8; font-size: 0.8rem; margin-bottom: 1rem; }
</style>
</head>
<body>
<nav>
  <a href="/">Marketplace</a>
  <a href="/stats">Stats</a>
</nav>
<h1>Marketplace Stats</h1>
<p class="subtitle">Payment analytics across all items.</p>
<p class="refresh" id="refresh-timer">Auto-refreshes every 30s</p>
<div class="stat-grid" id="stat-cards"></div>
<h2>Per-Item Breakdown</h2>
<table id="items-table">
  <thead><tr><th>Item</th><th>Payments</th><th>Volume (USDC)</th></tr></thead>
  <tbody id="items-body"><tr><td colspan="3" style="color:#94a3b8">Loading...</td></tr></tbody>
</table>
<script>
async function loadStats() {
  try {
    const res = await fetch('/api/stats');
    const s = await res.json();
    document.getElementById('stat-cards').innerHTML = `
      <div class="stat-card"><div class="stat-label">Total Payments</div><div class="stat-value">${s.total_payments}</div></div>
      <div class="stat-card"><div class="stat-label">Volume (USDC)</div><div class="stat-value">${(s.total_volume_atomic/1e6).toFixed(2)}</div></div>
      <div class="stat-card"><div class="stat-label">Unique Buyers</div><div class="stat-value">${s.unique_buyers}</div></div>
      <div class="stat-card"><div class="stat-label">Unique Sellers</div><div class="stat-value">${s.unique_sellers}</div></div>
    `;
    document.getElementById('items-body').innerHTML = s.items.length
      ? s.items.map(i => `<tr><td>${esc(i.title)}<br><span style="color:#94a3b8;font-size:0.8rem">${esc(i.item_id)}</span></td><td>${i.payments}</td><td>${(i.volume_atomic/1e6).toFixed(4)}</td></tr>`).join('')
      : '<tr><td colspan="3" style="color:#94a3b8">No payments yet.</td></tr>';
  } catch(e) {
    console.error(e);
  }
}

function esc(s) { return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;'); }

loadStats();
setInterval(loadStats, 30000);

let secs = 30;
const timer = setInterval(() => {
  secs--;
  if (secs <= 0) secs = 30;
  document.getElementById('refresh-timer').textContent = `Auto-refreshes in ${secs}s`;
}, 1000);
</script>
</body>
</html>"#.to_string()
}
