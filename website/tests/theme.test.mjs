import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { runInNewContext } from 'node:vm';
import test from 'node:test';

const source = await readFile(new URL('../theme.js', import.meta.url), 'utf8');

function loadTheme({ saved = null, dark = false, blocked = false } = {}) {
  const events = {};
  const attributes = {};
  const button = { hidden: true, setAttribute: (key, value) => { attributes[key] = value; }, addEventListener: (event, callback) => { events[event] = callback; } };
  const root = { dataset: {} };
  const media = { matches: dark, addEventListener: (_, callback) => { events.system = callback; } };
  const document = {
    documentElement: root,
    querySelector: selector => selector === '.theme-toggle' ? button : null,
    addEventListener: (_, callback) => { events.ready = callback; }
  };
  const storage = new Map(saved === null ? [] : [['kitlane-theme', saved]]);
  runInNewContext(source, { document, window: { matchMedia: () => media }, localStorage: {
    getItem: key => { if (blocked) throw new Error('Storage blocked'); return storage.get(key) ?? null; },
    setItem: (key, value) => { if (blocked) throw new Error('Storage blocked'); storage.set(key, value); }
  } });
  const initialTheme = root.dataset.theme;
  events.ready?.();
  return { root, button, attributes, events, storage, initialTheme, media };
}

test('applies system preference before page content and follows system changes', () => {
  const page = loadTheme({ dark: true });
  assert.equal(page.initialTheme, 'dark');
  assert.equal(page.button.hidden, false);
  assert.equal(page.attributes['aria-pressed'], 'true');
  page.media.matches = false;
  page.events.system();
  assert.equal(page.root.dataset.theme, 'light');
  assert.equal(page.attributes['aria-pressed'], 'false');
});

test('saved light and dark choices override the system preference', () => {
  for (const saved of ['light', 'dark']) {
    const page = loadTheme({ saved, dark: saved === 'light' });
    assert.equal(page.initialTheme, saved);
    page.events.system();
    assert.equal(page.root.dataset.theme, saved);
  }
});

test('toggle saves a choice that survives navigation and system changes', () => {
  const page = loadTheme();
  page.events.click?.();
  assert.equal(page.root.dataset.theme, 'dark');
  assert.equal(page.storage.get('kitlane-theme'), 'dark');
  assert.equal(page.attributes['aria-pressed'], 'true');
  page.events.system();
  assert.equal(page.root.dataset.theme, 'dark');
  assert.equal(loadTheme({ saved: page.storage.get('kitlane-theme') }).initialTheme, 'dark');
  page.events.click();
  assert.equal(page.root.dataset.theme, 'light');
  assert.equal(page.attributes['aria-pressed'], 'false');
});

test('invalid saved values fall back to system preference', () => {
  assert.equal(loadTheme({ saved: 'invalid', dark: true }).initialTheme, 'dark');
});

test('blocked storage still allows toggling and keeps the in-page choice', () => {
  const page = loadTheme({ blocked: true });
  page.events.click?.();
  assert.equal(page.root.dataset.theme, 'dark');
  page.events.system();
  assert.equal(page.root.dataset.theme, 'dark');
});
