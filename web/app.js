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
    tabPref: 'preview',
    player: null,
    exportable: false,
    codeView: 'as',
    method: null,
    editor: null,
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
    package: 'M2 5l6-3 6 3v6l-6 3-6-3zM2 5l6 3 6-3M8 8v6',
    class: 'M3 3h10v10H3zM10 6.5a2.5 2.5 0 1 0 0 3',
    interface: 'M8 2l6 6-6 6-6-6z',
    function: 'M6 13c2 0 2-10 4-10M5 7h5',
    var: 'M4 5l4 7 4-7',
    const: 'M4 5l4 7 4-7M3 13h10',
    script: 'M5 4L1.5 8 5 12M11 4l3.5 4-3.5 4',
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
    if (!doc || !leaveEditor()) return;
    if (doc.modified && !(await call('confirm_discard', { name: doc.name }))) return;
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
        const cat = n.kind === 'folder' && n.category !== 'package' ? 'folder' : n.category;
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
        out.push(rowHtml(key, doc.modified ? `${doc.name} •` : doc.name, 'doc', 0, true, open, ' doc'));
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
    const doc = selectedDoc();
    $('btn-save').disabled = !doc || !doc.modified;
    $('btn-save-as').disabled = !doc;
    $('btn-close').disabled = !state.selected;
    const tagSelected = !!(state.selected && (state.selected.node.kind === 'tag' || state.selected.node.kind === 'script'));
    $('btn-export').disabled = !tagSelected;
    $('btn-replace').disabled = !state.importable;
    $('btn-raw').disabled = !tagSelected;
    $('btn-remove').disabled = !(state.selected && state.selected.node.kind === 'tag');
    $('btn-export-all').disabled = !has;
}

function select(sel) {
    if (state.selected && sel.key !== state.selected.key && !leaveEditor()) return;
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

// Which timeline a node previews: the root (path '') or a sprite, and at which frame.
function previewTarget(n) {
    if (n.kind === 'header') return { path: '', frame: 0 };
    if (n.kind === 'frame') {
        const frame = Math.max(0, (parseInt(n.label.replace(/\D+/g, ''), 10) || 1) - 1);
        const parts = (n.path || '').split('/');
        return { path: parts.length > 1 ? parts.slice(0, -1).join('/') : '', frame };
    }
    if (n.kind === 'tag') return { path: n.path, frame: 0 };
    if (n.kind === 'script') return { path: n.path, frame: 0, script: n.script };
    return null;
}

async function showSelection() {
    const sel = state.selected;
    $('content').hidden = !sel;
    if (!sel) return;
    const doc = selectedDoc();
    const n = sel.node;
    $('sel-title').textContent = n.kind === 'header' ? doc.name : n.label;
    const isTag = !!n.path && n.kind !== 'folder' && n.kind !== 'script';
    document.querySelector('.tab[data-tab="hex"]').hidden = !isTag;
    stopPlayback();
    clearPreview();
    state.exportable = false;
    state.importable = false;
    state.editor = null;
    updateButtons();

    const target = previewTarget(n);
    const previewCall = target ? call('preview_info', { id: doc.id, path: target.path, frame: target.frame, script: target.script || null }).catch(() => null) : null;
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
        state.importable = info.importable && n.kind === 'tag';
        if (n.kind === 'frame') fields.unshift({ name: 'Tags in frame', value: String(n.children.length) });
        hexState = { docId: doc.id, path: n.path, total: info.size, pages: new Map() };
    }
    const preview = await previewCall;
    if (state.selected !== sel) return;
    $('fields').innerHTML = fieldRows(fields);
    $('details-bar').hidden = n.kind !== 'header';
    if (isTag) resetHex();

    const hasPreview = !!preview;
    document.querySelector('.tab[data-tab="preview"]').hidden = !hasPreview;
    state.exportable = !!(preview && ((preview.exportable && n.kind === 'tag') || n.kind === 'script'));
    updateButtons();
    if (hasPreview) {
        state.player = { docId: doc.id, path: target.path, info: preview };
        const data = preview.size ? await invoke('preview_data', { id: doc.id, path: target.path }) : new ArrayBuffer(0);
        if (state.selected !== sel) return;
        renderPreview(preview, data);
    }
    // Follow the tab the user last picked, falling back when it is not available here.
    let tab = state.tabPref;
    if (tab === 'preview' && !hasPreview) tab = 'details';
    if (tab === 'hex' && !isTag) tab = 'details';
    setTab(tab, false);
}

