'use strict';
const api = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);
const $ = (s, root = document) => root.querySelector(s);
const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const time = (s) => { const t = Math.max(0, Math.floor(s || 0)); return `${Math.floor(t / 60)}:${String(t % 60).padStart(2, '0')}`; };
const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

const icon = {
  note: '<svg viewBox="0 0 24 24"><path d="M12 3v10.6a4 4 0 1 0 2 3.4V7h4V3z"/></svg>',
  play: '<svg viewBox="0 0 24 24"><path d="M8 5v14l11-7z"/></svg>',
  pause: '<svg viewBox="0 0 24 24"><path d="M6 5h4v14H6zm8 0h4v14h-4z"/></svg>',
  next: '<svg viewBox="0 0 24 24"><path d="M6 5l10 7-10 7zm11 0h2v14h-2z"/></svg>',
  prev: '<svg viewBox="0 0 24 24"><path d="M18 5L8 12l10 7zM5 5h2v14H5z"/></svg>',
  shuffle: '<svg viewBox="0 0 24 24"><path d="M17 3l4 4-4 4V8h-2.2l-8 9H3v-2h2.8l8-9H17zm0 10l4 4-4 4v-3h-3.2l-3-3.4 1.4-1.5 2.4 2.9H17zM3 7h3.8l3 3.4-1.4 1.5L6 9H3z"/></svg>',
  repeat: '<svg viewBox="0 0 24 24"><path d="M7 7h10V4l4 4-4 4V9H9v3H7zm10 10H7v3l-4-4 4-4v3h8v-3h2z"/></svg>',
  one: '<svg viewBox="0 0 24 24"><path d="M7 7h10V4l4 4-4 4V9H9v3H7zm10 10H7v3l-4-4 4-4v3h8v-3h2zM11.5 10H13v5h-1.5v-3.5l-1 .4v-1.2z"/></svg>',
  more: '<svg viewBox="0 0 24 24"><circle cx="5" cy="12" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="19" cy="12" r="2"/></svg>',
  close: '<svg viewBox="0 0 24 24"><path d="M6.4 5L12 10.6 17.6 5 19 6.4 13.4 12l5.6 5.6-1.4 1.4-5.6-5.6L6.4 19 5 17.6l5.6-5.6L5 6.4z"/></svg>',
  up: '<svg viewBox="0 0 24 24"><path d="M12 7l7 8H5z"/></svg>',
  clock: '<svg viewBox="0 0 24 24"><path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm0 18a8 8 0 1 1 0-16 8 8 0 0 1 0 16zm.5-13H11v6l5.2 3.1.8-1.3-4.5-2.7z"/></svg>',
  speaker: '<svg viewBox="0 0 24 24"><path d="M3 9v6h4l5 5V4L7 9zm13.5 3A4.5 4.5 0 0 0 14 8v8a4.5 4.5 0 0 0 2.5-4zM14 3.2v2.1a7 7 0 0 1 0 13.4v2.1a9 9 0 0 0 0-17.6z"/></svg>',
};

const ui = { devices: { running: false, locked: true, nearby: [], paired: [], pairing: null }, license: { pro: false, expired: false, devices: [], limit: 4 }, keyDraft: '', keyShown: false, licenseBusy: false, licenseMessage: '', page: 'playing', songs: [], playlists: [], folders: [], sound: null, state: null, constants: null,
  library: { tab: 1, term: '', detail: null, sort: 0 }, playlist: null, dragging: false, editing: false, sleepMinutes: 0, lastScan: null, scanning: false };
let sleepTimeout;

// ---- Artwork, loaded as rows come into view -------------------------------------------------
const artLoaded = new Map();
const artWatcher = new IntersectionObserver((entries) => {
  for (const entry of entries) {
    if (!entry.isIntersecting) continue;
    artWatcher.unobserve(entry.target);
    fillArt(entry.target);
  }
}, { rootMargin: '300px' });

const artKey = (el) => `${el.dataset.art}@${Number(el.dataset.size || 120)}`;
function showArt(el, found) { el.style.backgroundImage = `url("${found}")`; el.classList.add('has'); }
async function fillArt(el) {
  const id = el.dataset.art, size = Number(el.dataset.size || 120);
  if (!id) return;
  const key = artKey(el);
  let found = artLoaded.get(key);
  if (found === undefined) { found = await api('art', { id, size }); if (found || !id.startsWith('r:')) artLoaded.set(key, found); }
  if (el.dataset.art === id && found) showArt(el, found);
}
// No artwork: a cover made from the album's name, the same colour and initials as on the phones.
function coverHue(name) {
  let hash = 2166136261;
  for (const byte of new TextEncoder().encode(name.trim().toLowerCase())) hash = Math.imul(hash ^ byte, 16777619) >>> 0;
  return hash % 360;
}
function coverInitials(name) {
  let found = '';
  for (const word of name.trim().split(/\s+/)) {
    const first = [...word].find((c) => /[\p{L}\p{N}]/u.test(c));
    if (first) found += first.toUpperCase();
    if ([...found].length >= 2) break;
  }
  return found;
}
function hsv(h, s, v) {
  const f = (n) => { const k = (n + h / 60) % 6; return Math.round(255 * (v - v * s * Math.max(0, Math.min(k, 4 - k, 1)))); };
  return `rgb(${f(5)},${f(3)},${f(1)})`;
}
const art = (track, cls = '', style = '', size = 120) => {
  if (!track) return `<div class="art ${cls}" style="${style}">${icon.note}</div>`;
  const name = track.album === 'Unknown album' ? track.title : track.album;
  return `<div class="art made ${cls}" data-art="${esc(track.id)}" data-size="${size}" style="background-color:${hsv(coverHue(name), 0.35, 0.5)};${style}"><span>${esc(coverInitials(name))}</span></div>`;
};
// Artwork already loaded is shown at once, so redrawing a page never flashes the made-up cover.
function watchArt(root = document) {
  root.querySelectorAll('.art[data-art]:not(.has)').forEach((el) => {
    const found = artLoaded.get(artKey(el));
    if (found) showArt(el, found); else artWatcher.observe(el);
  });
}

// ---- Screens ---------------------------------------------------------------------------------
function show(page) {
  ui.page = page;
  document.querySelectorAll('#rail button').forEach((b) => { const on = b.dataset.page === page; b.classList.toggle('on', on); if (on) b.setAttribute('aria-current', 'page'); else b.removeAttribute('aria-current'); });
  render();
}

function render() { ({ playing: renderPlaying, library: renderLibrary, playlists: renderPlaylists, sound: renderSound, settings: renderSettings })[ui.page](); watchArt(); api('set_meter', { wanted: ui.page === 'playing' }); }

const statusWords = () => {
  const s = ui.sound; if (!s) return '';
  if (ui.comparing) return 'The original, at the same loudness';
  if (!s.effects_on) return 'Effects off';
  const exciter = s.exciter_on ? `Exciter ${Math.round(s.amount * 100)} % from ${(s.start_hz / 1000).toString()} kHz` : 'Exciter off';
  return `${exciter} · ${s.bass_on ? `Bass ${Math.round(s.bass_db * 10)} %` : 'Bass off'}`;
};

function renderPlaying() {
  const page = $('#page'); page.className = 'fixed';
  const st = ui.state || {}, t = st.current;
  const format = t ? [t.kind, t.sample_rate ? `${(t.sample_rate / 1000).toFixed(1)} kHz` : ''].filter(Boolean).join(', ') : `${ui.songs.length} songs in your library`;
  page.innerHTML = `
    <div class="head"><span class="grow"></span><button class="chip ${ui.sleepEnd ? 'on' : ''}" id="sleep" aria-label="Sleep timer">${icon.clock}<span id="sleepText">${sleepWords()}</span></button><span class="chip" id="outChip" title="Current audio output">${icon.speaker}<span id="outText">${esc(st.device || 'System audio')}</span></span></div>
    <div id="stage">
      <div id="hero" class="card">
        ${art(t || (ui.suggestions || [])[0], '', '', 700)}
        <div><div class="label">${t ? 'Now playing' : 'Your music'}</div>
        <div class="t">${esc(t ? t.title : 'Ready when you are')}</div>
        <div class="s">${esc(t ? `${t.artist}, ${t.album}` : ui.songs.length ? 'Press play to shuffle your music' : 'Add a music folder to begin')}</div>
        <div class="f">${st.fetching ? `Getting this song from ${esc(st.fetching)}…` : st.failed ? `${t && t.id.startsWith('r:') ? `Couldn't get this song from ${esc(hostName(t))}` : "Can't play this file"} <button class="text-action" id="retry">Retry</button><button class="text-action" id="skip">Skip</button>` : esc(format)}</div></div>
      </div>
      <div id="side">
        <div class="card" id="spectrumCard">
          <div class="pills"><button class="pill ${ui.comparing ? 'on' : ''}" id="compare" aria-pressed="${!!ui.comparing}" aria-label="Compare with the original">${ui.comparing ? 'Original' : 'Compare'}</button>
          <span class="spacer"></span>
          <button class="pill ${ui.sound && ui.sound.effects_on ? 'on' : ''}" id="effectsP" aria-pressed="${!!(ui.sound && ui.sound.effects_on)}" aria-label="Effects on or off">Effects</button>
          <button class="pill ${ui.sound && ui.sound.eq_on ? 'on' : ''}" id="eqSwitch" aria-pressed="${!!(ui.sound && ui.sound.eq_on)}" aria-label="Equalizer on or off">EQ</button></div>
          <div class="status">${esc(statusWords())}</div>
          <canvas id="spectrum"></canvas>
          <div class="hints"><span>Bass</span><span>What Xciter adds in champagne</span><span>Air</span></div>
        </div>
        <div class="card hug">${upNext(st)}</div>
      </div>
    </div>`;
  $('#compare').onclick = async () => { ui.comparing = !ui.comparing; await api('compare', { on: ui.comparing }); renderPlaying(); };
  $('#sleep').onclick = chooseSleepTimer;
  if ($('#addQueue')) $('#addQueue').onclick = addFolder;
  $('#effectsP').onclick = () => setSound({ effects_on: !ui.sound.effects_on }).then(renderPlaying);
  $('#eqSwitch').onclick = () => setSound({ eq_on: !ui.sound.eq_on }).then(renderPlaying);
  if ($('#retry')) $('#retry').onclick = () => api('retry_failed').then(refreshState);
  if ($('#skip')) $('#skip').onclick = () => api('skip_failed').then(refreshState);
  wireRows(page);
  const edit = $('#edit'); if (edit) edit.onclick = () => { ui.editing = !ui.editing; renderPlaying(); };
  page.querySelectorAll('[data-mv]').forEach((b) => b.onclick = (e) => { e.stopPropagation(); const [from, to] = b.dataset.mv.split(',').map(Number); api('move_up_next', { from, to }).then(refreshState); });
  page.querySelectorAll('[data-rm]').forEach((b) => b.onclick = (e) => { e.stopPropagation(); api('remove_up_next', { id: b.dataset.rm }).then(refreshState); });
  watchArt(page);
}

