const API = '/api';
const THEME_STORAGE = 'scargo.theme';
const DEFAULT_ROOT = '/OBD Fusion/CsvLogs';

let currentAccount = null;
let currentConnection = null;
let syncPollTimer = null;

function loadTheme() {
  const saved = localStorage.getItem(THEME_STORAGE);
  return saved === 'light' || saved === 'dark' ? saved : 'dark';
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
}

function setStatus(id, text, kind = '') {
  const node = document.getElementById(id);
  if (!node) return;
  node.textContent = text;
  node.className = kind;
}

function accountLabel(account = currentAccount) {
  if (!account) return 'Signed out';
  if (account.is_guest) return 'Guest';
  return account.display_name || account.username || 'Signed in';
}

function redirectToAuth() {
  window.location.replace('/auth.html');
}

async function apiError(res) {
  let message = `${res.status}`;
  try {
    const payload = await res.json();
    if (payload.err) message = payload.err;
    if (payload.error) message = payload.error;
  } catch {
    // ignore malformed payloads
  }
  return new Error(message);
}

async function apiGet(path) {
  const res = await fetch(API + path, { credentials: 'same-origin' });
  if (!res.ok) throw await apiError(res);
  return res.json();
}

async function apiSend(path, method, body) {
  const res = await fetch(API + path, {
    method,
    credentials: 'same-origin',
    headers: body ? { 'Content-Type': 'application/json' } : {},
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) throw await apiError(res);
  return res.status === 204 ? {} : res.json();
}

async function loadAccount() {
  const payload = await apiGet('/auth/me');
  currentAccount = payload.account ? {
    ...payload.account,
    capabilities: payload.capabilities || {},
  } : null;
  document.getElementById('profile-user-key').textContent = accountLabel();
  if (!currentAccount || currentAccount.is_guest) {
    redirectToAuth();
    return null;
  }
  setStatus('account-status', `Signed in as ${accountLabel()}`, 'ok');
  return currentAccount;
}

async function logout() {
  try {
    await apiSend('/auth/logout', 'POST');
  } finally {
    redirectToAuth();
  }
}

function formatTimestamp(value) {
  if (!value) return 'Never';
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? 'Never' : parsed.toLocaleString();
}

function fileCounts(payload) {
  const ingested = payload?.ingested_count || 0;
  const duplicate = payload?.duplicate_count || 0;
  return `${ingested} / ${duplicate}`;
}

function stopSyncPolling() {
  if (syncPollTimer) {
    clearInterval(syncPollTimer);
    syncPollTimer = null;
  }
}

function ensureSyncPolling() {
  const needsPolling = currentConnection?.connected && currentConnection?.sync_state !== 'idle';
  if (!needsPolling) {
    stopSyncPolling();
    return;
  }
  if (syncPollTimer) return;
  syncPollTimer = setInterval(() => {
    loadDropboxConnection().catch(() => {
      stopSyncPolling();
    });
  }, 5000);
}

function renderConnection(payload) {
  currentConnection = payload || null;
  const enabled = Boolean(payload?.enabled);
  const connected = Boolean(payload?.connected);
  const paused = payload?.status === 'paused';
  const syncState = payload?.sync_state || 'idle';

  const rootInput = document.getElementById('dropbox-root-input');
  if (rootInput) {
    rootInput.placeholder = DEFAULT_ROOT;
    if (!rootInput.dataset.dirty) {
      rootInput.value = payload?.root_path || DEFAULT_ROOT;
    }
  }

  document.getElementById('saved-root-label').textContent = payload?.root_path || DEFAULT_ROOT;
  document.getElementById('last-sync-at').textContent = formatTimestamp(payload?.last_sync_at);
  document.getElementById('last-success-at').textContent = formatTimestamp(payload?.last_success_at);
  document.getElementById('file-counts').textContent = fileCounts(payload);
  document.getElementById('sync-state').textContent = syncState;

  const connect = document.getElementById('dropbox-connect-btn');
  const save = document.getElementById('dropbox-save-btn');
  const pause = document.getElementById('dropbox-pause-btn');
  const sync = document.getElementById('dropbox-sync-btn');
  const del = document.getElementById('dropbox-delete-btn');

  connect.disabled = !enabled;
  connect.hidden = connected;
  save.disabled = !connected;
  save.hidden = !connected;
  pause.disabled = !connected;
  pause.hidden = !connected;
  pause.textContent = paused ? 'Resume' : 'Pause';
  sync.disabled = !connected || paused;
  sync.hidden = !connected;
  del.disabled = !connected;
  del.hidden = !connected;

  if (!enabled) {
    setStatus('dropbox-status', 'Dropbox OAuth is disabled on this server', 'err');
  } else if (!connected) {
    setStatus('dropbox-status', 'Dropbox not connected', '');
  } else {
    const latestError = payload?.latest_error ? ` - ${payload.latest_error}` : '';
    const state = paused ? 'Paused' : `Active - ${syncState}`;
    setStatus('dropbox-status', `${state}${latestError}`, payload?.latest_error ? 'err' : 'ok');
  }

  ensureSyncPolling();
}

async function loadDropboxConnection() {
  try {
    renderConnection(await apiGet('/dropbox/connection'));
    setStatus('page-status', '');
  } catch (err) {
    stopSyncPolling();
    setStatus('dropbox-status', `Dropbox unavailable: ${err.message}`, 'err');
  }
}

function currentRootPath() {
  const value = document.getElementById('dropbox-root-input').value.trim();
  return value || DEFAULT_ROOT;
}

async function startDropboxOAuth() {
  setStatus('page-status', 'Redirecting to Dropbox...');
  try {
    const payload = await apiSend('/dropbox/oauth/start', 'POST', {
      redirect_path: '/dropbox.html',
      root_path: currentRootPath(),
    });
    if (payload.authorize_url) {
      window.location.assign(payload.authorize_url);
      return;
    }
    throw new Error('missing Dropbox authorize URL');
  } catch (err) {
    setStatus('page-status', `Connect failed: ${err.message}`, 'err');
  }
}

async function saveDropboxFolder() {
  setStatus('page-status', 'Saving folder...');
  try {
    renderConnection(await apiSend('/dropbox/connection/folder', 'POST', {
      root_path: currentRootPath(),
    }));
    document.getElementById('dropbox-root-input').dataset.dirty = '';
    setStatus('page-status', 'Dropbox folder saved.', 'ok');
  } catch (err) {
    setStatus('page-status', `Folder save failed: ${err.message}`, 'err');
  }
}

async function toggleDropboxPause() {
  setStatus('page-status', 'Updating...');
  try {
    const paused = currentConnection?.status !== 'paused';
    renderConnection(await apiSend('/dropbox/connection/pause', 'POST', { paused }));
    setStatus('page-status', 'Dropbox state updated.', 'ok');
  } catch (err) {
    setStatus('page-status', `Update failed: ${err.message}`, 'err');
  }
}

async function syncDropboxNow() {
  setStatus('page-status', 'Queued Dropbox sync...');
  try {
    renderConnection(await apiSend('/dropbox/connection/sync-now', 'POST'));
    ensureSyncPolling();
    setStatus('page-status', 'Dropbox sync queued.', 'ok');
  } catch (err) {
    await loadDropboxConnection();
    setStatus('page-status', `Sync failed: ${err.message}`, 'err');
  }
}

async function disconnectDropbox() {
  const confirmed = window.confirm('Disconnect Dropbox for this account? Ingested telemetry stays in Scargo.');
  if (!confirmed) return;
  setStatus('page-status', 'Disconnecting...');
  try {
    await apiSend('/dropbox/connection', 'DELETE');
    renderConnection({
      enabled: true,
      connected: false,
      root_path: DEFAULT_ROOT,
      sync_state: 'idle',
      ingested_count: 0,
      duplicate_count: 0,
    });
    setStatus('page-status', 'Dropbox disconnected.', 'ok');
  } catch (err) {
    setStatus('page-status', `Disconnect failed: ${err.message}`, 'err');
  }
}

function wireFlashMessage() {
  const params = new URLSearchParams(window.location.search);
  if (params.get('dropbox') === 'connected') {
    setStatus('page-status', 'Dropbox connected.', 'ok');
    params.delete('dropbox');
    const next = `${window.location.pathname}${params.toString() ? `?${params.toString()}` : ''}`;
    window.history.replaceState({}, '', next);
  }
}

document.getElementById('theme-toggle').addEventListener('click', toggleTheme);
document.getElementById('refresh-btn').addEventListener('click', loadDropboxConnection);
document.getElementById('logout-btn').addEventListener('click', logout);
document.getElementById('dropbox-connect-btn').addEventListener('click', startDropboxOAuth);
document.getElementById('dropbox-save-btn').addEventListener('click', saveDropboxFolder);
document.getElementById('dropbox-pause-btn').addEventListener('click', toggleDropboxPause);
document.getElementById('dropbox-sync-btn').addEventListener('click', syncDropboxNow);
document.getElementById('dropbox-delete-btn').addEventListener('click', disconnectDropbox);
document.getElementById('dropbox-root-input').addEventListener('input', (event) => {
  event.target.dataset.dirty = '1';
});

(async function init() {
  applyTheme(loadTheme());
  wireFlashMessage();
  const account = await loadAccount();
  if (!account) return;
  await loadDropboxConnection();
})();
