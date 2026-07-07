const API = '/api';
const THEME_STORAGE = 'scargo.theme';

let currentAccount = null;

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
    // ignore malformed error payloads
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
  return res.json();
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

function renderSharedLinkStatus(payload) {
  const configured = Boolean(payload?.configured);
  const active = Boolean(payload?.active);
  const label = configured ? (payload.link_label || 'Saved Dropbox link') : 'Not saved';
  const counts = [
    payload?.ingested_count || 0,
    payload?.duplicate_count || 0,
    payload?.skipped_count || 0,
  ];

  const input = document.getElementById('shared-link-input');
  if (input) {
    input.placeholder = configured ? label : 'https://www.dropbox.com/scl/fo/...';
  }
  document.getElementById('saved-link-label').textContent = label;
  document.getElementById('last-sync-at').textContent = formatTimestamp(payload?.last_sync_at);
  document.getElementById('last-success-at').textContent = formatTimestamp(payload?.last_success_at);
  document.getElementById('file-counts').textContent = counts.join(' / ');

  const pause = document.getElementById('shared-link-pause-btn');
  const del = document.getElementById('shared-link-delete-btn');
  const sync = document.getElementById('shared-link-sync-btn');
  if (pause) {
    pause.disabled = !configured;
    pause.textContent = active ? 'Pause' : 'Resume';
  }
  if (del) del.disabled = !configured;
  if (sync) sync.disabled = !configured || !active;

  if (!configured) {
    setStatus('shared-link-status', 'No shared link saved');
    return;
  }

  const summary = `${active ? 'Active' : 'Paused'} - ${counts[0]} ingested, ${counts[1]} duplicate, ${counts[2]} skipped`;
  const latestError = payload?.latest_error ? ` - ${payload.latest_error}` : '';
  setStatus('shared-link-status', `${summary}${latestError}`, payload?.latest_error ? 'err' : 'ok');
}

async function loadSharedLinkStatus() {
  try {
    renderSharedLinkStatus(await apiGet('/ingest-sources/shared-link'));
    setStatus('page-status', '');
  } catch (err) {
    setStatus('shared-link-status', `Shared link unavailable: ${err.message}`, 'err');
  }
}

async function saveSharedLink() {
  const input = document.getElementById('shared-link-input');
  const url = input.value.trim();
  if (!url) {
    setStatus('page-status', 'Dropbox shared folder URL required', 'err');
    return;
  }
  setStatus('page-status', 'Saving...');
  try {
    const payload = await apiSend('/ingest-sources/shared-link', 'PUT', { url });
    input.value = '';
    renderSharedLinkStatus(payload);
    setStatus('page-status', 'Shared link saved.', 'ok');
  } catch (err) {
    setStatus('page-status', `Save failed: ${err.message}`, 'err');
  }
}

async function toggleSharedLinkPause() {
  setStatus('page-status', 'Updating...');
  try {
    renderSharedLinkStatus(await apiSend('/ingest-sources/shared-link/pause', 'POST'));
    setStatus('page-status', 'Shared link state updated.', 'ok');
  } catch (err) {
    setStatus('page-status', `Update failed: ${err.message}`, 'err');
  }
}

async function deleteSharedLink() {
  const confirmed = window.confirm('Delete the saved Dropbox source for this account? Ingested telemetry stays in Scargo.');
  if (!confirmed) return;
  setStatus('page-status', 'Deleting...');
  try {
    renderSharedLinkStatus(await apiSend('/ingest-sources/shared-link', 'DELETE'));
    setStatus('page-status', 'Shared link deleted.', 'ok');
  } catch (err) {
    setStatus('page-status', `Delete failed: ${err.message}`, 'err');
  }
}

async function syncSharedLink() {
  setStatus('page-status', 'Syncing...');
  try {
    renderSharedLinkStatus(await apiSend('/ingest-sources/shared-link/sync-now', 'POST'));
    setStatus('page-status', 'Sync complete.', 'ok');
  } catch (err) {
    await loadSharedLinkStatus();
    setStatus('page-status', `Sync failed: ${err.message}`, 'err');
  }
}

document.getElementById('theme-toggle').addEventListener('click', toggleTheme);
document.getElementById('refresh-btn').addEventListener('click', loadSharedLinkStatus);
document.getElementById('logout-btn').addEventListener('click', logout);
document.getElementById('shared-link-save-btn').addEventListener('click', saveSharedLink);
document.getElementById('shared-link-pause-btn').addEventListener('click', toggleSharedLinkPause);
document.getElementById('shared-link-delete-btn').addEventListener('click', deleteSharedLink);
document.getElementById('shared-link-sync-btn').addEventListener('click', syncSharedLink);

(async function init() {
  applyTheme(loadTheme());
  const account = await loadAccount();
  if (!account) return;
  await loadSharedLinkStatus();
})();