let previewUrl = null;
let previewFont = null;

function clearPreview() {
    if (previewUrl) URL.revokeObjectURL(previewUrl);
    if (previewFont) document.fonts.delete(previewFont);
    previewUrl = null;
    previewFont = null;
    $('preview').innerHTML = '';
    $('preview-note').textContent = '';
}

function frameControls(info) {
    if (info.frames <= 1) return '';
    return `<div class="player" role="group" aria-label="Frames">`
        + `<button class="btn icon" data-act="prev" title="Previous frame">◀</button>`
        + `<button class="btn icon" data-act="play" title="Play">▶</button>`
        + `<button class="btn icon" data-act="next" title="Next frame">▶|</button>`
        + `<input type="range" min="1" max="${info.frames}" value="${info.frame + 1}" aria-label="Frame">`
        + `<span class="frame-no">${info.frame + 1} / ${info.frames}</span></div>`;
}

function renderPreview(info, data) {
    clearPreview();
    const el = $('preview');
    $('preview-note').textContent = info.note || '';
    if (info.kind === 'image' || info.kind === 'vector') {
        previewUrl = URL.createObjectURL(new Blob([data], { type: info.mime }));
        const cls = info.kind === 'vector' ? 'checker vector' : 'checker';
        el.innerHTML = frameControls(info) + `<div class="${cls}"><img alt="" src="${previewUrl}"></div>`;
        const img = el.querySelector('img');
        img.addEventListener('load', () => {
            // Blow small pictures up by a whole factor so pixels and lines stay crisp.
            const k = Math.max(1, Math.floor(256 / Math.max(img.naturalWidth, img.naturalHeight, 1)));
            img.style.width = `${img.naturalWidth * k}px`;
        });
        bindPlayer(el);
        if (info.kind === 'vector' && info.text) {
            el.insertAdjacentHTML('beforeend', `<pre class="text-preview">${esc(info.text)}</pre>`);
        }
        textEditButton(el);
    } else if (info.kind === 'audio') {
        previewUrl = URL.createObjectURL(new Blob([data], { type: info.mime }));
        el.innerHTML = `<audio controls preload="metadata" src="${previewUrl}"></audio>`;
    } else if (info.kind === 'font') {
        const family = `ffdec-preview-${Date.now()}`;
        const face = new FontFace(family, data);
        previewFont = face;
        face.load().then((f) => {
            if (previewFont !== face) return;
            document.fonts.add(f);
            const chars = [...(info.text || '')];
            const sample = chars.filter((c) => c.trim()).slice(0, 60).join('') || 'The quick brown fox';
            el.innerHTML = `<input class="font-sample" spellcheck="false" style="font-family:'${family}'" value="${esc(sample)}" aria-label="Sample text">`
                + `<div class="glyphs" style="font-family:'${family}'">${chars.map((c) => `<span title="U+${c.codePointAt(0).toString(16).toUpperCase().padStart(4, '0')}">${esc(c)}</span>`).join('')}</div>`;
            fontAddBar(el);
        }).catch(() => {
            el.innerHTML = '<p class="preview-empty">This font could not be loaded for preview.</p>';
        });
    } else if (info.kind === 'text') {
        el.innerHTML = info.text ? `<pre class="text-preview">${esc(info.text)}</pre>` : '<p class="preview-empty">No text.</p>';
        textEditButton(el);
    } else if (info.kind === 'code') {
        renderCode(el, info);
    }
}

