'use strict';

/**
 * Gedeelde slide-renderer voor de zender (preview) en de ontvanger (scherm).
 * Eén specificatie, overal hetzelfde beeld — inclusief alle lay-outs en het
 * vrije canvas met tekst, afbeeldingen en vormen.
 */
(function () {
  const LAYOUTS = ['title', 'text', 'image-left', 'image-right', 'full', 'quote', 'free'];
  const SHAPES = ['rect', 'round', 'ellipse', 'triangle', 'line', 'arrow', 'star'];
  const ANIMS = ['none', 'fade', 'up', 'zoom'];
  const FONTS = {
    sans: 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif',
    serif: 'Georgia, "Times New Roman", serif',
    mono: '"Cascadia Mono", Consolas, "Courier New", monospace',
    display: '"Arial Black", "Segoe UI", Impact, sans-serif',
  };
  const SVG = 'http://www.w3.org/2000/svg';

  function el(tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined && text !== null) node.textContent = text;
    return node;
  }

  function clamp(value, min, max) {
    const number = Number(value);
    if (!Number.isFinite(number)) return min;
    return Math.min(max, Math.max(min, number));
  }

  function bullets(body) {
    return String(body)
      .split(/\n+/)
      .map((line) => line.trim().replace(/^[-•*]\s*/, ''))
      .filter(Boolean);
  }

  function starPoints() {
    const points = [];
    for (let i = 0; i < 10; i++) {
      const radius = i % 2 === 0 ? 50 : 19;
      const angle = (Math.PI / 5) * i - Math.PI / 2;
      points.push((50 + radius * Math.cos(angle)).toFixed(2) + ',' + (50 + radius * Math.sin(angle)).toFixed(2));
    }
    return points.join(' ');
  }

  function svgNode(tag, attributes) {
    const node = document.createElementNS(SVG, tag);
    for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, String(value));
    return node;
  }

  /** Vorm als SVG, zodat vulling, rand en afmetingen altijd meeschalen. */
  function shapeNode(element) {
    const shape = SHAPES.includes(element.shape) ? element.shape : 'rect';
    const svg = svgNode('svg', {
      class: 'freeShape',
      viewBox: '0 0 100 100',
      preserveAspectRatio: 'none',
      'aria-hidden': 'true',
    });

    const radius = clamp(element.radius, 0, 50);
    const fill = element.fill && element.fill !== 'transparent' ? element.fill : 'none';
    const stroke = element.stroke && element.stroke !== 'transparent' ? element.stroke : 'none';
    const strokeWidth = clamp(element.strokeWidth, 0, 20);

    let node;
    if (shape === 'ellipse') node = svgNode('ellipse', { cx: 50, cy: 50, rx: 50, ry: 50 });
    else if (shape === 'triangle') node = svgNode('polygon', { points: '50,1 99,99 1,99' });
    else if (shape === 'line') node = svgNode('line', { x1: 0, y1: 50, x2: 100, y2: 50 });
    else if (shape === 'arrow') node = svgNode('polygon', { points: '2,36 60,36 60,16 98,50 60,84 60,64 2,64' });
    else if (shape === 'star') node = svgNode('polygon', { points: starPoints() });
    else if (shape === 'round') node = svgNode('rect', { x: 1, y: 1, width: 98, height: 98, rx: radius });
    else node = svgNode('rect', { x: 1, y: 1, width: 98, height: 98 });

    const solid = shape === 'line';
    node.setAttribute('fill', solid ? 'none' : fill);
    node.setAttribute('stroke', solid && stroke === 'none' ? (element.fill || '#f8fafc') : stroke);
    node.setAttribute('stroke-width', String(strokeWidth || (solid ? 3 : 0)));
    node.setAttribute('stroke-linejoin', 'round');
    node.setAttribute('stroke-linecap', 'round');
    svg.appendChild(node);
    return svg;
  }

  function textElement(element) {
    const node = el('div', 'freeEl freeText');
    const size = clamp(element.size || 6, 1, 40);
    node.style.fontSize = 'calc(' + size + ' * 1cqh)';
    node.style.fontFamily = FONTS[element.font] || FONTS.sans;
    node.style.fontWeight = element.weight === 'bold' ? '700' : '400';
    node.style.fontStyle = element.italic ? 'italic' : 'normal';
    node.style.textDecoration = element.underline ? 'underline' : 'none';
    node.style.lineHeight = String(clamp(element.lineHeight || 1.28, 0.9, 2.4));
    node.style.letterSpacing = clamp(element.letterSpacing || 0, -0.05, 0.2) + 'em';
    node.style.textAlign = ['left', 'center', 'right'].includes(element.align) ? element.align : 'left';
    node.style.color = element.color || 'inherit';
    node.style.alignItems =
      element.valign === 'top' ? 'flex-start' : element.valign === 'bottom' ? 'flex-end' : 'center';

    const lines = String(element.text || '').split(/\n+/).filter((line) => line.trim() !== '');
    if (element.bullet && lines.length) {
      const list = el('ul', 'freeBullets');
      for (const line of lines) list.appendChild(el('li', null, line));
      node.appendChild(list);
    } else {
      node.textContent = element.text || '';
    }
    return node;
  }

  function imageElement(element) {
    const node = el('div', 'freeEl freeImage');
    if (element.imageUrl) {
      const img = el('img');
      img.src = element.imageUrl;
      img.alt = '';
      img.style.objectFit = element.fit === 'contain' ? 'contain' : 'cover';
      node.appendChild(img);
    }
    return node;
  }

  function elementNode(element) {
    const type = element.type === 'image' ? 'image' : element.type === 'shape' ? 'shape' : 'text';
    const node = type === 'image' ? imageElement(element) : type === 'shape' ? el('div', 'freeEl freeShapeBox') : textElement(element);

    if (element.id) node.setAttribute('data-el', element.id);
    node.style.left = clamp(element.x, 0, 100) + '%';
    node.style.top = clamp(element.y, 0, 100) + '%';
    node.style.width = clamp(element.w, 2, 100) + '%';
    node.style.height = clamp(element.h, 2, 100) + '%';
    if (element.background && element.background !== 'transparent' && type !== 'shape') {
      node.style.background = element.background;
    }
    if (element.radius && type !== 'shape') node.style.borderRadius = clamp(element.radius, 0, 24) + 'px';
    if (element.opacity !== undefined) node.style.opacity = String(clamp(element.opacity, 0.05, 1));

    const rotation = clamp(element.rotation, -180, 180);
    const flips = (element.flipX ? ' scaleX(-1)' : '') + (element.flipY ? ' scaleY(-1)' : '');
    if (rotation || flips) node.style.transform = 'rotate(' + rotation + 'deg)' + flips;
    if (element.shadow) node.style.filter = 'drop-shadow(0 0.6cqh 1.4cqh rgba(0, 0, 0, .45))';

    if (type === 'shape') node.appendChild(shapeNode(element));

    const anim = ANIMS.includes(element.anim) ? element.anim : 'none';
    if (anim !== 'none') {
      node.dataset.anim = anim;
      node.style.setProperty('--anim-delay', clamp(element.delay, 0, 20) + 's');
    }
    return node;
  }

  function textBlock(spec, options) {
    const withBar = !options || options.withBar !== false;
    const wrap = el('div', 'slideText');

    if (withBar && spec.title) wrap.appendChild(el('span', 'slideBar'));
    if (spec.title) wrap.appendChild(el('h1', null, spec.title));
    if (spec.subtitle) wrap.appendChild(el('p', 'slideSub', spec.subtitle));

    if (spec.body) {
      const lines = bullets(spec.body);
      if (lines.length) {
        const list = el('ul', 'slideList');
        for (const line of lines) list.appendChild(el('li', null, line));
        wrap.appendChild(list);
      }
    }
    if (spec.footer) wrap.appendChild(el('div', 'slideFooter', spec.footer));
    return wrap;
  }

  function imageEl(spec) {
    if (!spec.imageUrl) return null;
    const img = el('img', 'slideImg');
    img.src = spec.imageUrl;
    img.alt = '';
    return img;
  }

  function background(view, spec) {
    const first = /^#[0-9a-f]{6}$/i.test(spec.bg || '') ? spec.bg : '#0f172a';
    const second = /^#[0-9a-f]{6}$/i.test(spec.bg2 || '') ? spec.bg2 : null;
    if (second && second.toLowerCase() !== first.toLowerCase()) {
      const angle = clamp(spec.bgAngle === undefined ? 135 : spec.bgAngle, 0, 360);
      view.style.background = 'linear-gradient(' + angle + 'deg, ' + first + ', ' + second + ')';
    } else {
      view.style.background = first;
    }
  }

  function render(spec, container) {
    const s = spec || {};
    const layout = LAYOUTS.includes(s.layout) ? s.layout : 'title';

    const view = el('div', 'slideView layout-' + layout + ' ' + (s.align === 'left' ? 'left' : 'center'));
    view.style.fontFamily = FONTS[s.font] || FONTS.sans;
    view.style.setProperty('--slide-bg', s.bg || '#0f172a');
    view.style.setProperty('--slide-fg', s.fg || '#f8fafc');
    view.style.setProperty('--slide-accent', s.accent || '#5b8cff');
    const scale = Number(s.scale) || 1;
    view.style.setProperty('--slide-scale', String(Math.min(1.5, Math.max(0.7, scale))));
    background(view, s);

    if (layout === 'free') {
      if (s.imageUrl) {
        const bg = imageEl(s);
        bg.classList.add('slideBg');
        view.appendChild(bg);
        const shade = el('div', 'slideShade');
        shade.style.opacity = String(clamp(s.shade === undefined ? 0.45 : s.shade, 0, 1));
        view.appendChild(shade);
      }

      const layer = el('div', 'freeLayer');
      for (const raw of s.elements || []) layer.appendChild(elementNode(raw || {}));
      view.appendChild(layer);
    } else if (layout === 'full') {
      const img = imageEl(s);
      if (img) {
        img.classList.add('slideBg');
        view.appendChild(img);
      }
      const shade = el('div', 'slideShade');
      shade.style.opacity = String(clamp(s.shade === undefined ? 0.45 : s.shade, 0, 1));
      view.appendChild(shade);
      view.appendChild(textBlock(s));
    } else if (layout === 'image-left' || layout === 'image-right') {
      const split = el('div', 'slideSplit');
      const figure = el('figure', 'slideFigure');
      const img = imageEl(s);
      if (img) figure.appendChild(img);
      const block = textBlock(s);
      if (layout === 'image-left') split.append(figure, block);
      else split.append(block, figure);
      view.appendChild(split);
    } else if (layout === 'quote') {
      const wrap = el('div', 'slideText');
      if (s.title) wrap.appendChild(el('h1', 'quoteTitle', s.title));
      if (s.body) wrap.appendChild(el('blockquote', null, s.body));
      if (s.subtitle) wrap.appendChild(el('p', 'quoteBy', s.subtitle));
      if (s.footer) wrap.appendChild(el('div', 'slideFooter', s.footer));
      view.appendChild(wrap);
    } else {
      view.appendChild(textBlock(s));
    }

    container.innerHTML = '';
    container.appendChild(view);
    return view;
  }

  window.SlideRender = { render, layouts: LAYOUTS, shapes: SHAPES, animations: ANIMS, fonts: Object.keys(FONTS) };
})();
