/*
 * The notes graph (TAN-529): the typed nodes and edges `/notes/graph.json`
 * answers, laid out by a small force simulation and drawn as SVG.
 *
 * The browser draws what the server answers and decides no node kind or
 * orphan rule. Every node is built with DOM APIs and `textContent`, never
 * `innerHTML`, so a note called `<img onerror>` stays text. The layout runs to
 * rest off-screen in a fixed tick budget, paints once and stops: nothing
 * animates, so reduced motion needs no special case. Dragging moves the
 * dragged node and its edges only.
 */
(function () {
  'use strict';

  var OPTIONS_KEY = 'tandem.graph.options';
  var SVG_NS = 'http://www.w3.org/2000/svg';
  var NAME_LIMIT = 24;
  var MAX_STEP = 20;
  var DEFAULTS = { notes: true, folders: true, names: false, daily: 'free' };
  var WIDE = window.matchMedia ? window.matchMedia('(min-width: 1101px)') : null;

  // MARK: - Options kept per browser

  function readOptions() {
    var options = {
      notes: DEFAULTS.notes,
      folders: DEFAULTS.folders,
      names: DEFAULTS.names,
      daily: DEFAULTS.daily
    };
    try {
      var stored = JSON.parse(window.localStorage.getItem(OPTIONS_KEY) || '{}');
      if (stored && typeof stored === 'object') {
        ['notes', 'folders', 'names'].forEach(function (key) {
          if (typeof stored[key] === 'boolean') {
            options[key] = stored[key];
          }
        });
        if (stored.daily === 'grouped' || stored.daily === 'free') {
          options.daily = stored.daily;
        }
      }
    } catch (error) {
      // Private mode or a damaged value: the defaults stand.
    }
    return options;
  }

  function writeOptions(options) {
    try {
      window.localStorage.setItem(OPTIONS_KEY, JSON.stringify(options));
    } catch (error) {
      // Private mode: the choice lasts for this page only.
    }
  }

  // MARK: - The layout

  function shorten(name) {
    return name.length > NAME_LIMIT ? name.slice(0, NAME_LIMIT - 1) + '…' : name;
  }

  function radius(node) {
    if (node.kind === 'project') {
      return 10;
    }
    if (node.kind === 'label') {
      return 8.5;
    }
    return node.kind === 'folder' ? 5 : 4;
  }

  /*
   * Pairwise repulsion, a spring per edge (`contains` shorter than `links`),
   * a weak pull to the centre and velocity damping, over a fixed number of
   * ticks with a cooling factor, so it always terminates. In Free mode the
   * `contains` edges out of the root `daily` folder add no force.
   */
  function layout(nodes, edges, freeDaily) {
    var count = nodes.length;
    var ticks = count > 600 ? 120 : 300;
    var byId = {};
    nodes.forEach(function (node, index) {
      byId[node.id] = node;
      var angle = index * 2.399963;
      var reach = 14 * Math.sqrt(index + 1);
      node.x = Math.cos(angle) * reach;
      node.y = Math.sin(angle) * reach;
      node.vx = 0;
      node.vy = 0;
    });
    var springs = edges.filter(function (edge) {
      if (!freeDaily || edge.kind !== 'contains') {
        return true;
      }
      var source = byId[edge.source];
      return !(source && source.kind === 'folder' && source.path === 'daily');
    });
    for (var tick = 0; tick < ticks; tick += 1) {
      var cooling = 1 - tick / ticks;
      for (var i = 0; i < count; i += 1) {
        var a = nodes[i];
        for (var j = i + 1; j < count; j += 1) {
          var b = nodes[j];
          var dx = b.x - a.x;
          var dy = b.y - a.y;
          var squared = dx * dx + dy * dy;
          if (squared < 0.01) {
            dx = (i % 2 ? 0.1 : -0.1);
            dy = (j % 2 ? 0.1 : -0.1);
            squared = dx * dx + dy * dy;
          }
          // A floor on the distance keeps two nodes almost on top of each
          // other from throwing one another out of the picture.
          var distance = Math.sqrt(squared);
          var push = 1800 / Math.max(squared, 36);
          dx = (dx / distance) * push;
          dy = (dy / distance) * push;
          a.vx -= dx;
          a.vy -= dy;
          b.vx += dx;
          b.vy += dy;
        }
      }
      springs.forEach(function (edge) {
        var source = byId[edge.source];
        var target = byId[edge.target];
        var dx = target.x - source.x;
        var dy = target.y - source.y;
        var distance = Math.sqrt(dx * dx + dy * dy) || 1;
        var rest = edge.kind === 'contains' ? 55 : 90;
        var pull = (distance - rest) * 0.04;
        dx = (dx / distance) * pull;
        dy = (dy / distance) * pull;
        source.vx += dx;
        source.vy += dy;
        target.vx -= dx;
        target.vy -= dy;
      });
      nodes.forEach(function (node) {
        node.vx -= node.x * 0.004;
        node.vy -= node.y * 0.004;
        var speed = Math.sqrt(node.vx * node.vx + node.vy * node.vy);
        if (speed > MAX_STEP) {
          node.vx = (node.vx / speed) * MAX_STEP;
          node.vy = (node.vy / speed) * MAX_STEP;
        }
        node.x += node.vx * cooling;
        node.y += node.vy * cooling;
        node.vx *= 0.6;
        node.vy *= 0.6;
      });
    }
  }

  // MARK: - One graph on the page

  function svgElement(name, attributes) {
    var element = document.createElementNS(SVG_NS, name);
    Object.keys(attributes || {}).forEach(function (key) {
      element.setAttribute(key, attributes[key]);
    });
    return element;
  }

  function nodeKindLabel(node) {
    var label = node.kind;
    if (node.orphan) {
      label += ', orphan';
    }
    if (node.unmatched) {
      label += ', ' + node.unmatched;
    }
    return label;
  }

  function setUp(container) {
    if (container.dataset.graphReady === '1') {
      return;
    }
    container.dataset.graphReady = '1';

    var svg = container.querySelector('[data-graph-svg]');
    var canvas = container.querySelector('[data-graph-canvas]');
    var loading = container.querySelector('[data-graph-loading]');
    var empty = container.querySelector('[data-graph-empty]');
    var errorBanner = container.querySelector('[data-graph-error]');
    var errorText = container.querySelector('[data-graph-error-text]');
    var capBanner = container.querySelector('[data-graph-cap]');
    var capText = container.querySelector('[data-graph-cap-text]');
    var controls = container.querySelector('[data-graph-controls]');
    var search = container.querySelector('[data-graph-search]');
    var tip = container.querySelector('[data-graph-tip]');
    if (!svg || !canvas) {
      return;
    }

    var options = readOptions();
    var nodes = [];
    var edges = [];
    var byId = {};
    var elements = {};
    var edgeElements = [];
    var viewport = null;
    var view = { x: 0, y: 0, k: 1 };
    var lit = null;
    var found = null;
    var generation = 0;

    // MARK: Controls

    function syncControls() {
      container.querySelectorAll('[data-graph-option]').forEach(function (box) {
        box.checked = !!options[box.getAttribute('data-graph-option')];
      });
      container.querySelectorAll('[data-graph-daily]').forEach(function (button) {
        button.setAttribute(
          'aria-pressed',
          button.getAttribute('data-graph-daily') === options.daily ? 'true' : 'false'
        );
      });
    }

    function showMessage(element, show) {
      if (element) {
        element.hidden = !show;
      }
    }

    // MARK: Drawing

    function applyView() {
      if (viewport) {
        viewport.setAttribute(
          'transform',
          'translate(' + view.x + ' ' + view.y + ') scale(' + view.k + ')'
        );
      }
    }

    function isShown(node) {
      if (node.kind === 'note') {
        return options.notes;
      }
      if (node.kind === 'folder') {
        return options.folders;
      }
      return true;
    }

    function fit() {
      var box = svg.getBoundingClientRect();
      var shown = nodes.filter(isShown);
      if (!shown.length || !box.width || !box.height) {
        return;
      }
      var minX = Infinity;
      var minY = Infinity;
      var maxX = -Infinity;
      var maxY = -Infinity;
      shown.forEach(function (node) {
        minX = Math.min(minX, node.x);
        maxX = Math.max(maxX, node.x);
        minY = Math.min(minY, node.y);
        maxY = Math.max(maxY, node.y + 18);
      });
      var width = Math.max(maxX - minX, 1);
      var height = Math.max(maxY - minY, 1);
      var k = Math.min((box.width - 140) / width, (box.height - 190) / height, 1.6);
      view.k = Math.max(k, 0.05);
      view.x = box.width / 2 - ((minX + maxX) / 2) * view.k;
      view.y = (box.height - 50) / 2 - ((minY + maxY) / 2) * view.k;
      applyView();
    }

    function placeNode(node) {
      var element = elements[node.id];
      if (element) {
        element.setAttribute('transform', 'translate(' + node.x + ' ' + node.y + ')');
      }
    }

    function placeEdges(node) {
      edgeElements.forEach(function (entry) {
        if (entry.edge.source === node.id || entry.edge.target === node.id) {
          var source = byId[entry.edge.source];
          var target = byId[entry.edge.target];
          entry.line.setAttribute('x1', source.x);
          entry.line.setAttribute('y1', source.y);
          entry.line.setAttribute('x2', target.x);
          entry.line.setAttribute('y2', target.y);
        }
      });
    }

    function applyVisibility() {
      nodes.forEach(function (node) {
        var element = elements[node.id];
        element.style.display = isShown(node) ? '' : 'none';
        element.classList.toggle(
          'gnode--named',
          options.names && (node.kind === 'note' || node.kind === 'folder')
        );
      });
      edgeElements.forEach(function (entry) {
        var visible =
          isShown(byId[entry.edge.source]) && isShown(byId[entry.edge.target]);
        entry.line.style.display = visible ? '' : 'none';
      });
    }

    function nodeElement(node) {
      var classes = 'gnode gnode--' + node.kind;
      if (node.kind === 'folder' && node.unmatched) {
        classes += ' gnode--unmatched';
      }
      if (node.kind === 'note') {
        if (node.orphan) {
          classes += ' gnode--orphan';
        } else if (node.daily) {
          classes += ' gnode--daily';
        }
      }
      var group = svgElement('g', {
        class: classes,
        tabindex: '0',
        role: 'link',
        'aria-label': nodeKindLabel(node) + ' ' + node.name,
        'data-id': node.id
      });
      var size = radius(node);
      if (node.kind === 'label') {
        group.appendChild(
          svgElement('rect', {
            class: 'gshape',
            x: -6,
            y: -6,
            width: 12,
            height: 12,
            transform: 'rotate(45)'
          })
        );
      } else {
        group.appendChild(svgElement('circle', { class: 'gshape', r: size }));
      }
      group.appendChild(svgElement('circle', { class: 'gring', r: size + 5 }));
      var text = svgElement('text', {
        class:
          node.kind === 'project' || node.kind === 'label'
            ? 'glabel'
            : 'glabel glabel--minor',
        y: size + 12,
        'text-anchor': 'middle'
      });
      text.textContent = shorten(node.name);
      group.appendChild(text);
      return group;
    }

    function draw() {
      while (svg.firstChild) {
        svg.removeChild(svg.firstChild);
      }
      elements = {};
      edgeElements = [];
      viewport = svgElement('g', { 'data-graph-viewport': '' });
      var edgeLayer = svgElement('g');
      var nodeLayer = svgElement('g');
      edges.forEach(function (edge) {
        var source = byId[edge.source];
        var target = byId[edge.target];
        var line = svgElement('line', {
          class: 'gedge gedge--' + edge.kind,
          x1: source.x,
          y1: source.y,
          x2: target.x,
          y2: target.y
        });
        edgeLayer.appendChild(line);
        edgeElements.push({ edge: edge, line: line });
      });
      nodes.forEach(function (node) {
        var element = nodeElement(node);
        elements[node.id] = element;
        nodeLayer.appendChild(element);
        placeNode(node);
      });
      viewport.appendChild(edgeLayer);
      viewport.appendChild(nodeLayer);
      svg.appendChild(viewport);
      applyVisibility();
      fit();
    }

    // MARK: Highlight

    function clearLight() {
      if (!lit) {
        return;
      }
      lit = null;
      viewport.querySelectorAll('.gdim, .glit').forEach(function (element) {
        element.classList.remove('gdim', 'glit');
      });
      tip.hidden = true;
    }

    function light(node) {
      if (!viewport) {
        return;
      }
      clearLight();
      lit = node;
      var near = {};
      near[node.id] = true;
      edgeElements.forEach(function (entry) {
        var incident = entry.edge.source === node.id || entry.edge.target === node.id;
        entry.line.classList.toggle('gdim', !incident);
        if (incident) {
          near[entry.edge.source] = true;
          near[entry.edge.target] = true;
        }
      });
      nodes.forEach(function (other) {
        var element = elements[other.id];
        element.classList.toggle('gdim', !near[other.id]);
        element.classList.toggle('glit', other === node);
      });
      tip.textContent = '';
      tip.appendChild(document.createTextNode(node.name + ' '));
      var small = document.createElement('small');
      small.textContent = nodeKindLabel(node);
      tip.appendChild(small);
      tip.hidden = false;
      tip.style.left = Math.round(node.x * view.k + view.x + 12) + 'px';
      tip.style.top = Math.round(node.y * view.k + view.y - 28) + 'px';
    }

    // MARK: Opening a node

    function open(node) {
      var base;
      var href;
      if (node.kind === 'note' && node.path) {
        base = container.getAttribute('data-note-href');
        href = base + '&path=' + encodeURIComponent(node.path);
      } else if (node.kind === 'project' || node.kind === 'label') {
        base = container.getAttribute('data-concept-href');
        href =
          base + '&kind=' + node.kind + '&name=' + encodeURIComponent(node.name);
      } else if (node.kind === 'folder') {
        selectInTree(node);
        return;
      } else {
        return;
      }
      if (window.htmx) {
        window.htmx.ajax('GET', href, { target: '#notes-root', swap: 'outerHTML' });
      } else {
        window.location.href = href;
      }
    }

    function selectInTree(node) {
      var chosen = null;
      document.querySelectorAll('.tree__row').forEach(function (row) {
        var match = row.getAttribute('data-path') === node.path;
        row.setAttribute('aria-selected', match ? 'true' : 'false');
        if (match) {
          chosen = row;
        }
      });
      if (chosen) {
        chosen.scrollIntoView({ block: 'nearest' });
      }
    }

    // MARK: Pointer: drag a node, pan the background, wheel and pinch zoom

    var pointers = {};
    var gesture = null;

    function nodeOf(target) {
      var element = target.closest ? target.closest('.gnode') : null;
      return element ? byId[element.getAttribute('data-id')] : null;
    }

    function zoomAt(factor, cx, cy) {
      var k = Math.min(Math.max(view.k * factor, 0.05), 8);
      var ratio = k / view.k;
      view.x = cx - (cx - view.x) * ratio;
      view.y = cy - (cy - view.y) * ratio;
      view.k = k;
      applyView();
    }

    function pinchDistance() {
      var ids = Object.keys(pointers);
      var a = pointers[ids[0]];
      var b = pointers[ids[1]];
      return Math.hypot(a.x - b.x, a.y - b.y);
    }

    svg.addEventListener('pointerdown', function (event) {
      if (event.button > 0) {
        return;
      }
      var box = svg.getBoundingClientRect();
      pointers[event.pointerId] = { x: event.clientX, y: event.clientY };
      if (Object.keys(pointers).length === 2) {
        gesture = { type: 'pinch', distance: pinchDistance() };
        return;
      }
      var node = nodeOf(event.target);
      gesture = {
        type: node ? 'node' : 'pan',
        node: node,
        startX: event.clientX,
        startY: event.clientY,
        lastX: event.clientX,
        lastY: event.clientY,
        moved: false,
        left: box.left,
        top: box.top
      };
      try {
        svg.setPointerCapture(event.pointerId);
      } catch (error) {
        // A synthetic pointer has nothing to capture.
      }
      if (!node) {
        svg.classList.add('gsvg--panning');
      }
    });

    svg.addEventListener('pointermove', function (event) {
      if (!pointers[event.pointerId] || !gesture) {
        return;
      }
      pointers[event.pointerId] = { x: event.clientX, y: event.clientY };
      if (gesture.type === 'pinch') {
        if (Object.keys(pointers).length >= 2) {
          var distance = pinchDistance();
          var ids = Object.keys(pointers);
          var box = svg.getBoundingClientRect();
          var cx = (pointers[ids[0]].x + pointers[ids[1]].x) / 2 - box.left;
          var cy = (pointers[ids[0]].y + pointers[ids[1]].y) / 2 - box.top;
          if (gesture.distance > 0) {
            zoomAt(distance / gesture.distance, cx, cy);
          }
          gesture.distance = distance;
        }
        return;
      }
      var dx = event.clientX - gesture.lastX;
      var dy = event.clientY - gesture.lastY;
      if (
        !gesture.moved &&
        Math.hypot(event.clientX - gesture.startX, event.clientY - gesture.startY) < 4
      ) {
        return;
      }
      gesture.moved = true;
      gesture.lastX = event.clientX;
      gesture.lastY = event.clientY;
      if (gesture.type === 'node') {
        gesture.node.x += dx / view.k;
        gesture.node.y += dy / view.k;
        placeNode(gesture.node);
        placeEdges(gesture.node);
        if (lit === gesture.node) {
          light(gesture.node);
        }
      } else {
        view.x += dx;
        view.y += dy;
        applyView();
      }
    });

    function endPointer(event) {
      if (!pointers[event.pointerId]) {
        return;
      }
      delete pointers[event.pointerId];
      svg.classList.remove('gsvg--panning');
      var finished = gesture;
      if (Object.keys(pointers).length === 0) {
        gesture = null;
      } else if (finished && finished.type === 'pinch') {
        gesture = null;
      }
      if (
        finished &&
        finished.type === 'node' &&
        !finished.moved &&
        event.type === 'pointerup'
      ) {
        open(finished.node);
      }
    }

    svg.addEventListener('pointerup', endPointer);
    svg.addEventListener('pointercancel', endPointer);

    svg.addEventListener(
      'wheel',
      function (event) {
        event.preventDefault();
        var box = svg.getBoundingClientRect();
        zoomAt(
          Math.exp(-event.deltaY * 0.0015),
          event.clientX - box.left,
          event.clientY - box.top
        );
      },
      { passive: false }
    );

    svg.addEventListener('pointerover', function (event) {
      var node = nodeOf(event.target);
      if (node && !gesture) {
        light(node);
      }
    });

    svg.addEventListener('pointerout', function (event) {
      var node = nodeOf(event.target);
      if (node && lit === node && !gesture) {
        clearLight();
      }
    });

    svg.addEventListener('focusin', function (event) {
      var node = nodeOf(event.target);
      if (node) {
        light(node);
      }
    });

    svg.addEventListener('focusout', function () {
      clearLight();
    });

    svg.addEventListener('keydown', function (event) {
      if (event.key !== 'Enter') {
        return;
      }
      var node = nodeOf(event.target);
      if (node) {
        event.preventDefault();
        open(node);
      }
    });

    // MARK: Search and the controls card

    function match() {
      var wanted = search.value.trim().toLowerCase();
      if (!wanted) {
        return null;
      }
      for (var index = 0; index < nodes.length; index += 1) {
        if (isShown(nodes[index]) && nodes[index].name.toLowerCase().indexOf(wanted) >= 0) {
          return nodes[index];
        }
      }
      return null;
    }

    search.addEventListener('input', function () {
      found = match();
      if (found) {
        light(found);
      } else {
        clearLight();
      }
    });

    search.addEventListener('keydown', function (event) {
      if (event.key === 'Enter') {
        event.preventDefault();
        var node = found || match();
        if (node) {
          open(node);
        }
      }
    });

    container.querySelectorAll('[data-graph-option]').forEach(function (box) {
      box.addEventListener('change', function () {
        options[box.getAttribute('data-graph-option')] = box.checked;
        writeOptions(options);
        if (viewport) {
          applyVisibility();
          clearLight();
        }
      });
    });

    container.querySelectorAll('[data-graph-daily]').forEach(function (button) {
      button.addEventListener('click', function () {
        options.daily = button.getAttribute('data-graph-daily');
        writeOptions(options);
        syncControls();
        if (nodes.length) {
          layout(nodes, edges, options.daily === 'free');
          draw();
        }
      });
    });

    var fitButton = container.querySelector('[data-graph-fit]');
    if (fitButton) {
      fitButton.addEventListener('click', fit);
    }

    // MARK: Loading

    function fail(message) {
      showMessage(loading, false);
      errorText.textContent = message;
      showMessage(errorBanner, true);
      if (controls) {
        controls.hidden = true;
      }
    }

    function show(graph) {
      showMessage(loading, false);
      showMessage(errorBanner, false);
      nodes = graph.nodes || [];
      var known = {};
      nodes.forEach(function (node) {
        known[node.id] = true;
      });
      byId = {};
      nodes.forEach(function (node) {
        byId[node.id] = node;
      });
      edges = (graph.edges || []).filter(function (edge) {
        return known[edge.source] && known[edge.target];
      });
      var total = graph.total || nodes.length;
      if (graph.shown !== undefined && graph.shown < total) {
        capText.textContent =
          'Showing ' + graph.shown.toLocaleString('en-US') + ' of ' +
          total.toLocaleString('en-US') +
          ' nodes. Pick a project in the focus picker to see the rest.';
        showMessage(capBanner, true);
      } else {
        showMessage(capBanner, false);
      }
      if (!nodes.length) {
        showMessage(empty, true);
        if (controls) {
          controls.hidden = true;
        }
        svg.setAttribute('aria-label', 'Graph of 0 notes, folders, projects and labels');
        return;
      }
      showMessage(empty, false);
      if (controls) {
        controls.hidden = false;
      }
      svg.setAttribute(
        'aria-label',
        'Graph of ' + nodes.length + ' notes, folders, projects and labels'
      );
      layout(nodes, edges, options.daily === 'free');
      draw();
    }

    function load() {
      generation += 1;
      var mine = generation;
      showMessage(errorBanner, false);
      showMessage(loading, true);
      window
        .fetch(container.getAttribute('data-src'), {
          headers: { Accept: 'application/json' },
          credentials: 'same-origin'
        })
        .then(function (response) {
          return response
            .json()
            .catch(function () {
              return {};
            })
            .then(function (body) {
              return { ok: response.ok, body: body };
            });
        })
        .then(function (answer) {
          if (mine !== generation || !container.isConnected) {
            return;
          }
          if (!answer.ok || !answer.body || !Array.isArray(answer.body.nodes)) {
            fail(
              'The graph could not be read' +
                (answer.body && answer.body.error ? ': ' + answer.body.error : '.')
            );
            return;
          }
          show(answer.body);
        })
        .catch(function () {
          if (mine === generation && container.isConnected) {
            fail('The graph could not be read: the request failed.');
          }
        });
    }

    var retry = container.querySelector('[data-graph-retry]');
    if (retry) {
      retry.addEventListener('click', load);
    }
    var dismiss = container.querySelector('[data-graph-dismiss]');
    if (dismiss) {
      dismiss.addEventListener('click', function () {
        showMessage(errorBanner, false);
      });
    }

    syncControls();
    syncCard();
    load();
  }

  // MARK: - The controls card: open on a wide page, collapsed under 1100px

  function syncCard() {
    var card = document.querySelector('[data-graph-controls]');
    if (card && WIDE) {
      card.open = WIDE.matches;
    }
  }

  if (WIDE && WIDE.addEventListener) {
    WIDE.addEventListener('change', syncCard);
  }

  // MARK: - Initialise on load and after every swap of the notes root

  function init() {
    var container = document.getElementById('notes-graph');
    if (container) {
      setUp(container);
    }
  }

  document.addEventListener('DOMContentLoaded', init);
  document.addEventListener('htmx:afterSettle', init);
  if (document.readyState !== 'loading') {
    init();
  }
})();