// DefineFont2/3 take new glyphs from a font file: the characters typed here, and new shapes
// for those they have.
function fontAddBar(el) {
    const sel = state.selected;
    const n = sel && sel.node;
    if (!n || n.kind !== 'tag' || !/^DefineFont[23] /.test(n.label)) return;
    el.insertAdjacentHTML('afterbegin', '<div class="code-bar"><input class="font-add" spellcheck="false" placeholder="Characters to add" aria-label="Characters to add">'
        + '<button class="btn" data-act="font-add" title="Pick a TrueType or OpenType font to redraw these glyphs and add the characters typed">From font file…</button></div>');
    el.querySelector('[data-act="font-add"]').addEventListener('click', async () => {
        const chars = el.querySelector('.font-add').value;
        let count;
        try {
            count = await invoke('add_font_chars', { id: sel.docId, path: n.path, chars });
        } catch (e) {
            status(String(e), true);
            return;
        }
        if (count == null) return;
        const doc = state.docs.find((d) => d.id === sel.docId);
        doc.modified = true;
        await loadTree(doc);
        render();
        status(`Set ${count} glyph${count === 1 ? '' : 's'}`);
        showSelection();
    });
}

function textEditButton(el) {
    const n = state.selected && state.selected.node;
    if (!n || n.kind !== 'tag' || n.category !== 'texts') return;
    el.insertAdjacentHTML('afterbegin', '<div class="code-bar"><button class="btn" data-act="edit-text" title="Edit this text">Edit</button></div>');
    el.querySelector('[data-act="edit-text"]').addEventListener('click', () => openTextEditor(el));
}

// Decompiled ActionScript, with a switch to p-code, which can be edited: AS1/2 scripts whole,
// AS3 scripts one method at a time.
async function renderCode(el, info) {
    const sel = state.selected;
    const n = sel.node;
    const pcode = state.codeView === 'pcode' && info.alt_text != null;
    const methods = pcode && n.kind === 'script' ? await call('script_methods', { id: sel.docId, path: n.path, script: n.script }).catch(() => []) : [];
    if (state.selected !== sel) return;
    if (!methods.includes(state.method)) state.method = null;
    const asEditable = !pcode && n.kind === 'script' && n.script.startsWith('as2:');
    const editable = asEditable || (pcode && n.kind === 'script' && (!methods.length || state.method !== null));
    let text = (pcode ? info.alt_text : info.text) || '';
    if (editable && methods.length) text = await call('script_pcode', { id: sel.docId, path: n.path, script: n.script, method: state.method });
    if (state.selected !== sel) return;
    const seg = info.alt_text == null ? '' : `<div class="seg" role="tablist" aria-label="Code view">`
        + `<button class="seg-btn${pcode ? '' : ' active'}" data-view="as" role="tab">ActionScript</button>`
        + `<button class="seg-btn${pcode ? ' active' : ''}" data-view="pcode" role="tab">P-code</button></div>`;
    const pick = methods.length ? `<select class="method" aria-label="Method"><option value="">Whole script</option>`
        + methods.map((m) => `<option${m === state.method ? ' selected' : ''}>${esc(m)}</option>`).join('') + '</select>' : '';
    const edit = editable ? `<button class="btn" data-act="edit" title="Edit this ${pcode ? 'p-code' : 'ActionScript'}">Edit</button>` : '';
    const hint = pcode && methods.length && !editable ? '<span class="code-hint">Pick a method to edit it.</span>' : '';
    el.innerHTML = `<div class="code-bar">${seg}${pick}${edit}${hint}</div><pre class="code-view">${esc(text)}</pre>`;
    el.querySelectorAll('.seg-btn').forEach((b) => b.addEventListener('click', () => {
        state.codeView = b.dataset.view;
        renderCode(el, info);
    }));
    el.querySelector('.method')?.addEventListener('change', (e) => {
        state.method = e.target.value || null;
        renderCode(el, info);
    });
    el.querySelector('[data-act="edit"]')?.addEventListener('click', () => openEditor(el, info, text, pcode));
}

function openEditor(el, info, text, pcode) {
    const sel = state.selected;
    const n = sel.node;
    editInPlace(el, {
        title: !pcode ? 'Editing ActionScript' : state.method ? `Editing ${state.method}` : 'Editing p-code',
        label: pcode ? 'P-code' : 'ActionScript',
        verb: pcode ? 'Assemble' : 'Compile',
        text,
        store: (value) => pcode
            ? invoke('apply_pcode', { id: sel.docId, path: n.path, script: n.script, method: state.method, text: value })
            : invoke('apply_source', { id: sel.docId, path: n.path, script: n.script, text: value }),
        stored: pcode && state.method ? `Updated ${state.method}` : 'Updated the script',
        back: () => renderCode(el, info),
    });
}

