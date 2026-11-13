/*
 * The board's act htmx does not own: a card move writes a status. It answers
 * both the mouse (drag and drop) and the keyboard (Alt+Left / Alt+Right on a
 * focused card), which is the route the React board's move took.
 *
 * The card carries the ticket id on dragstart, the column carries its stage in
 * `data-status`, and the board carries the revision it was drawn from in
 * `data-revision`. The POST is the same form the server's routes read, and it
 * sends `HX-Request` so the answer is the `#board-root` partial rather than the
 * whole shell. The partial replaces the root, so the filter bar, the notice and
 * the new revision all land together, and focus returns to the moved card.
 * Listeners are delegated from the document, so a swap never needs them rebound.
 */
(function () {
  var dragged = null;

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

  function move(ticketId, column) {
    var root = document.getElementById('board-root');
    if (!root || !column || !ticketId) return;
    // The board's answer replaces the root, so the POST has to carry the
    // filters the board was drawn with; without them the partial redraws an
    // unfiltered board and the human's project, lane, assignee and search are
    // gone. The filter bar's own controls are that state, and they are the
    // same field names the add form carries as hidden inputs.
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
        // The card stays where the human left it; the page says nothing, so the
        // console is the one record of the failed write.
        console.error('the card move failed:', error);
      });
  }

  document.addEventListener('dragstart', function (event) {
    var card = closest(event.target, '.ticket-card');
    if (!card) return;
    dragged = card.getAttribute('data-ticket');
    event.dataTransfer.setData('text/plain', dragged);
    event.dataTransfer.effectAllowed = 'move';
  });

  document.addEventListener('dragover', function (event) {
    var column = closest(event.target, '.board__column');
    if (!column || !dragged) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
    column.setAttribute('data-targeted', '');
  });

  document.addEventListener('dragleave', function (event) {
    var column = closest(event.target, '.board__column');
    if (column) column.removeAttribute('data-targeted');
  });

  document.addEventListener('drop', function (event) {
    var column = closest(event.target, '.board__column');
    if (!column || !dragged) return;
    event.preventDefault();
    column.removeAttribute('data-targeted');
    move(dragged, column);
    dragged = null;
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
    move(card.getAttribute('data-ticket'), target);
  });
})();
