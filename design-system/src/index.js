/** Apply a light, dark, or system theme. Returns a cleanup function. */
export function setTheme(mode = 'system', root = document.documentElement) {
  if (!['light', 'dark', 'system'].includes(mode)) throw new TypeError('Unknown theme');
  const media = window.matchMedia('(prefers-color-scheme: dark)');
  const apply = () => { root.dataset.theme = mode === 'system' ? (media.matches ? 'dark' : 'light') : mode; };
  apply();
  if (mode === 'system') media.addEventListener('change', apply);
  return () => media.removeEventListener('change', apply);
}

/** Activate a tabs component with roving focus and optional manual activation. */
export function initTabs(root, { manual = false } = {}) {
  const tabs = [...root.querySelectorAll('[role="tab"]')];
  const select = tab => {
    if (!tabs.includes(tab) || tab.disabled) return;
    for (const item of tabs) {
      item.setAttribute('aria-selected', String(item === tab));
      item.tabIndex = item === tab ? 0 : -1;
      const panel = document.getElementById(item.getAttribute('aria-controls'));
      if (panel) panel.hidden = item !== tab;
    }
  };
  const click = event => { const tab = event.target.closest('[role="tab"]'); if (tab) select(tab); };
  const keydown = event => {
    const tab = event.target.closest('[role="tab"]'); if (!tab) return;
    const available = tabs.filter(t => !t.disabled), i = available.indexOf(tab);
    let next;
    if (event.key === 'ArrowRight') next = available[(i+1)%available.length];
    if (event.key === 'ArrowLeft') next = available[(i-1+available.length)%available.length];
    if (event.key === 'Home') next = available[0];
    if (event.key === 'End') next = available.at(-1);
    if (next) { event.preventDefault(); next.focus(); if (!manual) select(next); }
  };
  root.addEventListener('click', click); root.addEventListener('keydown', keydown);
  select(tabs.find(t => t.getAttribute('aria-selected') === 'true') || tabs[0]);
  return () => { root.removeEventListener('click', click); root.removeEventListener('keydown', keydown); };
}

/** Announce and display a dismissible notification; message is always plain text. */
export function toast(message, { tone = 'success', duration = 5000, region = document.getElementById('kl-toast-region') } = {}) {
  if (!['success', 'info', 'warning', 'danger'].includes(tone)) throw new TypeError('Unknown toast tone');
  if (!region) {
    region = document.createElement('div'); region.id = 'kl-toast-region'; region.className = 'kl-toast-region';
    document.body.append(region);
  }
  const element = document.createElement('div'); element.className = `kl-toast kl-alert--${tone}`;
  element.setAttribute('role', tone === 'danger' ? 'alert' : 'status');
  const text = document.createElement('span'); text.textContent = message;
  const close = document.createElement('button'); close.className = 'kl-button kl-button--ghost kl-button--icon';
  close.setAttribute('aria-label', 'Dismiss notification'); close.textContent = '×';
  element.append(text, close); region.append(element);
  let timer;
  const dismiss = () => { clearTimeout(timer); element.remove(); };
  const start = () => { if (duration > 0) timer = setTimeout(dismiss, duration); };
  element.addEventListener('pointerenter', () => clearTimeout(timer));
  element.addEventListener('pointerleave', start);
  element.addEventListener('focusin', () => clearTimeout(timer));
  element.addEventListener('focusout', start);
  close.addEventListener('click', dismiss); start();
  return dismiss;
}
