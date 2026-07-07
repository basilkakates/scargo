// Scargo dashboard — vanilla JS + Chart.js
// Fetches from /api endpoints, renders multi-channel charts.

const API = '/api';
const USER_UNIT_STORAGE = 'scargo.displayUnits';
const THEME_STORAGE = 'scargo.theme';
const GUEST_CONSENT_STORAGE = 'scargo.guestConsent';
const FLASH_UPLOAD_TOKEN_STORAGE = 'scargo.flashUploadToken';
const INITIAL_CHART_LIMIT = 12;
const COLORS  = ['#9b6cff','#66df7c','#ffcb45','#4fc3ff','#ff625d','#d5dbe6','#7ee787','#f2cc60','#bc8cff','#39c5cf','#ffb86b','#8bd5ca','#c6a0f6','#ed8796','#79c0ff','#56d364','#e5534b','#db6d28','#d2a8ff','#ffa198'];
const DATE_FORMAT = new Intl.DateTimeFormat([], { year: 'numeric', month: '2-digit', day: '2-digit' });
const TIME_FORMAT = new Intl.DateTimeFormat([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
const TIMESTAMP_FORMAT = new Intl.DateTimeFormat([], {
  year: 'numeric', month: '2-digit', day: '2-digit',
  hour: '2-digit', minute: '2-digit', second: '2-digit',
});
let charts = {};
let renderController = null;
let renderSeq = 0;
let channelController = null;
let channelSeq = 0;
let availableChannels = [];
let visibleChartLimit = INITIAL_CHART_LIMIT;
let currentAccount = null;

function cssVar(name) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

function chartTheme() {
  return {
    tick: cssVar('--muted') || '#95a3b6',
    grid: cssVar('--line') || '#243449',
  };
}

function loadTheme() {
  const saved = localStorage.getItem(THEME_STORAGE);
  if (saved === 'light' || saved === 'dark') return saved;
  return 'dark';
}

function hasGuestConsent() {
  return sessionStorage.getItem(GUEST_CONSENT_STORAGE) === '1';
}

function setGuestConsent(enabled) {
  if (enabled) {
    sessionStorage.setItem(GUEST_CONSENT_STORAGE, '1');
  } else {
    sessionStorage.removeItem(GUEST_CONSENT_STORAGE);
  }
}

function stashFlashUploadToken(token) {
  if (token) {
    sessionStorage.setItem(FLASH_UPLOAD_TOKEN_STORAGE, token);
  } else {
    sessionStorage.removeItem(FLASH_UPLOAD_TOKEN_STORAGE);
  }
}

function consumeFlashUploadToken() {
  const token = sessionStorage.getItem(FLASH_UPLOAD_TOKEN_STORAGE) || '';
  sessionStorage.removeItem(FLASH_UPLOAD_TOKEN_STORAGE);
  return token;
}

function applyTheme(theme) {
  const active = theme === 'light' ? 'light' : 'dark';
  document.documentElement.dataset.theme = active;
  localStorage.setItem(THEME_STORAGE, active);
  const toggle = document.getElementById('theme-toggle');
  if (toggle) toggle.textContent = active === 'light' ? 'Dark mode' : 'Light mode';
}

function toggleTheme() {
  applyTheme(document.documentElement.dataset.theme === 'light' ? 'dark' : 'light');
  render();
}

function loadUnitPreferences() {
  try {
    const parsed = JSON.parse(localStorage.getItem(USER_UNIT_STORAGE) || '{}');
    return parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    return {};
  }
}

function saveUnitPreference(key, unit) {
  const prefs = loadUnitPreferences();
  if (unit) prefs[key] = unit;
  else delete prefs[key];
  localStorage.setItem(USER_UNIT_STORAGE, JSON.stringify(prefs));
}

function channelByKey(key) {
  return availableChannels.find(channel => channel.key === key) || null;
}

function selectedDisplayUnit(channel) {
  if (!channel) return null;
  const prefs = loadUnitPreferences();
  const options = Array.isArray(channel.display_units) ? channel.display_units : [];
  const saved = prefs[channel.key];
  if (saved && options.includes(saved)) return saved;
  if (channel.default_display_unit && options.includes(channel.default_display_unit)) {
    return channel.default_display_unit;
  }
  return options[0] || null;
}

function convertValueForDisplay(channel, value, displayUnit) {
  if (typeof value !== 'number' || !channel || !displayUnit) return value;
  switch (channel.unit_family) {
    case 'speed':
      return displayUnit === 'km/h' ? value * 1.609344 : value;
    case 'distance':
      return displayUnit === 'km' ? value * 1.609344 : value;
    case 'temperature':
      return displayUnit === 'F' ? (value * 9 / 5) + 32 : value;
    case 'pressure_kpa':
      if (displayUnit === 'psi') return value / 6.8947572932;
      if (displayUnit === 'inHg') return value / 3.38638815789;
      return value;
    case 'pressure_pa':
      return displayUnit === 'inH2O' ? value / 248.84 : value;
    case 'acceleration':
      return displayUnit === 'ft/s²' ? value / 0.3048 : value;
    case 'fuel_economy':
      return displayUnit === 'km/l' ? value / 2.352145833 : value;
    case 'fuel_rate':
      return displayUnit === 'l/hr' ? value / 0.2641720524 : value;
    case 'volume':
      return displayUnit === 'l' ? value / 0.2641720524 : value;
    case 'air_flow':
      return displayUnit === 'lb/min' ? value / 7.5598728333 : value;
    case 'co2_rate':
      return displayUnit === 'g/km' ? value / 0.0056680609 : value;
    case 'co2_total':
      return displayUnit === 'lbs' ? value / 0.45359237 : value;
    case 'co2_flow':
      return displayUnit === 'lb/min' ? value / 7.5598728333 : value;
    case 'power':
      return displayUnit === 'kW' ? value / 1.34102209 : value;
    case 'torque':
      return displayUnit === 'N-m' ? value / 0.737562149 : value;
    case 'length':
      return displayUnit === 'ft' ? value / 0.3048 : value;
    default:
      return value;
  }
}

function formatNumber(value) {
  if (typeof value !== 'number' || !Number.isFinite(value)) return '';
  if (Math.abs(value) >= 100 || Number.isInteger(value)) return value.toFixed(0);
  if (Math.abs(value) >= 10) return value.toFixed(1);
  return value.toFixed(2);
}

function metricTitle(channel, fallbackKey) {
  return channel?.label || fallbackKey;
}

function metricSummaryLabel(channel) {
  const unit = selectedDisplayUnit(channel);
  return unit ? `${metricTitle(channel, channel?.key)} (${unit})` : metricTitle(channel, channel?.key);
}

function buildCardHead(title, dateText, channel, key) {
  const unit = selectedDisplayUnit(channel);
  const selector = channel && Array.isArray(channel.display_units) && channel.display_units.length > 1
    ? `<select class="unit-select" data-unit-key="${key}">${channel.display_units
      .map(option => `<option value="${option}"${option === unit ? ' selected' : ''}>${option}</option>`)
      .join('')}</select>`
    : '';
  return `<div class="card-head"><div class="card-copy"><h3>${title}${unit && !selector ? ` (${unit})` : ''}</h3><div class="chart-date">${dateText}</div></div>${selector}</div>`;
}

function scopedHeaders(extra = {}) {
  return { ...extra };
}

// ── helpers ─────────────────────────────────────────────────
async function apiGet(path, options = {}) {
  const r = await fetch(API + path, {
    headers: scopedHeaders(),
    credentials: 'same-origin',
    signal: options.signal,
  });
  if (!r.ok) throw new Error(`${r.status}`);
  return r.json();
}

async function apiPostCsv(file, vin) {
  const r = await fetch(`${API}/ingest/csv?vin=${encodeURIComponent(vin)}`, {
    method: 'POST',
    headers: scopedHeaders({ 'Content-Type': file.type || 'text/csv' }),
    credentials: 'same-origin',
    body: file
  });
  if (!r.ok) {
    let message = `${r.status}`;
    try {
      const payload = await r.json();
      if (payload.error) message = payload.error;
    } catch { /* ignore */ }
    throw new Error(message);
  }
  return r.json();
}

async function apiPostJson(path, body = {}) {
  const r = await fetch(API + path, {
    method: 'POST',
    headers: scopedHeaders({ 'Content-Type': 'application/json' }),
    credentials: 'same-origin',
    body: JSON.stringify(body),
  });
  if (!r.ok) {
    let message = `${r.status}`;
    try {
      const payload = await r.json();
      if (payload.error) message = payload.error;
    } catch { /* ignore */ }
    throw new Error(message);
  }
  return r.json();
}

async function apiPutJson(path, body = {}) {
  const r = await fetch(API + path, {
    method: 'PUT',
    headers: scopedHeaders({ 'Content-Type': 'application/json' }),
    credentials: 'same-origin',
    body: JSON.stringify(body),
  });
  if (!r.ok) {
    let message = `${r.status}`;
    try {
      const payload = await r.json();
      if (payload.error) message = payload.error;
    } catch { /* ignore */ }
    throw new Error(message);
  }
  return r.json();
}

async function apiDelete(path) {
  const r = await fetch(API + path, {
    method: 'DELETE',
    headers: scopedHeaders(),
    credentials: 'same-origin',
  });
  if (!r.ok) {
    let message = `${r.status}`;
    try {
      const payload = await r.json();
      if (payload.error) message = payload.error;
    } catch { /* ignore */ }
    throw new Error(message);
  }
  return r.json();
}

async function checkHealth() {
  try { await apiGet('/health'); setStatus(true); }
  catch { setStatus(false); }
}
function setStatus(ok) {
  const dot = document.getElementById('status-dot');
  const txt = document.getElementById('status-text');
  dot.className = 'dot ' + (ok ? 'online' : 'offline');
  txt.textContent = ok ? 'Connected' : 'Offline';
}
function setUploadStatus(text, kind = '') {
  const status = document.getElementById('upload-status');
  status.textContent = text;
  status.className = kind;
}

function setTokenStatus(text) {
  const status = document.getElementById('token-status');
  if (status) status.textContent = text;
}

function updateChromeState() {
  const profileUser = document.getElementById('profile-user-key');
  if (profileUser) profileUser.textContent = accountLabel();

  const vehicleSelect = document.getElementById('vehicle-select');
  const vehicleTitle = document.getElementById('vehicle-title');
  const vehicleMeta = document.getElementById('vehicle-meta');
  const selected = vehicleSelect?.selectedOptions?.[0];
  if (vehicleTitle) {
    vehicleTitle.textContent = selected?.value ? selected.textContent.split(' - ')[0] : 'All vehicles';
  }
  if (vehicleMeta) {
    vehicleMeta.textContent = selected?.value ? 'Selected vehicle telemetry' : 'Account telemetry';
  }

  const channelCount = document.getElementById('health-channel-count');
  if (channelCount) channelCount.textContent = availableChannels.length.toLocaleString();

  const mode = document.getElementById('health-view-mode');
  if (mode) mode.textContent = 'Summary';
  updateLoadMoreButton();
}

function totalVisibleMetricTarget() {
  return selectedMetricKeys().length || availableChannels.length;
}

function updateLoadMoreButton() {
  const button = document.getElementById('load-more-btn');
  if (!button) return;
  const hasMore = totalVisibleMetricTarget() > visibleChartLimit;
  button.style.display = hasMore ? '' : 'none';
  button.disabled = !hasMore;
}

function resetVisibleChartLimit() {
  visibleChartLimit = INITIAL_CHART_LIMIT;
  updateLoadMoreButton();
}

function renderStats(payload) {
  const stats = document.getElementById('stats-bar');
  if (!stats) return;

  const series = Array.isArray(payload?.series) ? payload.series : [];
  const pointCount = series.reduce((sum, channel) => {
    return sum + (Array.isArray(channel.points) ? channel.points.length : 0);
  }, 0);
  const numericCount = series.filter(channel => {
    return Array.isArray(channel.points) && channel.points.some(point => {
      return typeof point.value === 'number' || typeof point.avg === 'number';
    });
  }).length;
  const selectedMetrics = selectedMetricKeys().length || availableChannels.length;
  const cards = [
    ['Visible metrics', selectedMetrics.toLocaleString()],
    ['Loaded series', series.length.toLocaleString()],
    ['Data points', pointCount.toLocaleString()],
    ['Numeric charts', numericCount.toLocaleString()],
  ];

  stats.innerHTML = cards.map(([label, value]) => {
    return `<div class="stat-card"><div class="stat-label">${label}</div><div class="stat-value">${value}</div></div>`;
  }).join('');
  updateChromeState();
}

function accountLabel() {
  if (!currentAccount) return 'Signed out';
  if (currentAccount.is_guest) return 'Guest';
  return currentAccount.display_name || currentAccount.username || 'Guest';
}

function isSignedInAccount(account = currentAccount) {
  return Boolean(account && !account.is_guest);
}

function redirectToAuthPage() {
  window.location.replace('/auth.html');
}

function setAccount(account) {
  currentAccount = account || null;
  const status = document.getElementById('auth-status');
  if (status) {
    status.textContent = isSignedInAccount()
      ? `Signed in as ${accountLabel()}`
      : currentAccount?.is_guest
        ? 'Guest mode active'
        : 'Account required';
  }
  const signInButton = document.getElementById('sign-in-btn');
  if (signInButton) signInButton.style.display = isSignedInAccount() ? 'none' : '';
  const logoutButton = document.getElementById('logout-btn');
  if (logoutButton) logoutButton.style.display = isSignedInAccount() ? '' : 'none';
  const tokenControls = document.getElementById('token-controls');
  if (tokenControls) tokenControls.style.display = isSignedInAccount() ? '' : 'none';
  const sharedPanel = document.getElementById('shared-link-panel');
  if (sharedPanel) sharedPanel.style.display = isSignedInAccount() ? '' : 'none';
  updateChromeState();
}

async function loadAccount() {
  try {
    const payload = await apiGet('/auth/me');
    setAccount(payload.account);
    return payload.account;
  } catch {
    setAccount(null);
    return null;
  }
}

function dashboardAccessAllowed(account = currentAccount) {
  if (!account) return false;
  if (account.is_guest) return hasGuestConsent();
  return true;
}

function enforceDashboardAccess(account = currentAccount) {
  if (dashboardAccessAllowed(account)) return true;
  redirectToAuthPage();
  return false;
}

async function logout() {
  try {
    await apiPostJson('/auth/logout');
  } finally {
    setGuestConsent(false);
    stashFlashUploadToken('');
    showUploadToken('');
    setTokenStatus('');
    redirectToAuthPage();
  }
}

async function generateUploadToken() {
  try {
    const payload = await apiPostJson('/auth/tokens', { label: 'dashboard' });
    showUploadToken(payload.upload_token || '');
    setTokenStatus('Token created');
  } catch (err) {
    setTokenStatus(`Token failed: ${err.message}`);
  }
}

function showUploadToken(token) {
  const output = document.getElementById('upload-token-output');
  if (output) output.value = token || '';
}

function copyUploadToken() {
  const output = document.getElementById('upload-token-output');
  if (!output?.value) return;
  output.select();
  navigator.clipboard?.writeText(output.value);
}

async function reloadAccountData() {
  resetVisibleChartLimit();
  await loadVehicles();
  await loadChannels();
  await render();
}

function setSharedLinkStatus(text, kind = '') {
  const status = document.getElementById('shared-link-status');
  if (!status) return;
  status.textContent = text;
  status.className = kind;
}

function renderSharedLinkStatus(payload) {
  const input = document.getElementById('shared-link-input');
  const pause = document.getElementById('shared-link-pause-btn');
  const del = document.getElementById('shared-link-delete-btn');
  const sync = document.getElementById('shared-link-sync-btn');
  const configured = Boolean(payload?.configured);
  if (input) input.placeholder = configured ? payload.link_label || 'Saved Dropbox link' : 'https://www.dropbox.com/sh/...';
  if (pause) {
    pause.disabled = !configured;
    pause.textContent = payload?.active ? 'Pause' : 'Resume';
  }
  if (del) del.disabled = !configured;
  if (sync) sync.disabled = !configured || !payload?.active;
  if (!configured) {
    setSharedLinkStatus('No shared link saved');
    return;
  }
  const counts = `${payload.ingested_count || 0} ingested, ${payload.duplicate_count || 0} duplicate, ${payload.skipped_count || 0} skipped`;
  const err = payload.latest_error ? ` - ${payload.latest_error}` : '';
  setSharedLinkStatus(`${payload.active ? 'Active' : 'Paused'} - ${counts}${err}`, payload.latest_error ? 'err' : 'ok');
}

async function loadSharedLinkStatus() {
  if (!isSignedInAccount()) return;
  try {
    renderSharedLinkStatus(await apiGet('/ingest-sources/shared-link'));
  } catch (err) {
    setSharedLinkStatus(`Shared link unavailable: ${err.message}`, 'err');
  }
}

async function saveSharedLink() {
  const input = document.getElementById('shared-link-input');
  const url = input.value.trim();
  if (!url) {
    setSharedLinkStatus('Dropbox shared folder URL required', 'err');
    return;
  }
  setSharedLinkStatus('Saving...');
  try {
    const payload = await apiPutJson('/ingest-sources/shared-link', { url });
    input.value = '';
    renderSharedLinkStatus(payload);
  } catch (err) {
    setSharedLinkStatus(`Save failed: ${err.message}`, 'err');
  }
}

async function toggleSharedLinkPause() {
  setSharedLinkStatus('Updating...');
  try {
    renderSharedLinkStatus(await apiPostJson('/ingest-sources/shared-link/pause'));
  } catch (err) {
    setSharedLinkStatus(`Update failed: ${err.message}`, 'err');
  }
}

async function deleteSharedLink() {
  setSharedLinkStatus('Deleting...');
  try {
    renderSharedLinkStatus(await apiDelete('/ingest-sources/shared-link'));
  } catch (err) {
    setSharedLinkStatus(`Delete failed: ${err.message}`, 'err');
  }
}

async function syncSharedLink() {
  setSharedLinkStatus('Syncing...');
  try {
    renderSharedLinkStatus(await apiPostJson('/ingest-sources/shared-link/sync-now'));
    await reloadAccountData();
  } catch (err) {
    await loadSharedLinkStatus();
    setSharedLinkStatus(`Sync failed: ${err.message}`, 'err');
  }
}

function formatTimestamp(value) {
  return TIMESTAMP_FORMAT.format(new Date(value));
}
function formatDate(value) {
  return DATE_FORMAT.format(new Date(value));
}
function formatTime(value) {
  return TIME_FORMAT.format(new Date(value));
}
function formatAxisTimestamp(value) {
  return [formatDate(value), formatTime(value)];
}
function formatDateRange(points, field) {
  const first = points[0]?.[field];
  const last = points[points.length - 1]?.[field];
  if (!first || !last) return '';

  const firstDate = formatDate(first);
  const lastDate = formatDate(last);
  if (firstDate === lastDate) return firstDate;

  return `${firstDate} - ${lastDate}`;
}
function toLocalDateTimeValue(date) {
  const pad = (n) => String(n).padStart(2, '0');
  return [
    date.getFullYear(),
    '-',
    pad(date.getMonth() + 1),
    '-',
    pad(date.getDate()),
    'T',
    pad(date.getHours()),
    ':',
    pad(date.getMinutes()),
    ':',
    pad(date.getSeconds()),
  ].join('');
}
function parseLocalDateTimeValue(value) {
  if (!value) return null;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date;
}
function syncTimeWindowControls() {
  const mode = document.getElementById('range-select').value;
  const start = document.getElementById('range-start');
  const end = document.getElementById('range-end');
  const custom = mode === 'custom';

  start.style.display = custom ? '' : 'none';
  end.style.display = custom ? '' : 'none';

  if (!custom) return;

  const now = new Date();
  if (!end.value) end.value = toLocalDateTimeValue(now);
  if (!start.value) start.value = toLocalDateTimeValue(new Date(now.getTime() - 7 * 24 * 60 * 60 * 1000));
}
function getTimeWindowQuery() {
  const mode = document.getElementById('range-select').value;
  const now = new Date();
  const presets = {
    '24h': 24 * 60 * 60 * 1000,
    '7d': 7 * 24 * 60 * 60 * 1000,
    '30d': 30 * 24 * 60 * 60 * 1000,
    '90d': 90 * 24 * 60 * 60 * 1000,
  };

  if (mode === 'latest') return {};
  if (mode === 'custom') {
    const start = parseLocalDateTimeValue(document.getElementById('range-start').value);
    const end = parseLocalDateTimeValue(document.getElementById('range-end').value);
    if (!start || !end) return {};
    return start <= end
      ? { start: start.toISOString(), end: end.toISOString() }
      : { start: end.toISOString(), end: start.toISOString() };
  }

  const duration = presets[mode];
  if (!duration) return {};
  return {
    start: new Date(now.getTime() - duration).toISOString(),
    end: now.toISOString(),
  };
}

function dashboardContextParams() {
  const params = new URLSearchParams();
  const vid = document.getElementById('vehicle-select').value;
  const timeWindow = getTimeWindowQuery();

  if (vid) params.set('vehicle_id', vid);
  if (timeWindow.start) params.set('start', timeWindow.start);
  if (timeWindow.end) params.set('end', timeWindow.end);

  return params;
}

function selectedMetricKeys() {
  return Array.from(document.querySelectorAll('#metric-options input:checked'))
    .map(input => input.value)
    .filter(Boolean);
}

function updateMetricFilterLabel() {
  const label = document.getElementById('metric-filter-label');
  const clear = document.getElementById('metric-clear-btn');
  const selected = selectedMetricKeys();
  const total = availableChannels.length;

  if (!total) label.textContent = 'No metrics';
  else if (!selected.length) label.textContent = `All metrics (${total})`;
  else label.textContent = `${selected.length} of ${total} metrics`;

  clear.disabled = selected.length === 0;
}

function renderMetricOptions(previousSelected = new Set()) {
  const options = document.getElementById('metric-options');
  options.innerHTML = '';

  availableChannels.forEach(channel => {
    const row = document.createElement('label');
    row.className = 'metric-option';

    const checkbox = document.createElement('input');
    checkbox.type = 'checkbox';
    checkbox.value = channel.key;
    checkbox.checked = previousSelected.has(channel.key);

    const text = document.createElement('span');
    const label = channel.label || channel.key;
    const displayUnit = selectedDisplayUnit(channel);
    const unit = displayUnit ? ` (${displayUnit})` : '';
    const kind = channel.has_numeric && channel.has_text
      ? 'mixed'
      : channel.has_numeric ? 'numeric' : 'text';
    const count = typeof channel.reading_count === 'number'
      ? channel.reading_count.toLocaleString()
      : '0';
    text.textContent = `${label}${unit}`;

    const meta = document.createElement('small');
    meta.textContent = `${kind} - ${count}`;

    row.appendChild(checkbox);
    row.appendChild(text);
    row.appendChild(meta);
    options.appendChild(row);
  });

  updateMetricFilterLabel();
}

function renderPairOptions() {
  const numeric = availableChannels.filter(channel => channel.has_numeric);
  ['pair-x-select', 'pair-y-select'].forEach((id, index) => {
    const select = document.getElementById(id);
    const previous = select.value;
    select.innerHTML = `<option value="">${index === 0 ? 'X metric' : 'Y metric'}</option>`;
    numeric.forEach(channel => {
      const option = document.createElement('option');
      option.value = channel.key;
      option.textContent = metricSummaryLabel(channel);
      select.appendChild(option);
    });
    if (numeric.some(channel => channel.key === previous)) select.value = previous;
  });
}

async function loadChannels() {
  const seq = ++channelSeq;
  if (channelController) channelController.abort();
  const controller = new AbortController();
  channelController = controller;
  const previousSelected = new Set(selectedMetricKeys());

  try {
    const params = dashboardContextParams();
    const query = params.toString();
    const channels = await apiGet(query ? `/channels?${query}` : '/channels', {
      signal: controller.signal,
    });
    if (seq !== channelSeq) return;
    availableChannels = Array.isArray(channels) ? channels : [];
    renderMetricOptions(previousSelected);
    renderPairOptions();
    updateChromeState();
  } catch (err) {
    if (err.name === 'AbortError') return;
    if (seq === channelSeq) {
      availableChannels = [];
      renderMetricOptions();
      renderPairOptions();
      updateChromeState();
    }
    console.error('Channel list failed', err);
  } finally {
    if (seq === channelSeq && channelController === controller) {
      channelController = null;
    }
  }
}

function makeChart(canvasId, label, color, unit = '', datasets = null, legend = false) {
  const ctx = document.getElementById(canvasId).getContext('2d');
  const theme = chartTheme();
  return new Chart(ctx, {
    type: 'line',
    data: {
      labels: [],
      datasets: datasets || [
        { label, data: [], borderColor: color, tension: 0.15, pointRadius: 0, borderWidth: 1.5 },
      ],
    },
    options: {
      responsive: true, maintainAspectRatio: false, animation: false,
      scales: {
        x: { ticks: { maxTicksLimit: 5, color: theme.tick, font: { size: 10 } }, grid: { color: theme.grid } },
        y: { ticks: { color: theme.tick, font: { size: 10 } }, grid: { color: theme.grid } }
      },
      plugins: {
        legend: { display: legend, labels: { color: theme.tick, font: { size: 10 }, usePointStyle: true, boxWidth: 6 } },
        tooltip: {
          callbacks: {
            label(context) {
              const value = context.parsed.y;
              if (typeof value !== 'number') return '';
              const prefix = legend ? `${context.dataset.label}: ` : '';
              return `${prefix}${formatNumber(value)}${unit ? ` ${unit}` : ''}`;
            }
          }
        }
      },
      interaction: { intersect: false, mode: 'index' }
    }
  });
}

function makeScatterChart(canvasId, xLabel, yLabel) {
  const ctx = document.getElementById(canvasId).getContext('2d');
  const theme = chartTheme();
  return new Chart(ctx, {
    type: 'scatter',
    data: {
      datasets: [{
        label: `${yLabel} vs ${xLabel}`,
        data: [],
        backgroundColor: COLORS[0],
        pointRadius: 2,
      }],
    },
    options: {
      responsive: true, maintainAspectRatio: false, animation: false,
      scales: {
        x: { title: { display: true, text: xLabel, color: theme.tick }, ticks: { color: theme.tick, font: { size: 10 } }, grid: { color: theme.grid } },
        y: { title: { display: true, text: yLabel, color: theme.tick }, ticks: { color: theme.tick, font: { size: 10 } }, grid: { color: theme.grid } }
      },
      plugins: {
        legend: { display: false },
        tooltip: {
          callbacks: {
            label(context) {
              return `${formatNumber(context.parsed.x)}, ${formatNumber(context.parsed.y)}`;
            }
          }
        }
      }
    }
  });
}

// ── upload ───────────────────────────────────────────────────
async function handleUpload(event) {
  event.preventDefault();

  const vinInput = document.getElementById('vin-input');
  const fileInput = document.getElementById('csv-input');
  const uploadBtn = document.getElementById('upload-btn');
  const vin = vinInput.value.trim();
  const file = fileInput.files[0];

  if (!vin || !file) {
    setUploadStatus('VIN and CSV required', 'err');
    return;
  }

  uploadBtn.disabled = true;
  setUploadStatus('Uploading...');
  try {
    const result = await apiPostCsv(file, vin);
    const rows = typeof result.rows_ingested === 'number' ? result.rows_ingested : 0;
    setUploadStatus(`Uploaded ${rows} readings`, 'ok');
    fileInput.value = '';
    await loadVehicles();
    await loadChannels();
    await render();
  } catch (err) {
    setUploadStatus(`Upload failed: ${err.message}`, 'err');
  } finally {
    uploadBtn.disabled = false;
  }
}

// ── vehicles ─────────────────────────────────────────────────
async function loadVehicles() {
  const sel = document.getElementById('vehicle-select');
  try {
    const vehicles = await apiGet('/vehicles?limit=200');
    sel.innerHTML = '<option value="">All vehicles</option>';
    vehicles.forEach(v => {
      const opt = document.createElement('option');
      opt.value = v.id;
      const vehicleName = [v.year || '', v.make || '', v.model || '', v.engine_family || '']
        .filter(Boolean)
        .join(' ')
        || v.id.slice(0, 8);
      opt.textContent = `${vehicleName} — ${typeof v.reading_count === 'number' ? v.reading_count : 0} readings`;
      sel.appendChild(opt);
    });
  } catch { /* endpoint may have no data */ }
  updateChromeState();
}

// ── raw trends ───────────────────────────────────────────────
function destroyCharts() {
  Object.values(charts).forEach(c => c.destroy());
  charts = {};
}

async function loadDashboard(view, signal) {
  const limit = document.getElementById('limit-select').value;
  const params = dashboardContextParams();
  const channels = selectedMetricKeys();

  params.set('view', view);
  params.set('limit', limit);
  params.set('channel_limit', String(visibleChartLimit));

  if (view === 'summary') {
    params.set('bucket', document.getElementById('bucket-select').value);
  }
  if (channels.length) params.set('channels', channels.join(','));
  return apiGet(`/analysis/dashboard?${params.toString()}`, { signal });
}

async function renderRaw(signal, seq) {
  const container = document.getElementById('charts');
  destroyCharts();
  container.innerHTML = '';

  const payload = await loadDashboard('raw', signal);
  if (seq !== renderSeq) return;
  renderStats(payload);

  let anyData = false;
  const series = Array.isArray(payload.series) ? payload.series : [];
  series.forEach((ch, i) => {
    const points = Array.isArray(ch.points) ? ch.points : [];
    if (!points.length) return;
    anyData = true;
    const channel = channelByKey(ch.key);
    const displayUnit = selectedDisplayUnit(channel);
    const id = `chart-raw-${ch.key}`;
    const card = document.createElement('div');
    card.className = 'card';
    const reversed = [...points].reverse();
    const numeric = reversed.filter(d => typeof d.value === 'number');
    if (!numeric.length) {
      const recent = reversed.slice(-8).reverse();
      card.innerHTML = `${buildCardHead(metricTitle(channel, ch.key), formatDateRange(reversed, 'time'), channel, ch.key)}<div class="metric-list"></div>`;
      const list = card.querySelector('.metric-list');
      recent.forEach(d => {
        const item = document.createElement('div');
        item.className = 'metric-row';
        item.textContent = `${formatTimestamp(d.time)}  ${d.text_value || ''}`;
        list.appendChild(item);
      });
      container.appendChild(card);
      return;
    }
    card.innerHTML = `${buildCardHead(metricTitle(channel, ch.key), formatDateRange(reversed, 'time'), channel, ch.key)}<div class="chart-wrap"><canvas id="${id}"></canvas></div>`;
    container.appendChild(card);
    charts[id] = makeChart(id, metricTitle(channel, ch.key), COLORS[i % COLORS.length], displayUnit || '');
    charts[id].data.labels = numeric.map(d => formatAxisTimestamp(d.time));
    charts[id].data.datasets[0].data = numeric.map(d => convertValueForDisplay(channel, d.value, displayUnit));
    charts[id].update();
  });

  document.getElementById('empty-state').style.display = anyData ? 'none' : 'block';
  updateLoadMoreButton();
}

async function renderPairScatter(container, signal) {
  const xKey = document.getElementById('pair-x-select').value;
  const yKey = document.getElementById('pair-y-select').value;
  if (!xKey || !yKey || xKey === yKey) return false;

  const params = dashboardContextParams();
  params.set('x', xKey);
  params.set('y', yKey);
  params.set('limit', document.getElementById('limit-select').value);

  const points = await apiGet(`/analysis/pairs?${params.toString()}`, { signal });
  if (!Array.isArray(points) || !points.length) return false;

  const xChannel = channelByKey(xKey);
  const yChannel = channelByKey(yKey);
  const xLabel = metricSummaryLabel(xChannel);
  const yLabel = metricSummaryLabel(yChannel);
  const id = `chart-pair-${xKey}-${yKey}`;
  const card = document.createElement('div');
  card.className = 'card';
  card.innerHTML = `${buildCardHead(`${yLabel} vs ${xLabel}`, `${points.length.toLocaleString()} paired points`, null, '')}<div class="chart-wrap"><canvas id="${id}"></canvas></div>`;
  container.appendChild(card);

  charts[id] = makeScatterChart(id, xLabel, yLabel);
  charts[id].data.datasets[0].data = points.map(point => ({ x: point.x, y: point.y }));
  charts[id].update();
  return true;
}

// ── summary ──────────────────────────────────────────────────
async function renderSummary(signal, seq) {
  const bucket = document.getElementById('bucket-select').value;
  const container = document.getElementById('charts');
  destroyCharts();
  container.innerHTML = '';

  const payload = await loadDashboard('summary', signal);
  if (seq !== renderSeq) return;
  renderStats(payload);

  let anyData = false;
  anyData = await renderPairScatter(container, signal) || anyData;
  const series = Array.isArray(payload.series) ? payload.series : [];
  series.forEach((ch, i) => {
    const points = Array.isArray(ch.points) ? ch.points : [];
    if (!points.length) return;
    anyData = true;
    const channel = channelByKey(ch.key);
    const displayUnit = selectedDisplayUnit(channel);
    const id = `chart-summary-${ch.key}`;
    const card = document.createElement('div');
    card.className = 'card';
    const reversed = [...points].reverse();
    card.innerHTML = `${buildCardHead(`${metricTitle(channel, ch.key)} (${bucket})`, formatDateRange(reversed, 'bucket'), channel, ch.key)}<div class="chart-wrap"><canvas id="${id}"></canvas></div>`;
    container.appendChild(card);

    charts[id] = makeChart(id, metricTitle(channel, ch.key), COLORS[i % COLORS.length], displayUnit || '', [
        { label: 'avg', data: [], borderColor: COLORS[i % COLORS.length], tension: 0.15, pointRadius: 0, borderWidth: 1.5 },
        { label: 'min', data: [], borderColor: COLORS[i % COLORS.length]+'44', tension: 0.15, pointRadius: 0, borderWidth: 0.8, borderDash: [4,4] },
        { label: 'max', data: [], borderColor: COLORS[i % COLORS.length]+'44', tension: 0.15, pointRadius: 0, borderWidth: 0.8, borderDash: [4,4] },
      ], true);

    charts[id].data.labels = reversed.map(d => formatAxisTimestamp(d.bucket));
    charts[id].data.datasets[0].data = reversed.map(d => convertValueForDisplay(channel, d.avg, displayUnit));
    charts[id].data.datasets[1].data = reversed.map(d => convertValueForDisplay(channel, d.min, displayUnit));
    charts[id].data.datasets[2].data = reversed.map(d => convertValueForDisplay(channel, d.max, displayUnit));
    charts[id].update();
  });

  document.getElementById('empty-state').style.display = anyData ? 'none' : 'block';
  updateLoadMoreButton();
}

// ── render dispatcher ────────────────────────────────────────
async function render() {
  const seq = ++renderSeq;
  if (renderController) renderController.abort();
  const controller = new AbortController();
  renderController = controller;
  document.getElementById('empty-state').style.display = 'none';
  try {
    await renderSummary(controller.signal, seq);
  } catch (err) {
    if (err.name === 'AbortError') return;
    if (seq === renderSeq) {
      destroyCharts();
      document.getElementById('charts').innerHTML = '';
      document.getElementById('empty-state').style.display = 'block';
    }
    console.error('Dashboard render failed', err);
  } finally {
    if (seq === renderSeq && renderController === controller) {
      renderController = null;
    }
  }
}

// ── tab switching ────────────────────────────────────────────

// ── event wiring ─────────────────────────────────────────────
document.getElementById('logout-btn').addEventListener('click', logout);
document.getElementById('sign-in-btn').addEventListener('click', redirectToAuthPage);
document.getElementById('token-btn').addEventListener('click', generateUploadToken);
document.getElementById('copy-token-btn').addEventListener('click', copyUploadToken);
document.getElementById('upload-form').addEventListener('submit', handleUpload);
document.getElementById('shared-link-save-btn').addEventListener('click', saveSharedLink);
document.getElementById('shared-link-pause-btn').addEventListener('click', toggleSharedLinkPause);
document.getElementById('shared-link-delete-btn').addEventListener('click', deleteSharedLink);
document.getElementById('shared-link-sync-btn').addEventListener('click', syncSharedLink);
document.getElementById('refresh-btn').addEventListener('click', async () => {
  await loadChannels();
  await render();
});
document.getElementById('refresh-top-btn').addEventListener('click', async () => {
  await loadChannels();
  await render();
});
document.getElementById('theme-toggle').addEventListener('click', toggleTheme);
document.getElementById('vehicle-select').addEventListener('change', async () => {
  resetVisibleChartLimit();
  updateChromeState();
  await loadChannels();
  await render();
});
document.getElementById('range-select').addEventListener('change', async () => {
  resetVisibleChartLimit();
  syncTimeWindowControls();
  await loadChannels();
  await render();
});
document.getElementById('range-start').addEventListener('change', async () => {
  resetVisibleChartLimit();
  await loadChannels();
  await render();
});
document.getElementById('range-end').addEventListener('change', async () => {
  resetVisibleChartLimit();
  await loadChannels();
  await render();
});
document.getElementById('limit-select').addEventListener('change', () => {
  resetVisibleChartLimit();
  render();
});
document.getElementById('bucket-select').addEventListener('change', () => {
  resetVisibleChartLimit();
  render();
});
document.getElementById('pair-x-select').addEventListener('change', render);
document.getElementById('pair-y-select').addEventListener('change', render);
document.getElementById('metric-options').addEventListener('change', () => {
  resetVisibleChartLimit();
  updateMetricFilterLabel();
  render();
});
document.getElementById('charts').addEventListener('change', (event) => {
  const target = event.target;
  if (!(target instanceof HTMLSelectElement) || !target.dataset.unitKey) return;
  saveUnitPreference(target.dataset.unitKey, target.value);
  render();
});
document.getElementById('metric-clear-btn').addEventListener('click', () => {
  document.querySelectorAll('#metric-options input:checked').forEach(input => {
    input.checked = false;
  });
  resetVisibleChartLimit();
  updateMetricFilterLabel();
  render();
});
document.getElementById('load-more-btn').addEventListener('click', () => {
  visibleChartLimit += INITIAL_CHART_LIMIT;
  updateLoadMoreButton();
  render();
});

// ── init ─────────────────────────────────────────────────────
(async function init() {
  applyTheme(loadTheme());
  resetVisibleChartLimit();
  const account = await loadAccount();
  if (!enforceDashboardAccess(account)) return;
  const flashedToken = isSignedInAccount(account) ? consumeFlashUploadToken() : '';
  if (flashedToken) {
    showUploadToken(flashedToken);
    setTokenStatus('Account created. Save this token now.');
  }
  updateChromeState();
  await loadSharedLinkStatus();
  checkHealth();
  syncTimeWindowControls();
  await loadVehicles();
  await loadChannels();
  await render();
  setInterval(checkHealth, 30000);
})();