function chooseSleepTimer() {
  const o = dialog(`<h2>Sleep timer</h2><div class="choices"><button data-minutes="0">Off</button><button data-minutes="15">15 minutes</button><button data-minutes="30">30 minutes</button><button data-minutes="60">1 hour</button></div><div class="end"><button class="button quiet" id="c">Cancel</button></div>`);
  o.querySelectorAll('[data-minutes]').forEach((b) => b.onclick = () => setSleepTimer(Number(b.dataset.minutes)));
  $('#c', o).onclick = closeDialog;
}

// What the Timer chip says: its name, or the time left when it is running.
function sleepWords() {
  if (!ui.sleepEnd) return 'Timer';
  const left = Math.max(0, Math.ceil((ui.sleepEnd - Date.now()) / 1000));
  return left >= 3600 ? `${Math.floor(left / 3600)}:${String(Math.floor(left / 60) % 60).padStart(2, '0')}:${String(left % 60).padStart(2, '0')}` : time(left);
}
setInterval(() => { const t = $('#sleepText'); if (t) t.textContent = sleepWords(); }, 1000);

function setSleepTimer(minutes) {
  clearTimeout(sleepTimeout); ui.sleepMinutes = minutes; ui.sleepEnd = minutes ? Date.now() + minutes * 60000 : 0; closeDialog();
  if (!minutes) { notice('Sleep timer off'); renderPlaying(); return; }
  notice(`Music will pause in ${minutes} minutes`);
  sleepTimeout = setTimeout(async () => { await api('pause'); ui.sleepMinutes = 0; ui.sleepEnd = 0; await refreshState(); notice('Sleep timer finished'); }, minutes * 60000);
  renderPlaying();
}

function upNext(st) {
  const queued = st.up_next || [];
  if (!queued.length) {
    const picks = ui.suggestions || [];
    ui.editing = false;
    if (!picks.length) return `<div class="label">Up next</div><div class="empty" style="padding:18px 8px 10px">Nothing is queued yet. Add a music folder and your songs will show up here.<br><button class="button" id="addQueue">Add folder</button></div>`;
    return `<div class="label">${picks.length ? 'Try these' : 'Up next'}</div><div class="list" style="margin-top:6px">${picks.map((t, i) => songRow(t, picks, i, { plain: true })).join('')}</div>`;
  }
  const rows = queued.map((t, i) => ui.editing
    ? `<div class="song">${art(t, '', 'width:40px;height:40px;border-radius:6px')}<div class="words"><div class="t">${esc(t.title)}</div><div class="s">${esc(t.artist)}</div></div>
       ${i > 0 ? `<button class="icon" data-mv="${i},${i - 1}" aria-label="Move up">${icon.up}</button>` : ''}
       ${i + 1 < queued.length ? `<button class="icon" style="transform:rotate(180deg)" data-mv="${i},${i + 1}" aria-label="Move down">${icon.up}</button>` : ''}
       <button class="icon" data-rm="${esc(t.id)}" aria-label="Remove">${icon.close}</button></div>`
    : `<div class="song" data-jump="${esc(t.id)}" tabindex="0">${art(t, '', 'width:40px;height:40px;border-radius:6px')}<div class="words"><div class="t">${esc(t.title)}</div><div class="s">${esc(t.artist)}</div></div><div class="d">${time(t.duration)}</div></div>`).join('');
  // A short queue leaves the card half empty, so more music follows it.
  const more = !ui.editing && queued.length < 8 ? ui.suggestions || [] : [];
  const extra = more.length ? `<div class="label" style="margin:14px 6px 4px">Try these</div>${more.map((t, i) => songRow(t, more, i, { plain: true })).join('')}` : '';
  return `<div class="row-between"><span class="label">Up next</span><button class="button quiet" id="edit">${ui.editing ? 'Done' : 'Edit'}</button></div><div class="list" style="margin-top:6px">${rows}${extra}</div>`;
}

function songRow(t, list, i, o = {}) {
  const now = ui.state && ui.state.current && ui.state.current.id === t.id;
  return `<div class="song ${now ? 'now' : ''}" data-i="${i}" tabindex="0" ${o.remove ? `data-pl-song="${esc(t.id)}"` : ''}>${art(t, '', 'width:44px;height:44px;border-radius:7px')}
    <div class="words"><div class="t">${esc(t.title)}</div><div class="s">${esc(o.plain ? t.artist : `${t.artist}, ${t.album}`)}${t.id.startsWith('r:') ? ` <span class="remote">on ${esc(hostName(t))}</span>` : ''}</div></div>
    <div class="d">${time(t.duration)}</div>
    ${o.plain ? '' : `<button class="icon" data-more="${esc(t.id)}" aria-label="More for ${esc(t.title)}">${icon.more}</button>`}
    ${o.reorder && i > 0 ? `<button class="icon" data-move="${i},${i - 1}" aria-label="Move up">${icon.up}</button>` : ''}
    ${o.reorder && i + 1 < list.length ? `<button class="icon" style="transform:rotate(180deg)" data-move="${i},${i + 1}" aria-label="Move down">${icon.up}</button>` : ''}
    ${o.remove ? `<button class="icon" data-remove="${esc(t.id)}" aria-label="Remove">${icon.close}</button>` : ''}</div>`;
}

function wireRows(root, list) {
  list = list || ui.suggestions || [];
  root.querySelectorAll('.song[data-i]').forEach((row) => {
    const go = () => api('play', { ids: list.map((t) => t.id), index: Number(row.dataset.i) }).then(refreshState);
    row.ondblclick = go; row.onkeydown = (e) => { if (e.key === 'Enter') go(); };
  });
  root.querySelectorAll('[data-jump]').forEach((row) => { row.ondblclick = () => api('jump', { id: row.dataset.jump }).then(refreshState); });
  root.querySelectorAll('[data-more]').forEach((b) => b.onclick = (e) => { e.stopPropagation(); songMenu(b, b.dataset.more); });
}

function albumOwner(t) { return t.album_artist || (t.folder || t.artist); }
function byAlbumOrder(a, b) { return (a.disc_number - b.disc_number) || (a.track_number - b.track_number) || a.title.localeCompare(b.title); }
function groupSongs(tracks, albums) {
  const groups = new Map();
  for (const t of tracks) {
    const key = albums ? `${t.album.toLowerCase()}|${albumOwner(t).toLowerCase()}` : t.artist.toLowerCase();
    if (!groups.has(key)) groups.set(key, { key, name: albums ? t.album : t.artist, owner: t.album_artist || t.artist, tracks: [] });
    groups.get(key).tracks.push(t);
  }
  const out = [...groups.values()];
  out.forEach((g) => g.tracks.sort((a, b) => (albums ? 0 : a.album.localeCompare(b.album)) || byAlbumOrder(a, b)));
  return out.sort((a, b) => a.name.localeCompare(b.name));
}

function searchSongs(songs = ui.songs) {
  const term = ui.library.term.trim().toLowerCase();
  const found = term ? songs.filter((t) => t.title.toLowerCase().includes(term) || t.artist.toLowerCase().includes(term) || t.album.toLowerCase().includes(term)) : [...songs];
  const field = ['title', 'artist', 'album'][ui.library.sort];
  return found.sort((a, b) => ui.library.sort === 3 ? (b.added - a.added) || a.title.localeCompare(b.title) : a[field].localeCompare(b[field]) || a.title.localeCompare(b.title));
}

function renderLibrary() {
  const page = $('#page'); page.className = 'fixed';
  const lib = ui.library, found = searchSongs();
  const groups = lib.tab === 0 ? [] : groupSongs(found, lib.tab === 1);
  const showing = lib.detail ? searchSongs(lib.detail.tracks) : found;
  const words = lib.detail || lib.tab === 0 ? `${showing.length} ${showing.length === 1 ? 'song' : 'songs'}` : `${groups.length} ${lib.tab === 1 ? 'albums' : 'artists'}`;
  page.innerHTML = `
    <div class="head">
      ${lib.detail ? `<button class="icon" id="back" aria-label="Back" style="transform:rotate(-90deg)">${icon.up}</button>` : ''}
      ${lib.detail ? `<h1 class="grow">${esc(lib.detail.name)}</h1><span class="count">${words}</span>` : `<span class="count grow">${words}</span>`}
      <button class="button quiet" id="add">Add folder</button><button class="button" id="shuffle">${icon.shuffle.replace('<svg', '<svg width="14" height="14"')} Shuffle</button>
    </div>
    <div class="head"><input id="search" placeholder="Search your music" aria-label="Search your music" value="${esc(lib.term)}">
      <select id="sort" aria-label="Sort songs"><option value="0">Title</option><option value="1">Artist</option><option value="2">Album</option><option value="3">Newest first</option></select>
      <div class="seg">${['Songs', 'Albums', 'Artists'].map((n, i) => `<button class="tile ${lib.tab === i ? 'on' : ''}" data-tab="${i}">${n}</button>`).join('')}</div></div>
    <div class="card hug" id="results"></div>`;
  const results = $('#results');
  if (!ui.songs.length) {
    results.innerHTML = '<div class="empty">Add a music folder and Xciter will read it where it is. Nothing gets copied or moved.<br><button class="button" id="addEmpty">Add folder</button></div>';
    $('#addEmpty').onclick = addFolder;
  } else if (!showing.length && !groups.length) {
    results.innerHTML = '<div class="empty">Nothing matches that search.</div>';
  } else if (!lib.detail && lib.tab !== 0) {
    results.innerHTML = `<div class="grid">${groups.map((g, i) => `<button class="album" data-group="${i}">${art(g.tracks[0], '', '', 320)}<div class="t">${esc(g.name)}</div><div class="s">${esc(lib.tab === 1 ? g.owner : `${g.tracks.length} ${g.tracks.length === 1 ? 'song' : 'songs'}`)}</div></button>`).join('')}</div>`;
    results.querySelectorAll('[data-group]').forEach((b) => b.onclick = () => { lib.detail = groups[Number(b.dataset.group)]; renderLibrary(); watchArt(); });
  } else {
    results.innerHTML = `<div class="list">${showing.map((t, i) => songRow(t, showing, i)).join('')}</div>`;
    wireRows(results, showing);
  }
  const search = $('#search');
  const sort = $('#sort'); sort.value = String(lib.sort); sort.onchange = () => { lib.sort = Number(sort.value); lib.detail = null; renderLibrary(); watchArt(); };
  search.oninput = () => { lib.term = search.value; lib.detail = null; const at = search.selectionStart; renderLibrary(); watchArt(); const s = $('#search'); s.focus(); s.setSelectionRange(at, at); };
  page.querySelectorAll('[data-tab]').forEach((b) => b.onclick = () => { lib.tab = Number(b.dataset.tab); lib.detail = null; renderLibrary(); watchArt(); });
  $('#add').onclick = addFolder;
  $('#shuffle').onclick = () => api('shuffle_all', { ids: showing.map((t) => t.id) }).then(refreshState);
  if (lib.detail) $('#back').onclick = () => { lib.detail = null; renderLibrary(); watchArt(); };
}

