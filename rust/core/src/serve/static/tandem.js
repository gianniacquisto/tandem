/*
 * The acts htmx does not own: a board card drop, a notes tree move, the
 * ticket sheet's label select and shortcut keys, and the notes editor's
 * debounced autosave and preview.
 *
 * The server draws every face; this file only turns a pointer or a key into the
 * same form POST the page already carries. Listeners are delegated from the
 * document, so an htmx swap never needs them rebound.
 */
// MARK: - Remembered UI state in cookies
//
// A state the server must draw on the first paint (the notes tree's folds, and
// later the tabs and the Relations pane) lives in one cookie per state,
// `<prefix><state>`, where the server names the prefix on
// `<html data-ui-cookie>` (`tandem_<port>_`; cookies ignore ports, so two
// serves on one host must not share a state). The value is
// `encodeURIComponent(JSON.stringify(value))`, a year, `Path=/`,
// `SameSite=Lax`, neither `Secure` (a plain-http LAN serve keeps it) nor
// `HttpOnly`. The server reads it (`serve/ui_state.rs`) and never trusts it.
(function () {
  var MAX_ENCODED = 3800;

  function prefix() {
    return document.documentElement.getAttribute('data-ui-cookie') || '';
  }

  function rawValue(state) {
    var name = prefix() + state + '=';
    if (!prefix()) return null;
    var pairs = document.cookie ? document.cookie.split(';') : [];
    for (var i = 0; i < pairs.length; i += 1) {
      var pair = pairs[i].replace(/^\s+/, '');
      if (pair.indexOf(name) === 0) return pair.slice(name.length);
    }
    return null;
  }

  // The state's parsed value, or null when it is absent or damaged.
  function read(state) {
    var raw = rawValue(state);
    if (raw === null) return null;
    try {
      return JSON.parse(decodeURIComponent(raw));
    } catch (error) {
      return null;
    }
  }

  // Writes the state; false when it is too large for a cookie or none is set.
  function write(state, value) {
    if (!prefix()) return false;
    var encoded = encodeURIComponent(JSON.stringify(value));
    if (encoded.length > MAX_ENCODED) return false;
    document.cookie = prefix() + state + '=' + encoded +
      '; Path=/; Max-Age=31536000; SameSite=Lax';
    return true;
  }

  window.tandem = window.tandem || {};
  window.tandem.uiState = { read: read, write: write, has: function (state) { return rawValue(state) !== null; } };
})();

