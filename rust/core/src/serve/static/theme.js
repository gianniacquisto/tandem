/*
 * The theme: a style (`neutral` or `tandem`) and an appearance (`system`,
 * `light` or `dark`), kept in localStorage and applied to <html> as
 * `data-style`, `data-appearance` and the resolved `data-theme`.
 *
 * Loaded synchronously in the head, before the stylesheets paint, so a reload
 * never flashes the wrong palette (the page's policy bars inline scripts, so
 * this is a file). The picker in the shell header drives it through delegated
 * clicks, which survive every htmx swap.
 */
(function () {
  var STYLE_KEY = 'tandem.style';
  var APPEARANCE_KEY = 'tandem.appearance';
  var STYLES = ['neutral', 'tandem'];
  var APPEARANCES = ['system', 'light', 'dark'];
  var root = document.documentElement;
  var dark = window.matchMedia ? window.matchMedia('(prefers-color-scheme: dark)') : null;

  function read(key, allowed, fallback) {
    try {
      var value = window.localStorage.getItem(key);
      return allowed.indexOf(value) >= 0 ? value : fallback;
    } catch (error) {
      return fallback;
    }
  }

  function write(key, value) {
    try {
      window.localStorage.setItem(key, value);
    } catch (error) {
      // Private mode: the choice holds for this page only.
    }
  }

  function apply(style, appearance) {
    var resolved = appearance === 'system' ? (dark && dark.matches ? 'dark' : 'light') : appearance;
    root.setAttribute('data-style', style);
    root.setAttribute('data-appearance', appearance);
    root.setAttribute('data-theme', resolved);
    var pressed = document.querySelectorAll('[data-theme-style], [data-theme-appearance]');
    for (var i = 0; i < pressed.length; i += 1) {
      var item = pressed[i];
      var on = item.getAttribute('data-theme-style') === style ||
        item.getAttribute('data-theme-appearance') === appearance;
      item.setAttribute('aria-pressed', on ? 'true' : 'false');
    }
  }

  function current() {
    return {
      style: read(STYLE_KEY, STYLES, 'neutral'),
      appearance: read(APPEARANCE_KEY, APPEARANCES, 'system')
    };
  }

  var now = current();
  apply(now.style, now.appearance);

  if (dark && dark.addEventListener) {
    dark.addEventListener('change', function () {
      var state = current();
      apply(state.style, state.appearance);
    });
  }

  document.addEventListener('DOMContentLoaded', function () {
    var state = current();
    apply(state.style, state.appearance);
  });

  document.addEventListener('click', function (event) {
    var target = event.target && event.target.closest
      ? event.target.closest('[data-theme-style], [data-theme-appearance]')
      : null;
    if (!target) return;
    var state = current();
    if (target.hasAttribute('data-theme-style')) {
      state.style = target.getAttribute('data-theme-style');
      write(STYLE_KEY, state.style);
    } else {
      state.appearance = target.getAttribute('data-theme-appearance');
      write(APPEARANCE_KEY, state.appearance);
    }
    apply(state.style, state.appearance);
  });

  // A menu closes on an outside click or Escape.
  document.addEventListener('click', function (event) {
    var menus = document.querySelectorAll('details.menu[open]');
    for (var i = 0; i < menus.length; i += 1) {
      if (!menus[i].contains(event.target)) menus[i].removeAttribute('open');
    }
  });
  document.addEventListener('keydown', function (event) {
    if (event.key !== 'Escape') return;
    var menus = document.querySelectorAll('details.menu[open]');
    for (var i = 0; i < menus.length; i += 1) menus[i].removeAttribute('open');
  });
})();
