'use strict';

const invoke = window.__TAURI__.core.invoke;
const $ = (id) => document.getElementById(id);

const state = {
    docs: [],
    raw: false,
    selected: null,
    filter: '',
    expanded: new Set(),
    tab: 'details',
};

const ICONS = {
    doc: 'M3 1h6l4 4v10H3zM9 1v4h4',
    header: 'M2 3h12M2 7h12M2 11h8',
    folder: 'M1.5 3.5h5l1.5 1.5h6.5v8h-13z',
    shapes: 'M8 2l6 11H2z',
    morphshapes: 'M2 13c3-9 9-9 12 0M5 4h6',
    sprites: 'M2 2h5v5H2zM9 9h5v5H9zM9 2h5v5H9z',
    texts: 'M3 3h10M8 3v10M6 13h4',
    images: 'M2 3h12v10H2zM2 11l4-4 3 3 2-2 3 3',
    movies: 'M2 4h9v8H2zM11 7l3-2v6l-3-2',
    sounds: 'M2 6h3l4-3v10l-4-3H2zM11 5c1.5 1.5 1.5 4.5 0 6',
    buttons: 'M2 5h12v6H2zM6 8h4',
    fonts: 'M3 13l5-10 5 10M5 9h6',
    binaryData: 'M3 3h3v4H3zM10 3v4M3 10v4M8 10h3v4H8z',
    frames: 'M2 3h12v10H2zM2 6h12M6 3v3M10 3v3',
    scripts: 'M5 4L1.5 8 5 12M11 4l3.5 4-3.5 4M9 3l-2 10',
    others: 'M3 8h.01M8 8h.01M13 8h.01',
};

function icon(cat) {
    const d = ICONS[cat] || ICONS.others;
    return `<svg class="icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" stroke-linecap="round"><path d="${d}"/></svg>`;
}

