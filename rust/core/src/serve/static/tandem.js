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
      .then(function (response) { return response.text(); })
      .then(function (html) {
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
  // A box's opener: GitHub's alert syntax, the shape the slash menu writes.
  var CALLOUT = /^(>)( ?)(\[!(NOTE|INFO|TIP|WARNING)\])\s*$/i;

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
    if ((m = CALLOUT.exec(line))) {
      return ['lp__line--quote lp__line--callout lp__line--callout-' + m[4].toLowerCase(),
        marker(m[1] + m[2] + m[3])];
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
    // The box a run of `> ` lines belongs to, from its opener on.
    var box = '';
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
      if (i === this.activeLine) classes += ' lp__line--active';
      html += '<span class="' + classes + '">' + body + '\n</span>';
    }
    this.root.innerHTML = html;
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

    root.addEventListener('input', function (event) {
      if (self.composing || event.isComposing) return;
      self.changed();
      self.menuInput(event);
    });

    // Leaving the editor shuts the menu; a click on the menu itself does not
    // blur, its mousedown is held back.
    root.addEventListener('blur', function () { self.menuClose(); });

    // A drag is left alone until the button is up: a re-draw under the pointer
    // would cut the selection short.
    root.addEventListener('mousedown', function () { self.dragging = true; });

    root.addEventListener('keydown', function (event) {
      if (self.menu && !self.composing && !event.isComposing && self.menuKey(event)) return;
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

  Live.prototype.menuInput = function (event) {
    if (this.menu) {
      this.menuCheck();
      return;
    }
    // Typed text can arrive in one piece (a fast burst), so `/todo` counts.
    var typed = event.inputType === 'insertText' && event.data ? /^\s*(\/\S*)$/.exec(event.data) : null;
    if (!typed) return;
    var selection = this.selection();
    if (!selection || selection.start !== selection.end) return;
    var from = selection.end - typed[1].length;
    // A `/` inside a word (a path, a URL, a/b) is just a slash.
    if (from > 0 && !/\s/.test(this.text.charAt(from - 1))) return;
    var el = document.createElement('div');
    el.className = 'slash';
    el.id = 'slash-menu';
    el.setAttribute('role', 'listbox');
    el.setAttribute('aria-label', 'Turn this line into');
    var self = this;
    // The mouse must not pull focus, or the editor blur would shut the menu
    // before the click lands.
    el.addEventListener('mousedown', function (e) { e.preventDefault(); });
    el.addEventListener('click', function (e) {
      var row = closest(e.target, '[data-slash]');
      if (row) self.menuApply(SLASH_ITEMS[Number(row.getAttribute('data-slash'))]);
    });
    el.addEventListener('mousemove', function (e) {
      var row = closest(e.target, '[data-slash]');
      if (row && self.menu && self.menu.items[self.menu.index] !== SLASH_ITEMS[Number(row.getAttribute('data-slash'))]) {
        self.menu.index = self.menu.items.indexOf(SLASH_ITEMS[Number(row.getAttribute('data-slash'))]);
        self.menuDraw();
      }
    });
    document.body.appendChild(el);
    this.menu = { from: from, el: el, items: [], index: 0, missAt: -1, query: null };
    this.root.setAttribute('aria-haspopup', 'listbox');
    this.root.setAttribute('aria-controls', 'slash-menu');
    this.root.setAttribute('aria-expanded', 'true');
    this.menuCheck();
  };

  // The text after the `/` up to the caret filters the list; a caret that has
  // left that span, or a `/` that is gone, ends it.
  Live.prototype.menuCheck = function () {
    var menu = this.menu;
    if (!menu) return;
    var selection = this.selection();
    if (!selection || selection.start !== selection.end || this.composing) {
      this.menuClose();
      return;
    }
    var caret = selection.end;
    var query = this.text.slice(menu.from + 1, caret);
    if (caret <= menu.from || this.text.charAt(menu.from) !== '/' || query.indexOf('\n') >= 0) {
      this.menuClose();
      return;
    }
    if (query === menu.query) return;
    menu.query = query;
    menu.items = slashMatches(query);
    menu.index = 0;
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

  Live.prototype.menuDraw = function () {
    var menu = this.menu;
    var html = '';
    for (var i = 0; i < menu.items.length; i += 1) {
      var item = menu.items[i];
      var on = i === menu.index;
      html += '<div class="slash__item" role="option" id="slash-opt-' + item.id + '" data-slash="' +
        SLASH_ITEMS.indexOf(item) + '" aria-selected="' + on + '">' +
        '<span class="slash__glyph" aria-hidden="true">' + esc(item.glyph) + '</span>' +
        '<span class="slash__label">' + esc(item.label) + '</span>' +
        '<span class="slash__hint" aria-hidden="true">' + esc(item.hint) + '</span></div>';
    }
    if (!menu.items.length) html = '<div class="slash__empty">No matching block</div>';
    menu.el.innerHTML = html;
    var current = menu.items[menu.index];
    if (current) {
      this.root.setAttribute('aria-activedescendant', 'slash-opt-' + current.id);
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

  // One listener for the page, aimed at the editor now showing: a listener per
  // editor would outlive each swap and keep its detached root alive.
  document.addEventListener('selectionchange', function () {
    if (live) {
      live.follow();
      live.menuCheck();
    }
  });
  document.addEventListener('mouseup', function () {
    if (live && live.dragging) {
      live.dragging = false;
      live.follow();
    }
  });

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
    // An editor of a note the page has left saves the text it holds and goes.
    if (pendingForm && !document.contains(pendingForm)) flushNote();
    if (live && !document.contains(live.root)) {
      live.menuClose();
      live = null;
    }
    // A menu left by an editor the swap took away.
    var stray = document.querySelectorAll('.slash');
    for (var i = 0; i < stray.length; i += 1) {
      if (!live || !live.menu || live.menu.el !== stray[i]) stray[i].remove();
    }
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
