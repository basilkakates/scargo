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

async function apiGet(path) {
  const res = await fetch(API + path, { credentials: 'same-origin' });
  if (!res.ok) throw new Error(`${res.status}`);
  return res.json();
}

async function apiSend(path, method, body) {
  const res = await fetch(API + path, {
    method,
    credentials: 'same-origin',
    headers: body ? { 'Content-Type': 'application/json' } : {},
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    let message = `${res.status}`;
    try {
      const payload = await res.json();
      if (payload.err) message = payload.err;
    } catch {
      // ignore malformed error payloads
    }
    throw new Error(message);
  }
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

function vehicleName(vehicle) {
  return [vehicle.year || '', vehicle.make || '', vehicle.model || '', vehicle.engine_family || '']
    .filter(Boolean)
    .join(' ')
    || vehicle.handle
    || vehicle.id.slice(0, 8);
}

function pillClass(status) {
  return status === 'public' ? 'public' : status === 'private' ? 'private' : 'pending';
}

function statusLabel(status) {
  return status === 'public' ? 'Public' : status === 'private' ? 'Private' : 'Pending approval';
}

function canApprovePendingPublicStats() {
  return Boolean(currentAccount?.capabilities?.approve_pending_public_stats);
}

function approvalButtons(vehicle) {
  if (!canApprovePendingPublicStats()) return '';

  const buttons = [];
  if (vehicle.exact_vin_share_enabled && vehicle.exact_vin_pending_approval_count > 0) {
    buttons.push(`
      <button class="primary-button" type="button" data-action="approve-exact-vin">
        Approve remaining exact-VIN uploads (${formatInt(vehicle.exact_vin_pending_approval_count)})
      </button>
    `);
  }
  if (vehicle.cohort_pending_approval_count > 0) {
    buttons.push(`
      <button class="primary-button" type="button" data-action="approve-cohort">
        Approve remaining cohort uploads (${formatInt(vehicle.cohort_pending_approval_count)})
      </button>
    `);
  }

  if (buttons.length === 0) return '';
  return `
    <div class="approval-actions">
      <div class="card-label">Pending approvals</div>
      <div class="approval-copy">Manual dev/test approval only affects eligible pending uploads. Cohort approval still respects metric policy and available vehicle metadata.</div>
      <div class="vehicle-actions">${buttons.join('')}</div>
    </div>
  `;
}

function renderVehicles(vehicles) {
  const grid = document.getElementById('vehicle-grid');
  if (!Array.isArray(vehicles) || vehicles.length === 0) {
    grid.innerHTML = '<article class="empty-card">No vehicles are linked to this account right now.</article>';
    return;
  }
  grid.innerHTML = vehicles.map((vehicle) => `
    <article class="vehicle-card" data-vehicle-id="${vehicle.id}">
      <div class="vehicle-head">
        <div>
          <div class="vehicle-title">${escapeHtml(vehicleName(vehicle))}</div>
          <div class="vehicle-copy">Handle ${escapeHtml(vehicle.handle || vehicle.id)}</div>
        </div>
        <div class="pill-row">
          <span class="pill ${pillClass(vehicle.exact_vin_public_status)}">
            <span class="pill-label">Exact VIN</span>${statusLabel(vehicle.exact_vin_public_status)}
          </span>
          <span class="pill ${pillClass(vehicle.cohort_public_status)}">
            <span class="pill-label">Cohort</span>${statusLabel(vehicle.cohort_public_status)}
          </span>
        </div>
      </div>
      <div class="stats">
        <div class="stat">
          <div class="card-label">Readings</div>
          <div class="stat-value">${formatInt(vehicle.reading_count)}</div>
        </div>
        <div class="stat">
          <div class="card-label">Uploads</div>
          <div class="stat-value">${formatInt(vehicle.upload_count)}</div>
        </div>
        <div class="stat">
          <div class="card-label">Make / model</div>
          <div class="stat-value">${escapeHtml([vehicle.make, vehicle.model].filter(Boolean).join(' ') || 'Unknown')}</div>
        </div>
        <div class="stat">
          <div class="card-label">Engine family</div>
          <div class="stat-value">${escapeHtml(vehicle.engine_family || 'Unknown')}</div>
        </div>
      </div>
      <div class="share-row">
        <div>
          <div class="card-label">Exact-VIN contribution</div>
          <div class="vehicle-note">Turn this off to retract your account's exact-VIN sharing preference for existing and future uploads on this vehicle.</div>
        </div>
        <div class="vehicle-actions">
          <label class="toggle">
            <input type="checkbox" data-action="toggle-exact" ${vehicle.exact_vin_share_enabled ? 'checked' : ''}>
            <span>Share exact-VIN stats</span>
          </label>
          ${approvalButtons(vehicle)}
          <button class="danger-button" type="button" data-action="drop-vehicle">Drop from my account</button>
        </div>
      </div>
    </article>
  `).join('');
}

function formatInt(value) {
  return typeof value === 'number' ? value.toLocaleString() : '0';
}

function escapeHtml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

async function loadVehicles() {
  setStatus('page-status', 'Loading vehicles...');
  try {
    const vehicles = await apiGet('/vehicles?limit=200');
    renderVehicles(vehicles);
    setStatus('page-status', `${vehicles.length} vehicle${vehicles.length === 1 ? '' : 's'} loaded.`, 'ok');
  } catch (err) {
    document.getElementById('vehicle-grid').innerHTML = '<article class="empty-card">Vehicle load failed.</article>';
    setStatus('page-status', `Vehicle load failed: ${err.message}`, 'err');
  }
}

async function approveVehicleSharing(vehicleId, approval, button) {
  const actionLabel = approval === 'exact_vin' ? 'exact-VIN' : 'cohort';
  const path = approval === 'exact_vin'
    ? `/vehicles/${encodeURIComponent(vehicleId)}/approve-exact-vin-sharing`
    : `/vehicles/${encodeURIComponent(vehicleId)}/approve-cohort-sharing`;
  button.disabled = true;
  setStatus('page-status', `Approving ${actionLabel} sharing...`);
  try {
    const result = await apiSend(path, 'POST');
    await loadVehicles();
    setStatus(
      'page-status',
      `${actionLabel} approval saved: approved ${formatInt(result.approved_upload_count)}, already approved ${formatInt(result.already_approved_upload_count)}.`,
      'ok',
    );
  } catch (err) {
    button.disabled = false;
    setStatus('page-status', `${actionLabel} approval failed: ${err.message}`, 'err');
  }
}

async function setExactVinSharing(vehicleId, enabled, input) {
  input.disabled = true;
  setStatus('page-status', 'Saving exact-VIN sharing...');
  try {
    await apiSend(`/vehicles/${encodeURIComponent(vehicleId)}/exact-vin-sharing`, 'POST', { enabled });
    await loadVehicles();
  } catch (err) {
    input.checked = !enabled;
    input.disabled = false;
    setStatus('page-status', `Sharing update failed: ${err.message}`, 'err');
  }
}

async function dropVehicle(vehicleId, button) {
  const confirmed = window.confirm('Drop this vehicle from your account? Private access goes away, but already approved public stats stay public.');
  if (!confirmed) return;
  button.disabled = true;
  setStatus('page-status', 'Dropping vehicle...');
  try {
    await apiSend(`/vehicles/${encodeURIComponent(vehicleId)}`, 'DELETE');
    await loadVehicles();
  } catch (err) {
    button.disabled = false;
    setStatus('page-status', `Vehicle drop failed: ${err.message}`, 'err');
  }
}

document.getElementById('theme-toggle').addEventListener('click', toggleTheme);
document.getElementById('refresh-btn').addEventListener('click', loadVehicles);
document.getElementById('logout-btn').addEventListener('click', logout);
document.getElementById('vehicle-grid').addEventListener('change', (event) => {
  const target = event.target;
  if (!(target instanceof HTMLInputElement) || target.dataset.action !== 'toggle-exact') return;
  const card = target.closest('[data-vehicle-id]');
  if (!card) return;
  setExactVinSharing(card.dataset.vehicleId, target.checked, target);
});
document.getElementById('vehicle-grid').addEventListener('click', (event) => {
  const target = event.target;
  if (!(target instanceof HTMLButtonElement)) return;
  const card = target.closest('[data-vehicle-id]');
  if (!card) return;
  if (target.dataset.action === 'drop-vehicle') {
    dropVehicle(card.dataset.vehicleId, target);
    return;
  }
  if (target.dataset.action === 'approve-exact-vin') {
    approveVehicleSharing(card.dataset.vehicleId, 'exact_vin', target);
    return;
  }
  if (target.dataset.action === 'approve-cohort') {
    approveVehicleSharing(card.dataset.vehicleId, 'cohort', target);
  }
});

(async function init() {
  applyTheme(loadTheme());
  const account = await loadAccount();
  if (!account) return;
  await loadVehicles();
})();