async function addFolder() { ui.songs = await api('add_folder'); ui.folders = await api('folders'); ui.suggestions = await api('suggestions'); render(); }

// The lists Xciter keeps by itself; empty ones are left out.
function automaticLists() {
  const h = ui.history || {}, count = (t) => (h[t.id] || [0, 0])[0], last = (t) => (h[t.id] || [0, 0])[1];
  const played = ui.songs.filter((t) => count(t) > 0);
  return [
    ['Most played', [...played].sort((x, y) => count(y) - count(x)).slice(0, 50)],
    ['Recently played', [...played].sort((x, y) => last(y) - last(x)).slice(0, 50)],
    ['Recently added', ui.songs.filter((t) => t.added > 0).sort((x, y) => y.added - x.added).slice(0, 50)],
  ].filter(([, tracks]) => tracks.length).map(([name, tracks]) => ({ name, tracks }));
}

function renderPlaylists() {
  const page = $('#page'); page.className = 'fixed';
  const smart = automaticLists(), auto = smart.find((l) => l.name === ui.auto);
  const open = auto ? null : ui.playlists.find((p) => p.id === ui.playlist);
  const byId = new Map(ui.songs.map((t) => [t.id, t]));
  const tracks = auto ? auto.tracks : open ? open.songs.map((id) => byId.get(id)).filter(Boolean) : [];
  const title = auto ? auto.name : open ? open.name : null;
  const again = () => { renderPlaylists(); watchArt(); };
  page.innerHTML = `
    <div class="head">
      ${title ? `<button class="icon" id="back" aria-label="Back" style="transform:rotate(-90deg)">${icon.up}</button>` : ''}
      ${title ? `<h1 class="grow">${esc(title)}</h1>` : ''}
      <span class="count${title ? '' : ' grow'}">${title ? `${tracks.length} ${tracks.length === 1 ? 'song' : 'songs'}` : `${ui.playlists.length} ${ui.playlists.length === 1 ? 'playlist' : 'playlists'}`}</span>
      ${title ? `${open ? '<button class="button quiet" id="rename">Rename</button>' : ''}<button class="button" id="shuffle">Shuffle</button>` : '<button class="button" id="new">New playlist</button>'}
    </div>
    <div class="card hug" id="results"></div>`;
  const results = $('#results');
  if (title) {
    $('#back').onclick = () => { ui.playlist = null; ui.auto = null; again(); };
    $('#shuffle').onclick = () => api('shuffle_all', { ids: tracks.map((t) => t.id) }).then(refreshState);
    if (open) $('#rename').onclick = () => askRename(open);
  } else {
    $('#new').onclick = () => askName(null);
  }
  if (auto) {
    results.innerHTML = `<div class="list">${tracks.map((t, i) => songRow(t, tracks, i)).join('')}</div>`;
    wireRows(results, tracks);
  } else if (open) {
    if (tracks.length) {
      results.innerHTML = `<div class="list">${tracks.map((t, i) => songRow(t, tracks, i, { plain: true, remove: true, reorder: true })).join('')}</div>`;
      wireRows(results, tracks);
      results.querySelectorAll('[data-move]').forEach((b) => b.onclick = async (e) => { e.stopPropagation(); const [from, to] = b.dataset.move.split(',').map(Number); await api('move_in_playlist', { id: open.id, from, to }); await loadPlaylists(); again(); });
      results.querySelectorAll('[data-remove]').forEach((b) => b.onclick = async (e) => { e.stopPropagation(); await api('remove_from_playlist', { id: open.id, song: b.dataset.remove }); await loadPlaylists(); again(); });
    } else results.innerHTML = '<div class="empty">This playlist is empty. Use the ... beside any song in Library to add it.</div>';
  } else if (ui.playlists.length || smart.length) {
    // The automatic lists come first, so this page is never empty once music has played.
    const made = smart.map((l) => `<div class="song" data-auto="${esc(l.name)}" tabindex="0">${art(l.tracks[0], '', 'width:44px;height:44px;border-radius:7px')}<div class="words"><div class="t">${esc(l.name)}</div><div class="s">${l.tracks.length} ${l.tracks.length === 1 ? 'song' : 'songs'}, kept by Xciter</div></div></div>`).join('');
    const mine = ui.playlists.map((p) => { const first = byId.get(p.songs[0]); return `<div class="song" data-open="${esc(p.id)}" tabindex="0">${art(first, '', 'width:44px;height:44px;border-radius:7px')}<div class="words"><div class="t">${esc(p.name)}</div><div class="s">${p.songs.length} ${p.songs.length === 1 ? 'song' : 'songs'}</div></div><button class="icon" data-delete="${esc(p.id)}" aria-label="Delete ${esc(p.name)}">${icon.close}</button></div>`; }).join('');
    results.innerHTML = `<div class="list">${made}${mine}</div>`;
    results.querySelectorAll('[data-auto]').forEach((row) => row.onclick = () => { ui.auto = row.dataset.auto; again(); });
    results.querySelectorAll('[data-open]').forEach((row) => row.onclick = () => { ui.playlist = row.dataset.open; again(); });
    results.querySelectorAll('[data-delete]').forEach((b) => b.onclick = (e) => { e.stopPropagation(); const p = ui.playlists.find((x) => x.id === b.dataset.delete); confirmDialog(`Delete ${p.name}?`, 'The songs stay on your computer.', 'Delete', async () => { await api('delete_playlist', { id: p.id }); await loadPlaylists(); again(); }); });
  } else { results.innerHTML = '<div class="empty">No playlists yet. Make one here, or use the ... beside any song in Library.<br><button class="button" id="newEmpty">New playlist</button></div>'; $('#newEmpty').onclick = () => askName(null); }
}

