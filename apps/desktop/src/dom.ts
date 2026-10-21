// A tiny DOM helper: h("div.row", {onclick}, "text", child).

type Child = Node | string | number | null | undefined | false;
type Attrs = Record<string, unknown>;

export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K | `${K}.${string}` | `${K}#${string}`,
  attrs?: Attrs | Child,
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const [name, ...rest] = tag.split(/(?=[.#])/);
  const el = document.createElement(name as K);
  for (const r of rest) {
    if (r.startsWith(".")) el.classList.add(r.slice(1));
    else el.id = r.slice(1);
  }
  if (attrs && !(attrs instanceof Node) && typeof attrs === "object") {
    for (const [k, v] of Object.entries(attrs)) {
      if (v === undefined || v === null || v === false) continue;
      if (k.startsWith("on") && typeof v === "function") el.addEventListener(k.slice(2), v as EventListener);
      else if (k === "class") el.className = `${el.className} ${String(v)}`.trim();
      else if (k === "style" && typeof v === "object") Object.assign(el.style, v);
      else if (k in el && typeof v !== "string") (el as unknown as Record<string, unknown>)[k] = v;
      else el.setAttribute(k, v === true ? "" : String(v));
    }
  } else if (attrs !== undefined) {
    children.unshift(attrs as Child);
  }
  for (const c of children) {
    if (c === null || c === undefined || c === false) continue;
    el.append(c instanceof Node ? c : String(c));
  }
  return el;
}

export function clear(el: Element): void {
  while (el.firstChild) el.removeChild(el.firstChild);
}

/** Highlight `query` inside `text` (case-insensitive) with <mark>. */
export function highlight(text: string, query: string): Node {
  const frag = document.createDocumentFragment();
  const q = query.trim().toLowerCase();
  const i = q ? text.toLowerCase().indexOf(q) : -1;
  if (i < 0) {
    frag.append(text);
    return frag;
  }
  frag.append(text.slice(0, i), h("mark", text.slice(i, i + q.length)), text.slice(i + q.length));
  return frag;
}

/** "Ba<u>n</u>king": underline the hotkey letter inside the label. */
export function hotkeyLabel(label: string, letter: string): Node {
  const frag = document.createDocumentFragment();
  const l = letter.toLowerCase();
  // A translated label (Arabic) shows the key as a cap before it, even when a
  // Latin word in it ("e-Way") happens to contain the letter.
  if (l && /[\u0600-\u06FF]/.test(label)) {
    frag.append(h("kbd.hk-key", letter.toUpperCase()), label);
    return frag;
  }
  // Prefer a word-initial match, else the first occurrence.
  let i = -1;
  for (let k = 0; k < label.length; k++) {
    if (label[k].toLowerCase() === l && (k === 0 || label[k - 1] === " ")) {
      i = k;
      break;
    }
  }
  if (i < 0) i = label.toLowerCase().indexOf(l);
  if (i < 0) {
    // Any other label without the letter: show the key as a cap before it.
    if (l && !/[a-z]/i.test(label)) frag.append(h("kbd.hk-key", letter.toUpperCase()));
    frag.append(label);
    return frag;
  }
  frag.append(label.slice(0, i), h("span.hk", label[i]), label.slice(i + 1));
  return frag;
}