async function openTextEditor(el) {
    const sel = state.selected;
    const n = sel.node;
    let text;
    try {
        text = await invoke('text_source', { id: sel.docId, path: n.path });
    } catch (e) {
        status(String(e), true);
        return;
    }
    if (state.selected !== sel || text == null) return;
    editInPlace(el, {
        title: 'Editing text',
        label: 'Text',
        verb: 'Check',
        text,
        store: (value) => invoke('apply_text', { id: sel.docId, path: n.path, text: value }),
        stored: 'Updated the text',
        back: () => showSelection(),
    });
}

// A text area over the preview whose Apply stores the text, showing errors by line.
function editInPlace(el, { title, label, verb, text, store, stored, back }) {
    const sel = state.selected;
    state.editor = { original: text };
    el.innerHTML = `<div class="code-bar"><span class="code-title">${esc(title)}</span>`
        + `<button class="btn primary" data-act="apply" title="${verb} and store (Ctrl+Enter)">Apply</button>`
        + '<button class="btn" data-act="cancel" title="Discard these edits (Esc)">Cancel</button></div>'
        + '<p class="code-error" hidden></p>'
        + `<textarea class="code-edit" spellcheck="false" wrap="off" aria-label="${label}">${esc(text)}</textarea>`;
    const area = el.querySelector('textarea');
    area.focus();
    const cancel = () => {
        state.editor = null;
        back();
    };
    const apply = async () => {
        try {
            await store(area.value);
        } catch (e) {
            showEditError(el, area, e);
            return;
        }
        state.editor = null;
        const doc = state.docs.find((d) => d.id === sel.docId);
        doc.modified = true;
        await loadTree(doc);
        render();
        status(stored);
        showSelection();
    };
    el.querySelector('[data-act="apply"]').addEventListener('click', apply);
    el.querySelector('[data-act="cancel"]').addEventListener('click', cancel);
    area.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' && e.ctrlKey) { e.preventDefault(); apply(); }
        else if (e.key === 'Escape') { e.preventDefault(); if (leaveEditor()) cancel(); }
        else if (e.key === 'Tab') { e.preventDefault(); area.setRangeText('   ', area.selectionStart, area.selectionEnd, 'end'); }
    });
}

// Shows an assembler error and selects the line it points at.
function showEditError(el, area, e) {
    const line = e && e.line ? e.line : 0;
    const msg = e && e.message ? e.message : String(e);
    const box = el.querySelector('.code-error');
    box.textContent = line ? `Line ${line}: ${msg}` : msg;
    box.hidden = false;
    status(box.textContent, true);
    if (!line) return;
    const lines = area.value.split('\n');
    const start = lines.slice(0, line - 1).reduce((a, l) => a + l.length + 1, 0);
    area.focus();
    area.setSelectionRange(start, start + (lines[line - 1] || '').length);
    area.scrollTop = Math.max(0, (line - 4) * parseFloat(getComputedStyle(area).lineHeight));
}

// True when it is fine to leave the editor, asking first if its text has changed.
function leaveEditor() {
    const ed = state.editor;
    const area = document.querySelector('.code-edit');
    if (!ed || !area || area.value === ed.original) return true;
    if (!window.confirm('Discard your edits?')) return false;
    state.editor = null;
    return true;
}

// ---- frame player ----

let playTimer = null;

function stopPlayback() {
    if (playTimer) clearTimeout(playTimer);
    playTimer = null;
    const b = document.querySelector('.player [data-act="play"]');
    if (b) {
        b.textContent = '▶';
        b.title = 'Play';
    }
}

// Swaps in another frame of the current preview without rebuilding the controls.
async function showFrame(frame) {
    const p = state.player;
    if (!p) return false;
    const n = p.info.frames;
    frame = ((frame % n) + n) % n;
    const info = await call('preview_info', { id: p.docId, path: p.path, frame });
    if (!info || state.player !== p) return false;
    const data = await invoke('preview_data', { id: p.docId, path: p.path });
    if (state.player !== p) return false;
    p.info = info;
    const url = URL.createObjectURL(new Blob([data], { type: info.mime }));
    const img = $('preview').querySelector('img');
    if (img) img.src = url;
    if (previewUrl) URL.revokeObjectURL(previewUrl);
    previewUrl = url;
    const range = $('preview').querySelector('.player input');
    if (range) range.value = String(info.frame + 1);
    const label = $('preview').querySelector('.frame-no');
    if (label) label.textContent = `${info.frame + 1} / ${info.frames}`;
    $('preview-note').textContent = info.note || '';
    return true;
}