function renderSound() {
  const page = $('#page'); page.className = '';
  const s = ui.sound, c = ui.constants;
  const names = ['Wide', 'Bite', 'Presence', 'Sparkle'], airNames = ['Warm', 'Clear', 'Bright', 'Airy'];
  const bandNames = ['Deep', 'Full', 'Tight', 'Firm'], keepNames = ['Low', 'Standard', 'Higher', 'Highest'];
  const choices = (values, current, key, names, unit) => `<div class="tiles">${values.map((v, i) => `<button class="tile ${current === v ? 'on' : ''}" data-choice="${key}" data-value="${v}">${unit === 'kHz' ? v / 1000 : v}<small>${names[i]}</small></button>`).join('')}</div>`;
  const percent = (v) => (v === 0 ? 'Off' : `${Math.round(v * 100)} %`);
  page.innerHTML = `
    <div class="head"><span class="count grow">Saved for every song</span></div>
    <div class="card"><span class="label">Sound profile</span><div class="tiles" id="profiles">${[['gentle', 'Gentle', 'Smooth'], ['signature', 'Signature', 'Balanced'], ['bold', 'Bold', 'Powerful'], ['custom', 'Custom', 'Yours']].map(([key, name, sub]) => `<button class="tile ${s.profile === key ? 'on' : ''}" data-profile="${key}" aria-pressed="${s.profile === key}">${name}<small>${sub}</small></button>`).join('')}</div></div>
    <div class="cols">
      <div class="card">
        <div class="row-between"><span class="label">Exciter</span><div class="right"><span class="value ${s.exciter_on ? '' : 'off'}" id="amountV">${Math.round(s.amount * 100)} %</span><button class="pill ${s.exciter_on ? 'on' : ''}" id="exciterP">${s.exciter_on ? 'On' : 'Off'}</button></div></div>
        <input type="range" id="amount" min="0" max="100" step="5" value="${Math.round(s.amount * 100)}" aria-label="Exciter amount">
        <div class="hints"><span>Subtle</span><span>Bold</span></div>
        <div class="label" style="margin-top:10px">Processes from, kHz</div>
        ${choices(c.exciterHz, s.start_hz, 'start_hz', names, 'kHz')}
        <div class="label" style="margin-top:10px">Harmonics played from, kHz</div>
        ${choices(c.airHz, s.air_hz, 'air_hz', airNames, 'kHz')}
      </div>
      <div class="card">
        <div class="row-between"><span class="label">Virtual bass</span><div class="right"><span class="value ${s.bass_on ? '' : 'off'}" id="bassV">${Math.round(s.bass_db * 10)} %</span><button class="pill ${s.bass_on ? 'on' : ''}" id="bassP">${s.bass_on ? 'On' : 'Off'}</button></div></div>
        <input type="range" id="bass" min="0" max="10" step="1" value="${Math.round(s.bass_db)}" aria-label="Virtual bass amount">
        <div class="hints"><span>Light</span><span>Heavy</span></div>
        <div class="label" style="margin-top:10px">Band from, Hz</div>
        ${choices(c.bassBandHz, s.bass_band_hz, 'bass_band_hz', bandNames, 'Hz')}
        <div class="label" style="margin-top:10px">Harmonics above, Hz</div>
        ${choices(c.bassAirHz, s.bass_air_hz, 'bass_air_hz', keepNames, 'Hz')}
      </div>
    </div>
    <div class="cols">
      <div class="card">
        <div class="row-between"><span class="label">Crystalizer</span><span class="value ${s.crystal === 0 ? 'off' : ''}" id="crystalV">${percent(s.crystal)}</span></div>
        <input type="range" id="crystal" min="0" max="100" step="5" value="${Math.round(s.crystal * 100)}" aria-label="Crystalizer amount">
        <div class="hints"><span>Off</span><span>Strong</span></div>
        <div class="count" style="margin-top:6px">Crisper highs from a clean treble lift. It adds no harmonics, so it can't distort.</div>
      </div>
      <div class="card">
        <div class="row-between"><span class="label">Even out</span><span class="value ${s.even === 0 ? 'off' : ''}" id="evenV">${percent(s.even)}</span></div>
        <input type="range" id="even" min="0" max="100" step="5" value="${Math.round(s.even * 100)}" aria-label="Even out amount">
        <div class="hints"><span>Off</span><span>Strong</span></div>
        <div class="count" style="margin-top:6px">Lifts quiet passages and eases loud ones, so soft intros stay audible.</div>
      </div>
    </div>
    <div class="card fill" style="min-height:360px">
      <div class="row-between"><span class="label">Equalizer, 31 bands</span><div class="right"><select id="preset" aria-label="Equalizer preset">${[...c.presets, 'Custom'].map((p) => `<option value="${esc(p)}" ${s.preset === p ? 'selected' : ''}>${esc(presetLabel(p))}</option>`).join('')}</select><button class="pill ${s.eq_on ? 'on' : ''}" id="eqP">${s.eq_on ? 'On' : 'Off'}</button></div></div>
      <div class="eq-wrap"><canvas id="eq" tabindex="0" role="application" aria-label="31-band equalizer curve. Left and right choose a band, up and down change it."></canvas></div>
      <div class="hints"><span>Drag the curve, up to 6 dB each way</span></div>
    </div>`;
  fillRange($('#amount')); fillRange($('#bass')); fillRange($('#crystal')); fillRange($('#even'));
  $('#amount').oninput = (e) => { fillRange(e.target); $('#amountV').textContent = `${e.target.value} %`; setSound({ amount: Number(e.target.value) / 100 }).then(markProfile); };
  $('#bass').oninput = (e) => { fillRange(e.target); $('#bassV').textContent = `${Number(e.target.value) * 10} %`; setSound({ bass_db: Number(e.target.value) }).then(markProfile); };
  for (const [id, key] of [['crystal', 'crystal'], ['even', 'even']]) {
    $(`#${id}`).oninput = (e) => { fillRange(e.target); const v = Number(e.target.value) / 100; $(`#${id}V`).textContent = percent(v); $(`#${id}V`).classList.toggle('off', v === 0); setSound({ [key]: v }).then(markProfile); };
  }
  $('#exciterP').onclick = () => setSound({ exciter_on: !s.exciter_on }).then(renderSound);
  $('#bassP').onclick = () => setSound({ bass_on: !s.bass_on }).then(renderSound);
  $('#eqP').onclick = () => setSound({ eq_on: !s.eq_on }).then(renderSound);
  page.querySelectorAll('[data-choice]').forEach((b) => b.onclick = () => setSound({ [b.dataset.choice]: Number(b.dataset.value) }).then(renderSound));
  $('#preset').onchange = async (e) => { ui.sound = await api('sound_preset', { name: e.target.value }); renderSound(); };
  page.querySelectorAll('[data-profile]').forEach((b) => b.onclick = async () => { ui.sound = await api('sound_profile', { name: b.dataset.profile }); renderSound(); });
  drawEq(); wireEq();
}

function libraryStats() {
  const albums = new Set(ui.songs.map((t) => `${t.album.toLowerCase()}|${albumOwner(t).toLowerCase()}`)).size;
  const artists = new Set(ui.songs.map((t) => t.artist.toLowerCase())).size;
  const hours = ui.songs.reduce((sum, t) => sum + t.duration, 0) / 3600;
  return [[ui.songs.length, 'songs'], [albums, 'albums'], [artists, 'artists'], [hours < 10 ? hours.toFixed(1) : Math.round(hours), 'hours']];
}

// ---- Xciter Pro: a license key bought in the Android or iPhone app, entered here ----------------------
const PLATFORMS = { android: 'Android', ios: 'iPhone', mac: 'Mac', windows: 'Windows', linux: 'Linux' };
// What is typed or pasted, tidied as it goes: upper case, letters and digits only, a dash after every four, XCIT plus 16 at most.
const tidyKey = (typed) => typed.toUpperCase().replace(/[^A-Z0-9]/g, '').slice(0, 20).replace(/(.{4})(?=.)/g, '$1-');
const keyComplete = (typed) => tidyKey(typed).length === 24;
function seen(seconds) {
  const days = Math.floor((Date.now() - seconds * 1000) / 86400000);
  return days <= 0 ? 'seen today' : days === 1 ? 'seen yesterday' : `seen ${days} days ago`;
}
function deviceRows(devices, removable) {
  return devices.map((d) => `<div class="device"><span class="who">${esc(d.name)}<small>${esc(PLATFORMS[d.platform] || d.platform)}</small></span><span class="when">${d.thisDevice ? 'This computer' : seen(d.lastSeen)}</span>${removable && !d.thisDevice ? `<button class="button quiet" data-remove="${esc(d.id)}" aria-label="Remove ${esc(d.name)}">Remove</button>` : ''}</div>`).join('');
}
function proCard() {
  const l = ui.license;
  if (l.pro) {
    return `<div class="card pro">
      <div class="row-between"><span class="label">Xciter Pro</span><span class="chip on">Pro</span></div>
      <div class="lead">Pro on a license key. Thank you.</div>
      <div class="keyrow"><span>License key</span><div class="right"><span class="key" id="keyText">${esc(ui.keyShown ? l.key : l.masked)}</span><button class="text-action" id="keyShow">${ui.keyShown ? 'Hide' : 'Show'}</button></div></div>
      <div class="count">Enter this key in Xciter on your other devices to use Pro there. It works on ${l.limit} devices.</div>
      <div class="row-between" style="margin-top:14px"><span class="label">Devices ${l.devices.length} of ${l.limit}</span></div>
      <div class="devices">${deviceRows(l.devices, true)}</div>
      ${ui.licenseMessage ? `<div class="bad" role="alert">${esc(ui.licenseMessage)}</div>` : ''}
      <div class="row-between" style="margin-top:10px"><span class="count">Pro also lets other devices find this computer and play your music.</span><button class="button quiet" id="deactivate">Deactivate this device</button></div>
    </div>`;
  }
  if (l.expired) {
    return `<div class="card pro">
      <div class="row-between"><span class="label">Xciter Pro</span><span class="chip soft">Needs a check</span></div>
      <div class="lead">Xciter could not check your license for a long while, so Pro is paused. Connect to the internet and check again.</div>
      ${ui.licenseMessage ? `<div class="bad" role="alert">${esc(ui.licenseMessage)}</div>` : ''}
      <div class="entry"><button class="button" id="recheck" ${ui.licenseBusy ? 'disabled' : ''}>${ui.licenseBusy ? 'Checking…' : 'Check again'}</button></div>
    </div>`;
  }
  return `<div class="card pro">
    <div class="row-between"><span class="label">Xciter Pro</span><span class="chip soft">Free</span></div>
    <div class="lead">Buy Xciter Pro once in the Android or iPhone app, then enter its license key here. Pro removes ads on mobile and lets your other devices find this computer and play its music.</div>
    <div class="entry"><input id="keyBox" value="${esc(ui.keyDraft)}" placeholder="XCIT-XXXX-XXXX-XXXX-XXXX" maxlength="24" spellcheck="false" autocomplete="off" autocapitalize="characters" aria-label="License key"><button class="button" id="unlock" ${ui.licenseBusy || !keyComplete(ui.keyDraft) ? 'disabled' : ''}>${ui.licenseBusy ? 'Checking…' : 'Unlock Pro'}</button></div>
    ${ui.licenseMessage ? `<div class="bad" role="alert">${esc(ui.licenseMessage)}</div>` : ''}
  </div>`;
}
function wirePro(page) {
  const box = $('#keyBox');
  if (box) {
    box.oninput = () => { box.value = ui.keyDraft = tidyKey(box.value); $('#unlock').disabled = !keyComplete(box.value); };
    box.onkeydown = (e) => { if (e.key === 'Enter' && keyComplete(box.value)) unlock(); };
    $('#unlock').onclick = unlock;
  }
  if ($('#keyShow')) $('#keyShow').onclick = () => { ui.keyShown = !ui.keyShown; renderSettings(); };
  if ($('#recheck')) $('#recheck').onclick = async () => { ui.licenseBusy = true; renderSettings(); ui.license = await api('license_check'); ui.licenseBusy = false; ui.licenseMessage = ui.license.pro ? '' : 'Xciter still could not check your license. Try again when you are online.'; await loadShare(); renderSettings(); };
  if ($('#deactivate')) $('#deactivate').onclick = confirmDeactivate;
  page.querySelectorAll('.pro [data-remove]').forEach((b) => b.onclick = async () => {
    b.disabled = true;
    const r = await api('license_remove', { key: null, target: b.dataset.remove });
    if (r.result === 'done') { ui.license = r.view; ui.licenseMessage = ''; } else ui.licenseMessage = r.message || '';
    renderSettings();
  });
}
async function unlock() {
  ui.licenseBusy = true; ui.licenseMessage = ''; renderSettings();
  const key = ui.keyDraft;
  await licenseOutcome(await api('license_activate', { key }), key);
}
// What came back from the server, in the words it gave.
async function licenseOutcome(r, key) {
  ui.licenseBusy = false;
  if (r.result === 'done') {
    ui.license = r.view; ui.keyDraft = ''; ui.licenseMessage = ''; ui.keyShown = false; closeDialog();
    await loadShare(); renderSettings(); notice('Xciter Pro is on');
  } else if (r.result === 'deviceLimit' || r.result === 'devices') {
    limitDialog(r, ''); renderSettings();
  } else {
    if (document.querySelector('#overlay .limit')) limitDialog(ui.limit, r.message); else { ui.licenseMessage = r.message; renderSettings(); }
  }
}
// The fifth device: the key is right but all places are taken. Each device has a Remove; after one the key is tried again.
function limitDialog(r, message) {
  ui.limit = r;
  const o = dialog(`<h2>This key is already on ${r.limit} devices</h2><div class="count">Remove one to use Xciter Pro on this computer.</div><div class="devices">${deviceRows(r.devices, true)}</div>${message ? `<div class="bad" role="alert">${esc(message)}</div>` : ''}<div class="end"><button class="button quiet" id="c">Cancel</button></div>`);
  o.firstElementChild.classList.add('wide', 'limit');
  $('#c', o).onclick = closeDialog;
  o.querySelectorAll('[data-remove]').forEach((b) => b.onclick = async () => {
    o.querySelectorAll('[data-remove]').forEach((x) => { x.disabled = true; });
    const removed = await api('license_remove', { key: r.key, target: b.dataset.remove });
    if (removed.result !== 'devices') return limitDialog(r, removed.message || '');
    await licenseOutcome(await api('license_activate', { key: r.key }), r.key);
  });
}
function confirmDeactivate() {
  const o = dialog(`<h2>Deactivate this device?</h2><div class="count">Xciter Pro turns off on this computer and its place goes back to your key, so you can use it on another device. You can enter the key here again later.</div><div class="end"><button class="button quiet" id="c">Cancel</button><button class="button" id="ok">Deactivate</button></div>`);
  $('#c', o).onclick = closeDialog;
  $('#ok', o).onclick = async () => {
    const r = await api('license_deactivate');
    closeDialog();
    if (r.result === 'done') { ui.license = r.view; ui.licenseMessage = ''; ui.keyShown = false; await loadShare(); renderSettings(); notice('This computer is no longer using Pro'); }
    else { ui.licenseMessage = r.message; renderSettings(); }
  };
}