(function () {
  var draggedCard = null;
  var draggedNote = null;
  var noteTimer = null;

  function closest(element, selector) {
    while (element && element.nodeType === 1) {
      if (element.matches(selector)) return element;
      element = element.parentElement;
    }
    return null;
  }

  // A 401 on a background request means the session ended. Say so in place:
  // no redirect and no reload, so an unsaved note edit or a half-typed comment
  // stays on screen. The link opens the login page in a new tab, carrying the
  // path this tab is on. `window.tandemSignedOut` is also what sse.js calls.
  function signedOut() {
    var banner = document.getElementById('signed-out-banner');
    if (!banner) return;
    var link = document.getElementById('signed-out-link');
    if (link) {
      link.setAttribute('href', '/login?next=' + encodeURIComponent(window.location.pathname + window.location.search));
    }
    banner.hidden = false;
  }

  function signedIn() {
    var banner = document.getElementById('signed-out-banner');
    if (banner) banner.hidden = true;
  }

  window.tandemSignedOut = signedOut;

  document.addEventListener('htmx:responseError', function (event) {
    var xhr = event.detail && event.detail.xhr;
    if (xhr && xhr.status === 401) signedOut();
  });

  function focusCard(id) {
    var card = document.querySelector('.ticket-card[data-ticket="' + id + '"]');
    if (card) card.focus();
  }

  // htmx 2 drops a request whose source is not in the document: it returns
  // before sending when `bodyContains` is false. A form built for one drag is
  // detached until it lands, so it is attached for the request and taken away
  // once the request settles.
  function htmxPost(form, target) {
    if (!window.htmx) return false;
    var detached = !form.isConnected;
    if (detached) {
      form.hidden = true;
      document.body.appendChild(form);
    }
    var request = window.htmx.ajax('POST', form.getAttribute('action'), {
      source: form,
      target: target,
      swap: 'outerHTML',
      values: new FormData(form)
    });
    if (detached) {
      var remove = function () {
        if (form.parentNode) form.parentNode.removeChild(form);
      };
      if (request && request.then) request.then(remove, remove);
      else window.setTimeout(remove, 0);
    }
    return true;
  }

  // MARK: - The board's card move

  function moveCard(ticketId, column) {
    var root = document.getElementById('board-root');
    if (!root || !column || !ticketId) return;
    // The board's answer replaces the root, so the POST carries the filters the
    // board was drawn with; without them the partial redraws an unfiltered board
    // and the human's project, label, assignee and search are gone.
    var bar = root.querySelector('.tickets__bar--board');
    var body = bar ? new URLSearchParams(new FormData(bar)) : new URLSearchParams();
    body.set('status', column.getAttribute('data-status') || '');
    body.set('expected_revision', root.getAttribute('data-revision') || '0');
    fetch('/tickets/' + encodeURIComponent(ticketId) + '/status', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
        'HX-Request': 'true'
      },
      body: body.toString()
    })
      .then(function (response) {
        if (response.status === 401) {
          signedOut();
          return null;
        }
        if (!response.ok) throw new Error('the move answered ' + response.status);
        signedIn();
        return response.text();
      })
      .then(function (html) {
        if (html === null) return;
        var current = document.getElementById('board-root');
        if (!current) return;
        current.outerHTML = html;
        focusCard(ticketId);
      })
      .catch(function (error) {
        console.error('the card move failed:', error);
      });
  }

  document.addEventListener('dragstart', function (event) {
    var card = closest(event.target, '.ticket-card');
    if (card) {
      draggedCard = card.getAttribute('data-ticket');
      event.dataTransfer.setData('text/plain', draggedCard);
      event.dataTransfer.effectAllowed = 'move';
      return;
    }
    var row = closest(event.target, '.tree__row[data-path]');
    if (row) {
      draggedNote = row.getAttribute('data-path');
      event.dataTransfer.setData('text/plain', draggedNote);
      event.dataTransfer.effectAllowed = 'move';
    }
  });

  document.addEventListener('dragover', function (event) {
    if (draggedCard) {
      var column = closest(event.target, '.board__column');
      if (!column) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = 'move';
      column.setAttribute('data-targeted', '');
      return;
    }
    if (draggedNote) {
      var into = closest(event.target, '.tree__row[data-kind="folder"], .tree__vault');
      if (!into) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = 'move';
      into.setAttribute('data-over', 'true');
    }
  });

  document.addEventListener('dragleave', function (event) {
    var column = closest(event.target, '.board__column');
    if (column) column.removeAttribute('data-targeted');
    var into = closest(event.target, '.tree__row[data-kind="folder"], .tree__vault');
    if (into) into.removeAttribute('data-over');
  });

  document.addEventListener('drop', function (event) {
    if (draggedCard) {
      var column = closest(event.target, '.board__column');
      if (!column) return;
      event.preventDefault();
      column.removeAttribute('data-targeted');
      moveCard(draggedCard, column);
      draggedCard = null;
      return;
    }
    if (draggedNote) {
      var into = closest(event.target, '.tree__row[data-kind="folder"], .tree__vault');
      if (!into) return;
      event.preventDefault();
      into.removeAttribute('data-over');
      var target = into.getAttribute('data-path') || '';
      var source = draggedNote;
      draggedNote = null;
      // A note cannot land on itself or inside its own folder, and a move into
      // the folder it already sits in is a no-op: the same rule the core keeps.
      var parent = source.lastIndexOf('/') === -1 ? '' : source.slice(0, source.lastIndexOf('/'));
      if (source === target || target === source + '/' || target.indexOf(source + '/') === 0 || parent === target) return;
      var form = document.createElement('form');
      form.setAttribute('action', '/notes/move');
      form.innerHTML =
        '<input name="path"><input name="into">' +
        '<input name="project"><input name="show_all">';
      form.elements.path.value = source;
      form.elements.into.value = target;
      var focus = document.querySelector('#notes-root .focus');
      form.elements.project.value = focus ? focus.elements.project.value : '';
      form.elements.show_all.value = focus && focus.elements.show_all.checked ? '1' : '';
      htmxPost(form, '#notes-root');
    }
  });

  document.addEventListener('keydown', function (event) {
    if (!event.altKey || (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight')) return;
    var card = closest(event.target, '.ticket-card');
    if (!card) return;
    var column = closest(card, '.board__column');
    var board = closest(card, '.board');
    if (!column || !board) return;
    var columns = board.querySelectorAll('.board__column');
    var index = Array.prototype.indexOf.call(columns, column);
    var target = columns[index + (event.key === 'ArrowRight' ? 1 : -1)];
    if (!target) return;
    event.preventDefault();
    moveCard(card.getAttribute('data-ticket'), target);
  });

  // MARK: - The New form's label choices

  // The Label select lists the chosen project's labels only, and goes back to
  // "No label" when the project changes. Every label carries its project in
  // data-project; the page draws them all, hidden but for the preset's.
  function syncNewLabels(form) {
    var project = form.elements.ticket_project;
    var label = form.elements.ticket_label;
    if (!project || !label) return;
    var chosen = project.value;
    Array.prototype.forEach.call(label.options, function (option) {
      var own = option.getAttribute('data-project');
      if (own === null) return;
      var show = own === chosen;
      option.hidden = !show;
      option.disabled = !show;
    });
    label.disabled = chosen === '';
  }

  document.addEventListener('change', function (event) {
    var project = closest(event.target, 'select[name="ticket_project"]');
    if (!project || !project.form) return;
    project.form.elements.ticket_label.value = '';
    syncNewLabels(project.form);
  });

  function setUpNewForm() {
    var form = document.querySelector('.new-form');
    if (form) syncNewLabels(form);
  }
  document.addEventListener('DOMContentLoaded', setUpNewForm);
  document.addEventListener('htmx:afterSwap', setUpNewForm);

  // MARK: - The ticket sheet

  // The label is the one metadata control that moves at once: its own form.
  document.addEventListener('change', function (event) {
    var select = closest(event.target, '[data-sheet-label]');
    if (select && select.form) htmxPost(select.form, '#board-root');
  });

  document.addEventListener('keydown', function (event) {
    var sheet = document.querySelector('.sheet');
    if (!sheet) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      // Close where the sheet was opened from, filters and all.
      var close = sheet.querySelector('.sheet__close');
      window.location.href = close ? close.getAttribute('href') : '/board';
      return;
    }
    if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
      event.preventDefault();
      var form;
      if (event.shiftKey) {
        form = sheet.querySelector('.sheet__discussion form');
      } else {
        form = document.getElementById('sheet-save');
      }
      if (form) htmxPost(form, '#board-root');
    }
  });

  // The description reads rendered; Edit swaps to the box. Without this script
  // the box shows, so the plain form still saves. An empty description has no
  // rendered block and keeps the box.
  function growField(field) {
    if (window.CSS && CSS.supports && CSS.supports('field-sizing', 'content')) return;
    field.style.height = 'auto';
    field.style.height = field.scrollHeight + 'px';
  }

  function setUpSheet() {
    var section = document.querySelector('[data-description]');
    if (section && section.querySelector('[data-description-view]')) {
      section.classList.add('is-reading');
    }
    var fields = document.querySelectorAll('.sheet__title, .sheet__description-field');
    for (var i = 0; i < fields.length; i++) growField(fields[i]);
  }

  document.addEventListener('click', function (event) {
    var button = closest(event.target, '[data-description-edit]');
    if (!button) return;
    var section = closest(button, '[data-description]');
    section.classList.remove('is-reading');
    var box = section.querySelector('.sheet__description-field');
    growField(box);
    box.focus();
  });

  document.addEventListener('input', function (event) {
    if (closest(event.target, '.sheet__title, .sheet__description-field')) {
      growField(event.target);
    }
  });

  // Enter in the title saves; a title is one line.
  document.addEventListener('keydown', function (event) {
    if (event.key !== 'Enter' || event.isComposing) return;
    if (!closest(event.target, '.sheet__title')) return;
    event.preventDefault();
    var form = document.getElementById('sheet-save');
    if (form) htmxPost(form, '#board-root');
  });

  document.addEventListener('DOMContentLoaded', setUpSheet);
  document.addEventListener('htmx:afterSwap', setUpSheet);

  // MARK: - The notes editor

  // The autosave writes without swapping the pane: a swap would replace the
  // textarea under the human's caret every second. The response still carries
  // the new baseline, which the hidden input adopts; a conflict or an error is
  // the one answer that does swap, because it must be seen. `writeNote`
  // resolves whether the buffer reached the disk, so a caller that navigates
  // can stay put when it did not. It posts the form it is handed, the one whose
  // text was edited, never whichever note the page shows by the time it runs.
  // `leaving` marks the write sent as the page closes: only that one asks for
  // keepalive, whose body is capped at 64 KB, so a long note still saves.
  function writeNote(form, leaving) {
    if (!form) return Promise.resolve(false);
    return fetch(form.getAttribute('action'), {
      method: 'POST',
      keepalive: !!leaving,
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
        'HX-Request': 'true'
      },
      body: new URLSearchParams(new FormData(form)).toString()
    })
      .then(function (response) {
        if (response.status === 401) {
          signedOut();
          return null;
        }
        signedIn();
        return response.text();
      })
      .then(function (html) {
        if (html === null) return false;
        if (html.indexOf('class="conflict"') !== -1 || html.indexOf('class="banner"') !== -1) {
          var root = document.getElementById('notes-root');
          // Through htmx, so the new nodes are processed and the editor is
          // made editable again.
          if (root) {
            window.htmx.swap(root, html, { swapStyle: 'outerHTML' });
            setUpNote();
          }
          return false;
        }
        var parsed = new DOMParser().parseFromString(html, 'text/html');
        var fresh = parsed.querySelector('[data-note-baseline]');
        var current = form.querySelector('[data-note-baseline]');
        if (fresh && current) current.value = fresh.getAttribute('value');
        return true;
      })
      .catch(function (error) {
        console.error('the note save failed:', error);
        return false;
      });
  }

  // One write at a time: a save asked for while another is out waits its turn
  // and then reads the buffer again, so the text the human sees is the text
  // that lands, and a navigation's answer belongs to its own save.
  var noteChain = Promise.resolve(true);
  var pendingForm = null;
  var savesOut = 0;

  function saveNote(form) {
    var write = function () { return writeNote(form); };
    savesOut += 1;
    noteChain = noteChain.then(write, write);
    noteChain.then(function () { savesOut -= 1; });
    return noteChain;
  }

  function scheduleSave() {
    if (noteTimer) window.clearTimeout(noteTimer);
    noteTimer = window.setTimeout(function () {
      var form = pendingForm;
      noteTimer = null;
      pendingForm = null;
      saveNote(form);
    }, 1000);
  }

  // Write now what the debounce still holds, and wait for any write in flight.
  // Resolves whether the disk has the text.
  function flushNote() {
    if (noteTimer) {
      var form = pendingForm;
      window.clearTimeout(noteTimer);
      noteTimer = null;
      pendingForm = null;
      return saveNote(form);
    }
    return noteChain;
  }

  document.addEventListener('input', function (event) {
    var editor = closest(event.target, '.note-editor');
    if (!editor) return;
    var form = closest(editor, 'form');
    // Edits to a form other than the pending one: the pending one goes first.
    if (pendingForm && pendingForm !== form) flushNote();
    pendingForm = form;
    scheduleSave();
  });

  // A request that swaps the pane waits for the save, so the last second of
  // typing lands before the page moves on, and a rename posts the old path
  // only after the text is on disk. A save that did not land (a conflict, an
  // error) keeps the page where it is, banner showing.
  document.addEventListener('htmx:confirm', function (event) {
    if (!noteTimer && !pendingForm && savesOut === 0) return;
    event.preventDefault();
    var issue = event.detail.issueRequest;
    flushNote().then(function (landed) {
      if (landed) issue();
    });
  });

  // Closing the tab inside the debounce: send what is held, past the unload.
  function flushOnLeave() {
    if (!noteTimer) return;
    var form = pendingForm;
    window.clearTimeout(noteTimer);
    noteTimer = null;
    pendingForm = null;
    writeNote(form, true);
  }

  window.addEventListener('pagehide', flushOnLeave);
  document.addEventListener('visibilitychange', function () {
    if (document.visibilityState === 'hidden') flushOnLeave();
  });

  // MARK: - The live editor
  //
  // One editable surface holding the note's plain markdown. Every line is
  // styled as the page it makes (headings large, bold bold, bullets as bullets)
  // with its markers hidden, except the line the cursor is in, which shows them.
  // The hidden textarea in the form stays the one source of truth: each edit
  // writes the text into it and fires its `input`, so the autosave above needs
  // no change. The text is never anything but the file's own characters.
  var lives = [];
  // The root being drawn, so a link's broken tint reads that root's own list.
  var drawing = null;

  function esc(text) {
    return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }

  function marker(text) {
    return '<span class="lp__m">' + esc(text) + '</span>';
  }

  // Bold, italic, strike, code, wikilinks and links inside one line.
  var INLINE = /(`[^`\n]+`)|(\*\*[^*\n]+\*\*)|(~~[^~\n]+~~)|(\*[^*\n]+\*)|(\b_[^_\n]+_\b)|(\[\[[^\]\n]+\]\])|(\[[^\]\n]+\]\([^)\n]+\))/g;

  function inline(source) {
    var out = '';
    var last = 0;
    var match;
    INLINE.lastIndex = 0;
    while ((match = INLINE.exec(source))) {
      out += esc(source.slice(last, match.index));
      var token = match[0];
      if (match[1]) {
        out += '<span class="lp__code">' + marker('`') + esc(token.slice(1, -1)) + marker('`') + '</span>';
      } else if (match[2]) {
        out += '<strong>' + marker('**') + esc(token.slice(2, -2)) + marker('**') + '</strong>';
      } else if (match[3]) {
        out += '<s>' + marker('~~') + esc(token.slice(2, -2)) + marker('~~') + '</s>';
      } else if (match[4] || match[5]) {
        out += '<em>' + marker(token.charAt(0)) + esc(token.slice(1, -1)) + marker(token.charAt(0)) + '</em>';
      } else if (match[6]) {
        var inner = token.slice(2, -2);
        var typed = /^\s*(project|label)\s*:/i.test(inner);
        var embed = match.index > 0 && source.charAt(match.index - 1) === '!';
        var follow = !embed;
        var cut = inner.search(/[#|]/);
        var name = linkKey(cut < 0 ? inner : inner.slice(0, cut));
        var broken = follow && brokenLinks().indexOf(name) >= 0;
        out += '<span class="lp__wiki' + (follow ? ' lp__wiki--follow' : '') +
          (typed ? ' lp__wiki--typed' : '') +
          (broken ? ' wikilink--broken' : '') + '"' +
          (follow ? ' data-link="' + esc(inner).replace(/"/g, '&quot;') + '"' : '') +
          (broken ? ' title="No single note named ' + esc(cut < 0 ? inner : inner.slice(0, cut)).replace(/"/g, '&quot;') + '"' : '') +
          '>' + marker('[[') + esc(inner) + marker(']]') + '</span>';
      } else {
        var close = token.indexOf('](');
        out += '<span class="lp__link">' + marker('[') + esc(token.slice(1, close)) +
          marker(token.slice(close)) + '</span>';
      }
      last = match.index + token.length;
    }
    return out + esc(source.slice(last));
  }

  // The key a link target is looked up by: lowercased, a typed one as
  // `kind:name` with the spaces around the colon dropped.
  function linkKey(target) {
    return target.replace(/^\s*(project|label)\s*:\s*/i, '$1:').trim().toLowerCase();
  }

  // The targets the server found broken when the page loaded, lowercased.
  function brokenLinks() {
    var root = drawing;
    try {
      return JSON.parse((root && root.getAttribute('data-broken-links')) || '[]');
    } catch (error) {
      return [];
    }
  }

  // Follow a link's raw text: the pending save lands first, then the server
  // resolves it (ADR 0005) and the pane swaps. A save that did not land keeps
  // the page where it is.
  function followWikilink(raw) {
    var form = document.getElementById('note-form');
    if (!form || !window.htmx) return;
    var cut = raw.search(/[#|]/);
    var target = cut < 0 ? raw : raw.slice(0, cut);
    var value = function (name) {
      var input = form.querySelector('input[name="' + name + '"]');
      return input ? input.value : '';
    };
    // A project or label link opens its page; the board's names decide there.
    var typed = /^\s*(project|label)\s*:\s*(.*)$/i.exec(target);
    if (typed) {
      var page = new URLSearchParams({
        kind: typed[1].toLowerCase(),
        name: typed[2].trim(),
        project: value('project'),
        show_all: value('show_all')
      });
      flushNote().then(function (landed) {
        if (!landed) return;
        window.htmx.ajax('GET', '/notes/concept?' + page.toString(), {
          target: '#notes-root',
          swap: 'outerHTML'
        });
      });
      return;
    }
    var query = new URLSearchParams({
      target: target,
      from: value('path'),
      project: value('project'),
      show_all: value('show_all')
    });
    flushNote().then(function (landed) {
      if (!landed) return;
      window.htmx.ajax('GET', '/notes/open?' + query.toString(), {
        target: '#notes-root',
        swap: 'outerHTML'
      });
    });
  }

  // The link the caret sits in, as its raw text, or null.
  function wikilinkAt(text, offset) {
    var re = /\[\[([^\]\n]+)\]\]/g;
    var match;
    while ((match = re.exec(text))) {
      if (offset >= match.index && offset <= match.index + match[0].length) {
        if (match.index > 0 && text.charAt(match.index - 1) === '!') return null;
        return match[1];
      }
    }
    return null;
  }

  var HEADING = /^(#{1,6})( +)(.*)$/;
  var RULE = /^\s*([-*_])(\s*\1){2,}\s*$/;
  var BULLET = /^(\s*)([-*+])( +)(\[[ xX]\] +)?(.*)$/;
  var NUMBER = /^(\s*)(\d+)([.)])( +)(.*)$/;
  var QUOTE = /^(>+)( ?)(.*)$/;
  var FENCE = /^\s*(```|~~~)/;
  // A box's opener: GitHub's alert syntax, the shape the slash menu writes.
  var CALLOUT = /^(>)( ?)(\[!(NOTE|INFO|TIP|WARNING)\])\s*$/i;

  // The column a run of leading whitespace reaches; a tab counts to the next
  // multiple of four.
  function columnOf(space) {
    var col = 0;
    for (var i = 0; i < space.length; i += 1) {
      col = space.charAt(i) === '\t' ? col + 4 - (col % 4) : col + 1;
    }
    return col;
  }

  // A list line's place in its list: its indent, its content column (the
  // CommonMark rule: indent plus marker width plus spaces) and, for a number,
  // its value. Null for any other line.
  function listLine(line) {
    if (RULE.test(line)) return null;
    var m = BULLET.exec(line);
    if (m) {
      var indent = columnOf(m[1]);
      return { indent: indent, content: indent + m[2].length + m[3].length, num: null };
    }
    m = NUMBER.exec(line);
    if (!m) return null;
    var at = columnOf(m[1]);
    return { indent: at, content: at + m[2].length + m[3].length + m[4].length, num: parseInt(m[2], 10) };
  }

  function alpha(n) {
    var out = '';
    while (n > 0) {
      n -= 1;
      out = String.fromCharCode(97 + (n % 26)) + out;
      n = Math.floor(n / 26);
    }
    return out;
  }

  function roman(n) {
    var table = [[1000, 'm'], [900, 'cm'], [500, 'd'], [400, 'cd'], [100, 'c'], [90, 'xc'],
      [50, 'l'], [40, 'xl'], [10, 'x'], [9, 'ix'], [5, 'v'], [4, 'iv'], [1, 'i']];
    var out = '';
    for (var i = 0; i < table.length; i += 1) {
      while (n >= table[i][0]) {
        out += table[i][1];
        n -= table[i][0];
      }
    }
    return out;
  }

  // The label a numbered item draws at a depth: decimal, lower-alpha, then
  // lower-roman, cycling. Empty where the digits as written stay.
  function numberLabel(depth, value, delimiter) {
    var level = depth % 3;
    if (level === 0 || value < 1 || value > 3999) return '';
    return (level === 1 ? alpha(value) : roman(value)) + delimiter;
  }

  function lineHtml(line, label) {
    var m;
    if ((m = HEADING.exec(line))) {
      return ['lp__line--h' + m[1].length, marker(m[1] + m[2]) + inline(m[3])];
    }
    if (RULE.test(line)) return ['lp__line--rule', esc(line)];
    if ((m = BULLET.exec(line))) {
      if (m[4]) {
        // A task line has no dot: its box is drawn over the `[ ]` characters.
        var done = m[4].charAt(1) !== ' ';
        return ['lp__line--task' + (done ? ' lp__line--done' : ''),
          marker(m[1]) + marker(m[2] + m[3]) + '<span class="lp__task">' + esc(m[4]) + '</span>' + inline(m[5])];
      }
      return ['lp__line--bullet', marker(m[1]) + marker(m[2] + m[3]) + inline(m[5])];
    }
    if ((m = NUMBER.exec(line))) {
      var digits = esc(m[2] + m[3] + m[4]);
      var num = label
        ? '<span class="lp__num" data-label="' + esc(label) + '"><span class="lp__digits">' + digits + '</span></span>'
        : '<span class="lp__num">' + digits + '</span>';
      return ['lp__line--number', marker(m[1]) + num + inline(m[5])];
    }
    if ((m = CALLOUT.exec(line))) {
      return ['lp__line--quote lp__line--callout lp__line--callout-' + m[4].toLowerCase(),
        marker(m[1] + m[2] + m[3])];
    }
    if ((m = QUOTE.exec(line))) {
      return ['lp__line--quote', marker(m[1] + m[2]) + inline(m[3])];
    }
    return ['', inline(line)];
  }

  // The one wording of the empty-line hint and of the editor's description.
  var SLASH_HINT = 'Type / for commands';
  var SLASH_DESCRIPTION = 'Type / to insert a block or command';

  function Live(root, store) {
    this.root = root;
    this.store = store;
    this.text = store.value.replace(/\n$/, '');
    this.composing = false;
    this.dragging = false;
    this.activeLine = -1;
    this.menu = null;
    this.history = [{ text: this.text, start: 0, end: 0, at: 0 }];
    this.position = 0;
    this.bind();
    this.render(null);
  }

  Live.prototype.lineIndex = function (offset) {
    return this.text.slice(0, offset).split('\n').length - 1;
  };

  Live.prototype.render = function (selection) {
    var lines = this.text.split('\n');
    var caret = selection ? (selection.backward ? selection.start : selection.end) : null;
    this.activeLine = caret === null ? -1 : this.lineIndex(caret);
    var fenced = false;
    drawing = this.root;
    // The box a run of `> ` lines belongs to, from its opener on.
    var box = '';
    var html = '';
    // The open list items, outermost first, and per depth the run of numbers
    // the last item there belongs to (its first value and its index).
    var stack = [];
    var runs = [];
    for (var i = 0; i < lines.length; i += 1) {
      var line = lines[i];
      var classes = 'lp__line';
      var body;
      var style = '';
      if (FENCE.test(line)) {
        stack = [];
        runs = [];
        fenced = !fenced;
        classes += ' lp__line--fence';
        body = esc(line);
      } else if (fenced) {
        classes += ' lp__line--code';
        body = esc(line);
      } else {
        var item = listLine(line);
        var label = '';
        var depth = 0;
        if (item) {
          while (stack.length && stack[stack.length - 1].content > item.indent) stack.pop();
          depth = stack.length;
          stack.push(item);
          runs.length = depth + 1;
          var run = runs[depth];
          if (item.num === null) {
            runs[depth] = null;
          } else {
            if (run) run.index += 1;
            else run = runs[depth] = { first: item.num, index: 0 };
            label = numberLabel(depth, run.first + run.index, NUMBER.exec(line)[3]);
          }
          classes += ' lp__line--list lp__line--d' + (depth % 3);
          style = ' style="--depth:' + depth + '"';
        } else if (line.trim() !== '' && stack.length && columnOf(/^\s*/.exec(line)[0]) < stack[0].content) {
          stack = [];
          runs = [];
        }
        var parts = lineHtml(line, label);
        var opener = /lp__line--callout-(\w+)/.exec(parts[0]);
        if (opener) {
          box = opener[1];
        } else if (parts[0] === 'lp__line--quote' && box) {
          parts[0] += ' lp__line--callout lp__line--callout-' + box + ' lp__line--callout-body';
        } else {
          box = '';
        }
        classes += parts[0] ? ' ' + parts[0] : '';
        body = parts[1];
      }
      if (i === this.activeLine) {
        classes += ' lp__line--active';
        // The hint rides the caret's own empty line, never a code line.
        if (line === '' && !fenced && this.text !== '') classes += ' lp__line--hint';
      }
      html += '<span class="' + classes + '"' + style + '>' + body + '\n</span>';
    }
    this.root.innerHTML = html;
    // The root's data-placeholder shows through CSS while the text is empty.
    this.root.classList.toggle('lp--empty', this.text === '');
    if (selection) this.select(selection.start, selection.end, selection.backward);
  };

  Live.prototype.read = function () {
    var text = this.root.textContent;
    return text.charAt(text.length - 1) === '\n' ? text.slice(0, -1) : text;
  };

  Live.prototype.selection = function () {
    var sel = window.getSelection();
    if (!sel.rangeCount || !this.root.contains(sel.anchorNode)) return null;
    var range = sel.getRangeAt(0);
    var before = document.createRange();
    before.selectNodeContents(this.root);
    before.setEnd(range.startContainer, range.startOffset);
    var start = before.toString().length;
    // The focus end is where the human is extending, so the line it sits in is
    // the active one, and a re-draw must keep the anchor where it was.
    var backward = !sel.isCollapsed && sel.focusNode === range.startContainer &&
      sel.focusOffset === range.startOffset &&
      !(sel.anchorNode === range.startContainer && sel.anchorOffset === range.startOffset);
    return { start: start, end: start + range.toString().length, backward: backward };
  };

  // A position that sits exactly between two text nodes belongs to the later
  // one: the cursor after a line's trailing newline is the start of the next
  // line, and the browser would otherwise draw it, and type into it, at the
  // end of the line above.
  Live.prototype.select = function (start, end, backward) {
    var walker = document.createTreeWalker(this.root, NodeFilter.SHOW_TEXT);
    var seen = 0;
    var node;
    var last = null;
    var from = null;
    var to = null;
    while ((node = walker.nextNode())) {
      var length = node.nodeValue.length;
      if (from === null && start < seen + length) from = { node: node, offset: start - seen };
      if (to === null && end < seen + length) to = { node: node, offset: end - seen };
      last = { node: node, offset: length };
      if (from && to) break;
      seen += length;
    }
    if (!last) return;
    var open = from || last;
    var close = to || last;
    var sel = window.getSelection();
    // The anchor stays where the selection began, so Shift+Arrow and a drag
    // keep extending from the same end after a re-draw.
    if (backward) {
      sel.setBaseAndExtent(close.node, close.offset, open.node, open.offset);
    } else {
      sel.setBaseAndExtent(open.node, open.offset, close.node, close.offset);
    }
  };

  // Following a link is the page's call: the root announces it, the surface
  // that hosts the editor decides how a link opens.
  Live.prototype.requestFollow = function (raw) {
    this.root.dispatchEvent(new CustomEvent('live:follow', { bubbles: true, detail: { link: raw } }));
  };

  Live.prototype.sync = function () {
    var tail = this.root.getAttribute('data-live-final-newline') === 'none' ? '' : '\n';
    this.store.value = this.text + tail;
    this.store.dispatchEvent(new Event('input', { bubbles: true }));
  };

  // Every change goes through here: record it, write it, draw it.
  Live.prototype.set = function (text, selection) {
    if (text !== this.text) {
      this.text = text;
      var now = Date.now();
      var top = this.history[this.position];
      var entry = { text: text, start: selection.start, end: selection.end, at: now };
      // Typing in a burst is one undo step.
      if (this.position > 0 && now - top.at < 600 && this.position === this.history.length - 1) {
        this.history[this.position] = entry;
      } else {
        this.history = this.history.slice(0, this.position + 1);
        this.history.push(entry);
        this.position = this.history.length - 1;
      }
      this.sync();
    }
    this.render(selection);
  };

  Live.prototype.travel = function (step) {
    var next = this.position + step;
    if (next < 0 || next >= this.history.length) return;
    this.position = next;
    var state = this.history[next];
    this.text = state.text;
    this.sync();
    this.render({ start: state.start, end: state.end });
  };

  Live.prototype.replace = function (start, end, insert, caretAt) {
    var text = this.text.slice(0, start) + insert + this.text.slice(end);
    var at = caretAt === undefined ? start + insert.length : caretAt;
    this.set(text, { start: at, end: at });
  };

  // A click on a drawn box flips `[ ]` and `[x]` in the text. The mousedown is
  // held back so the caret stays put and the line stays rendered; the active
  // line shows raw markdown, so a click there is an ordinary caret click.
  Live.prototype.toggleTask = function (event) {
    var box = event.target.closest ? event.target.closest('.lp__task') : null;
    if (!box || !this.root.contains(box) || box.closest('.lp__line--active')) return false;
    event.preventDefault();
    var before = document.createRange();
    before.selectNodeContents(this.root);
    before.setEnd(box, 0);
    var at = before.toString().length + 1;
    var sel = this.selection();
    // Its own undo step, never folded into a burst of typing before it.
    this.history[this.position].at = 0;
    this.replace(at, at + 1, this.text.charAt(at) === ' ' ? 'x' : ' ', sel ? sel.start : at);
    return true;
  };

  Live.prototype.lineBounds = function (offset) {
    var from = this.text.lastIndexOf('\n', offset - 1) + 1;
    var to = this.text.indexOf('\n', offset);
    return { from: from, to: to === -1 ? this.text.length : to };
  };

  Live.prototype.bind = function () {
    var self = this;
    var root = this.root;

    root.addEventListener('compositionstart', function () { self.composing = true; });
    root.addEventListener('compositionend', function () {
      self.composing = false;
      self.changed();
    });

    root.addEventListener('beforeinput', function (event) {
      if (event.inputType === 'historyUndo') {
        event.preventDefault();
        self.travel(-1);
      } else if (event.inputType === 'historyRedo') {
        event.preventDefault();
        self.travel(1);
      }
    });

    root.addEventListener('input', function (event) {
      if (self.composing || event.isComposing) return;
      self.changed();
      self.menuInput(event);
    });

    // Leaving the editor shuts the menu; a click on the menu itself does not
    // blur, its mousedown is held back.
    root.addEventListener('blur', function () {
      self.escaped = false;
      self.menuClose();
    });

    // A drag is left alone until the button is up: a re-draw under the pointer
    // would cut the selection short.
    root.addEventListener('mousedown', function (event) {
      if (self.toggleTask(event)) return;
      if ((event.metaKey || event.ctrlKey) && event.button === 0) {
        var wiki = event.target.closest ? event.target.closest('.lp__wiki--follow') : null;
        if (wiki && self.root.contains(wiki)) {
          event.preventDefault();
          self.requestFollow(wiki.getAttribute('data-link') || '');
          return;
        }
      }
      self.dragging = true;
    });

    root.addEventListener('keyup', function (event) {
      if (!event.metaKey && !event.ctrlKey) root.classList.remove('lp--mod');
    });
    root.addEventListener('blur', function () { root.classList.remove('lp--mod'); });

    root.addEventListener('keydown', function (event) {
      if (self.menu && !self.composing && !event.isComposing && self.menuKey(event)) return;
      var meta = event.metaKey || event.ctrlKey;
      var key = event.key.toLowerCase();
      if (meta) root.classList.add('lp--mod');
      // Escape arms the next key: Tab then leaves the editor instead of
      // indenting. A bare modifier neither arms nor disarms.
      var armed = false;
      var typing = !self.composing && !event.isComposing;
      if (!/^(Shift|Control|Alt|Meta)$/.test(event.key)) {
        armed = self.escaped;
        self.escaped = false;
      }
      if (event.key === 'Escape' && typing) {
        self.escaped = true;
        return;
      }
      if (event.key === 'Tab' && typing && !meta && !event.altKey) {
        if (!armed) self.tab(event);
        return;
      }
      if (event.key === 'Backspace' && typing && !meta && !event.altKey && !event.shiftKey) {
        self.backspace(event);
        return;
      }
      if (meta && key === 'enter') {
        var at = self.selection();
        var raw = at ? wikilinkAt(self.text, at.end) : null;
        if (raw !== null) {
          event.preventDefault();
          self.requestFollow(raw);
        }
        return;
      }
      if (meta && key === 'z') {
        event.preventDefault();
        self.travel(event.shiftKey ? 1 : -1);
        return;
      }
      if (meta && key === 'y') {
        event.preventDefault();
        self.travel(1);
        return;
      }
      if (meta && (key === 'b' || key === 'i')) {
        event.preventDefault();
        self.wrap(key === 'b' ? '**' : '*');
        return;
      }
      if (event.key === 'Enter' && !event.shiftKey && !meta) self.enter(event);
    });
  };

  // The caret moved: draw the line it is in with its markers.
  Live.prototype.follow = function () {
    if (document.activeElement !== this.root || this.composing || this.dragging) return;
    var selection = this.selection();
    if (!selection) return;
    var caret = selection.backward ? selection.start : selection.end;
    if (this.lineIndex(caret) !== this.activeLine) this.render(selection);
  };

  // The editor's text after a native edit.
  Live.prototype.changed = function () {
    var selection = this.selection() || { start: this.text.length, end: this.text.length };
    this.set(this.read(), selection);
  };

  // Bold or italic around the selection; around the cursor when it is bare.
  Live.prototype.wrap = function (mark) {
    var selection = this.selection();
    if (!selection) return;
    var chosen = this.text.slice(selection.start, selection.end);
    var text = this.text.slice(0, selection.start) + mark + chosen + mark + this.text.slice(selection.end);
    this.set(text, { start: selection.start + mark.length, end: selection.end + mark.length });
  };

  // Return starts a new line. In a list it carries the list on, and on an
  // empty item it ends the list (a nested one outdents). The editor writes the
  // newline itself rather than leave it to the browser, so it lands the same
  // way everywhere.
  Live.prototype.enter = function (event) {
    var selection = this.selection();
    if (!selection) return;
    event.preventDefault();
    if (selection.start === selection.end) {
      var result = enterList(this.text, selection.start);
      if (result) {
        this.commit(result, selection, false);
        return;
      }
    }
    this.replace(selection.start, selection.end, '\n');
  };

  // MARK: - List indent

  var BLANK = /^\s*$/;
  var TAB_WIDTH = 4;

  // One list change as its own undo step, caret kept.
  Live.prototype.commit = function (result, selection, step) {
    if (step !== false) this.history[this.position].at = 0;
    this.set(result.text, { start: result.start, end: result.end, backward: selection.backward });
    this.history[this.position].at = 0;
  };

  // Tab and Shift+Tab: indent or outdent, and never leave the editor.
  Live.prototype.tab = function (event) {
    var selection = this.selection();
    if (!selection) return;
    event.preventDefault();
    var result = indentKey(this.text, selection.start, selection.end, event.shiftKey ? -1 : 1);
    if (result) this.commit(result, selection);
  };

  // Backspace at an item's content column outdents or removes the marker.
  Live.prototype.backspace = function (event) {
    var selection = this.selection();
    if (!selection || selection.start !== selection.end) return;
    var result = backspaceList(this.text, selection.start);
    if (!result) return;
    event.preventDefault();
    this.commit(result, selection);
  };

  // The width a run of leading whitespace draws at: a tab runs to the next
  // multiple of four.
  function indentWidth(ws) {
    var width = 0;
    for (var i = 0; i < ws.length; i += 1) {
      width = ws.charAt(i) === '\t' ? width + TAB_WIDTH - (width % TAB_WIDTH) : width + 1;
    }
    return width;
  }

  function lineIndent(line) {
    return indentWidth(/^[ \t]*/.exec(line)[0]);
  }

  // A list line's parts, or null. `content` is the column its text starts at,
  // which is where a child begins.
  function listParts(line) {
    var m = BULLET.exec(line);
    if (m) {
      var bullet = indentWidth(m[1]);
      return { kind: 'bullet', indent: bullet, lead: m[1].length, content: bullet + m[2].length + m[3].length, body: m[5], box: !!m[4], marker: m[2], gap: m[3] };
    }
    m = NUMBER.exec(line);
    if (m) {
      var number = indentWidth(m[1]);
      return { kind: 'number', indent: number, lead: m[1].length, content: number + m[2].length + m[3].length + m[4].length, body: m[5], number: Number(m[2]), digits: m[2].length, sep: m[3], gap: m[4] };
    }
    return null;
  }

  // Which lines sit in a fenced block, fence lines included.
  function fenceMask(lines) {
    var mask = [];
    var open = false;
    for (var i = 0; i < lines.length; i += 1) {
      if (FENCE.test(lines[i])) {
        open = !open;
        mask.push(true);
      } else {
        mask.push(open);
      }
    }
    return mask;
  }

  // The nearest list line above at the same indent, or -1.
  function previousSibling(lines, fenced, at, indent) {
    for (var i = at - 1; i >= 0; i -= 1) {
      if (fenced[i]) return -1;
      if (BLANK.test(lines[i])) continue;
      var here = lineIndent(lines[i]);
      if (here > indent) continue;
      return here === indent && listParts(lines[i]) ? i : -1;
    }
    return -1;
  }

  // The nearest list line above at a lesser indent, or -1.
  function parentLine(lines, fenced, at, indent) {
    for (var i = at - 1; i >= 0; i -= 1) {
      if (fenced[i]) return -1;
      if (BLANK.test(lines[i])) continue;
      if (lineIndent(lines[i]) < indent) return listParts(lines[i]) ? i : -1;
    }
    return -1;
  }

  // The last line of an item's children: the following lines indented deeper.
  function childrenEnd(lines, fenced, at) {
    var base = lineIndent(lines[at]);
    var j = at + 1;
    while (j < lines.length && !BLANK.test(lines[j]) && !fenced[j] && lineIndent(lines[j]) > base) j += 1;
    return j - 1;
  }

  // A cursor's column on a line whose first `before` characters became `after`.
  function shiftCursors(cursors, line, before, after) {
    for (var i = 0; i < cursors.length; i += 1) {
      if (cursors[i].l !== line) continue;
      cursors[i].c = cursors[i].c >= before ? cursors[i].c + after - before : Math.min(cursors[i].c, after);
    }
  }

  // Lines from..to take `delta` columns of indent (leading whitespace written
  // as spaces); blank lines stay.
  function moveLines(ctx, from, to, delta) {
    for (var i = from; i <= to; i += 1) {
      if (BLANK.test(ctx.lines[i])) continue;
      var ws = /^[ \t]*/.exec(ctx.lines[i])[0];
      var width = Math.max(0, indentWidth(ws) + delta);
      var pad = '';
      for (var k = 0; k < width; k += 1) pad += ' ';
      ctx.lines[i] = pad + ctx.lines[i].slice(ws.length);
      shiftCursors(ctx.cursors, i, ws.length, pad.length);
    }
  }

  // The consecutive numbered siblings at one indent around a line, children
  // between them allowed, renumbered from the first item's number (1 when the
  // first item is one the edit just moved).
  function renumberRun(ctx, at, fresh) {
    var lines = ctx.lines;
    var item = listParts(lines[at]);
    if (!item || item.kind !== 'number' || ctx.fenced[at]) return;
    var run = [at];
    var i;
    for (i = at - 1; i >= 0; i -= 1) {
      if (BLANK.test(lines[i]) || ctx.fenced[i]) break;
      var above = lineIndent(lines[i]);
      if (above > item.indent) continue;
      var up = listParts(lines[i]);
      if (above < item.indent || !up || up.kind !== 'number') break;
      run.unshift(i);
    }
    for (i = at + 1; i < lines.length; i += 1) {
      if (BLANK.test(lines[i]) || ctx.fenced[i]) break;
      var below = lineIndent(lines[i]);
      if (below > item.indent) continue;
      var down = listParts(lines[i]);
      if (below < item.indent || !down || down.kind !== 'number') break;
      run.push(i);
    }
    var first = listParts(lines[run[0]]);
    var next = fresh[run[0]] ? 1 : first.number;
    for (i = 0; i < run.length; i += 1) {
      var part = listParts(lines[run[i]]);
      var digits = String(next + i);
      if (part.number !== next + i || part.digits !== digits.length) {
        var lead = lines[run[i]].slice(0, part.lead);
        lines[run[i]] = lead + digits + lines[run[i]].slice(part.lead + part.digits);
        shiftCursors(ctx.cursors, run[i], part.lead + part.digits, part.lead + digits.length);
      }
    }
  }

  // Runs `change` over the lines of a text with the selection as line and
  // column pairs, renumbers what it names, and answers the new text and
  // selection, or null when nothing moved.
  function listEdit(text, start, end, change) {
    var lines = text.split('\n');
    var starts = [];
    var offset = 0;
    for (var i = 0; i < lines.length; i += 1) {
      starts.push(offset);
      offset += lines[i].length + 1;
    }
    function at(off) {
      var l = lines.length - 1;
      while (starts[l] > off) l -= 1;
      return { l: l, c: off - starts[l] };
    }
    var ctx = { lines: lines, fenced: fenceMask(lines), cursors: [at(start), at(end)] };
    ctx.a = ctx.cursors[0].l;
    ctx.b = ctx.cursors[1].l;
    if (ctx.b > ctx.a && ctx.cursors[1].c === 0) ctx.b -= 1;
    var plan = change(ctx);
    if (!plan) return null;
    for (var k = 0; k < plan.anchors.length; k += 1) {
      var anchor = plan.anchors[k];
      if (anchor >= 0 && anchor < ctx.lines.length) renumberRun(ctx, anchor, plan.fresh || {});
    }
    var out = ctx.lines.join('\n');
    if (out === text) return null;
    var base = [];
    var sum = 0;
    for (var j = 0; j < ctx.lines.length; j += 1) {
      base.push(sum);
      sum += ctx.lines[j].length + 1;
    }
    return {
      text: out,
      start: base[ctx.cursors[0].l] + ctx.cursors[0].c,
      end: base[ctx.cursors[1].l] + ctx.cursors[1].c
    };
  }

  // Tab (direction 1) or Shift+Tab (-1) over the selection. A list item moves
  // to its previous sibling's content column, or to its parent's indent, with
  // its children; anything else, and anything in a fence, moves four spaces.
  function indentKey(text, start, end, direction) {
    return listEdit(text, start, end, function (ctx) {
      var lines = ctx.lines;
      var item = ctx.fenced[ctx.a] ? null : listParts(lines[ctx.a]);
      var i;
      if (item) {
        var delta;
        if (direction > 0) {
          var sibling = previousSibling(lines, ctx.fenced, ctx.a, item.indent);
          if (sibling < 0) return null;
          delta = listParts(lines[sibling]).content - item.indent;
        } else {
          if (item.indent === 0) return null;
          var parent = parentLine(lines, ctx.fenced, ctx.a, item.indent);
          delta = parent < 0 ? -item.indent : lineIndent(lines[parent]) - item.indent;
        }
        var last = childrenEnd(lines, ctx.fenced, ctx.b);
        var fresh = {};
        for (i = ctx.a; i <= ctx.b; i += 1) fresh[i] = true;
        var after = last + 1;
        while (after < lines.length && BLANK.test(lines[after])) after += 1;
        moveLines(ctx, ctx.a, last, delta);
        var anchors = [after, ctx.a - 1];
        for (i = ctx.a; i <= ctx.b; i += 1) anchors.push(i);
        return { anchors: anchors, fresh: fresh };
      }
      if (ctx.a === ctx.b && direction > 0) {
        var row = lines[ctx.a];
        var from = ctx.cursors[0];
        var to = ctx.cursors[1];
        var head = row.slice(0, from.c) + '    ';
        lines[ctx.a] = head + row.slice(to.c);
        ctx.cursors[0] = { l: ctx.a, c: head.length };
        ctx.cursors[1] = { l: ctx.a, c: head.length };
        return { anchors: [] };
      }
      for (i = ctx.a; i <= ctx.b; i += 1) {
        if (BLANK.test(lines[i])) continue;
        if (direction > 0) {
          lines[i] = '    ' + lines[i];
          shiftCursors(ctx.cursors, i, 0, 4);
        } else {
          var cut = lines[i].charAt(0) === '\t' ? 1 : Math.min(4, /^ */.exec(lines[i])[0].length);
          lines[i] = lines[i].slice(cut);
          shiftCursors(ctx.cursors, i, cut, 0);
        }
      }
      return { anchors: [] };
    });
  }

  // Return at the end of a list item: carry the list on, or end it. Null when
  // the line is not a list item (or sits in a fence).
  function enterList(text, caret) {
    return listEdit(text, caret, caret, function (ctx) {
      var line = ctx.lines[ctx.a];
      var item = ctx.fenced[ctx.a] ? null : listParts(line);
      if (!item || ctx.cursors[0].c !== line.length) return null;
      if (item.body.trim() === '') {
        if (item.indent > 0) return null;
        ctx.lines[ctx.a] = '';
        ctx.cursors[0] = { l: ctx.a, c: 0 };
        ctx.cursors[1] = { l: ctx.a, c: 0 };
        return { anchors: [] };
      }
      var lead = line.slice(0, item.lead);
      var made = item.kind === 'bullet'
        ? lead + item.marker + item.gap + (item.box ? '[ ] ' : '')
        : lead + (item.number + 1) + item.sep + item.gap;
      ctx.lines.splice(ctx.a + 1, 0, made);
      ctx.fenced.splice(ctx.a + 1, 0, false);
      ctx.cursors[0] = { l: ctx.a + 1, c: made.length };
      ctx.cursors[1] = { l: ctx.a + 1, c: made.length };
      return { anchors: [ctx.a + 1] };
    }) || enterOutdent(text, caret);
  }

  // True when the line holding the caret sits in a fenced block.
  function caretFenced(text, caret) {
    var row = text.slice(0, caret).split('\n').length - 1;
    return fenceMask(text.split('\n'))[row] === true;
  }

  // Return on an empty nested item moves it out a level.
  function enterOutdent(text, caret) {
    if (caretFenced(text, caret)) return null;
    var from = text.lastIndexOf('\n', caret - 1) + 1;
    var to = text.indexOf('\n', caret);
    var line = text.slice(from, to === -1 ? text.length : to);
    var item = listParts(line);
    if (!item || item.indent === 0 || item.body.trim() !== '' || caret !== from + line.length) return null;
    return indentKey(text, caret, caret, -1);
  }

  // Backspace with the caret just after an item's marker: nested, it outdents;
  // at the top level it removes the marker and the task box, keeping the text.
  function backspaceList(text, caret) {
    if (caretFenced(text, caret)) return null;
    var from = text.lastIndexOf('\n', caret - 1) + 1;
    var to = text.indexOf('\n', caret);
    var line = text.slice(from, to === -1 ? text.length : to);
    var item = listParts(line);
    if (!item || caret !== from + line.length - item.body.length) return null;
    if (item.indent > 0) return indentKey(text, caret, caret, -1);
    return listEdit(text, caret, caret, function (ctx) {
      if (ctx.fenced[ctx.a]) return null;
      ctx.lines[ctx.a] = item.body;
      ctx.cursors[0] = { l: ctx.a, c: 0 };
      ctx.cursors[1] = { l: ctx.a, c: 0 };
      return { anchors: [ctx.a + 1] };
    });
  }

  // MARK: - The slash menu
  //
  // `/` at the start of a line or after a space opens a list of block kinds
  // under the caret. What follows the `/` filters it and stays in the text as
  // ordinary characters until an item turns the line into that block.
  var SLASH_ITEMS = [
    { id: 'text', glyph: 'T', label: 'Text', hint: '', terms: ['text', 'plain', 'paragraph', 'p'] },
    { id: 'h1', glyph: 'H1', label: 'Heading 1', hint: '#', terms: ['heading 1', 'h1', 'title'] },
    { id: 'h2', glyph: 'H2', label: 'Heading 2', hint: '##', terms: ['heading 2', 'h2'] },
    { id: 'h3', glyph: 'H3', label: 'Heading 3', hint: '###', terms: ['heading 3', 'h3'] },
    { id: 'bullet', glyph: '•', label: 'Bullet list', hint: '-', terms: ['bullet list', 'bullet', 'ul', 'list', 'unordered'] },
    { id: 'number', glyph: '1.', label: 'Numbered list', hint: '1.', terms: ['numbered list', 'number', 'ol', 'list', 'ordered'] },
    { id: 'todo', glyph: '☐', label: 'To-do', hint: '[ ]', terms: ['to-do', 'todo', 'task', 'checkbox', 'check', 'checklist'] },
    { id: 'quote', glyph: '“', label: 'Quote', hint: '>', terms: ['quote', 'blockquote', 'cite'] },
    { id: 'code', glyph: '</>', label: 'Code block', hint: '```', terms: ['code block', 'code', 'fence', 'snippet'] },
    { id: 'note', glyph: 'i', label: 'Info box', hint: '[!NOTE]', terms: ['info box', 'info', 'callout', 'note'] },
    { id: 'warning', glyph: '!', label: 'Warning box', hint: '[!WARNING]', terms: ['warning box', 'warning', 'warn', 'callout', 'alert'] },
    { id: 'tip', glyph: '*', label: 'Tip box', hint: '[!TIP]', terms: ['tip box', 'tip', 'hint', 'callout'] },
    { id: 'divider', glyph: '─', label: 'Divider', hint: '---', terms: ['divider', 'hr', 'rule', 'line', 'separator'] }
  ];
  var BLOCK_PREFIX = /^(?:#{1,6} +|[-*+] +(?:\[[ xX]\] +)?|\d+[.)] +|> ?)+/;

  function slashMatches(query) {
    var q = query.toLowerCase();
    var first = [];
    var rest = [];
    for (var i = 0; i < SLASH_ITEMS.length; i += 1) {
      var terms = SLASH_ITEMS[i].terms;
      var rank = 2;
      for (var t = 0; t < terms.length; t += 1) {
        var at = terms[t].indexOf(q);
        if (at === 0) rank = 0;
        else if (at > 0 && rank > 1) rank = 1;
      }
      if (rank === 0) first.push(SLASH_ITEMS[i]);
      else if (rank === 1) rest.push(SLASH_ITEMS[i]);
    }
    return first.concat(rest);
  }

  // The editor's two menus share everything but their source. `slash` turns a
  // line into a block; `link` writes a wikilink after `[[`. `menu.kind` is the
  // one difference the open, check, draw and apply steps branch on.
  Live.prototype.menuInput = function (event) {
    if (this.menu) {
      this.menuCheck();
      return;
    }
    if (event.inputType !== 'insertText' || !event.data) return;
    var selection = this.selection();
    if (!selection || selection.start !== selection.end) return;
    var caret = selection.end;
    // `[[` opens the link menu, except inside code, where it is just text. The second `[` of a pair, or a burst of typing that carries the query
    // with it (`[[por`), opens it.
    var opener = /\[\[[^\]|#\n]*$/.exec(event.data);
    var from = opener ? caret - opener[0].length : caret - 2;
    if (this.text.slice(from, from + 2) === '[[' && !this.inCode(from)) {
      this.menuOpen('link', from);
      return;
    }
    // Typed text can arrive in one piece (a fast burst), so `/todo` counts.
    var typed = /^\s*(\/\S*)$/.exec(event.data);
    if (!typed) return;
    from = caret - typed[1].length;
    // A `/` inside a word (a path, a URL, a/b) is just a slash.
    if (from > 0 && !/\s/.test(this.text.charAt(from - 1))) return;
    this.menuOpen('slash', from);
  };

  // True when the offset sits in a fenced block or an inline code span, by the
  // same line rules `render` draws with.
  Live.prototype.inCode = function (offset) {
    var lines = this.text.split('\n');
    var index = this.lineIndex(offset);
    var fenced = false;
    for (var i = 0; i < index; i += 1) {
      if (FENCE.test(lines[i])) fenced = !fenced;
    }
    if (fenced || FENCE.test(lines[index])) return true;
    var column = offset - this.lineBounds(offset).from;
    var line = lines[index];
    var match;
    INLINE.lastIndex = 0;
    while ((match = INLINE.exec(line))) {
      if (match[1] && match.index <= column && column < match.index + match[0].length) return true;
    }
    return false;
  };

  Live.prototype.menuOpen = function (kind, from) {
    var el = document.createElement('div');
    el.className = kind === 'link' ? 'slash slash--link' : 'slash';
    el.id = kind === 'link' ? 'link-menu' : 'slash-menu';
    el.setAttribute('role', 'listbox');
    el.setAttribute('aria-label', kind === 'link' ? 'Link to' : 'Turn this line into');
    var self = this;
    // The mouse must not pull focus, or the editor blur would shut the menu
    // before the click lands.
    el.addEventListener('mousedown', function (e) { e.preventDefault(); });
    el.addEventListener('click', function (e) {
      var row = closest(e.target, '[data-index]');
      if (row && self.menu) self.menuApply(self.menu.items[Number(row.getAttribute('data-index'))]);
    });
    el.addEventListener('mousemove', function (e) {
      var row = closest(e.target, '[data-index]');
      var at = row ? Number(row.getAttribute('data-index')) : -1;
      if (at >= 0 && self.menu && self.menu.index !== at) {
        self.menu.index = at;
        self.menuDraw();
      }
    });
    document.body.appendChild(el);
    this.menu = { kind: kind, from: from, el: el, items: [], index: 0, missAt: -1, query: null, more: 0 };
    this.root.setAttribute('aria-haspopup', 'listbox');
    this.root.setAttribute('aria-controls', el.id);
    this.root.setAttribute('aria-expanded', 'true');
    if (kind === 'link') this.loadTargets();
    this.menuCheck();
  };

  // The link targets come once per editor, from the one route that joins the
  // board's names to the vault's notes. A failure is not kept: the next `[[`
  // asks again.
  Live.prototype.loadTargets = function () {
    if (this.targets || this.targetsLoading) return;
    var self = this;
    this.targetsLoading = true;
    this.targetsFailed = false;
    fetch('/notes/targets', { headers: { Accept: 'application/json' } })
      .then(function (response) {
        if (!response.ok) throw new Error('targets ' + response.status);
        return response.json();
      })
      .then(function (data) { self.targets = data; })
      .catch(function () { self.targetsFailed = true; })
      .then(function () {
        self.targetsLoading = false;
        if (self.menu && self.menu.kind === 'link') {
          self.menu.query = null;
          self.menuCheck();
        }
      });
  };

  // The text after the trigger up to the caret filters the list; a caret that
  // has left that span, or a trigger that is gone, ends it. A link query that
  // gains `]`, `|` or `#` is the human writing the link by hand.
  Live.prototype.menuCheck = function () {
    var menu = this.menu;
    if (!menu) return;
    var selection = this.selection();
    if (!selection || selection.start !== selection.end || this.composing) {
      this.menuClose();
      return;
    }
    var link = menu.kind === 'link';
    var lead = link ? '[[' : '/';
    var caret = selection.end;
    var query = this.text.slice(menu.from + lead.length, caret);
    if (caret < menu.from + lead.length || this.text.substr(menu.from, lead.length) !== lead ||
        query.indexOf('\n') >= 0 || (link && /[\]|#]/.test(query))) {
      this.menuClose();
      return;
    }
    if (query === menu.query) return;
    menu.query = query;
    menu.index = 0;
    if (link) {
      var found = this.targets ? linkMatches(query, this.targets, this.notePath()) : { items: [], more: 0 };
      menu.items = found.items;
      menu.more = found.more;
      this.menuDraw();
      return;
    }
    menu.items = slashMatches(query);
    if (!menu.items.length) {
      // A space with nothing to match is a sentence, and a couple of strays
      // after the first miss is a typo for nothing.
      if (menu.missAt < 0) menu.missAt = query.length;
      if (/\s$/.test(query) || query.length - menu.missAt >= 2) {
        this.menuClose();
        return;
      }
    } else {
      menu.missAt = -1;
    }
    this.menuDraw();
  };

  // The path of the note being edited, so the link menu can leave it out.
  Live.prototype.notePath = function () {
    return this.root.getAttribute('data-live-path') || '';
  };

  Live.prototype.menuDraw = function () {
    var menu = this.menu;
    var link = menu.kind === 'link';
    var html = '';
    var last = '';
    for (var i = 0; i < menu.items.length; i += 1) {
      var item = menu.items[i];
      var on = i === menu.index;
      if (link) {
        if (item.group !== last) {
          last = item.group;
          html += '<div class="slash__group" role="presentation">' + esc(GROUP_TITLES[item.group]) + '</div>';
        }
        html += '<div class="slash__item" role="option" id="link-opt-' + i + '" data-index="' + i +
          '" aria-selected="' + on + '" title="' + esc(item.title).replace(/"/g, '&quot;') + '">' +
          '<span class="slash__glyph" aria-hidden="true">' + esc(GROUP_GLYPHS[item.group]) + '</span>' +
          '<span class="slash__label">' + item.labelHtml + '</span>' +
          (item.hint ? '<span class="slash__hint" aria-hidden="true">' + esc(item.hint) + '</span>' : '') + '</div>';
      } else {
        html += '<div class="slash__item" role="option" id="slash-opt-' + item.id + '" data-index="' +
          i + '" aria-selected="' + on + '">' +
          '<span class="slash__glyph" aria-hidden="true">' + esc(item.glyph) + '</span>' +
          '<span class="slash__label">' + esc(item.label) + '</span>' +
          '<span class="slash__hint" aria-hidden="true">' + esc(item.hint) + '</span></div>';
      }
    }
    if (!link) {
      if (!menu.items.length) html = '<div class="slash__empty">No matching block</div>';
      menu.el.innerHTML = html;
    } else {
      var foot = '';
      if (this.targetsFailed && !this.targets) {
        html = '<div class="slash__error" role="status">Could not load link targets. Keep typing to write the link by hand.</div>';
      } else if (!menu.items.length) {
        html = '<div class="slash__empty">No note, project or label matches "' + esc(menu.query || '') + '".</div>';
      }
      if (menu.more > 0) {
        foot = '<div class="slash__foot">' + menu.more + ' more notes: type to narrow</div>';
      }
      menu.el.innerHTML = '<div class="slash__list">' + html + '</div>' + foot;
      // Nothing is drawn while the first fetch runs.
      menu.el.style.display = !this.targets && !this.targetsFailed ? 'none' : '';
    }
    var current = menu.items[menu.index];
    if (current) {
      this.root.setAttribute('aria-activedescendant',
        link ? 'link-opt-' + menu.index : 'slash-opt-' + current.id);
      var row = menu.el.querySelector('[aria-selected="true"]');
      if (row && row.scrollIntoView) row.scrollIntoView({ block: 'nearest' });
    } else {
      this.root.removeAttribute('aria-activedescendant');
    }
    this.menuPlace();
  };

  // Under the caret; above it when the page ends first. A caret on an empty
  // line has no box of its own, so the line's is borrowed.
  Live.prototype.menuPlace = function () {
    var menu = this.menu;
    var sel = window.getSelection();
    if (!sel.rangeCount) return;
    var range = sel.getRangeAt(0).cloneRange();
    var rect = range.getBoundingClientRect();
    if (!rect.height) {
      var node = range.startContainer;
      var el = node.nodeType === 1 ? node : node.parentNode;
      var box = el && el.getBoundingClientRect ? el.getBoundingClientRect() : null;
      if (box && box.height) rect = { left: box.left, top: box.top, bottom: box.bottom };
    }
    var width = menu.el.offsetWidth;
    var height = menu.el.offsetHeight;
    var left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8));
    var top = rect.bottom + 6;
    if (top + height > window.innerHeight - 8 && rect.top - height - 6 > 8) top = rect.top - height - 6;
    menu.el.style.left = left + 'px';
    menu.el.style.top = top + 'px';
  };

  Live.prototype.menuClose = function () {
    var menu = this.menu;
    if (!menu) return;
    this.menu = null;
    if (menu.el.parentNode) menu.el.parentNode.removeChild(menu.el);
    this.root.removeAttribute('aria-expanded');
    this.root.removeAttribute('aria-activedescendant');
    this.root.removeAttribute('aria-controls');
    this.root.removeAttribute('aria-haspopup');
  };

  // The menu owns the arrows, Enter, Tab and Escape while it is open. True
  // when the key was taken.
  Live.prototype.menuKey = function (event) {
    var menu = this.menu;
    var key = event.key;
    if (event.metaKey || event.ctrlKey || event.altKey) return false;
    if (key === 'Escape') {
      event.preventDefault();
      this.menuClose();
      return true;
    }
    if (key === 'ArrowDown' || key === 'ArrowUp') {
      if (!menu.items.length) return false;
      event.preventDefault();
      var count = menu.items.length;
      menu.index = (menu.index + (key === 'ArrowDown' ? 1 : count - 1)) % count;
      this.menuDraw();
      return true;
    }
    if (key === 'Enter' || key === 'Tab') {
      if (!menu.items.length) {
        this.menuClose();
        return false;
      }
      event.preventDefault();
      this.menuApply(menu.items[menu.index]);
      return true;
    }
    return false;
  };

  // The line without its `/query`, and without a block prefix it already had,
  // becomes the item's block. One `replace`, so one undo step.
  Live.prototype.menuApply = function (item) {
    var menu = this.menu;
    var selection = this.selection();
    this.menuClose();
    if (!menu || !item || !selection) return;
    if (menu.kind === 'link') {
      this.applyLink(menu, item, selection.end);
      return;
    }
    var bounds = this.lineBounds(menu.from);
    var line = this.text.slice(bounds.from, bounds.to);
    var at = menu.from - bounds.from;
    var rest = line.slice(0, at) + line.slice(Math.max(selection.end, menu.from + 1) - bounds.from);
    var indent = /^\s*/.exec(rest)[0];
    rest = rest.slice(indent.length).replace(BLOCK_PREFIX, '').trim();
    var out;
    var caret;
    var id = item.id;
    if (id === 'text') {
      out = rest;
    } else if (id === 'h1' || id === 'h2' || id === 'h3') {
      out = '###'.slice(0, Number(id.charAt(1))) + ' ' + rest;
    } else if (id === 'bullet') {
      out = indent + '- ' + rest;
    } else if (id === 'number') {
      var number = 1;
      if (bounds.from > 0) {
        var above = this.text.slice(this.text.lastIndexOf('\n', bounds.from - 2) + 1, bounds.from - 1);
        var m = NUMBER.exec(above);
        if (m && m[1] === indent) number = Number(m[2]) + 1;
      }
      out = indent + number + '. ' + rest;
    } else if (id === 'todo') {
      out = indent + '- [ ] ' + rest;
    } else if (id === 'quote') {
      out = '> ' + rest;
    } else if (id === 'code') {
      out = '```\n' + rest;
      caret = bounds.from + out.length;
      out += '\n```';
    } else if (id === 'note' || id === 'warning' || id === 'tip') {
      out = '> [!' + id.toUpperCase() + ']\n> ' + rest;
    } else {
      out = (rest ? rest + '\n' : '') + '---\n';
    }
    if (caret === undefined) caret = bounds.from + out.length;
    // A step of its own, so one undo gives back the line as it was typed.
    this.history[this.position].at = 0;
    this.replace(bounds.from, bounds.to, out, caret);
  };

  // `[[` plus the query becomes the link; a `]]` already after the caret is
  // taken in, not doubled. One `replace`, so one undo step, and the caret lands
  // after the closing brackets.
  Live.prototype.applyLink = function (menu, item, caret) {
    var link = '[[' + item.insert + ']]';
    var end = this.text.slice(caret, caret + 2) === ']]' ? caret + 2 : caret;
    this.history[this.position].at = 0;
    this.replace(menu.from, end, link, menu.from + link.length);
  };

  // MARK: - The link menu's list

  var GROUP_TITLES = { project: 'Projects', label: 'Labels', note: 'Notes' };
  var GROUP_GLYPHS = { project: 'P', label: '#', note: '[[' };
  var NOTE_CAP = 50;

  // 0 starts with the query, 1 has a word that does, 2 contains it, -1 misses.
  function matchTier(name, q) {
    if (!q) return 0;
    var at = name.toLowerCase().indexOf(q);
    if (at === 0) return 0;
    if (at < 0) return -1;
    return /[\s_\-\/.:]/.test(name.charAt(at - 1)) ? 1 : 2;
  }

  // The name with the matched run in bold, escaped piece by piece.
  function matchHtml(name, q) {
    var at = q ? name.toLowerCase().indexOf(q) : -1;
    if (at < 0) return esc(name);
    return esc(name.slice(0, at)) + '<b class="slash__match">' + esc(name.slice(at, at + q.length)) + '</b>' +
      esc(name.slice(at + q.length));
  }

  function byName(a, b) {
    var x = a.name.toLowerCase();
    var y = b.name.toLowerCase();
    return x < y ? -1 : x > y ? 1 : a.name < b.name ? -1 : a.name > b.name ? 1 : 0;
  }

  // Projects, labels, then notes, each ranked: prefix, word prefix, substring,
  // alphabetical in a tier; a note matched only by its folder comes last. A
  // `project:` or `label:` query narrows to that group.
  function linkMatches(query, data, self) {
    var q = query.toLowerCase();
    var only = '';
    var typed = /^(project|label):/.exec(q);
    if (typed) {
      only = typed[1];
      q = q.slice(typed[0].length).replace(/^\s+/, '');
    }
    var groups = [];
    function rank(rows, group) {
      var out = [];
      for (var i = 0; i < rows.length; i += 1) {
        var row = rows[i];
        var tier = matchTier(row.name, q);
        if (tier < 0 && row.folder && q && row.folder.toLowerCase().indexOf(q) >= 0) tier = 3;
        if (tier < 0) continue;
        row.tier = tier;
        row.group = group;
        out.push(row);
      }
      out.sort(function (a, b) { return a.tier - b.tier || byName(a, b); });
      return out;
    }
    function build(row) {
      return {
        group: row.group,
        insert: row.insert,
        title: row.title,
        hint: row.hint,
        labelHtml: matchHtml(row.name, row.tier === 3 ? '' : q)
      };
    }
    if (!only || only === 'project') {
      groups.push(rank((data.projects || []).map(function (p) {
        return { name: p.name, insert: 'project:' + p.name, title: p.name, hint: '' };
      }), 'project'));
    }
    if (!only || only === 'label') {
      groups.push(rank((data.labels || []).map(function (l) {
        return { name: l.name, insert: 'label:' + l.name, title: l.project ? l.name + ' in ' + l.project : l.name, hint: l.project || '' };
      }), 'label'));
    }
    var more = 0;
    if (!only) {
      var notes = (data.notes || []).filter(function (n) { return n.path !== self; }).map(function (n) {
        var parts = n.path.split('/');
        var base = parts.pop().replace(/\.md$/i, '');
        return {
          name: base, insert: n.target, title: n.path,
          hint: parts.length ? parts[parts.length - 1] : '',
          folder: parts.join('/')
        };
      });
      var ranked = rank(notes, 'note');
      more = Math.max(0, ranked.length - NOTE_CAP);
      groups.push(ranked.slice(0, NOTE_CAP));
    }
    var items = [];
    for (var g = 0; g < groups.length; g += 1) {
      for (var r = 0; r < groups[g].length; r += 1) items.push(build(groups[g][r]));
    }
    return { items: items, more: more };
  }

  // One listener for the page, aimed at the editor now showing: a listener per
  // editor would outlive each swap and keep its detached root alive.
  function activeLive() {
    var at = document.activeElement;
    for (var i = 0; i < lives.length; i += 1) {
      if (lives[i].root === at || lives[i].root.contains(at)) return lives[i];
    }
    return null;
  }

  document.addEventListener('selectionchange', function () {
    var live = activeLive();
    if (live) {
      live.follow();
      live.menuCheck();
    }
  });
  document.addEventListener('mouseup', function () {
    for (var i = 0; i < lives.length; i += 1) {
      if (lives[i].dragging) {
        lives[i].dragging = false;
        lives[i].follow();
      }
    }
  });

  // The notes face opens a followed link in its own pane.
  document.addEventListener('live:follow', function (event) {
    if (event.target.closest && event.target.closest('#notes-root')) {
      followWikilink(event.detail.link);
    }
  });

  // Enter in a note's title asks for the caret at the start of the body once the
  // rename swap has run. One-shot: every other path clears it (TAN-869).
  var enterToBody = false;

  document.addEventListener('keydown', function (event) {
    if (event.key !== 'Enter' || event.isComposing) return;
    var field = event.target && event.target.closest ? event.target.closest('.page-title') : null;
    if (field && field.closest('#notes-root') && !field.classList.contains('page-title--static')) {
      enterToBody = true;
    }
  });

  document.addEventListener('htmx:responseError', function () { enterToBody = false; });

  function focusBody() {
    if (!enterToBody) return;
    enterToBody = false;
    var root = document.querySelector('#notes-root [data-live][data-live-ready]');
    if (!root) return;
    var instance = null;
    for (var i = 0; i < lives.length; i += 1) if (lives[i].root === root) instance = lives[i];
    root.focus();
    if (instance) instance.select(0, 0);
  }

  function initLive() {
    var roots = document.querySelectorAll('[data-live][data-live-store]');
    for (var i = 0; i < roots.length; i += 1) {
      var root = roots[i];
      var store = document.getElementById(root.getAttribute('data-live-store'));
      if (!store || root.hasAttribute('data-live-ready')) continue;
      root.setAttribute('data-live-ready', '');
      root.contentEditable = 'plaintext-only';
      // A browser without plaintext editing gets the plain textarea instead.
      if (root.contentEditable !== 'plaintext-only') {
        root.hidden = true;
        store.hidden = false;
        continue;
      }
      root.classList.add('lp');
      root.setAttribute('spellcheck', 'true');
      root.setAttribute('role', 'textbox');
      root.setAttribute('aria-multiline', 'true');
      root.setAttribute('aria-label', root.getAttribute('aria-label') || 'Note');
      var desc = document.createElement('span');
      desc.className = 'lp__sr';
      desc.id = 'lp-desc-' + lives.length + '-' + Math.floor(Math.random() * 1e6);
      desc.textContent = SLASH_DESCRIPTION;
      root.parentNode.insertBefore(desc, root);
      root.setAttribute('aria-describedby', desc.id);
      root.setAttribute('aria-description', 'Tab indents. Press Escape, then Tab, to leave the editor.');
      root.setAttribute('data-slash-hint', SLASH_HINT);
      lives.push(new Live(root, store));
    }
  }

  // A page of its own (a note just created, a rename) names the title field to
  // put the cursor in.
  function focusTitle() {
    var title = document.querySelector('[data-autofocus]');
    if (title) {
      title.focus();
      title.select();
    }
  }

  // MARK: - Collapsing folders in the notes tree
  //
  // The tree is a flat list with each row's depth, so a folder hides the rows
  // below it that sit deeper, up to the next row at its own depth or shallower.
  // Which folders are shut is kept per browser in the `tree` cookie
  // (`{"default":"shut"|"open","except":[paths],"open":note}`), and the server
  // draws it on the first paint and on every swap with the same rules
  // (`serve/ui_state.rs`): this script only keeps it up to date and applies a
  // click at once, so its redraw changes nothing the server drew (TAN-880).
  var TREE = 'tree';
  var LEGACY_COLLAPSED = 'tandem.collapsed';
  var LEGACY_CHOSEN = 'tandem.collapsedSet';

  function folderPaths() {
    var rows = document.querySelectorAll('.tree__row[data-kind="folder"]');
    var paths = [];
    for (var i = 0; i < rows.length; i += 1) paths.push(rows[i].getAttribute('data-path'));
    return paths;
  }

  function openNotePath() {
    var row = document.querySelector('.tree__row[data-kind="note"][aria-selected="true"]');
    return row ? row.getAttribute('data-path') : null;
  }

  // The folder paths above a note path: 'a/b/c.md' gives 'a' and 'a/b'.
  function ancestorsOf(path) {
    var parts = (path || '').split('/');
    var found = [];
    for (var i = 1; i < parts.length; i += 1) found.push(parts.slice(0, i).join('/'));
    return found;
  }

  function withoutAncestors(shut, path) {
    var above = ancestorsOf(path);
    return shut.filter(function (entry) { return above.indexOf(entry) < 0; });
  }

  function storedTree() {
    var state = window.tandem.uiState.read(TREE);
    if (!state || (state.default !== 'shut' && state.default !== 'open')) return null;
    if (!Array.isArray(state.except)) return null;
    return state;
  }

  // The old localStorage folds carry over once: the shut list becomes an
  // `open` default with that list as the exceptions, which draws exactly what
  // the old code drew. The keys go afterwards.
  function migrateLegacyTree() {
    if (window.tandem.uiState.has(TREE)) return;
    try {
      var chosen = window.localStorage.getItem(LEGACY_CHOSEN) !== null;
      var raw = window.localStorage.getItem(LEGACY_COLLAPSED);
      if (!chosen && raw === null) return;
      var shut = [];
      try {
        shut = JSON.parse(raw || '[]');
      } catch (error) {
        shut = [];
      }
      if (!Array.isArray(shut)) shut = [];
      window.tandem.uiState.write(TREE, { default: 'open', except: shut, open: openNotePath() });
      window.localStorage.removeItem(LEGACY_COLLAPSED);
      window.localStorage.removeItem(LEGACY_CHOSEN);
    } catch (error) {
      // Private mode: the folds start from the first-visit layout.
    }
  }

  // The shut folders to draw, the same rule `ui_state::shut_folders` applies:
  // no stored state shuts every folder; a stored one is kept exactly. Either
  // way a note that is not the last one revealed opens its ancestors.
  function shutFolders(state, folders, open) {
    var shut;
    if (!state) {
      shut = folders.slice();
    } else {
      shut = folders.filter(function (path) {
        return (state.except.indexOf(path) >= 0) === (state.default === 'open');
      });
    }
    if (!state || (open !== null && state.open !== open)) shut = withoutAncestors(shut, open);
    return shut;
  }

  // Store a shut list in the smaller of its two forms, trimmed to fit a cookie.
  function saveShut(shut, folders, open) {
    var opens = folders.filter(function (path) { return shut.indexOf(path) < 0; });
    var state = opens.length <= shut.length
      ? { default: 'shut', except: opens, open: open }
      : { default: 'open', except: shut.slice(), open: open };
    while (!window.tandem.uiState.write(TREE, state) && state.except.length > 0) {
      state.except.length = Math.floor(state.except.length / 2);
    }
  }

  function currentShut() {
    return shutFolders(storedTree(), folderPaths(), openNotePath());
  }

  // A note that has just opened shows its ancestors: the stored layout drops
  // them. The server drew that already; this keeps the cookie in step so the
  // same note drawn again keeps the layout exactly.
  function recordOpenNote() {
    var state = storedTree();
    var open = openNotePath();
    if (!state || open === null || state.open === open) return;
    saveShut(shutFolders(state, folderPaths(), open), folderPaths(), open);
  }

  function applyTree() {
    var items = document.querySelectorAll('.tree > li');
    var shut = currentShut();
    var hideBelow = null;
    for (var i = 0; i < items.length; i += 1) {
      var row = items[i].querySelector('.tree__row');
      if (!row) continue;
      var level = Number(row.getAttribute('aria-level'));
      var folder = row.getAttribute('data-kind') === 'folder';
      var closed = folder && shut.indexOf(row.getAttribute('data-path')) >= 0;
      if (row.hasAttribute('aria-expanded')) {
        row.setAttribute('aria-expanded', closed ? 'false' : 'true');
      }
      if (hideBelow !== null) {
        if (level > hideBelow) {
          items[i].hidden = true;
          continue;
        }
        hideBelow = null;
      }
      items[i].hidden = false;
      if (closed) hideBelow = level;
    }
  }

  // Runs on load and on every swap of the tree, before the swap is painted.
  function setUpTree() {
    migrateLegacyTree();
    recordOpenNote();
    applyTree();
  }

  document.addEventListener('click', function (event) {
    var tool = closest(event.target, '[data-tree-action]');
    if (tool) {
      var all = folderPaths();
      saveShut(tool.getAttribute('data-tree-action') === 'collapse' ? all : [], all, openNotePath());
      applyTree();
      return;
    }
    if (closest(event.target, '.row-menu')) return;
    var row = closest(event.target, '.tree__row[data-kind="folder"]');
    if (!row || !row.hasAttribute('aria-expanded')) return;
    var path = row.getAttribute('data-path');
    var shut = currentShut();
    var at = shut.indexOf(path);
    if (at >= 0) shut.splice(at, 1);
    else shut.push(path);
    saveShut(shut, folderPaths(), openNotePath());
    applyTree();
  });

  // The graph asks for a folder's row: open its ancestors, select the row.
  // The folder itself is not toggled.
  document.addEventListener('tandem:reveal-path', function (event) {
    var path = event.detail && event.detail.path;
    if (!path) return;
    var rows = document.querySelectorAll('.tree__row');
    var chosen = null;
    for (var i = 0; i < rows.length; i += 1) {
      var match = rows[i].getAttribute('data-path') === path;
      rows[i].setAttribute('aria-selected', match ? 'true' : 'false');
      if (match) chosen = rows[i];
    }
    var shut = currentShut();
    var next = withoutAncestors(shut, path);
    if (next.length !== shut.length) saveShut(next, folderPaths(), openNotePath());
    applyTree();
    if (chosen) chosen.scrollIntoView({ block: 'nearest' });
  });

  function setUpNote() {
    // An editor of a note the page has left saves the text it holds and goes.
    if (pendingForm && !document.contains(pendingForm)) flushNote();
    lives = lives.filter(function (instance) {
      if (document.contains(instance.root)) return true;
      instance.menuClose();
      return false;
    });
    // A menu left by an editor the swap took away.
    var stray = document.querySelectorAll('.slash');
    for (var i = 0; i < stray.length; i += 1) {
      var owned = lives.some(function (instance) { return instance.menu && instance.menu.el === stray[i]; });
      if (!owned) stray[i].remove();
    }
    initLive();
    focusTitle();
    focusBody();
  }

  document.addEventListener('DOMContentLoaded', setUpTree);
  document.addEventListener('htmx:afterSwap', setUpTree);
  document.addEventListener('DOMContentLoaded', setUpNote);
  document.addEventListener('htmx:afterSwap', setUpNote);
})();

// MARK: - The Relations pane beside a note
//
// The open pane and each section's open state live in the `relations` cookie
// (TAN-886), which the server reads to draw the first paint; this script only
// writes it and applies the toggles. Wide windows apply the stored (or default)
// choice; narrow ones start with the drawer closed and open it only on the
// toggle, and that choice is not kept.
(function () {
  var LEGACY_OPEN = 'tandem.relations.open';
  var LEGACY_SECTIONS = 'tandem.relations.sections';
  var narrow = window.matchMedia ? window.matchMedia('(max-width: 1100px)') : null;
  var drawerOpen = false;

  function isNarrow() {
    return !!(narrow && narrow.matches);
  }

  // The stored state: open unless stored shut; a damaged value is the default.
  function stored() {
    var state = window.tandem.uiState.read('relations');
    if (!state || typeof state !== 'object') return { open: true, sections: {} };
    var sections = state.sections && typeof state.sections === 'object' ? state.sections : {};
    return { open: state.open !== false, sections: sections };
  }

  function saveStored(state) {
    window.tandem.uiState.write('relations', state);
  }

  // The old localStorage choices carry over once, booleans only, then the keys go.
  function migrateLegacy() {
    if (window.tandem.uiState.has('relations')) return;
    try {
      var openRaw = window.localStorage.getItem(LEGACY_OPEN);
      var sectionsRaw = window.localStorage.getItem(LEGACY_SECTIONS);
      if (openRaw === null && sectionsRaw === null) return;
      var parsed = {};
      try {
        parsed = JSON.parse(sectionsRaw || '{}') || {};
      } catch (error) {
        parsed = {};
      }
      var sections = {};
      Object.keys(parsed).forEach(function (kind) {
        if (typeof parsed[kind] === 'boolean') sections[kind] = parsed[kind];
      });
      saveStored({ open: openRaw !== '0', sections: sections });
      window.localStorage.removeItem(LEGACY_OPEN);
      window.localStorage.removeItem(LEGACY_SECTIONS);
    } catch (error) {
      // Private mode or no storage: the choices start from the default.
    }
  }

  function paneOpen() {
    return isNarrow() ? drawerOpen : stored().open;
  }

  // Applies the open pane and the toggles' state. The server has already drawn
  // the sections and the wide pane's shut state, so nothing here reopens them.
  function applyRelations() {
    var pane = document.getElementById('relations');
    if (!pane) return;
    var open = paneOpen();
    pane.classList.toggle('relations--open', open);
    pane.classList.toggle('relations--shut', !isNarrow() && !open);
    var toggles = document.querySelectorAll('[data-relations-toggle]');
    for (var i = 0; i < toggles.length; i += 1) toggles[i].setAttribute('aria-expanded', open ? 'true' : 'false');
  }

  function setPane(open) {
    if (isNarrow()) {
      drawerOpen = open;
    } else {
      var state = stored();
      state.open = open;
      saveStored(state);
    }
    applyRelations();
    if (!open) {
      var toggle = document.querySelector('[data-relations-toggle]');
      if (toggle) toggle.focus();
    }
  }

  document.addEventListener('click', function (event) {
    var target = event.target;
    if (!target.closest) return;
    if (target.closest('[data-relations-toggle]')) setPane(!paneOpen());
    else if (target.closest('[data-relations-close]')) setPane(false);
  });

  document.addEventListener('keydown', function (event) {
    if (event.key === 'Escape' && isNarrow() && drawerOpen) setPane(false);
  });

  // toggle does not bubble: listen in the capture phase.
  document.addEventListener('toggle', function (event) {
    var details = event.target;
    if (!details.getAttribute || !details.closest || !details.closest('#relations')) return;
    var kind = details.getAttribute('data-section');
    if (!kind) return;
    var state = stored();
    if (state.sections[kind] === details.open) return;
    state.sections[kind] = details.open;
    saveStored(state);
  }, true);

  if (narrow && narrow.addEventListener) narrow.addEventListener('change', applyRelations);
  document.addEventListener('DOMContentLoaded', function () {
    migrateLegacy();
    applyRelations();
  });
  document.addEventListener('htmx:afterSwap', applyRelations);
})();

// MARK: - The board bar's filter chips and the New ticket form
(function () {
  // A chip clears one filter: the control goes back to its empty value and the
  // form's own `change` trigger redraws the board.
  document.addEventListener('click', function (event) {
    var chip = event.target.closest ? event.target.closest('.chip[data-clear]') : null;
    if (!chip) return;
    var form = chip.closest('form');
    var field = form && form.elements[chip.getAttribute('data-clear')];
    if (!field) return;
    if (field.type === 'checkbox') field.checked = false;
    else field.value = '';
    field.dispatchEvent(new Event('change', { bubbles: true }));
  });

  // Opening New puts the cursor in the title, or in the Project select when no
  // project is chosen yet, so the one required choice is the one in reach.
  document.addEventListener('toggle', function (event) {
    var details = event.target;
    if (!details.classList || !details.classList.contains('new-menu') || !details.open) return;
    var project = details.querySelector('select[name="ticket_project"]');
    var input = details.querySelector('input[name="title"]');
    if (project && !project.disabled && project.value === '') project.focus();
    else if (input) input.focus();
  }, true);

  // The daily note is today's page by the reader's own calendar, not the server's.
  document.addEventListener('submit', function (event) {
    var form = event.target;
    if (!form.hasAttribute || !form.hasAttribute('data-daily')) return;
    var now = new Date();
    var pad = function (value) { return (value < 10 ? '0' : '') + value; };
    form.elements.date.value = now.getFullYear() + '-' + pad(now.getMonth() + 1) + '-' + pad(now.getDate());
  }, true);

  // A destructive form names what it deletes and waits for a yes.
  document.addEventListener('submit', function (event) {
    var message = event.target.getAttribute && event.target.getAttribute('data-confirm');
    if (message && !window.confirm(message)) {
      event.preventDefault();
      event.stopPropagation();
    }
  }, true);
})();

// MARK: - Tabs for the open notes on the notes face
//
// The server keeps the list and draws the strip (`notes.html`, TAN-887): the
// `tabs` cookie (`{"open":[paths],"active":path}`) is written by the server
// alone, on every notes response, and each tab and its close control are links
// the server aims at the right note. This script only adds the keys: roving
// focus along the strip, and Delete or a middle click to close, which follow
// the close link. It also carries the old `tandem.noteTabs` localStorage list
// over once.
(function () {
  var LEGACY_KEY = 'tandem.noteTabs';

  function closest(element, selector) {
    while (element && element.nodeType === 1) {
      if (element.matches(selector)) return element;
      element = element.parentElement;
    }
    return null;
  }

  // Once, while the old key is present: its list goes in front of what the
  // server's cookie holds (the first response has written it by now, with the
  // open note), the key goes, and a notes page reloads so the server draws it.
  function migrateLegacyTabs() {
    if (!window.tandem.uiState) return;
    try {
      var raw = window.localStorage.getItem(LEGACY_KEY);
      if (raw === null) return;
      var list = [];
      try {
        list = JSON.parse(raw);
      } catch (error) {
        list = [];
      }
      if (!Array.isArray(list)) list = [];
      list = list.filter(function (path, at) {
        return typeof path === 'string' && path !== '' && list.indexOf(path) === at;
      });
      var stored = window.tandem.uiState.read('tabs') || {};
      var active = typeof stored.active === 'string' ? stored.active : '';
      var current = Array.isArray(stored.open) ? stored.open : [];
      current.forEach(function (path) {
        if (typeof path === 'string' && list.indexOf(path) < 0) list.push(path);
      });
      while (list.length > 0 && !window.tandem.uiState.write('tabs', { open: list, active: active })) {
        var drop = list.findIndex(function (path) { return path !== active; });
        if (drop < 0) break;
        list.splice(drop, 1);
      }
      window.localStorage.removeItem(LEGACY_KEY);
      if (list.length > 0 && document.getElementById('notes-root')) window.location.reload();
    } catch (error) {
      // Private mode: the strip starts from the open note alone.
    }
  }

  function followClose(item) {
    var control = item && item.querySelector('.note-tabs__close');
    if (control) control.click();
  }

  document.addEventListener('mousedown', function (event) {
    if (event.button === 1 && closest(event.target, '.note-tabs__item')) event.preventDefault();
  });

  document.addEventListener('auxclick', function (event) {
    if (event.button !== 1) return;
    var item = closest(event.target, '.note-tabs__item');
    if (!item) return;
    event.preventDefault();
    followClose(item);
  });

  document.addEventListener('keydown', function (event) {
    var link = closest(event.target, '.note-tabs__tab');
    if (!link) return;
    var links = Array.prototype.slice.call(document.querySelectorAll('.note-tabs__tab'));
    var at = links.indexOf(link);
    var to = -1;
    if (event.key === 'ArrowRight') to = (at + 1) % links.length;
    else if (event.key === 'ArrowLeft') to = (at - 1 + links.length) % links.length;
    else if (event.key === 'Home') to = 0;
    else if (event.key === 'End') to = links.length - 1;
    else if (event.key === 'Delete') {
      event.preventDefault();
      followClose(link.parentElement);
      return;
    }
    if (to < 0) return;
    event.preventDefault();
    links[at].tabIndex = -1;
    links[to].tabIndex = 0;
    links[to].focus();
  });

  document.addEventListener('DOMContentLoaded', migrateLegacyTabs);
})();

/*
 * The Personal tokens screens (CUT-41): the Copy token button, and the focus a
 * page names for itself. Without script the field is still selectable and the
 * page still works; this only adds the copy and moves focus to the status line
 * or the field a page marks.
 */
(function () {
  function selectAll(field) {
    field.focus();
    field.select();
    if (field.setSelectionRange) field.setSelectionRange(0, field.value.length);
  }

  function marked() {
    var field = document.querySelector('[data-select-all]');
    if (field) {
      selectAll(field);
      return;
    }
    var target = document.querySelector('[data-autofocus]');
    if (target && document.activeElement !== target) target.focus();
  }

  document.addEventListener('DOMContentLoaded', marked);

  document.addEventListener('click', function (event) {
    var button = event.target.closest ? event.target.closest('[data-copy-token]') : null;
    if (!button) return;
    var field = document.getElementById(button.getAttribute('data-copy-token'));
    var status = document.getElementById(button.getAttribute('data-copy-status'));
    if (!field || !status) return;
    selectAll(field);
    function failed() {
      status.textContent = 'Copy failed, select the text and copy it';
    }
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(field.value).then(function () {
        status.textContent = 'Copied.';
      }, failed);
    } else {
      failed();
    }
  });
})();
