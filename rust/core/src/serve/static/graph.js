/*
 * The notes graph (TAN-529, TAN-856): the typed nodes and edges
 * `/notes/graph.json` answers, moved by a live force simulation and drawn as
 * SVG.
 *
 * The browser draws what the server answers and decides no node kind or
 * orphan rule (ADR 0054): a note's `orphan` and `daily` flags arrive in the
 * JSON and the browser only picks the style. Every node is built with DOM
 * APIs and `textContent`, never `innerHTML`, so a note called `<img onerror>`
 * stays text.
 *
 * The simulation is hand-written in the style of d3-force: many-body charge
 * with collision in one pair loop, springs per edge, gravity and velocity
 * damping, all cooled by `alpha`. It runs on `requestAnimationFrame` only
 * while it is hot (alpha above ALPHA_MIN, or a held alphaTarget) and stops
 * once it settles. Dragging holds a node and reheats its neighbours; a node
 * can be pinned, and pins are kept per browser and per focus project. With
 * `prefers-reduced-motion` the same ticks run synchronously and the picture
 * is rendered once.
 */
(function () {
  'use strict';

  var OPTIONS_KEY = 'tandem.graph.options';
  var PINS_KEY = 'tandem.graph.pins';
  var SVG_NS = 'http://www.w3.org/2000/svg';
  var NAME_LIMIT = 28;
  var ALPHA_MIN = 0.002;
  var MAX_SPEED = 60;
  var REACH_SQUARED = 160000;
  var DRAG_THRESHOLD = 3;
  var DOUBLE_CLICK_MS = 300;
  var DAILY_ROOT = 'daily';
  var DEFAULTS = { notes: true, folders: true, names: false, daily: 'grouped', feel: 'springy' };
  var FEELS = {
    springy: { keep: 0.6, charge: 1, rest: 1, spring: 1, gravity: 0.045, life: 300 },
    calm: { keep: 0.35, charge: 0.85, rest: 1.05, spring: 0.7, gravity: 0.05, life: 160 },
    bouncy: { keep: 0.82, charge: 1.15, rest: 0.95, spring: 1.6, gravity: 0.04, life: 520 }
  };
  var CHARGE = { project: -420, label: -200, folder: -170, note: -120 };
  var WIDE = window.matchMedia ? window.matchMedia('(min-width: 1101px)') : null;
  var REDUCED = window.matchMedia ? window.matchMedia('(prefers-reduced-motion: reduce)') : null;

  function reduceMotion() {
    return !!(REDUCED && REDUCED.matches);
  }

  // MARK: - Options and pins kept per browser

  function readOptions() {
    var options = {
      notes: DEFAULTS.notes,
      folders: DEFAULTS.folders,
      names: DEFAULTS.names,
      daily: DEFAULTS.daily,
      feel: DEFAULTS.feel
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
        if (typeof stored.feel === 'string' && FEELS.hasOwnProperty(stored.feel)) {
          options.feel = stored.feel;
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

  function readAllPins() {
    try {
      var stored = JSON.parse(window.localStorage.getItem(PINS_KEY) || '{}');
      if (stored && typeof stored === 'object' && !Array.isArray(stored)) {
        return stored;
      }
    } catch (error) {
      // A damaged value: no pins are restored.
    }
    return {};
  }

  function writePins(focus, pins) {
    try {
      var all = readAllPins();
      if (Object.keys(pins).length) {
        all[focus] = pins;
      } else {
        delete all[focus];
      }
      window.localStorage.setItem(PINS_KEY, JSON.stringify(all));
    } catch (error) {
      // Private mode: the pins last for this page only.
    }
  }

  function validPoint(point) {
    return (
      Array.isArray(point) &&
      point.length === 2 &&
      typeof point[0] === 'number' &&
      typeof point[1] === 'number' &&
      isFinite(point[0]) &&
      isFinite(point[1])
    );
  }

  // MARK: - Data prep

  function shorten(name) {
    return name.length > NAME_LIMIT ? name.slice(0, NAME_LIMIT - 1) + '…' : name;
  }

  function radius(node) {
    if (node.kind === 'project') {
      return 11;
    }
    if (node.kind === 'label') {
      return 8;
    }
    if (node.kind === 'folder') {
      return 5.5;
    }
    return 3.5 + Math.min(5, Math.sqrt(node.deg) * 0.75);
  }

  /*
   * One entry per node pair: two `links` between the same pair, in either
   * direction, become one entry with `both`; `contains` edges are keyed by
   * direction; self-edges and repeats are dropped. Each node learns its drawn
   * degree, neighbours, incident edges and raw out/in counts of `links`.
   */
  function prepare(rawNodes, rawEdges) {
    var byId = {};
    var nodes = rawNodes.map(function (raw, index) {
      // A tight spiral: the first settle blooms out from the middle.
      var angle = index * 2.399963;
      var reach = 4 * Math.sqrt(index + 1);
      var node = {
        id: raw.id,
        kind: raw.kind,
        name: String(raw.name === undefined || raw.name === null ? '' : raw.name),
        path: raw.path,
        daily: !!raw.daily,
        orphan: !!raw.orphan,
        unmatched: raw.unmatched || null,
        x: Math.cos(angle) * reach,
        y: Math.sin(angle) * reach,
        vx: 0,
        vy: 0,
        fx: null,
        fy: null,
        deg: 0,
        out: 0,
        inn: 0,
        near: {},
        edges: [],
        count: 0,
        active: true,
        index: index
      };
      byId[node.id] = node;
      return node;
    });
    var kept = {};
    var edges = [];
    rawEdges.forEach(function (raw) {
      var source = byId[raw.source];
      var target = byId[raw.target];
      if (!source || !target || source === target) {
        return;
      }
      if (raw.kind === 'links') {
        var key = 'l' + [source.id, target.id].sort().join('\u0000');
        var seen = kept[key];
        if (seen) {
          if (seen.source !== source && !seen.both) {
            seen.both = true;
            source.out += 1;
            target.inn += 1;
          }
          return;
        }
        source.out += 1;
        target.inn += 1;
        var entry = { source: source, target: target, kind: 'links', both: false, active: true };
        kept[key] = entry;
        edges.push(entry);
      } else {
        var ckey = 'c' + source.id + '\u0000' + target.id;
        if (kept[ckey]) {
          return;
        }
        var contained = { source: source, target: target, kind: 'contains', both: false, active: true };
        kept[ckey] = contained;
        edges.push(contained);
      }
    });
    edges.forEach(function (edge) {
      edge.source.deg += 1;
      edge.target.deg += 1;
      edge.source.near[edge.target.id] = true;
      edge.target.near[edge.source.id] = true;
      edge.source.edges.push(edge);
      edge.target.edges.push(edge);
    });
    nodes.forEach(function (node) {
      node.r = radius(node);
      node.charge = CHARGE[node.kind] || CHARGE.note;
    });
    return { nodes: nodes, edges: edges, byId: byId };
  }

  function describe(node) {
    if (node.kind === 'note') {
      if (node.daily && node.orphan) {
        return 'no wikilinks yet';
      }
      if (node.orphan) {
        return 'orphan: no wikilinks and not filed under a project';
      }
      return node.out + ' out, ' + node.inn + ' in';
    }
    var text = node.deg + (node.deg === 1 ? ' connection' : ' connections');
    if (node.unmatched) {
      text += ', ' + node.unmatched;
    }
    return text;
  }

  // MARK: - The simulation

  function Simulation(onFrame) {
    this.nodes = [];
    this.edges = [];
    this.active = [];
    this.live = [];
    this.alpha = 0;
    this.alphaTarget = 0;
    this.feel = FEELS.springy;
    this.decay = 0;
    this.raf = 0;
    this.onFrame = onFrame;
    this.onSettle = null;
    this.setFeel('springy');
  }

  Simulation.prototype.setFeel = function (name) {
    this.feel = FEELS[name] || FEELS.springy;
    this.decay = 1 - Math.pow(ALPHA_MIN, 1 / this.feel.life);
  };

  Simulation.prototype.load = function (nodes, edges) {
    this.nodes = nodes;
    this.edges = edges;
    this.refresh();
  };

  /* Rebuild the active lists and each node's active edge count. */
  Simulation.prototype.refresh = function () {
    this.active = this.nodes.filter(function (node) {
      return node.active;
    });
    this.live = this.edges.filter(function (edge) {
      return edge.active;
    });
    this.nodes.forEach(function (node) {
      node.count = 0;
    });
    this.live.forEach(function (edge) {
      edge.source.count += 1;
      edge.target.count += 1;
    });
  };

  Simulation.prototype.tick = function () {
    var feel = this.feel;
    var alpha = this.alpha + (this.alphaTarget - this.alpha) * this.decay;
    this.alpha = alpha;
    var nodes = this.active;
    var count = nodes.length;
    var kick = alpha * feel.charge;
    var i;
    var j;
    var a;
    var b;
    var dx;
    var dy;
    var d2;
    var distance;
    var weight;

    for (i = 0; i < count; i += 1) {
      a = nodes[i];
      var ax = a.x + a.vx;
      var ay = a.y + a.vy;
      var bigA = a.kind === 'project' || a.kind === 'label';
      for (j = i + 1; j < count; j += 1) {
        b = nodes[j];
        dx = b.x + b.vx - ax;
        dy = b.y + b.vy - ay;
        d2 = dx * dx + dy * dy;
        if (d2 === 0) {
          // Coincident nodes: a small jitter tells them apart.
          dx = ((i + j) % 2 ? 1 : -1) * 0.01 * (1 + (j % 3));
          dy = ((i * j) % 2 ? 1 : -1) * 0.01 * (1 + (i % 3));
          d2 = dx * dx + dy * dy;
        }
        if (d2 > REACH_SQUARED) {
          continue;
        }
        distance = Math.sqrt(d2);
        // Charge: each node is pushed away by the other's strength.
        weight = kick / (d2 < 1 ? 1 : d2);
        a.vx += dx * b.charge * weight;
        a.vy += dy * b.charge * weight;
        b.vx -= dx * a.charge * weight;
        b.vy -= dy * a.charge * weight;
        // Collision shares the loop: keep a minimum gap.
        var bigB = b.kind === 'project' || b.kind === 'label';
        var minimum = a.r + b.r + (bigA && bigB ? 26 : bigA || bigB ? 12 : 5);
        if (distance < minimum) {
          var push = ((minimum - distance) / distance) * 0.35 * 0.5;
          a.vx -= dx * push;
          a.vy -= dy * push;
          b.vx += dx * push;
          b.vy += dy * push;
        }
      }
    }

    var live = this.live;
    for (i = 0; i < live.length; i += 1) {
      var edge = live[i];
      var s = edge.source;
      var t = edge.target;
      dx = t.x + t.vx - s.x - s.vx;
      dy = t.y + t.vy - s.y - s.vy;
      if (dx === 0 && dy === 0) {
        dx = 0.01;
        dy = 0.01;
      }
      distance = Math.sqrt(dx * dx + dy * dy);
      var rest = (edge.kind === 'contains' ? 26 + 7 * Math.sqrt(s.count) : 70) * feel.rest;
      var strength = Math.min(1, (1 / Math.min(s.count, t.count)) * feel.spring);
      var pull = ((distance - rest) / distance) * alpha * strength;
      dx *= pull;
      dy *= pull;
      var bias = s.count / (s.count + t.count);
      t.vx -= dx * bias;
      t.vy -= dy * bias;
      s.vx += dx * (1 - bias);
      s.vy += dy * (1 - bias);
    }

    var gravity = feel.gravity * alpha;
    for (i = 0; i < count; i += 1) {
      a = nodes[i];
      var g = a.count < 3 ? gravity * 2.2 : gravity;
      a.vx -= a.x * g;
      a.vy -= a.y * g;
      if (a.fx !== null) {
        a.x = a.fx;
        a.y = a.fy;
        a.vx = 0;
        a.vy = 0;
        continue;
      }
      a.vx *= feel.keep;
      a.vy *= feel.keep;
      var speed = Math.sqrt(a.vx * a.vx + a.vy * a.vy);
      if (speed > MAX_SPEED) {
        a.vx = (a.vx / speed) * MAX_SPEED;
        a.vy = (a.vy / speed) * MAX_SPEED;
      }
      a.x += a.vx;
      a.y += a.vy;
    }
  };

  Simulation.prototype.hot = function () {
    return this.alpha > ALPHA_MIN || this.alphaTarget > 0;
  };

  /* Reheat to at least `alpha` and make sure the frame loop is running. */
  Simulation.prototype.reheat = function (alpha) {
    this.alpha = Math.max(this.alpha, alpha);
    if (reduceMotion()) {
      if (this.alphaTarget <= 0) {
        this.settle();
      }
      return;
    }
    this.start();
  };

  Simulation.prototype.start = function () {
    var self = this;
    if (this.raf) {
      return;
    }
    this.raf = window.requestAnimationFrame(function frame() {
      self.raf = 0;
      self.tick();
      self.onFrame(false);
      if (self.hot()) {
        self.raf = window.requestAnimationFrame(frame);
      } else if (self.onSettle) {
        self.onSettle();
      }
    });
  };

  /* Run ticks until cool, without a frame in between, then render once. */
  Simulation.prototype.settle = function () {
    var guard = 0;
    while (this.alpha > ALPHA_MIN && guard < 4000) {
      this.tick();
      guard += 1;
    }
    this.onFrame(true);
    if (this.onSettle) {
      this.onSettle();
    }
  };

  Simulation.prototype.stop = function () {
    if (this.raf) {
      window.cancelAnimationFrame(this.raf);
      this.raf = 0;
    }
  };

  // MARK: - One graph on the page

  function svgElement(name, attributes) {
    var element = document.createElementNS(SVG_NS, name);
    Object.keys(attributes || {}).forEach(function (key) {
      element.setAttribute(key, attributes[key]);
    });
    return element;
  }

  function easeOutCubic(t) {
    return 1 - Math.pow(1 - t, 3);
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
    var unpinButton = container.querySelector('[data-graph-unpin]');
    if (!svg || !canvas) {
      return;
    }

    var options = readOptions();
    var nodes = [];
    var edges = [];
    var byId = {};
    var elements = {};
    var viewport = null;
    var view = { x: 0, y: 0, k: 1 };
    var lit = null;
    var found = null;
    var generation = 0;
    var focusKey = '';
    var pins = {};
    var fitAt = null;
    var fitOnSettle = false;
    var viewFrame = 0;
    var tapTimer = 0;
    var lastTap = null;
    var pointers = {};
    var gesture = null;

    try {
      focusKey =
        new URL(container.getAttribute('data-src') || '', window.location.href).searchParams.get(
          'project'
        ) || '';
    } catch (error) {
      focusKey = '';
    }

    var sim = new Simulation(function (immediate) {
      render();
      if (fitAt !== null && sim.alpha < fitAt) {
        fitAt = null;
        fit(!immediate);
      }
    });
    sim.onSettle = function () {
      if (fitOnSettle) {
        fitOnSettle = false;
        fitAt = null;
        fit(true);
      }
    };
    sim.setFeel(options.feel);

    // MARK: Controls

    function syncControls() {
      container.querySelectorAll('[data-graph-option]').forEach(function (box) {
        box.checked = !!options[box.getAttribute('data-graph-option')];
      });
      ['daily', 'feel'].forEach(function (name) {
        container.querySelectorAll('[data-graph-' + name + ']').forEach(function (button) {
          button.setAttribute(
            'aria-pressed',
            button.getAttribute('data-graph-' + name) === options[name] ? 'true' : 'false'
          );
          if (button.parentNode) {
            button.parentNode.setAttribute('data-value', options[name]);
          }
        });
      });
    }

    function syncUnpin() {
      if (!unpinButton) {
        return;
      }
      var count = Object.keys(pins).length;
      unpinButton.textContent = count ? 'Unpin ' + count : 'Unpin';
      unpinButton.disabled = count === 0;
    }

    function showMessage(element, show) {
      if (element) {
        element.hidden = !show;
      }
    }

    // MARK: Drawing

    function dragging() {
      return gesture && gesture.dragging ? gesture.node : null;
    }

    /* Above the focused node, on every frame and every pan or zoom. */
    function placeTip() {
      if (!lit || !tip.classList.contains('gtip--on')) {
        return;
      }
      var scale = dragging() === lit ? 1.55 : 1.35;
      tip.style.left = Math.round(lit.x * view.k + view.x) + 'px';
      tip.style.top = Math.round(lit.y * view.k + view.y - lit.r * scale - 8) + 'px';
    }

    function applyView() {
      if (viewport) {
        viewport.setAttribute(
          'transform',
          'translate(' + view.x + ' ' + view.y + ') scale(' + view.k + ')'
        );
      }
      svg.style.setProperty('--k', String(view.k));
      svg.classList.toggle('gsvg--zoomed', view.k > 1.6);
      placeTip();
    }

    function render() {
      sim.active.forEach(function (node) {
        var element = elements[node.id];
        if (element) {
          element.setAttribute('transform', 'translate(' + node.x + ' ' + node.y + ')');
        }
      });
      sim.live.forEach(function (edge) {
        var line = edge.line;
        if (line) {
          line.setAttribute('x1', edge.source.x);
          line.setAttribute('y1', edge.source.y);
          line.setAttribute('x2', edge.target.x);
          line.setAttribute('y2', edge.target.y);
        }
      });
      placeTip();
    }

    function stopViewAnimation() {
      if (viewFrame) {
        window.cancelAnimationFrame(viewFrame);
        viewFrame = 0;
      }
    }

    function fitTarget() {
      var box = svg.getBoundingClientRect();
      var shown = sim.active;
      if (!shown.length || !box.width || !box.height) {
        return null;
      }
      var minX = Infinity;
      var minY = Infinity;
      var maxX = -Infinity;
      var maxY = -Infinity;
      shown.forEach(function (node) {
        minX = Math.min(minX, node.x - node.r);
        maxX = Math.max(maxX, node.x + node.r);
        minY = Math.min(minY, node.y - node.r);
        maxY = Math.max(maxY, node.y + node.r + 16);
      });
      // Leave room for the controls card on the right and the key below.
      var reserve = 0;
      if (controls && !controls.hidden && WIDE && WIDE.matches) {
        reserve = Math.max(0, box.right - controls.getBoundingClientRect().left + 12);
      }
      var left = 48;
      var top = 48;
      var availW = Math.max(box.width - left - 48 - reserve, 80);
      var availH = Math.max(box.height - top - 96, 80);
      var width = Math.max(maxX - minX, 1);
      var height = Math.max(maxY - minY, 1);
      var k = Math.max(Math.min(availW / width, availH / height, 1.6), 0.05);
      return {
        k: k,
        x: left + availW / 2 - ((minX + maxX) / 2) * k,
        y: top + availH / 2 - ((minY + maxY) / 2) * k
      };
    }

    /* The Fit button and the auto fits glide there in 600ms, ease-out cubic. */
    function fit(animate) {
      var target = fitTarget();
      if (!target) {
        return;
      }
      stopViewAnimation();
      if (!animate || reduceMotion()) {
        view.x = target.x;
        view.y = target.y;
        view.k = target.k;
        applyView();
        return;
      }
      var from = { x: view.x, y: view.y, k: view.k };
      var started = null;
      viewFrame = window.requestAnimationFrame(function step(now) {
        if (started === null) {
          started = now;
        }
        var t = Math.min((now - started) / 600, 1);
        var e = easeOutCubic(t);
        view.k = Math.exp(Math.log(from.k) + (Math.log(target.k) - Math.log(from.k)) * e);
        view.x = from.x + (target.x - from.x) * e;
        view.y = from.y + (target.y - from.y) * e;
        applyView();
        viewFrame = t < 1 ? window.requestAnimationFrame(step) : 0;
      });
    }

    function nodeElement(node) {
      var classes = 'gnode gnode--' + node.kind;
      if (node.kind === 'folder' && node.unmatched) {
        classes += ' gnode--unmatched';
      }
      if (node.kind === 'note') {
        if (node.daily) {
          classes += ' gnode--daily';
        } else if (node.orphan) {
          classes += ' gnode--orphan';
        }
      }
      var group = svgElement('g', {
        class: classes,
        tabindex: '0',
        role: 'link',
        'aria-label': node.kind + ' ' + node.name + ', ' + describe(node),
        'data-id': node.id
      });
      var size = node.r;
      group.appendChild(svgElement('circle', { class: 'ghit', r: Math.max(size + 4, 9) }));
      if (node.kind === 'label') {
        var reach = size * 1.25;
        group.appendChild(
          svgElement('polygon', {
            class: 'gshape',
            points: '0,' + -reach + ' ' + reach + ',0 0,' + reach + ' ' + -reach + ',0'
          })
        );
      } else {
        group.appendChild(svgElement('circle', { class: 'gshape', r: size }));
      }
      var pin = svgElement('g', { class: 'gpin' });
      pin.appendChild(svgElement('circle', { class: 'gpin__ring', r: size + 5 }));
      pin.appendChild(
        svgElement('circle', {
          class: 'gpin__dot',
          r: 2,
          cx: (size + 5) * 0.72,
          cy: -(size + 5) * 0.72
        })
      );
      group.appendChild(pin);
      group.appendChild(svgElement('circle', { class: 'gring', r: size + 6 }));
      var text = svgElement('text', {
        class:
          node.kind === 'project' || node.kind === 'label'
            ? 'glabel'
            : 'glabel glabel--minor',
        y: size,
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
      viewport = svgElement('g', { 'data-graph-viewport': '' });
      var edgeLayer = svgElement('g');
      var nodeLayer = svgElement('g');
      edges.forEach(function (edge) {
        var classes = 'gedge gedge--' + edge.kind + (edge.both ? ' gedge--both' : '');
        edge.line = svgElement('line', { class: classes });
        edgeLayer.appendChild(edge.line);
      });
      nodes.forEach(function (node) {
        var element = nodeElement(node);
        elements[node.id] = element;
        nodeLayer.appendChild(element);
        element.classList.toggle('gnode--pinned', node.fx !== null);
      });
      viewport.appendChild(edgeLayer);
      viewport.appendChild(nodeLayer);
      svg.appendChild(viewport);
      applyVisibility();
      applyView();
      render();
    }

    function applyVisibility() {
      nodes.forEach(function (node) {
        var element = elements[node.id];
        if (!element) {
          return;
        }
        element.classList.toggle('gnode--off', !node.active);
        element.setAttribute('tabindex', node.active ? '0' : '-1');
        if (node.active) {
          element.removeAttribute('aria-hidden');
        } else {
          element.setAttribute('aria-hidden', 'true');
        }
      });
      edges.forEach(function (edge) {
        if (edge.line) {
          edge.line.classList.toggle('gedge--off', !edge.active);
        }
      });
      svg.classList.toggle('gsvg--names', !!options.names);
    }

    // MARK: Which nodes and edges take part

    function wanted(node) {
      if (node.kind === 'note') {
        return options.notes;
      }
      if (node.kind === 'folder') {
        return options.folders;
      }
      return true;
    }

    /* Free daily mode drops the spokes out of the root daily folder. */
    function edgeWanted(edge) {
      if (!edge.source.active || !edge.target.active) {
        return false;
      }
      return !(
        options.daily === 'free' &&
        edge.kind === 'contains' &&
        edge.source.kind === 'folder' &&
        edge.source.path === DAILY_ROOT
      );
    }

    function setEdges() {
      edges.forEach(function (edge) {
        edge.active = edgeWanted(edge);
      });
    }

    /*
     * Toggle `active` on nodes and edges from the options, bring a returning
     * node back beside an active neighbour, and rebuild the lists. Positions
     * are kept: nothing is laid out cold.
     */
    function applyActive() {
      var before = {};
      nodes.forEach(function (node) {
        before[node.id] = node.active;
        node.active = wanted(node);
      });
      nodes.forEach(function (node) {
        if (!node.active || before[node.id]) {
          return;
        }
        var anchor = null;
        var ids = Object.keys(node.near);
        for (var pass = 0; pass < 2 && !anchor; pass += 1) {
          for (var i = 0; i < ids.length; i += 1) {
            var other = byId[ids[i]];
            if (other.active && (pass === 1 || before[other.id])) {
              anchor = other;
              break;
            }
          }
        }
        if (anchor && node.fx === null) {
          var angle = (node.index * 2.399963) % (Math.PI * 2);
          node.x = anchor.x + Math.cos(angle) * 15;
          node.y = anchor.y + Math.sin(angle) * 15;
        }
        node.vx = 0;
        node.vy = 0;
      });
      setEdges();
      sim.refresh();
      applyVisibility();
      if (lit && !lit.active) {
        clearLight();
      }
      render();
    }

    function filterChanged(heat) {
      applyActive();
      fitAt = 0.08;
      sim.reheat(heat);
    }

    // MARK: Pins

    function savePins() {
      writePins(focusKey, pins);
      syncUnpin();
    }

    function markPinned(node) {
      var element = elements[node.id];
      if (element) {
        element.classList.toggle('gnode--pinned', node.fx !== null);
      }
    }

    function pinNode(node) {
      node.fx = node.x;
      node.fy = node.y;
      pins[node.id] = [Math.round(node.x * 10) / 10, Math.round(node.y * 10) / 10];
      markPinned(node);
      savePins();
    }

    function unpinNode(node) {
      node.fx = null;
      node.fy = null;
      delete pins[node.id];
      markPinned(node);
      savePins();
    }

    function restorePins() {
      var stored = readAllPins()[focusKey];
      pins = {};
      var dropped = false;
      if (stored && typeof stored === 'object') {
        Object.keys(stored).forEach(function (id) {
          var node = byId[id];
          if (node && validPoint(stored[id])) {
            node.x = node.fx = stored[id][0];
            node.y = node.fy = stored[id][1];
            pins[id] = [stored[id][0], stored[id][1]];
          } else {
            dropped = true;
          }
        });
      }
      if (dropped) {
        writePins(focusKey, pins);
      }
      syncUnpin();
    }

    // MARK: Highlight

    function clearLight() {
      if (!lit) {
        return;
      }
      lit = null;
      svg.classList.remove('gsvg--focus');
      svg.querySelectorAll('.gnear, .glit').forEach(function (element) {
        element.classList.remove('gnear', 'glit');
      });
      tip.classList.remove('gtip--on');
    }

    function light(node) {
      if (!viewport || !node.active) {
        return;
      }
      if (lit !== node) {
        clearLight();
        lit = node;
        svg.classList.add('gsvg--focus');
        elements[node.id].classList.add('glit', 'gnear');
        Object.keys(node.near).forEach(function (id) {
          if (byId[id].active) {
            elements[id].classList.add('gnear');
          }
        });
        node.edges.forEach(function (edge) {
          if (edge.active) {
            edge.line.classList.add('gnear');
          }
        });
        tip.textContent = '';
        tip.appendChild(document.createTextNode(node.name + ' '));
        var small = document.createElement('small');
        small.textContent = describe(node);
        tip.appendChild(small);
      }
      tip.classList.add('gtip--on');
      placeTip();
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
        // Push, not replace: Back returns to the graph.
        window.history.pushState(null, '', href);
      } else {
        window.location.href = href;
      }
    }

    function selectInTree(node) {
      // The tree's fold state is owned by tandem.js: ask it by event.
      document.dispatchEvent(
        new CustomEvent('tandem:reveal-path', { detail: { path: node.path } })
      );
    }

    function ripple(node) {
      var element = elements[node.id];
      if (reduceMotion() || !element) {
        return;
      }
      var ring = svgElement('circle', { class: 'gripple', r: node.r });
      element.appendChild(ring);
      var remove = function () {
        if (ring.parentNode) {
          ring.parentNode.removeChild(ring);
        }
      };
      ring.addEventListener('animationend', remove);
      window.setTimeout(remove, 900);
    }

    // MARK: Pointer: drag a node, pan the background, wheel and pinch zoom

    function nodeOf(target) {
      var element = target.closest ? target.closest('.gnode') : null;
      var node = element ? byId[element.getAttribute('data-id')] : null;
      return node && node.active ? node : null;
    }

    function zoomAt(factor, cx, cy) {
      stopViewAnimation();
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

    function worldOf(event) {
      var box = svg.getBoundingClientRect();
      return {
        x: (event.clientX - box.left - view.x) / view.k,
        y: (event.clientY - box.top - view.y) / view.k
      };
    }

    /* Past the threshold the node is held at the pointer and its neighbours follow. */
    function beginDrag(node) {
      gesture.dragging = true;
      sim.alphaTarget = 0.3;
      window.clearTimeout(tapTimer);
      tapTimer = 0;
      lastTap = null;
      elements[node.id].classList.add('gnode--dragging');
      light(node);
      sim.reheat(0.3);
    }

    function dragTo(event) {
      var node = gesture.node;
      var point = worldOf(event);
      var now = window.performance.now();
      var dt = Math.max(now - gesture.stamp, 1);
      // A smoothed pointer velocity, in world units per 16ms frame.
      gesture.vx = gesture.vx * 0.7 + ((point.x - gesture.wx) / dt) * 16 * 0.3;
      gesture.vy = gesture.vy * 0.7 + ((point.y - gesture.wy) / dt) * 16 * 0.3;
      gesture.wx = point.x;
      gesture.wy = point.y;
      gesture.stamp = now;
      node.fx = point.x;
      node.fy = point.y;
      node.x = point.x;
      node.y = point.y;
      if (reduceMotion()) {
        for (var i = 0; i < 4; i += 1) {
          sim.tick();
        }
        render();
      } else {
        sim.start();
      }
    }

    function endDrag(finished, event) {
      var node = finished.node;
      elements[node.id].classList.remove('gnode--dragging');
      sim.alphaTarget = 0;
      if (event.shiftKey || pins[node.id] !== undefined) {
        // Shift on drop pins here; a pinned node keeps its pin, moved.
        pinNode(node);
      } else {
        node.fx = null;
        node.fy = null;
        if (!reduceMotion() && window.performance.now() - finished.stamp < 100) {
          node.vx = finished.vx * 0.9;
          node.vy = finished.vy * 0.9;
        }
      }
      sim.reheat(0.25);
      var under = document.elementFromPoint(event.clientX, event.clientY);
      var over = under ? nodeOf(under) : null;
      if (over !== node) {
        clearLight();
        if (over) {
          light(over);
        }
      }
    }

    /* Two taps on one node within 300ms pin it; one tap opens it after that. */
    function tapNode(node) {
      ripple(node);
      var now = window.performance.now();
      window.clearTimeout(tapTimer);
      if (lastTap && lastTap.id === node.id && now - lastTap.time < DOUBLE_CLICK_MS) {
        tapTimer = 0;
        lastTap = null;
        if (node.fx !== null) {
          unpinNode(node);
          sim.reheat(0.3);
        } else {
          pinNode(node);
        }
        return;
      }
      lastTap = { id: node.id, time: now };
      tapTimer = window.setTimeout(function () {
        tapTimer = 0;
        lastTap = null;
        open(node);
      }, DOUBLE_CLICK_MS);
    }

    svg.addEventListener('pointerdown', function (event) {
      if (event.button > 0) {
        return;
      }
      stopViewAnimation();
      pointers[event.pointerId] = { x: event.clientX, y: event.clientY };
      if (Object.keys(pointers).length === 2) {
        gesture = { type: 'pinch', distance: pinchDistance() };
        return;
      }
      var node = nodeOf(event.target);
      var point = worldOf(event);
      gesture = {
        type: node ? 'node' : 'pan',
        node: node,
        startX: event.clientX,
        startY: event.clientY,
        lastX: event.clientX,
        lastY: event.clientY,
        moved: false,
        dragging: false,
        wx: point.x,
        wy: point.y,
        vx: 0,
        vy: 0,
        stamp: window.performance.now()
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
      if (
        !gesture.moved &&
        Math.hypot(event.clientX - gesture.startX, event.clientY - gesture.startY) <
          DRAG_THRESHOLD
      ) {
        return;
      }
      var dx = event.clientX - gesture.lastX;
      var dy = event.clientY - gesture.lastY;
      gesture.moved = true;
      gesture.lastX = event.clientX;
      gesture.lastY = event.clientY;
      if (gesture.type === 'node') {
        if (!gesture.dragging) {
          beginDrag(gesture.node);
        }
        dragTo(event);
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
      if (Object.keys(pointers).length === 0 || (finished && finished.type === 'pinch')) {
        gesture = null;
      }
      if (!finished || finished.type !== 'node') {
        return;
      }
      if (finished.dragging) {
        endDrag(finished, event);
      } else if (!finished.moved && event.type === 'pointerup') {
        tapNode(finished.node);
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
      if (!gesture) {
        clearLight();
      }
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
      var text = search.value.trim().toLowerCase();
      if (!text) {
        return null;
      }
      for (var index = 0; index < nodes.length; index += 1) {
        if (nodes[index].active && nodes[index].name.toLowerCase().indexOf(text) >= 0) {
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
        var name = box.getAttribute('data-graph-option');
        options[name] = box.checked;
        writeOptions(options);
        if (!viewport) {
          return;
        }
        if (name === 'names') {
          // Every name reheats nothing.
          applyVisibility();
          return;
        }
        filterChanged(0.6);
      });
    });

    container.querySelectorAll('[data-graph-daily]').forEach(function (button) {
      button.addEventListener('click', function () {
        var choice = button.getAttribute('data-graph-daily');
        if (choice === options.daily) {
          return;
        }
        options.daily = choice;
        writeOptions(options);
        syncControls();
        if (viewport) {
          filterChanged(0.7);
        }
      });
    });

    container.querySelectorAll('[data-graph-feel]').forEach(function (button) {
      button.addEventListener('click', function () {
        var choice = button.getAttribute('data-graph-feel');
        if (choice === options.feel || !FEELS.hasOwnProperty(choice)) {
          return;
        }
        options.feel = choice;
        writeOptions(options);
        sim.setFeel(choice);
        syncControls();
        if (viewport) {
          sim.reheat(0.5);
        }
      });
    });

    var fitButton = container.querySelector('[data-graph-fit]');
    if (fitButton) {
      fitButton.addEventListener('click', function () {
        fit(true);
      });
    }

    if (unpinButton) {
      unpinButton.addEventListener('click', function () {
        var ids = Object.keys(pins);
        if (!ids.length) {
          return;
        }
        ids.forEach(function (id) {
          unpinNode(byId[id]);
        });
        sim.reheat(0.3);
      });
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
      sim.stop();
      var prepared = prepare(graph.nodes || [], graph.edges || []);
      nodes = prepared.nodes;
      edges = prepared.edges;
      byId = prepared.byId;
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
      restorePins();
      nodes.forEach(function (node) {
        node.active = wanted(node);
      });
      setEdges();
      sim.load(nodes, edges);
      draw();
      // The first settle fits twice: once mid-way, once when it rests.
      fitAt = 0.12;
      fitOnSettle = true;
      sim.alpha = 1;
      sim.alphaTarget = 0;
      if (reduceMotion()) {
        sim.settle();
      } else {
        sim.start();
      }
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
    syncUnpin();
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
  // A history snapshot carries the ready flag and the drawn svg: a restored
  // container is rebuilt, not skipped.
  document.addEventListener('htmx:historyRestore', function () {
    var container = document.getElementById('notes-graph');
    if (!container) {
      return;
    }
    delete container.dataset.graphReady;
    var svg = container.querySelector('[data-graph-svg]');
    if (svg) {
      while (svg.firstChild) {
        svg.removeChild(svg.firstChild);
      }
    }
    init();
  });
  if (document.readyState !== 'loading') {
    init();
  }
})();