function bindPlayer(el) {
    const bar = el.querySelector('.player');
    if (!bar) return;
    bar.addEventListener('click', (e) => {
        const act = e.target.closest('button')?.dataset.act;
        const p = state.player;
        if (!act || !p) return;
        if (act === 'prev') { stopPlayback(); showFrame(p.info.frame - 1); }
        if (act === 'next') { stopPlayback(); showFrame(p.info.frame + 1); }
        if (act === 'play') {
            if (playTimer) return stopPlayback();
            e.target.textContent = '❚❚';
            e.target.title = 'Pause';
            const step = async () => {
                const started = performance.now();
                if (!(await showFrame(state.player.info.frame + 1)) || !playTimer) return;
                const delay = 1000 / Math.max(1, state.player.info.frameRate || 24);
                playTimer = setTimeout(step, Math.max(0, delay - (performance.now() - started)));
            };
            playTimer = setTimeout(step, 0);
        }
    });
    bar.querySelector('input').addEventListener('input', (e) => {
        stopPlayback();
        showFrame(parseInt(e.target.value, 10) - 1);
    });
}

function setTab(tab, remember = true) {
    if (remember) state.tabPref = tab;
    state.tab = tab;
    document.querySelectorAll('.tab').forEach((t) => t.classList.toggle('active', t.dataset.tab === tab));
    $('tab-preview').hidden = tab !== 'preview';
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

async function saveSelected() {
    const doc = selectedDoc();
    if (!doc || !doc.modified) return;
    status(`Saved ${await call('save', { id: doc.id })}`);
    doc.modified = false;
    render();
}

$('btn-save').addEventListener('click', saveSelected);

$('btn-save-as').addEventListener('click', (e) => {
    e.stopPropagation();
    $('export-menu').hidden = true;
    $('save-menu').hidden = !$('save-menu').hidden;
});

$('btn-replace').addEventListener('click', async () => {
    const sel = state.selected;
    if (!sel || !state.importable) return;
    const file = await call('replace_asset', { id: sel.docId, path: sel.node.path });
    if (!file) return;
    const doc = state.docs.find((d) => d.id === sel.docId);
    doc.modified = true;
    await loadTree(doc);
    render();
    status(`Replaced ${sel.node.label} with ${file}`);
    const again = nodeIndex.get(sel.key);
    state.selected = null;
    if (again) select(again);
});
document.addEventListener('click', (e) => {
    if (!e.target.closest('.menu-wrap')) {
        $('save-menu').hidden = true;
        $('export-menu').hidden = true;
    }
});

$('btn-save-go').addEventListener('click', async () => {
    $('save-menu').hidden = true;
    const doc = selectedDoc();
    if (!doc) return;
    const compression = document.querySelector('input[name="comp"]:checked').value;
    const out = await call('save_as', { id: doc.id, compression, encrypt: $('opt-encrypt').checked });
    if (!out) return;
    Object.assign(doc, { name: out.name, path: out.path, modified: false });
    render();
    status(`Saved ${out.path}`);
});

$('btn-export').addEventListener('click', async () => {
    const sel = state.selected;
    if (!sel || !sel.node.path) return;
    const out = state.exportable
        ? await call('export_asset', { id: sel.docId, path: sel.node.path, script: sel.node.script || null, options: { ...exportOptions(), pcode: sel.node.script ? state.codeView === 'pcode' : $('opt-pcode').checked } })
        : await call('export_tag', { id: sel.docId, path: sel.node.path });
    if (out) status(`Exported ${out}`);
});

$('btn-raw').addEventListener('click', async () => {
    const sel = state.selected;
    if (!sel || !sel.node.path) return;
    const out = await call('export_tag', { id: sel.docId, path: sel.node.path });
    if (out) status(`Exported ${out}`);
});

$('btn-remove').addEventListener('click', async () => {
    const sel = state.selected;
    if (!sel || sel.node.kind !== 'tag' || !leaveEditor()) return;
    if (!window.confirm(`Remove ${sel.node.label}? Tags that refer to it are kept.`)) return;
    try {
        await invoke('remove_tag', { id: sel.docId, path: sel.node.path });
    } catch (e) {
        status(String(e), true);
        return;
    }
    const doc = state.docs.find((d) => d.id === sel.docId);
    doc.modified = true;
    state.selected = null;
    await loadTree(doc);
    render();
    status(`Removed ${sel.node.label}`);
});

// The header's editable fields as a form in place of the details table.
$('btn-edit-header').addEventListener('click', async () => {
    const sel = state.selected;
    if (!sel || sel.node.kind !== 'header') return;
    const h = await call('get_header', { id: sel.docId });
    const row = (label, input) => `<tr><td>${label}</td><td>${input}</td></tr>`;
    const bg = h.background || '#ffffff';
    $('details-bar').hidden = true;
    $('fields').innerHTML = row('SWF version', `<input class="field-input" id="h-version" type="number" min="1" max="255" value="${h.version}">`)
        + row('Frame rate', `<input class="field-input" id="h-rate" type="number" min="0" max="255.99" step="0.01" value="${h.frame_rate}"> fps`)
        + row('Width', `<input class="field-input" id="h-width" type="number" min="0.05" step="0.05" value="${h.width}"> px`)
        + row('Height', `<input class="field-input" id="h-height" type="number" min="0.05" step="0.05" value="${h.height}"> px`)
        + row('Background', `<input id="h-bg-on" type="checkbox"${h.background ? ' checked' : ''} aria-label="Set a background colour"> <input id="h-bg" type="color" value="${esc(bg)}" aria-label="Background colour">`)
        + row('', '<button class="btn primary" id="h-apply">Apply</button> <button class="btn" id="h-cancel">Cancel</button> <span class="code-error" id="h-error" hidden></span>');
    $('h-cancel').addEventListener('click', () => showSelection());
    $('h-apply').addEventListener('click', async () => {
        const header = {
            version: parseInt($('h-version').value, 10) || 0,
            frame_rate: parseFloat($('h-rate').value),
            width: parseFloat($('h-width').value),
            height: parseFloat($('h-height').value),
            background: $('h-bg-on').checked ? $('h-bg').value : null,
        };
        try {
            await invoke('set_header', { id: sel.docId, header });
        } catch (e) {
            $('h-error').textContent = String(e);
            $('h-error').hidden = false;
            return;
        }
        const doc = state.docs.find((d) => d.id === sel.docId);
        doc.modified = true;
        render();
        status('Updated the header');
        showSelection();
    });
});

function exportOptions() {
    return {
        png: $('opt-png').checked,
        wav: $('opt-wav').checked,
        shapesPng: $('opt-shapes-png').checked,
        framesSvg: $('opt-frames-svg').checked,
        textsSvg: $('opt-texts-svg').checked,
        zoom: parseFloat($('opt-zoom').value) || 1,
        pcode: $('opt-pcode').checked,
    };
}

$('btn-export-all').addEventListener('click', (e) => {
    e.stopPropagation();
    $('save-menu').hidden = true;
    $('export-menu').hidden = !$('export-menu').hidden;
});

$('btn-export-go').addEventListener('click', async () => {
    $('export-menu').hidden = true;
    const doc = selectedDoc() || state.docs[0];
    if (!doc) return;
    const kinds = [...document.querySelectorAll('input[name="kind"]:checked')].map((i) => i.value);
    status('Exporting…');
    const r = await call('export_all', { id: doc.id, kinds, options: exportOptions() });
    if (!r) return status('Ready');
    if (r.errors.length) status(`Exported ${r.written} files to ${r.dir}; ${r.errors.length} failed: ${r.errors[0]}`, true);
    else status(`Exported ${r.written} files to ${r.dir}`);
});

document.addEventListener('keydown', (e) => {
    if (!e.ctrlKey) return;
    if (e.key === 'o') openDialog();
    else if (e.key.toLowerCase() === 's' && e.shiftKey) { if (selectedDoc()) $('btn-save-as').click(); }
    else if (e.key === 's') saveSelected();
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
