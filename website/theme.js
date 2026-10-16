(() => {
  const root = document.documentElement;
  const system = window.matchMedia('(prefers-color-scheme: dark)');
  let preference;
  try {
    const saved = localStorage.getItem('kitlane-theme');
    if (saved === 'light' || saved === 'dark') preference = saved;
  } catch { /* System preference still works when storage is unavailable. */ }

  function applyTheme() {
    root.dataset.theme = preference || (system.matches ? 'dark' : 'light');
    document.querySelector('.theme-toggle')?.setAttribute('aria-pressed', String(root.dataset.theme === 'dark'));
  }

  applyTheme();
  system.addEventListener('change', applyTheme);
  document.addEventListener('DOMContentLoaded', () => {
    const toggle = document.querySelector('.theme-toggle');
    if (!toggle) return;
    toggle.hidden = false;
    applyTheme();
    toggle.addEventListener('click', () => {
      preference = root.dataset.theme === 'dark' ? 'light' : 'dark';
      try { localStorage.setItem('kitlane-theme', preference); } catch { /* Keep the choice for this page. */ }
      applyTheme();
    });
  });
})();