/** Xciter's privacy policy, terms and support pages, in the browser. */
function openWeb(which) {
  const url = `https://softiva.app/xciter/${which}`;
  const opener = window.__TAURI__ && window.__TAURI__.opener;
  if (opener) opener.openUrl(url).catch(() => notice('Could not open the browser. Visit softiva.app/xciter yourself.'));
}

function renderSettings() {
  const page = $('#page'); page.className = '';
  const s = ui.sound;
  page.innerHTML = `
    ${proCard()}
    <div class="cols">
      <div class="card"><span class="label">Appearance</span><div class="tiles">${[['auto', 'System'], ['scheduled', '7 PM–7 AM'], ['light', 'Light'], ['dark', 'Dark']].map(([v, n]) => `<button class="tile ${s.appearance === v ? 'on' : ''}" data-theme="${v}">${n}</button>`).join('')}</div><div class="count" style="margin-top:6px">System follows this computer. 7 PM–7 AM uses dark mode overnight.</div></div>
      <div class="card"><span class="label">Volume levelling</span><div class="tiles">${[['none', 'Off'], ['track', 'By song'], ['album', 'By album']].map(([v, n]) => `<button class="tile ${s.levelling === v ? 'on' : ''}" data-level="${v}">${n}</button>`).join('')}</div></div>
      <div class="card">
        <div class="row-between"><span class="label">Crossfade</span><div class="right"><span class="value ${s.crossfade_on ? '' : 'off'}" id="xfV">${Math.round(s.crossfade_seconds)} s</span><button class="pill ${s.crossfade_on ? 'on' : ''}" id="xfP">${s.crossfade_on ? 'On' : 'Off'}</button></div></div>
        <input type="range" id="xf" min="1" max="12" step="1" value="${Math.round(s.crossfade_seconds)}" aria-label="Crossfade length"><div class="hints"><span>1 s</span><span>12 s</span></div>
      </div>
      <div class="card">
        <div class="row-between"><span class="label">Master gain</span><span class="value" id="mV">${signed(s.master_db)} dB</span></div>
        <input type="range" id="m" min="-12" max="6" step="1" value="${Math.round(s.master_db)}" aria-label="Master gain">
        <div class="row-between"><span>Headroom, so peaks don't clip</span><button class="pill ${s.headroom ? 'on' : ''}" id="hrP">${s.headroom ? 'On' : 'Off'}</button></div>
        <div class="row-between" style="margin-top:6px"><span>Dither, smoother quiet parts</span><button class="pill ${s.dither ? 'on' : ''}" id="ditherP">${s.dither ? 'On' : 'Off'}</button></div>
      </div>
    </div>
    <div class="cols">
    <div class="card">
      <div class="row-between"><span class="label">Music folders</span><div class="right"><button class="button quiet" id="scan">Scan now</button><button class="button" id="add">Add folder</button></div></div>
      <div style="margin-top:8px">${ui.folders.length ? ui.folders.map((f, i) => `<div class="folder"><span>${esc(f)}</span><button class="icon" data-folder="${i}" aria-label="Stop using this folder">${icon.close}</button></div>`).join('') : '<div class="count">No folders yet.</div>'}</div>
      <div class="count" style="margin-top:6px">Xciter scans these folders automatically while it is open. Removing one here never deletes your files.<span id="scanStatus">${ui.lastScan ? ` Last checked ${ui.lastScan.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}.` : ''}</span></div>
    </div>
    <div class="card">
      <div class="row-between"><span class="label">Share with my devices</span>${ui.share.locked ? '<span class="chip soft">Needs Pro</span>' : `<button class="pill ${ui.share.enabled ? 'on' : ''}" id="shareP" aria-label="Share with my devices">${ui.share.enabled ? 'On' : 'Off'}</button>`}</div>
      <div class="count" style="margin-top:6px">${!ui.share.available ? 'Sharing could not start on this computer.' : ui.share.locked ? 'Sharing with your other devices is part of Xciter Pro. Enter your license key above and this computer can be found by your phones again.' : `Phones running Xciter on this network can ask to pair with ${esc(ui.share.name)}. Only devices you allow can see your music.`}</div>
      <div style="margin-top:6px">${ui.share.devices.map((d) => `<div class="folder"><span>${esc(d.name)}</span><button class="icon" data-device="${esc(d.id)}" aria-label="Unpair ${esc(d.name)}">${icon.close}</button></div>`).join('')}</div>
    </div>
    <div class="card" id="otherDevices">${devicesCard()}</div>
    </div>
    <div class="cols">
      <div class="card">
        <span class="label">Your library</span>
        <div class="stats">${libraryStats().map(([n, w]) => `<div><div class="value">${n}</div><div class="count">${w}</div></div>`).join('')}</div>
      </div>
      <div class="card">
        <span class="label">About</span>
        <div style="margin-top:8px">Xciter ${esc(ui.constants.version)}, by Softiva</div>
        <div class="count" style="margin-top:4px">Xciter collects nothing about you or your music. If you use a Pro license key, the license server keeps a random device id, this computer's name and platform, and dates, and nothing else: no email, no name, no payment details.</div>
        <div class="count" style="margin-top:8px"><button class="button quiet" data-web="privacy">Privacy policy</button> <button class="button quiet" data-web="terms">Terms of use</button> <button class="button quiet" data-web="support">Support</button></div>
      </div>
    </div>`;
  fillRange($('#xf')); fillRange($('#m'));
  page.querySelectorAll('[data-web]').forEach((b) => b.onclick = () => openWeb(b.dataset.web));
  page.querySelectorAll('[data-theme]').forEach((b) => b.onclick = () => setSound({ appearance: b.dataset.theme }).then(() => { applyTheme(); renderSettings(); }));
  page.querySelectorAll('[data-level]').forEach((b) => b.onclick = () => setSound({ levelling: b.dataset.level }).then(renderSettings));
  $('#xfP').onclick = () => setSound({ crossfade_on: !s.crossfade_on }).then(renderSettings);
  $('#hrP').onclick = () => setSound({ headroom: !s.headroom }).then(renderSettings);
  $('#ditherP').onclick = () => setSound({ dither: !s.dither }).then(renderSettings);
  $('#xf').oninput = (e) => { fillRange(e.target); $('#xfV').textContent = `${e.target.value} s`; setSound({ crossfade_seconds: Number(e.target.value) }); };
  $('#m').oninput = (e) => { fillRange(e.target); $('#mV').textContent = `${signed(Number(e.target.value))} dB`; setSound({ master_db: Number(e.target.value) }); };
  $('#add').onclick = addFolder;
  $('#scan').onclick = () => syncLibrary(true);
  wirePro(page);
  if ($('#shareP')) $('#shareP').onclick = async () => { await api('share_enable', { on: !ui.share.enabled }); await loadShare(); renderSettings(); };
  wireDevices(page);
  page.querySelectorAll('[data-device]').forEach((b) => b.onclick = async () => { await api('share_forget', { id: b.dataset.device }); await loadShare(); renderSettings(); });
  page.querySelectorAll('[data-folder]').forEach((b) => b.onclick = async () => { ui.songs = await api('remove_folder', { folder: ui.folders[Number(b.dataset.folder)] }); ui.folders = await api('folders'); ui.suggestions = await api('suggestions'); renderSettings(); });
}

const signed = (n) => `${n >= 0 ? '+' : ''}${Math.round(n)}`;
function fillRange(el) { el.style.setProperty('--fill', `${((el.value - el.min) / (el.max - el.min)) * 100}%`); }
async function setSound(patch) { ui.sound = await api('sound_set', { patch }); return ui.sound; }
function applyTheme() {
  const t = ui.sound.appearance;
  if (t === 'auto') document.documentElement.removeAttribute('data-theme');
  else document.documentElement.dataset.theme = t === 'scheduled' ? (new Date().getHours() >= 19 || new Date().getHours() < 7 ? 'dark' : 'light') : t;
  if (ui.page === 'sound') drawEq();
}

