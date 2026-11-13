/*
 * The board's one act htmx does not own: a card drop writes a status move.
 *
 * The card carries the ticket id on dragstart, the column carries its stage in
 * `data-status`, and the board carries the revision it was drawn from in
 * `data-revision`. The POST is the same form the server's routes read, and the
 * answer replaces `#board-root` whole, so the filter bar, the notice and the
 * new revision all land together. Listeners are delegated from the document,
 * so a swap never needs them rebound.
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
    var root = document.getElementById('board-root');
    if (!column || !dragged || !root) return;
    event.preventDefault();
    column.removeAttribute('data-targeted');
    var body = new URLSearchParams();
    body.set('status', column.getAttribute('data-status') || '');
    body.set('expected_revision', root.getAttribute('data-revision') || '0');
    fetch('/tickets/' + encodeURIComponent(dragged) + '/status', {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: body.toString()
    })
      .then(function (response) { return response.text(); })
      .then(function (html) {
        var current = document.getElementById('board-root');
        if (current) current.outerHTML = html;
      });
    dragged = null;
  });
})();
