const API = '/api';
const THEME_STORAGE = 'scargo.theme';
const GUEST_CONSENT_STORAGE = 'scargo.guestConsent';
const FLASH_UPLOAD_TOKEN_STORAGE = 'scargo.flashUploadToken';

let authMode = 'login';
let guestAvailable = false;

function loadTheme() {
  const saved = localStorage.getItem(THEME_STORAGE);
  if (saved === 'light' || saved === 'dark') return saved;
  return 'dark';
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

function setStatus(id, text, kind = '') {
  const node = document.getElementById(id);
  if (!node) return;
  node.textContent = text;
  node.className = kind;
}

async function apiGet(path) {
  const response = await fetch(API + path, {
    credentials: 'same-origin',
  });
  if (!response.ok) throw new Error(`${response.status}`);
  return response.json();
}

async function apiPostJson(path, body = {}) {
  const response = await fetch(API + path, {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    let message = `${response.status}`;
    try {
      const payload = await response.json();
      if (payload.error) message = payload.error;
    } catch {
      // ignore malformed error payloads
    }
    throw new Error(message);
  }
  return response.json();
}

function setAuthMode(mode) {
  authMode = mode === 'register' ? 'register' : 'login';
  document.getElementById('mode-title').textContent = authMode === 'register' ? 'Create account' : 'Sign in';
  document.getElementById('auth-submit').textContent = authMode === 'register' ? 'Create account' : 'Log in';
  document.getElementById('auth-password').autocomplete = authMode === 'register' ? 'new-password' : 'current-password';
  document.getElementById('mode-login-btn').classList.toggle('active', authMode === 'login');
  document.getElementById('mode-register-btn').classList.toggle('active', authMode === 'register');
  setStatus('auth-status', authMode === 'register' ? 'Register with the same API validation rules.' : 'Use your dashboard account credentials.');
}

function redirectToDashboard() {
  window.location.replace('/');
}

function setGuestAvailability(enabled) {
  guestAvailable = enabled;
  const button = document.getElementById('guest-btn');
  button.disabled = !enabled;
  setStatus(
    'guest-status',
    enabled
      ? 'Guest access is available in this environment.'
      : 'Guest access is unavailable here.',
    enabled ? 'ok' : ''
  );
}

async function handleAuth(event) {
  event.preventDefault();
  const username = document.getElementById('auth-username').value.trim();
  const password = document.getElementById('auth-password').value;
  setStatus('auth-status', authMode === 'register' ? 'Creating account...' : 'Signing in...');
  try {
    const payload = await apiPostJson(`/auth/${authMode}`, { username, password });
    setGuestConsent(false);
    stashFlashUploadToken(payload.upload_token || '');
    redirectToDashboard();
  } catch (err) {
    setStatus('auth-status', `Auth failed: ${err.message}`, 'err');
  } finally {
    document.getElementById('auth-password').value = '';
  }
}

function continueAsGuest() {
  if (!guestAvailable) return;
  setGuestConsent(true);
  stashFlashUploadToken('');
  redirectToDashboard();
}

async function init() {
  applyTheme(loadTheme());
  setAuthMode('login');
  try {
    const payload = await apiGet('/auth/me');
    if (payload.account?.is_guest) {
      setGuestAvailability(true);
      setStatus('auth-status', 'Guest fallback detected. Sign in or continue as guest.');
      return;
    }
    redirectToDashboard();
  } catch {
    setGuestAvailability(false);
    setStatus('auth-status', 'No active session. Sign in or create an account.');
  }
}

document.getElementById('theme-toggle').addEventListener('click', toggleTheme);
document.getElementById('mode-login-btn').addEventListener('click', () => setAuthMode('login'));
document.getElementById('mode-register-btn').addEventListener('click', () => setAuthMode('register'));
document.getElementById('auth-form').addEventListener('submit', handleAuth);
document.getElementById('guest-btn').addEventListener('click', continueAsGuest);

init();