// ---- Equalizer ---------------------------------------------------------------------------------
function sizeCanvas(canvas) {
  const rect = canvas.getBoundingClientRect(), scale = devicePixelRatio || 1;
  canvas.width = Math.round(rect.width * scale); canvas.height = Math.round(rect.height * scale);
  const ctx = canvas.getContext('2d'); ctx.setTransform(scale, 0, 0, scale, 0, 0);
  return { ctx, w: rect.width, h: rect.height };
}
const EQ_PAD = { left: 12, right: 12, top: 16, bottom: 22 };
const presetLabel = (name) => (name === 'V Curve' ? 'V Curve ★' : name);
// The tiles follow the profile the saved sound says is selected (editing the exciter, bass or Crystalizer switches it to Custom).
function markProfile() {
  document.querySelectorAll('[data-profile]').forEach((b) => { const on = ui.sound.profile === b.dataset.profile; b.classList.toggle('on', on); b.setAttribute('aria-pressed', on); });
}
const rgba = (hex, alpha) => { const v = hex.replace('#', ''), n = parseInt(v.length === 3 ? v.replace(/./g, '$&$&') : v, 16); return `rgba(${n >> 16},${(n >> 8) & 255},${n & 255},${alpha})`; };
const bandLabel = (band) => { const hz = ui.constants.frequencies[band]; return hz < 1000 ? `${hz} Hz` : `${hz / 1000} kHz`; };
function eqGeometry(canvas, n) {
  const r = canvas.getBoundingClientRect(), p = EQ_PAD, width = r.width - p.left - p.right, height = r.height - p.top - p.bottom;
  return { r, x: (i) => p.left + (i / (n - 1)) * width, y: (db) => p.top + ((6 - db) / 12) * height, width, height };
}
// One smooth line (Catmull-Rom through the 31 points, as cubic Beziers) over a soft fill down to 0 dB. No dots at rest:
// only the band being touched (or focused with the keyboard) shows a dot with a halo, a guide line and its value.
function drawEq(active = ui.eqActive ?? -1) {
  const canvas = $('#eq'); if (!canvas) return;
  const { ctx, w, h } = sizeCanvas(canvas), curve = ui.sound.curve, n = curve.length, g = eqGeometry(canvas, n);
  const accent = css('--accent'), muted = css('--muted'), grid = css('--off'), zero = css('--dry'), onAccent = css('--on-accent');
  const px = curve.map((db, i) => [g.x(i), g.y(db)]), y0 = g.y(0);
  // Grid: 0 dB stronger, ±3 and ±6 faint.
  ctx.lineWidth = 1;
  for (const db of [6, 3, 0, -3, -6]) {
    ctx.strokeStyle = db === 0 ? zero : rgba(grid, 0.55); ctx.beginPath(); ctx.moveTo(EQ_PAD.left, g.y(db) + .5); ctx.lineTo(w - EQ_PAD.right, g.y(db) + .5); ctx.stroke();
  }
  ctx.font = '11px -apple-system, "Segoe UI", system-ui, sans-serif'; ctx.fillStyle = muted; ctx.textAlign = 'left'; ctx.textBaseline = 'alphabetic';
  for (const db of [6, 0, -6]) ctx.fillText(db > 0 ? `+${db}` : `${db}`, EQ_PAD.left + 2, g.y(db) - 3);
  [[0, '20', 'left'], [7, '100', 'center'], [17, '1k', 'center'], [27, '10k', 'center'], [30, '20k', 'right']].forEach(([i, text, align]) => {
    ctx.textAlign = align; ctx.fillText(text, align === 'left' ? EQ_PAD.left : align === 'right' ? w - EQ_PAD.right : g.x(i), h - 6);
  });
  const trace = () => {
    ctx.moveTo(px[0][0], px[0][1]);
    for (let i = 0; i < n - 1; i++) {
      const p0 = px[Math.max(0, i - 1)], p1 = px[i], p2 = px[i + 1], p3 = px[Math.min(n - 1, i + 2)];
      ctx.bezierCurveTo(p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6, p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6, p2[0], p2[1]);
    }
  };
  // The fill between the curve and the 0 dB line, strongest at the curve and fading toward the line.
  const fill = ctx.createLinearGradient(0, g.y(6), 0, g.y(-6));
  fill.addColorStop(0, rgba(accent, .35)); fill.addColorStop(.5, rgba(accent, .06)); fill.addColorStop(1, rgba(accent, .35));
  ctx.beginPath(); trace(); ctx.lineTo(px[n - 1][0], y0); ctx.lineTo(px[0][0], y0); ctx.closePath(); ctx.fillStyle = fill; ctx.fill();
  ctx.beginPath(); trace(); ctx.strokeStyle = accent; ctx.lineWidth = 2.5; ctx.lineCap = 'round'; ctx.lineJoin = 'round'; ctx.stroke();
  if (active < 0 || active >= n) return;
  const [x, y] = px[active];
  ctx.strokeStyle = rgba(accent, .45); ctx.lineWidth = 1; ctx.beginPath(); ctx.moveTo(x + .5, EQ_PAD.top - 6); ctx.lineTo(x + .5, h - EQ_PAD.bottom + 4); ctx.stroke();
  ctx.fillStyle = rgba(accent, .25); ctx.beginPath(); ctx.arc(x, y, 13, 0, Math.PI * 2); ctx.fill();
  ctx.fillStyle = accent; ctx.beginPath(); ctx.arc(x, y, 5, 0, Math.PI * 2); ctx.fill();
  const db = curve[active], text = `${bandLabel(active)} ${db >= 0 ? '+' : ''}${db.toFixed(1)} dB`;
  ctx.font = '600 12px -apple-system, "Segoe UI", system-ui, sans-serif';
  const pw = ctx.measureText(text).width + 18, ph = 22, left = Math.max(2, Math.min(w - pw - 2, x - pw / 2)), top = Math.max(2, Math.min(h - ph - 2, (y < h / 2 ? y + 16 : y - 16 - ph)));
  ctx.fillStyle = accent; ctx.beginPath(); ctx.roundRect(left, top, pw, ph, 11); ctx.fill();
  ctx.fillStyle = onAccent; ctx.textAlign = 'center'; ctx.textBaseline = 'middle'; ctx.fillText(text, left + pw / 2, top + ph / 2 + .5);
}
function wireEq() {
  const canvas = $('#eq'); let last = -1; ui.eqActive = -1;
  const setGain = (at, value) => { if (ui.sound.curve[at] !== value) { ui.sound.curve[at] = value; ui.sound.preset = 'Custom'; const select = $('#preset'); if (select) select.value = 'Custom'; api('sound_band', { band: at, gain: value }); } };
  const touch = (e) => {
    const n = ui.sound.curve.length, g = eqGeometry(canvas, n);
    const band = Math.max(0, Math.min(n - 1, Math.round(((e.clientX - g.r.left - EQ_PAD.left) / g.width) * (n - 1))));
    const gain = Math.round(Math.max(-6, Math.min(6, 6 - ((e.clientY - g.r.top - EQ_PAD.top) / g.height) * 12)) * 10) / 10;
    const from = last < 0 ? band : last, start = ui.sound.curve[from], steps = Math.abs(band - from);
    for (let i = 0; i <= steps; i++) {
      const at = from + (band > from ? i : -i);
      if (at === from && steps > 0) continue;
      setGain(at, steps === 0 ? gain : Math.round((start + ((gain - start) * i) / steps) * 10) / 10);
    }
    last = band; ui.eqActive = band;
    drawEq();
  };
  canvas.onpointerdown = (e) => { canvas.setPointerCapture(e.pointerId); last = -1; touch(e); canvas.onpointermove = touch; };
  canvas.onpointerup = canvas.onpointercancel = () => { canvas.onpointermove = null; ui.eqActive = -1; renderSound(); };
  // From the keyboard: left and right choose a band, up and down move it 0.5 dB (1 dB with Shift).
  canvas.onfocus = () => { if (ui.eqKey == null) ui.eqKey = 15; ui.eqActive = ui.eqKey; drawEq(); };
  canvas.onblur = () => { ui.eqActive = -1; drawEq(); };
  canvas.onkeydown = (e) => {
    const n = ui.sound.curve.length, step = e.shiftKey ? 1 : 0.5;
    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') ui.eqKey = Math.max(0, Math.min(n - 1, (ui.eqKey ?? 15) + (e.key === 'ArrowRight' ? 1 : -1)));
    else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') setGain(ui.eqKey ?? 15, Math.max(-6, Math.min(6, Math.round((ui.sound.curve[ui.eqKey ?? 15] + (e.key === 'ArrowUp' ? step : -step)) * 10) / 10)));
    else return;
    e.preventDefault(); ui.eqActive = ui.eqKey; drawEq();
  };
}

// ---- Spectrum ----------------------------------------------------------------------------------
const bars = { dry: new Array(34).fill(0), wet: new Array(34).fill(0) };
let lastFrame = 0, meterBusy = false, meterLevels = null;
// With no music the bars rest on a picture of the sound settings, so the card is never blank:
// the equalizer's shape in graphite, and where the virtual bass and the exciter will add in champagne.
function idleProfile() {
  const s = ui.sound, f = ui.constants.frequencies, dry = [], wet = [];
  const curveAt = (hz) => {
    if (hz <= f[0]) return s.curve[0];
    for (let i = 1; i < f.length; i++) if (hz <= f[i]) return s.curve[i - 1] + (Math.log(hz / f[i - 1]) / Math.log(f[i] / f[i - 1])) * (s.curve[i] - s.curve[i - 1]);
    return s.curve[f.length - 1];
  };
  // While comparing, it shows the plain original instead.
  const plain = !!ui.comparing, exciter = s.effects_on && s.exciter_on && !plain ? s.amount : 0, bass = s.effects_on && s.bass_on && !plain ? s.bass_db : 0;
  for (let bar = 0; bar < 34; bar++) {
    const hz = 40 * Math.pow(500, (bar + 0.5) / 34);
    dry.push(Math.max(0.2, Math.min(0.72, 0.46 + (s.eq_on && !plain ? (curveAt(hz) / 6) * 0.12 : 0))));
    let added = 0;
    if (bass > 0 && hz <= 500) added += (bass / 10) * 0.22 * (hz < 60 || hz > 250 ? 0.5 : 1);
    if (exciter > 0 && hz >= s.air_hz) added += exciter * 0.22;
    wet.push(added * 1.5);
  }
  return [dry, wet];
}

