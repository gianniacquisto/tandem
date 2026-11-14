/*
 * The acts htmx does not own: a board card drop, a bikeshed tree move, the
 * ticket sheet's lane select and shortcut keys, and the notes editor's
 * debounced autosave and preview.
 *
 * The server draws every face; this file only turns a pointer or a key into the
 * same form POST the page already carries. Listeners are delegated from the
 * document, so an htmx swap never needs them rebound.
 */
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
    // and the human's project, lane, assignee and search are gone.
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
        if (!response.ok) throw new Error('the move answered ' + response.status);
        return response.text();
      })
      .then(function (html) {
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

  // MARK: - The ticket sheet

  // The lane is the one metadata control that moves at once: its own form.
  document.addEventListener('change', function (event) {
    var select = closest(event.target, '[data-sheet-lane]');
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

  // MARK: - The notes editor

  function noteForm() {
    return document.getElementById('note-form');
  }

  // The autosave writes without swapping the pane: a swap would replace the
  // textarea under the human's caret every second. The response still carries
  // the new baseline, which the hidden input adopts; a conflict or an error is
  // the one answer that does swap, because it must be seen. `writeNote`
  // resolves whether the buffer reached the disk, so a caller that navigates
  // (Escape) can stay put when it did not.
  function writeNote() {
    var form = noteForm();
    if (!form) return Promise.resolve(false);
    return fetch(form.getAttribute('action'), {
      method: 'POST',
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
        'HX-Request': 'true'
      },
      body: new URLSearchParams(new FormData(form)).toString()
    })
      .then(function (response) { return response.text(); })
      .then(function (html) {
        if (html.indexOf('class="conflict"') !== -1 || html.indexOf('class="banner"') !== -1) {
          var root = document.getElementById('notes-root');
          if (root) root.outerHTML = html;
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
  // that lands, and Escape's answer belongs to its own save.
  var noteChain = Promise.resolve(true);

  function saveNote() {
    noteChain = noteChain.then(writeNote, writeNote);
    return noteChain;
  }

  document.addEventListener('input', function (event) {
    var editor = closest(event.target, '.note-editor');
    if (!editor) return;
    if (noteTimer) window.clearTimeout(noteTimer);
    noteTimer = window.setTimeout(saveNote, 1000);
  });

  // MARK: - The live editor
  //
  // One editable surface holding the note's plain markdown. Every line is
  // styled as the page it makes (headings large, bold bold, bullets as bullets)
  // with its markers hidden, except the line the cursor is in, which shows them.
  // The hidden textarea in the form stays the one source of truth: each edit
  // writes the text into it and fires its `input`, so the autosave above needs
  // no change. The text is never anything but the file's own characters.
  var live = null;

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
        out += '<span class="lp__wiki">' + marker('[[') + esc(token.slice(2, -2)) + marker(']]') + '</span>';
      } else {
        var close = token.indexOf('](');
        out += '<span class="lp__link">' + marker('[') + esc(token.slice(1, close)) +
          marker(token.slice(close)) + '</span>';
      }
      last = match.index + token.length;
    }
    return out + esc(source.slice(last));
  }

  var HEADING = /^(#{1,6})( +)(.*)$/;
  var RULE = /^\s*([-*_])(\s*\1){2,}\s*$/;
  var BULLET = /^(\s*)([-*+])( +)(\[[ xX]\] +)?(.*)$/;
  var NUMBER = /^(\s*)(\d+)([.)])( +)(.*)$/;
  var QUOTE = /^(>+)( ?)(.*)$/;
  var FENCE = /^\s*(```|~~~)/;

  function lineHtml(line) {
    var m;
    if ((m = HEADING.exec(line))) {
      return ['lp__line--h' + m[1].length, marker(m[1] + m[2]) + inline(m[3])];
    }
    if (RULE.test(line)) return ['lp__line--rule', esc(line)];
    if ((m = BULLET.exec(line))) {
      var task = m[4] ? '<span class="lp__task">' + esc(m[4]) + '</span>' : '';
      return ['lp__line--bullet', esc(m[1]) + marker(m[2] + m[3]) + task + inline(m[5])];
    }
    if ((m = NUMBER.exec(line))) {
      return ['lp__line--number', esc(m[1]) + '<span class="lp__num">' + esc(m[2] + m[3] + m[4]) + '</span>' + inline(m[5])];
    }
    if ((m = QUOTE.exec(line))) {
      return ['lp__line--quote', marker(m[1] + m[2]) + inline(m[3])];
    }
    return ['', inline(line)];
  }

  function Live(root, store) {
    this.root = root;
    this.store = store;
    this.text = store.value.replace(/\n$/, '');
    this.composing = false;
    this.activeLine = -1;
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
    var caret = selection ? selection.start : null;
    this.activeLine = caret === null ? -1 : this.lineIndex(caret);
    var fenced = false;
    var html = '';
    for (var i = 0; i < lines.length; i += 1) {
      var line = lines[i];
      var classes = 'lp__line';
      var body;
      if (FENCE.test(line)) {
        fenced = !fenced;
        classes += ' lp__line--fence';
        body = esc(line);
      } else if (fenced) {
        classes += ' lp__line--code';
        body = esc(line);
      } else {
        var parts = lineHtml(line);
        classes += parts[0] ? ' ' + parts[0] : '';
        body = parts[1];
      }
      if (i === this.activeLine) classes += ' lp__line--active';
      html += '<span class="' + classes + '">' + body + '\n</span>';
    }
    this.root.innerHTML = html;
    if (selection) this.select(selection.start, selection.end);
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
    return { start: start, end: start + range.toString().length };
  };

  // A position that sits exactly between two text nodes belongs to the later
  // one: the cursor after a line's trailing newline is the start of the next
  // line, and the browser would otherwise draw it, and type into it, at the
  // end of the line above.
  Live.prototype.select = function (start, end) {
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
    var range = document.createRange();
    var open = from || last;
    var close = to || last;
    range.setStart(open.node, open.offset);
    range.setEnd(close.node, close.offset);
    var sel = window.getSelection();
    sel.removeAllRanges();
    sel.addRange(range);
  };

  Live.prototype.sync = function () {
    this.store.value = this.text + '\n';
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

    root.addEventListener('input', function () {
      if (!self.composing) self.changed();
    });

    document.addEventListener('selectionchange', function () {
      if (document.activeElement !== root || self.composing) return;
      var selection = self.selection();
      if (!selection) return;
      if (self.lineIndex(selection.start) !== self.activeLine) self.render(selection);
    });

    root.addEventListener('keydown', function (event) {
      var meta = event.metaKey || event.ctrlKey;
      var key = event.key.toLowerCase();
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
  // empty item it ends the list. The editor writes the newline itself rather
  // than leave it to the browser, so it lands the same way everywhere.
  Live.prototype.enter = function (event) {
    var selection = this.selection();
    if (!selection) return;
    event.preventDefault();
    if (selection.start === selection.end) {
      var bounds = this.lineBounds(selection.start);
      if (selection.start === bounds.to) {
        var line = this.text.slice(bounds.from, bounds.to);
        var m = BULLET.exec(line);
        var next = null;
        var body = '';
        if (m) {
          body = m[5];
          next = m[1] + m[2] + m[3] + (m[4] ? '[ ] ' : '');
        } else if ((m = NUMBER.exec(line))) {
          body = m[5];
          next = m[1] + (Number(m[2]) + 1) + m[3] + m[4];
        }
        if (next !== null) {
          if (body.trim() === '') {
            this.replace(bounds.from, bounds.to, '', bounds.from);
          } else {
            this.replace(selection.start, selection.start, '\n' + next);
          }
          return;
        }
      }
    }
    this.replace(selection.start, selection.end, '\n');
  };

  function initLive() {
    var root = document.querySelector('[data-live]');
    var store = document.querySelector('#note-form .note-editor');
    if (!root || !store || root.hasAttribute('data-live-ready')) return;
    root.setAttribute('data-live-ready', '');
    root.contentEditable = 'plaintext-only';
    // A browser without plaintext editing gets the plain textarea instead.
    if (root.contentEditable !== 'plaintext-only') {
      root.hidden = true;
      store.hidden = false;
      return;
    }
    root.classList.add('lp');
    root.setAttribute('spellcheck', 'true');
    root.setAttribute('role', 'textbox');
    root.setAttribute('aria-multiline', 'true');
    root.setAttribute('aria-label', 'Note');
    live = new Live(root, store);
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
  // Which folders are shut is kept per browser.
  var COLLAPSED_KEY = 'tandem.collapsed';

  function collapsedFolders() {
    try {
      return JSON.parse(window.localStorage.getItem(COLLAPSED_KEY) || '[]');
    } catch (error) {
      return [];
    }
  }

  function applyTree() {
    var items = document.querySelectorAll('.tree > li');
    var shut = collapsedFolders();
    var hideBelow = null;
    for (var i = 0; i < items.length; i += 1) {
      var row = items[i].querySelector('.tree__row');
      if (!row) continue;
      var level = Number(row.getAttribute('aria-level'));
      if (hideBelow !== null) {
        if (level > hideBelow) {
          items[i].hidden = true;
          continue;
        }
        hideBelow = null;
      }
      items[i].hidden = false;
      if (row.getAttribute('data-kind') === 'folder') {
        var closed = shut.indexOf(row.getAttribute('data-path')) >= 0;
        row.setAttribute('aria-expanded', closed ? 'false' : 'true');
        var twisty = row.querySelector('.tree__twisty');
        if (twisty) twisty.textContent = closed ? '▸' : '▾';
        if (closed) hideBelow = level;
      }
    }
  }

  document.addEventListener('click', function (event) {
    if (closest(event.target, '.row-menu')) return;
    var row = closest(event.target, '.tree__row[data-kind="folder"]');
    if (!row) return;
    var path = row.getAttribute('data-path');
    var shut = collapsedFolders();
    var at = shut.indexOf(path);
    if (at >= 0) shut.splice(at, 1);
    else shut.push(path);
    try {
      window.localStorage.setItem(COLLAPSED_KEY, JSON.stringify(shut));
    } catch (error) {
      // Private mode: the fold holds until the next redraw.
    }
    applyTree();
  });

  function setUpNote() {
    live = null;
    initLive();
    focusTitle();
    applyTree();
  }

  document.addEventListener('DOMContentLoaded', setUpNote);
  document.addEventListener('htmx:afterSettle', setUpNote);
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

  // Opening New puts the cursor in the title.
  document.addEventListener('toggle', function (event) {
    var details = event.target;
    if (!details.classList || !details.classList.contains('new-menu') || !details.open) return;
    var input = details.querySelector('input[name="title"]');
    if (input) input.focus();
  }, true);

  // The daily note is today's page by the reader's own calendar, not the server's.
  document.addEventListener('submit', function (event) {
    var form = event.target;
    if (!form.classList || !form.classList.contains('tree__daily')) return;
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