function esc(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

function status(msg, isError = false) {
    $('status').textContent = msg;
    $('status').classList.toggle('error', isError);
}

async function call(cmd, args) {
    try {
        return await invoke(cmd, args);
    } catch (e) {
        status(String(e), true);
        throw e;
    }
}

function selectedDoc() {
    return state.selected && state.docs.find((d) => d.id === state.selected.docId);
}

// ---- documents ----

async function loadTree(doc) {
    doc.tree = await call('get_tree', { id: doc.id, raw: state.raw });
}

async function addDocs(opened) {
    for (const o of opened) {
        const doc = { ...o, tree: null };
        await loadTree(doc);
        state.docs.push(doc);
        state.expanded.add(`${doc.id}:`);
        if (o.warnings.length) status(`${o.name}: ${o.warnings[0]}`, true);
        else status(`Opened ${o.path}`);
        render();
        select({ docId: doc.id, key: `${doc.id}:`, node: { kind: 'header', label: doc.name, category: 'header' } });
    }
    render();
}

async function openDialog() {
    addDocs(await call('open_dialog'));
}

async function openPaths(paths) {
    const opened = [];
    for (const p of paths) {
        try {
            opened.push(await call('open_path', { path: p }));
        } catch (_) { /* reported in status */ }
    }
    addDocs(opened);
}

async function closeSelected() {
    const doc = selectedDoc();
    if (!doc) return;
    await call('close_doc', { id: doc.id });
    state.docs = state.docs.filter((d) => d !== doc);
    state.selected = null;
    render();
    showSelection();
}

async function setView(raw) {
    if (state.raw === raw) return;
    state.raw = raw;
    $('view-res').classList.toggle('active', !raw);
    $('view-tags').classList.toggle('active', raw);
    for (const d of state.docs) await loadTree(d);
    render();
}

// ---- tree ----

function matches(node, f) {
    if (node.label.toLowerCase().includes(f)) return true;
    return node.children.some((c) => matches(c, f));
}

function highlight(label) {
    const f = state.filter;
    if (!f) return esc(label);
    const i = label.toLowerCase().indexOf(f);
    if (i < 0) return esc(label);
    return esc(label.slice(0, i)) + '<mark>' + esc(label.slice(i, i + f.length)) + '</mark>' + esc(label.slice(i + f.length));
}

function rowHtml(key, label, cat, depth, hasKids, open, extraClass, path) {
    const sel = state.selected && state.selected.key === key ? ' selected' : '';
    const tw = hasKids ? `<svg viewBox="0 0 10 10" class="${open ? 'open' : ''}"><path d="M3 2l4 3-4 3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>` : '';
    const pathHtml = path && state.raw ? `<span class="path">${esc(path)}</span>` : '';
    return `<li role="treeitem" aria-expanded="${hasKids ? open : ''}"><div class="row${sel}${extraClass}" data-key="${esc(key)}" style="padding-left:${6 + depth * 14}px"><span class="twisty">${tw}</span>${icon(cat)}<span class="label">${highlight(label)}</span>${pathHtml}</div>`;
}

const nodeIndex = new Map();

function renderNodes(doc, nodes, prefix, depth, out) {
    const f = state.filter;
    nodes.forEach((n, i) => {
        if (f && !matches(n, f)) return;
        const key = `${doc.id}:${prefix}${i}`;
        nodeIndex.set(key, { docId: doc.id, key, node: n });
        const hasKids = n.children.length > 0;
        const open = hasKids && (!!f || state.expanded.has(key));
        const cat = n.kind === 'folder' ? 'folder' : n.category;
        out.push(rowHtml(key, n.kind === 'folder' ? `${n.label} (${n.children.length})` : n.label, cat, depth, hasKids, open, '', n.path));
        if (open) {
            out.push('<ul role="group">');
            renderNodes(doc, n.children, `${prefix}${i}.`, depth + 1, out);
            out.push('</ul>');
        }
        out.push('</li>');
    });
}

function render() {
    nodeIndex.clear();
    const out = ['<ul>'];
    for (const doc of state.docs) {
        const key = `${doc.id}:`;
        const open = state.expanded.has(key) || !!state.filter;
        nodeIndex.set(key, { docId: doc.id, key, node: { kind: 'header', label: doc.name, category: 'header' } });
        out.push(rowHtml(key, doc.name, 'doc', 0, true, open, ' doc'));
        if (open) {
            out.push('<ul role="group">');
            renderNodes(doc, doc.tree, '', 1, out);
            out.push('</ul>');
        }
        out.push('</li>');
    }
    out.push('</ul>');
    $('tree').innerHTML = out.join('');
    updateButtons();
}

function toggle(key) {
    if (state.expanded.has(key)) state.expanded.delete(key);
    else state.expanded.add(key);
    render();
}

function updateButtons() {
    const has = state.docs.length > 0;
    $('empty').hidden = has;
    $('content').hidden = !has || !state.selected;
    $('btn-save').disabled = !has;
    $('btn-close').disabled = !state.selected;
    $('btn-export').disabled = !(state.selected && state.selected.node.kind === 'tag');
}

function select(sel) {
    state.selected = sel;
    $('tree').querySelectorAll('.row.selected').forEach((r) => r.classList.remove('selected'));
    const el = [...$('tree').querySelectorAll('.row')].find((r) => r.dataset.key === sel.key);
    if (el) el.classList.add('selected');
    updateButtons();
    showSelection();
    const row = $('tree').querySelector('.row.selected');
    if (row) row.scrollIntoView({ block: 'nearest' });
}

$('tree').addEventListener('click', (e) => {
    const row = e.target.closest('.row');
    if (!row) return;
    const sel = nodeIndex.get(row.dataset.key);
    if (e.target.closest('.twisty')) return toggle(sel.key);
    select(sel);
});

$('tree').addEventListener('dblclick', (e) => {
    const row = e.target.closest('.row');
    if (row) toggle(row.dataset.key);
});

$('tree').addEventListener('keydown', (e) => {
    const rows = [...$('tree').querySelectorAll('.row')];
    if (!rows.length) return;
    const i = rows.findIndex((r) => r.classList.contains('selected'));
    const cur = i >= 0 ? nodeIndex.get(rows[i].dataset.key) : null;
    const go = (j) => {
        const r = rows[Math.max(0, Math.min(rows.length - 1, j))];
        select(nodeIndex.get(r.dataset.key));
    };
    const hasKids = cur && (cur.node.children ? cur.node.children.length > 0 : true);
    if (e.key === 'ArrowDown') go(i + 1);
    else if (e.key === 'ArrowUp') go(i - 1);
    else if (e.key === 'Home') go(0);
    else if (e.key === 'End') go(rows.length - 1);
    else if (e.key === 'ArrowRight' && cur && hasKids && !state.expanded.has(cur.key)) toggle(cur.key);
    else if (e.key === 'ArrowLeft' && cur && state.expanded.has(cur.key)) toggle(cur.key);
    else if (e.key === 'Enter' && cur && hasKids) toggle(cur.key);
    else return;
    e.preventDefault();
});

$('filter').addEventListener('input', (e) => {
    state.filter = e.target.value.trim().toLowerCase();
    render();
});

// ---- details panel ----

function fieldRows(fields) {
    return fields
        .map((f) => `<tr class="${/warning|error/i.test(f.name) ? 'warn' : ''}"><td>${esc(f.name)}</td><td>${esc(f.value)}</td></tr>`)
        .join('');
}

let hexState = null;

async function showSelection() {
    const sel = state.selected;
    $('content').hidden = !sel;
    if (!sel) return;
    const doc = selectedDoc();
    const n = sel.node;
    $('sel-title').textContent = n.kind === 'header' ? doc.name : n.label;
    const isTag = !!n.path && n.kind !== 'folder';
    document.querySelector('.tab[data-tab="hex"]').hidden = !isTag;
    if (!isTag && state.tab === 'hex') setTab('details');

    let fields;
    if (n.kind === 'header') {
        fields = await call('header_info', { id: doc.id });
    } else if (n.kind === 'folder') {
        fields = [{ name: 'Items', value: String(n.children.length) }];
    } else if (n.kind === 'frame' && !n.path) {
        fields = [{ name: 'Tags in frame', value: String(n.children.length) }];
    } else {
        const info = await call('tag_info', { id: doc.id, path: n.path });
        fields = info.fields;
        if (n.kind === 'frame') fields.unshift({ name: 'Tags in frame', value: String(n.children.length) });
        hexState = { docId: doc.id, path: n.path, total: info.size, pages: new Map() };
    }
    if (state.selected !== sel) return;
    $('fields').innerHTML = fieldRows(fields);
    if (isTag) resetHex();
}

function setTab(tab) {
    state.tab = tab;
    document.querySelectorAll('.tab').forEach((t) => t.classList.toggle('active', t.dataset.tab === tab));
    $('tab-details').hidden = tab !== 'details';
    $('tab-hex').hidden = tab !== 'hex';
    if (tab === 'hex') drawHex();
}

document.querySelectorAll('.tab').forEach((t) => t.addEventListener('click', () => setTab(t.dataset.tab)));

// ---- hex view (virtualised, fetched in 64 KiB pages) ----

const ROW = 20;
const PAGE = 65536;

function resetHex() {
    $('hex').scrollTop = 0;
    $('hex-spacer').style.height = `${Math.max(1, Math.ceil(hexState.total / 16)) * ROW}px`;
    drawHex();
}

async function page(hs, p) {
    if (!hs.pages.has(p)) {
        hs.pages.set(p, call('tag_bytes', { id: hs.docId, path: hs.path, offset: p * PAGE, len: PAGE }).then((b) => b.data));
    }
    return hs.pages.get(p);
}

const hx = (v, w) => v.toString(16).padStart(w, '0');

async function drawHex() {
    const hs = hexState;
    if (!hs || state.tab !== 'hex') return;
    if (hs.total === 0) {
        $('hex-rows').innerHTML = '<div class="hex-row">(empty tag body)</div>';
        return;
    }
    const el = $('hex');
    const first = Math.floor(el.scrollTop / ROW);
    const count = Math.ceil(el.clientHeight / ROW) + 2;
    const last = Math.min(Math.ceil(hs.total / 16), first + count);
    const startByte = first * 16;
    const endByte = Math.min(hs.total, last * 16);
    const pages = [];
    for (let p = Math.floor(startByte / PAGE); p <= Math.floor((endByte - 1) / PAGE); p++) pages.push([p, await page(hs, p)]);
    if (hexState !== hs) return;
    const byteAt = (o) => {
        const [, data] = pages.find(([p]) => p === Math.floor(o / PAGE));
        return data[o % PAGE];
    };
    const lines = [];
    for (let r = first; r < last; r++) {
        const off = r * 16;
        let hex = '';
        let asc = '';
        for (let j = 0; j < 16; j++) {
            const o = off + j;
            if (o < hs.total) {
                const b = byteAt(o);
                hex += hx(b, 2) + (j === 7 ? '  ' : ' ');
                asc += b >= 32 && b < 127 ? String.fromCharCode(b) : '.';
            } else {
                hex += j === 7 ? '    ' : '   ';
            }
        }
        lines.push(`<div class="hex-row"><span class="off">${hx(off, 8)}</span>${hex}<span class="asc">${esc(asc)}</span></div>`);
    }
    $('hex-rows').style.transform = `translateY(${first * ROW}px)`;
    $('hex-rows').innerHTML = lines.join('');
}

$('hex').addEventListener('scroll', () => requestAnimationFrame(drawHex));
window.addEventListener('resize', () => requestAnimationFrame(drawHex));

// ---- toolbar ----

$('btn-open').addEventListener('click', openDialog);
$('btn-open-empty').addEventListener('click', openDialog);
$('btn-close').addEventListener('click', closeSelected);
$('view-res').addEventListener('click', () => setView(false));
$('view-tags').addEventListener('click', () => setView(true));

$('btn-save').addEventListener('click', (e) => {
    e.stopPropagation();
    $('save-menu').hidden = !$('save-menu').hidden;
});
document.addEventListener('click', (e) => {
    if (!e.target.closest('.menu-wrap')) $('save-menu').hidden = true;
});

$('btn-save-go').addEventListener('click', async () => {
    $('save-menu').hidden = true;
    const doc = selectedDoc() || state.docs[0];
    if (!doc) return;
    const compression = document.querySelector('input[name="comp"]:checked').value;
    const out = await call('save_as', { id: doc.id, compression, encrypt: $('opt-encrypt').checked });
    if (out) status(`Saved ${out}`);
});

$('btn-export').addEventListener('click', async () => {
    const sel = state.selected;
    if (!sel || !sel.node.path) return;
    const out = await call('export_tag', { id: sel.docId, path: sel.node.path });
    if (out) status(`Exported ${out}`);
});

document.addEventListener('keydown', (e) => {
    if (!e.ctrlKey) return;
    if (e.key === 'o') openDialog();
    else if (e.key === 's' && state.docs.length) $('btn-save').click();
    else if (e.key === 'f') $('filter').focus();
    else return;
    e.preventDefault();
});

// ---- sidebar resize ----

$('splitter').addEventListener('pointerdown', (e) => {
    const sp = e.currentTarget;
    sp.setPointerCapture(e.pointerId);
    const move = (ev) => {
        const w = Math.max(200, Math.min(window.innerWidth - 300, ev.clientX));
        document.querySelector('.workspace').style.setProperty('--side', `${w}px`);
    };
    sp.addEventListener('pointermove', move);
    sp.addEventListener('pointerup', () => sp.removeEventListener('pointermove', move), { once: true });
});

// ---- drag and drop, startup files ----

window.__TAURI__.webview.getCurrentWebview().onDragDropEvent((e) => {
    const p = e.payload;
    if (p.type === 'enter' || p.type === 'over') $('drop').hidden = false;
    else if (p.type === 'leave') $('drop').hidden = true;
    else if (p.type === 'drop') {
        $('drop').hidden = true;
        openPaths(p.paths);
    }
});

call('startup_files').then((files) => files.length && openPaths(files));
render();