// Each bar has a "top hat": a cap that jumps up to the bar's peak (the music plus what was added), rests there 450 ms,
// then falls with gravity (accelerating, 2.4 heights per second squared) until it meets the bar again.
const caps = { value: new Array(34).fill(0), speed: new Array(34).fill(0), hold: new Array(34).fill(0) };
const CAP_HOLD_MS = 640, CAP_GRAVITY = 1.7, CAP_ROOM = 9;
// A bar rises toward the music with this time constant (seconds) and falls this much of its height per second: calm, not twitchy.
const BAR_RISE = 0.035, BAR_FALL = 1.5;
const follow = (now, target, dt) => (target > now ? now + (target - now) * (1 - Math.exp(-dt / BAR_RISE)) : Math.max(target, now - BAR_FALL * dt));
let lastTime = 0;
function spectrumFrame(now) {
  requestAnimationFrame(spectrumFrame);
  const dt = Math.min(0.1, Math.max(0, (now - lastTime) / 1000)); lastTime = now;
  const canvas = $('#spectrum'); if (!canvas || !canvas.getBoundingClientRect().width) return;
  const playing = ui.state && ui.state.playing;
  if (playing && !meterBusy && now - lastFrame >= 33) { lastFrame = now; meterBusy = true; api('meter').then((m) => { meterLevels = m; meterBusy = false; }); }
  const levels = playing ? meterLevels : null;
  const idle = playing ? null : idleProfile();
  const { ctx, w, h } = sizeCanvas(canvas), n = 34, gap = 3, bar = (w - gap * (n - 1)) / n, area = h - CAP_ROOM, radius = Math.min(bar / 2, 3);
  const dry = css('--dry'), accent = css('--accent'), text = css('--text'), floor = css('--off');
  // The bars use the whole height: the scale follows the loudest recent bar (slowly, so the picture keeps its shape), never boosting more than 1.8 times.
  let loudest = 0;
  for (let i = 0; i < n; i++) loudest = Math.max(loudest, levels ? levels[0][i] * 0.72 + levels[1][i] * 0.75 : idle ? idle[0][i] + idle[1][i] : 0);
  caps.scale = Math.max(loudest, (caps.scale || 0) - 0.12 * dt);
  const zoom = idle ? 1 : Math.min(1.8, 0.94 / Math.max(0.5, caps.scale));
  const music = ctx.createLinearGradient(0, CAP_ROOM, 0, h); music.addColorStop(0, rgba(dry, 1)); music.addColorStop(1, rgba(dry, .3));
  const added = ctx.createLinearGradient(0, CAP_ROOM, 0, h); added.addColorStop(0, rgba(accent, 1)); added.addColorStop(1, rgba(accent, .7));
  const top = (y, height, fillStyle, round) => {
    if (height <= 0) return;
    ctx.fillStyle = fillStyle; ctx.beginPath();
    if (ctx.roundRect) ctx.roundRect(y.x, y.y, bar, height, round ? [radius, radius, 0, 0] : 0); else ctx.rect(y.x, y.y, bar, height);
    ctx.fill();
  };
  ctx.fillStyle = floor; ctx.fillRect(0, h - 1, w, 1);
  for (let i = 0; i < n; i++) {
    bars.dry[i] = follow(bars.dry[i], levels ? levels[0][i] * 0.72 * zoom : idle ? idle[0][i] : 0, dt);
    bars.wet[i] = follow(bars.wet[i], levels ? levels[1][i] * 0.75 * zoom : idle ? idle[1][i] : 0, dt);
    const x = i * (bar + gap), dryH = Math.max(1.5, Math.min(1, bars.dry[i]) * area), wetH = Math.max(0, Math.min(1 - bars.dry[i], bars.wet[i]) * area);
    // The music in graphite, what Xciter adds stacked on top in champagne.
    top({ x, y: h - dryH }, dryH, music, wetH <= 0.5);
    if (wetH > 0.5) top({ x, y: h - dryH - wetH }, wetH + 0.5, added, true);
    // The top hat.
    const peak = Math.min(1, bars.dry[i] + Math.min(1 - bars.dry[i], bars.wet[i]));
    if (peak >= caps.value[i]) { caps.value[i] = peak; caps.speed[i] = 0; caps.hold[i] = now + CAP_HOLD_MS; }
    else if (now > caps.hold[i]) {
      caps.speed[i] += CAP_GRAVITY * dt; caps.value[i] -= caps.speed[i] * dt;
      if (caps.value[i] <= peak) { caps.value[i] = peak; caps.speed[i] = 0; }
    }
    const capY = h - Math.max(1.5, caps.value[i] * area) - 6;
    ctx.fillStyle = rgba(text, .8); ctx.beginPath();
    if (ctx.roundRect) ctx.roundRect(x, Math.max(0, capY), bar, 3, 1.5); else ctx.rect(x, Math.max(0, capY), bar, 3);
    ctx.fill();
  }
}

// ---- Player bar --------------------------------------------------------------------------------
function renderBar() {
  const st = ui.state || {}, t = st.current;
  const repeatIcon = st.repeat === 'one' ? icon.one : icon.repeat;
  $('#bar').innerHTML = `
    <div class="now">${art(t, '', '', 160)}<div style="min-width:0"><div class="t">${esc(t ? t.title : 'Nothing playing')}</div><div class="s">${esc(t ? t.artist : '')}</div></div></div>
    <div class="mid"><div class="buttons">
      <button class="icon ${st.shuffle ? 'lit' : 'dim'}" id="sh" aria-label="Shuffle" aria-pressed="${!!st.shuffle}">${icon.shuffle}</button>
      <button class="icon" id="pv" aria-label="Previous">${icon.prev}</button>
      <button class="play" id="pp" aria-label="${st.playing ? 'Pause' : 'Play'}">${st.playing ? icon.pause : icon.play}</button>
      <button class="icon" id="nx" aria-label="Next">${icon.next}</button>
      <button class="icon ${st.repeat !== 'off' ? 'lit' : 'dim'}" id="rp" aria-label="Repeat">${repeatIcon}</button>
    </div>
    <div class="time"><span id="at">${time(st.seconds)}</span><input type="range" id="seek" min="0" max="1000" value="0" aria-label="Position in the song"><span>${time(st.duration)}</span></div></div>
    <div class="extra" title="Current audio output">${esc(st.device || 'System audio output')}</div>`;
  $('#sh').onclick = () => api('toggle_shuffle').then(refreshState);
  $('#pv').onclick = () => api('previous').then(refreshState);
  $('#pp').onclick = () => api('toggle').then(refreshState);
  $('#nx').onclick = () => api('next').then(refreshState);
  $('#rp').onclick = () => api('cycle_repeat').then(refreshState);
  const seek = $('#seek');
  seek.onpointerdown = () => { ui.dragging = true; };
  seek.oninput = () => { fillRange(seek); $('#at').textContent = time((seek.value / 1000) * (ui.state.duration || 0)); };
  seek.onchange = () => { ui.dragging = false; api('seek', { seconds: Math.min((seek.value / 1000) * (ui.state.duration || 0), Math.max(0, (ui.state.duration || 0) - 0.5)) }).then(refreshState); };
  updatePosition();
  watchArt($('#bar'));
}
function updatePosition() {
  const st = ui.state, seek = $('#seek'); if (!st || !seek || ui.dragging) return;
  seek.value = st.duration > 0 ? Math.round((st.seconds / st.duration) * 1000) : 0;
  fillRange(seek);
  $('#at').textContent = time(st.seconds);
}

async function refreshState() {
  const previous = ui.state;
  ui.state = await api('state');
  $('#device').textContent = ui.state.device || '';
  const out = $('#outText'); if (out) out.textContent = ui.state.device || 'System audio';
  const key = (s) => s ? JSON.stringify([s.current && s.current.id, s.playing, s.shuffle, s.repeat, s.failed, s.fetching, s.up_next.map((t) => t.id)]) : '';
  const queue = (s) => s ? JSON.stringify([s.current && s.current.id, s.up_next.map((t) => t.id)]) : '';
  if (!ui.suggestions || queue(previous) !== queue(ui.state)) [ui.suggestions, ui.history] = await Promise.all([api('suggestions'), api('play_history')]);
  if (key(previous) !== key(ui.state)) { renderBar(); if (ui.page === 'playing') { renderPlaying(); } else if (ui.page === 'library') markCurrent(); }
  else updatePosition();
}
function markCurrent() { const id = ui.state.current && ui.state.current.id; document.querySelectorAll('#page .song').forEach((row) => { const b = row.querySelector('[data-more]'); if (b) row.classList.toggle('now', b.dataset.more === id); }); }

