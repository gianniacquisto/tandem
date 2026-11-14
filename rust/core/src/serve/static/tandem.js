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

  document.addEventListener('keydown', function (event) {
    var editor = closest(event.target, '.note-editor');
    if (!editor) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      if (noteTimer) window.clearTimeout(noteTimer);
      var done = document.querySelector('.bikeshed__breadcrumb-end a.bikeshed__mode');
      saveNote().then(function (saved) {
        // A refused save drew its reason into the root. Navigating now would
        // replace the banner before the human reads it, and their text with it.
        if (saved && done) window.location.href = done.getAttribute('href');
      });
    }
  });

  document.addEventListener('click', function (event) {
    var toggle = closest(event.target, '[data-note-preview]');
    if (!toggle) return;
    var form = noteForm();
    var out = document.querySelector('[data-note-preview-out]');
    if (!form || !out) return;
    fetch('/notes/preview', {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({ text: form.elements.text.value }).toString()
    })
      .then(function (response) { return response.text(); })
      .then(function (html) {
        out.innerHTML = html;
        out.hidden = !out.hidden;
      })
      .catch(function (error) {
        console.error('the note preview failed:', error);
      });
  });
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
})();