// ---- Menus and dialogs -------------------------------------------------------------------------
function closeMenu() { document.querySelectorAll('.menu').forEach((m) => m.remove()); }
let noticeTimer;
function notice(words) {
  let toast = $('#toast');
  if (!toast) { toast = document.createElement('div'); toast.id = 'toast'; toast.setAttribute('role', 'status'); document.body.append(toast); }
  toast.textContent = words; toast.classList.add('show'); clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => toast.classList.remove('show'), 2200);
}
function songMenu(anchor, id) {
  closeMenu();
  const menu = document.createElement('div'); menu.className = 'menu'; menu.setAttribute('role', 'menu');
  [['Play next', () => api('play_next', { id }).then(refreshState).then(() => notice('Playing next'))], ['Add to queue', () => api('add_to_queue', { id }).then(refreshState).then(() => notice('Added to queue'))], ['Add to playlist', () => choosePlaylist(id)]].forEach(([words, act]) => {
    const b = document.createElement('button'); b.textContent = words; b.setAttribute('role', 'menuitem'); b.onclick = () => { closeMenu(); act(); }; menu.append(b);
  });
  document.body.append(menu);
  const r = anchor.getBoundingClientRect(), m = menu.getBoundingClientRect();
  menu.style.left = `${Math.min(r.right - m.width, innerWidth - m.width - 8)}px`;
  menu.style.top = `${Math.min(r.bottom + 2, innerHeight - m.height - 8)}px`;
  menu.querySelector('button').focus();
}
function dialog(html) { const o = $('#overlay'); o.innerHTML = `<div class="dialog" role="dialog" aria-modal="true">${html}</div>`; o.hidden = false; return o; }
function closeDialog() { const o = $('#overlay'); o.hidden = true; o.innerHTML = ''; }
function choosePlaylist(id) {
  const o = dialog(`<h2>Add to playlist</h2><div class="choices"><button data-new>New playlist</button>${ui.playlists.map((p) => `<button data-id="${esc(p.id)}">${esc(p.name)}</button>`).join('')}</div><div class="end"><button class="button quiet" id="c">Cancel</button></div>`);
  $('[data-new]', o).onclick = () => askName(id);
  o.querySelectorAll('[data-id]').forEach((b) => b.onclick = async () => { const name = b.textContent; await api('add_to_playlist', { id: b.dataset.id, song: id }); await loadPlaylists(); closeDialog(); notice(`Added to ${name}`); });
  $('#c', o).onclick = closeDialog;
}
function askName(first) {
  const o = dialog('<h2>New playlist</h2><input id="n" placeholder="Playlist name" aria-label="Playlist name" maxlength="80"><div class="end"><button class="button quiet" id="c">Cancel</button><button class="button" id="ok">Create</button></div>');
  const input = $('#n', o); input.focus();
  const ok = async () => { const name = input.value.trim(); if (!name) return; const id = await api('create_playlist', { name }); if (first) await api('add_to_playlist', { id, song: first }); await loadPlaylists(); closeDialog(); notice(first ? 'Playlist created and song added' : 'Playlist created'); if (ui.page === 'playlists') { renderPlaylists(); watchArt(); } };
  $('#ok', o).onclick = ok; input.onkeydown = (e) => { if (e.key === 'Enter') ok(); };
  $('#c', o).onclick = closeDialog;
}
function askRename(playlist) {
  const o = dialog(`<h2>Rename playlist</h2><input id="n" placeholder="Playlist name" aria-label="Playlist name" maxlength="80" value="${esc(playlist.name)}"><div class="end"><button class="button quiet" id="c">Cancel</button><button class="button" id="ok">Save</button></div>`);
  const input = $('#n', o); input.focus(); input.select();
  const ok = async () => { const name = input.value.trim(); if (!name) return; await api('rename_playlist', { id: playlist.id, name }); await loadPlaylists(); closeDialog(); renderPlaylists(); watchArt(); };
  $('#ok', o).onclick = ok; input.onkeydown = (e) => { if (e.key === 'Enter') ok(); };
  $('#c', o).onclick = closeDialog;
}
function confirmDialog(title, body, action, act) {
  const o = dialog(`<h2>${esc(title)}</h2><div class="count">${esc(body)}</div><div class="end"><button class="button quiet" id="c">Keep</button><button class="button" id="ok">${esc(action)}</button></div>`);
  $('#c', o).onclick = closeDialog; $('#ok', o).onclick = async () => { closeDialog(); await act(); };
}
async function loadPlaylists() { ui.playlists = await api('playlists'); }
async function loadShare() { ui.share = await api('share_state'); }
async function syncLibrary(manual = false) {
  if (ui.scanning) return;
  ui.scanning = true;
  try {
    if (manual) api('devices_refresh');
    const next = await api('refresh_library');
    const before = JSON.stringify(ui.songs), after = JSON.stringify(next);
    ui.songs = next; ui.lastScan = new Date();
    if (before !== after) { ui.suggestions = await api('suggestions'); render(); }
    else if (manual && ui.page === 'settings') renderSettings();
  } finally { ui.scanning = false; }
}
// ---- Other devices: finding them, pairing, and their music in the library -----------------------------------
function hostName(t) {
  const id = (t.folder || '').replace(/^r:/, '');
  const found = ui.devices.paired.find((d) => d.id === id);
  return found ? found.name : 'another device';
}
function devicesCard() {
  const d = ui.devices;
  if (d.locked) return `<div class="row-between"><span class="label">Other devices</span><span class="chip soft">Needs Pro</span></div><div class="count" style="margin-top:6px">With Xciter Pro this computer can find your other computers and play their music. Enter your license key above to turn it on.</div>`;
  const paired = d.paired.map((h) => `<div class="folder"><span>${esc(h.name)} <span class="count">${h.online ? `${h.songs} ${h.songs === 1 ? 'song' : 'songs'}` : 'not nearby'}</span></span><button class="icon" data-forget="${esc(h.id)}" aria-label="Forget ${esc(h.name)}">${icon.close}</button></div>`).join('');
  const nearby = d.nearby.map((n) => `<div class="folder"><span>${esc(n.name)}</span><button class="button quiet" data-pairwith="${esc(n.id)}">Pair</button></div>`).join('');
  return `<div class="row-between"><span class="label">Other devices</span></div>
    <div class="count" style="margin-top:6px">Music from computers you pair appears in your Library while they are on this network.</div>
    <div class="count" style="margin-top:4px">A song you play from another computer is copied here first and kept for a while${d.cacheBytes ? ` (${(d.cacheBytes / 1048576).toFixed(d.cacheBytes < 104857600 ? 1 : 0)} MB now)` : ''}.${d.cacheBytes ? ' <button class="text-action" id="clearCopies">Delete the copies</button>' : ''}</div>
    <div style="margin-top:6px">${paired}${nearby}${!paired && !nearby ? `<div class="count">${d.running ? 'Looking for Xciter on this network. On the other computer, open Xciter and turn on Share with my devices.' : 'Not looking right now.'}</div>` : ''}</div>`;
}
function wireDevices(root) {
  root.querySelectorAll('[data-pairwith]').forEach((b) => b.onclick = () => api('devices_pair', { id: b.dataset.pairwith }));
  const clear = root.querySelector('#clearCopies'); if (clear) clear.onclick = async () => { await api('devices_clear_cache'); await loadDevices(); };
  root.querySelectorAll('[data-forget]').forEach((b) => b.onclick = async () => { await api('devices_forget', { id: b.dataset.forget }); await loadDevices(); });
}
let pairShown = '';
function showPairing() {
  const p = ui.devices.pairing;
  const signature = p ? JSON.stringify(p) : '';
  if (signature === pairShown) return;
  pairShown = signature;
  if (!p) { if ($('#overlay').dataset.pairing) closeDialog(); return; }
  if (p.state === 'done') { if ($('#overlay').dataset.pairing) closeDialog(); notice(`Paired with ${p.name}`); api('devices_cancel'); return; }
  const body = {
    connecting: `<h2>Pairing with ${esc(p.name)}</h2><div class="count">Reaching it…</div>`,
    confirm: `<h2>Pairing with ${esc(p.name)}</h2><div class="count">Check that ${esc(p.name)} shows the same code, then choose Allow on it.</div><div class="value" style="text-align:center;letter-spacing:.12em;font-size:32px;margin:10px 0">${esc(p.code || '')}</div>`,
    failed: `<h2>Couldn't pair</h2><div class="bad" role="alert">${esc(p.message || '')}</div>`,
  }[p.state] || '';
  const o = dialog(`${body}<div class="end"><button class="button quiet" id="c">${p.state === 'failed' ? 'Close' : 'Cancel'}</button></div>`);
  o.dataset.pairing = '1';
  $('#c', o).onclick = () => { closeDialog(); api('devices_cancel'); };
}
async function loadDevices() {
  const [devices, songs] = await Promise.all([api('devices_state'), api('songs')]);
  const songsChanged = JSON.stringify(songs) !== JSON.stringify(ui.songs);
  const changed = JSON.stringify(devices) !== JSON.stringify(ui.devices);
  ui.devices = devices; ui.songs = songs;
  showPairing();
  if (songsChanged) { ui.suggestions = await api('suggestions'); if (ui.page === 'library' || ui.page === 'playing') render(); }
  else if (changed && ui.page === 'settings' && $('#overlay').hidden) { const card = $('#otherDevices'); if (card) { card.innerHTML = devicesCard(); wireDevices(card); } }
}
function pairRequest(request) {
  const o = dialog(`<h2>${esc(request.name)} wants to pair</h2><div class="count">Check that your device shows this code, then allow it to see your music.</div><div class="value" style="text-align:center;letter-spacing:.12em;font-size:32px">${esc(request.code)}</div><div class="end"><button class="button quiet" id="c">Don't allow</button><button class="button" id="ok">Allow</button></div>`);
  const answer = async (allow) => { closeDialog(); await api('share_respond', { id: request.id, allow }); setTimeout(async () => { await loadShare(); if (ui.page === 'settings') renderSettings(); }, 400); };
  $('#c', o).onclick = () => answer(false);
  $('#ok', o).onclick = () => answer(true);
}

// ---- Start -------------------------------------------------------------------------------------
document.querySelectorAll('#rail button').forEach((b) => b.onclick = () => show(b.dataset.page));
document.addEventListener('click', (e) => { if (!e.target.closest('.menu')) closeMenu(); });
document.addEventListener('keydown', (e) => {
  const typing = e.target.matches('input:not([type=range])');
  if (e.key === 'Escape') { closeMenu(); closeDialog(); }
  if (typing) return;
  if (e.code === 'Space' && !e.target.matches('button')) { e.preventDefault(); api('toggle').then(refreshState); }
  if (e.key === 'ArrowRight' && (e.metaKey || e.ctrlKey)) api('next').then(refreshState);
  if (e.key === 'ArrowLeft' && (e.metaKey || e.ctrlKey)) api('previous').then(refreshState);
});
matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => { if (ui.page === 'sound') drawEq(); });
addEventListener('resize', () => { if (ui.page === 'sound') drawEq(); });

(async function start() {
  [ui.constants, ui.sound, ui.songs, ui.playlists, ui.folders, ui.share, ui.license, ui.devices] = await Promise.all([api('constants'), api('sound_get'), api('songs'), api('playlists'), api('folders'), api('share_state'), api('license_state'), api('devices_state')]);
  applyTheme();
  await refreshState();
  renderBar();
  show(ui.songs.length ? 'playing' : 'library');
  // The splash goes once the first state is in: after at least 400 ms in all, so a fast start isn't a flicker, then a 250 ms fade.
  setTimeout(() => { const splash = $('#splash'); splash.classList.add('gone'); setTimeout(() => splash.remove(), 300); }, Math.max(0, 400 - performance.now()));
  window.__TAURI__.event.listen('changed', refreshState);
  // A check-in changed the license (renewed, removed elsewhere, refunded): show it only if it is a real change and nothing is being typed or asked.
  window.__TAURI__.event.listen('license-changed', async () => {
    const [license, share] = await Promise.all([api('license_state'), api('share_state')]);
    const changed = JSON.stringify([license, share]) !== JSON.stringify([ui.license, ui.share]);
    ui.license = license; ui.share = share; await loadDevices();
    if (changed && ui.page === 'settings' && !ui.licenseBusy && $('#overlay').hidden && document.activeElement?.id !== 'keyBox') renderSettings();
  });
  window.__TAURI__.event.listen('pair-request', (e) => pairRequest(e.payload));
  window.__TAURI__.event.listen('devices-changed', loadDevices);
  setInterval(refreshState, 500);
  setInterval(syncLibrary, 10000);
  setInterval(() => { if (ui.sound.appearance === 'scheduled') applyTheme(); }, 60000);
  requestAnimationFrame(spectrumFrame);
})();
