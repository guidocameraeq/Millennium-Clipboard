// Millennium Clipboard // GRID — frontend (Fase 7)

(() => {
  'use strict';

  const { invoke } = window.__TAURI__.core;

  // Startup diagnostic. Reports viewport + computed layout + ACTUAL
  // rendered dimensions, so we can tell from the log whether elements
  // are overflowing horizontally (which produces the "everything cut
  // off the right side" symptom even when the CSS layout is correct).
  setTimeout(() => {
    try {
      const vp = `${window.innerWidth}x${window.innerHeight}`;
      const dpr = window.devicePixelRatio;
      const docW = document.documentElement.clientWidth;
      const scrollW = document.documentElement.scrollWidth;
      const overflow = scrollW > docW ? `OVERFLOW:${scrollW}>${docW}` : 'noverflow';

      const dims = (sel) => {
        const el = document.querySelector(sel);
        if (!el) return `${sel}=null`;
        return `${sel}=${Math.round(el.offsetWidth)}x${Math.round(el.offsetHeight)}`;
      };

      const isMobileClass = document.documentElement.classList.contains('is-mobile');
      invoke('record_frontend_log', {
        level: 'INFO',
        msg: `[viewport] inner=${vp} doc=${docW} scroll=${scrollW} ${overflow} dpr=${dpr} class=${isMobileClass} ${dims('.hud')} ${dims('.hud-right')} ${dims('.grid')} ${dims('.composer')}`
      }).catch(() => {});
    } catch (_) {}
  }, 1500);
  const { listen } = window.__TAURI__.event;
  const dialog = window.__TAURI__.dialog;
  const notification = window.__TAURI__.notification;

  // Native OS notification wrapper. Honors the settings toggle and falls
  // back silently if the user hasn't granted permission yet (we ask on
  // first use). Title + body keep it readable in the Windows toast.
  let nativePermissionAsked = false;
  async function notify(title, body) {
    if (!state.settings || state.settings.notificationsEnabled === false) return;
    if (!notification) return;
    try {
      let granted = await notification.isPermissionGranted();
      if (!granted && !nativePermissionAsked) {
        nativePermissionAsked = true;
        const perm = await notification.requestPermission();
        granted = perm === 'granted';
      }
      if (granted) {
        await notification.sendNotification({ title, body });
      }
    } catch (err) {
      console.warn('[notify]', err);
    }
  }

  // Null-safe textContent setter. If the target element disappears
  // (HTML rev mismatch, hot-reload, modal not yet rendered) we just log
  // instead of breaking the whole event chain with a TypeError.
  const setText = (el, value) => {
    if (el) {
      el.textContent = value;
    } else {
      console.warn('[setText] target element is null, value=', value);
    }
  };

  // Surface uncaught JS errors in the status bar so users can paste them
  // back. Without this, a release build hides every error.
  window.addEventListener('error', (e) => {
    console.error('[uncaught]', e);
    try {
      const sm = document.getElementById('status-msg');
      if (sm) sm.textContent = `JS ERR · ${e.message} @ ${e.filename?.split(/[\\/]/).pop()}:${e.lineno}`;
    } catch (_) {}
  });
  window.addEventListener('unhandledrejection', (e) => {
    console.error('[unhandled rejection]', e);
    try {
      const sm = document.getElementById('status-msg');
      const reason = e.reason?.message || e.reason || 'unknown';
      if (sm) sm.textContent = `JS REJECT · ${reason}`;
    } catch (_) {}
  });

  // ---------- DOM refs -----------------------------------------------------
  const textarea = document.getElementById('text-composer');
  const peerList = document.getElementById('peer-list');
  const targetName = document.getElementById('target-name');
  const targetHex = document.getElementById('target-hex');
  const statusMsg = document.getElementById('status-msg');
  const peerCount = document.getElementById('peer-count');
  const filterHint = document.getElementById('filter-hint');
  const statusPeers = document.getElementById('status-peers');
  const statusFav = document.getElementById('status-fav');
  const hudHost = document.getElementById('hud-host');
  const hudIp = document.getElementById('hud-ip');
  const hudVersion = document.getElementById('hud-version');
  const sendBtn = document.getElementById('send-btn');
  const progressBlock = document.getElementById('progress-block');
  const progressSegments = document.getElementById('progress-segments');
  const progressText = document.getElementById('progress-text');
  const progressPct = document.getElementById('progress-pct');
  const rxProgressBlock = document.getElementById('rx-progress-block');
  const rxProgressSegments = document.getElementById('rx-progress-segments');
  const rxProgressText = document.getElementById('rx-progress-text');
  const rxProgressPct = document.getElementById('rx-progress-pct');
  const toast = document.getElementById('toast');
  const toastText = document.getElementById('toast-text');
  const incomingToast = document.getElementById('incoming-toast');
  const incomingToastText = document.getElementById('incoming-toast-text');
  const dropzone = document.getElementById('dropzone');
  const fileQueue = document.getElementById('file-queue');
  const dropzoneCount = document.getElementById('dropzone-count');
  const soundToggle = document.getElementById('sound-toggle');

  // Modals
  const incomingModal = document.getElementById('incoming-modal');
  const incomingSenderName = document.getElementById('incoming-sender-name');
  const incomingSenderHex = document.getElementById('incoming-sender-hex');
  const incomingFileCount = document.getElementById('incoming-file-count');
  const incomingTotalSize = document.getElementById('incoming-total-size');
  const incomingFileList = document.getElementById('incoming-file-list');
  const incomingTimer = document.getElementById('incoming-timer');
  const incomingAcceptBtn = document.getElementById('incoming-accept');
  const incomingRejectBtn = document.getElementById('incoming-reject');

  const settingsModal = document.getElementById('settings-modal');
  const settingsDownloadDir = document.getElementById('settings-download-dir');
  const settingsPickDir = document.getElementById('settings-pick-dir');
  const settingsAutoAccept = document.getElementById('settings-auto-accept');
  const settingsAutoAcceptLabel = document.getElementById('settings-auto-accept-label');
  const settingsCloseBtn = document.getElementById('settings-close');
  const settingsNotifications = document.getElementById('settings-notifications');
  const settingsNotificationsLabel = document.getElementById('settings-notifications-label');
  const settingsAutostart = document.getElementById('settings-autostart');
  const settingsAutostartLabel = document.getElementById('settings-autostart-label');
  const settingsCloseTray = document.getElementById('settings-close-tray');
  const settingsCloseTrayLabel = document.getElementById('settings-close-tray-label');
  const settingsFx = document.getElementById('settings-fx');
  const settingsFxLabel = document.getElementById('settings-fx-label');

  const addPeerBtn = document.getElementById('add-peer-btn');
  const addPeerModal = document.getElementById('add-peer-modal');
  const addPeerIp = document.getElementById('add-peer-ip');
  const addPeerPort = document.getElementById('add-peer-port');
  const addPeerError = document.getElementById('add-peer-error');
  const addPeerSubmit = document.getElementById('add-peer-submit');

  const peerDetailsModal = document.getElementById('peer-details-modal');
  const peerDetailsTitle = document.getElementById('peer-details-title');
  const peerDetailsName = document.getElementById('peer-details-name');
  const peerDetailsFp = document.getElementById('peer-details-fp');
  const peerDetailsAddr = document.getElementById('peer-details-addr');
  const peerDetailsStatus = document.getElementById('peer-details-status');
  const peerDetailsFav = document.getElementById('peer-details-fav');
  const peerDetailsFavLabel = document.getElementById('peer-details-fav-label');
  const peerDetailsClip = document.getElementById('peer-details-clip');
  const peerDetailsClipLabel = document.getElementById('peer-details-clip-label');
  const peerDetailsRemove = document.getElementById('peer-details-remove');
  const peerDetailsCloseBtn = document.getElementById('peer-details-close');
  let peerDetailsCurrentId = null;

  const backendBanner = document.getElementById('backend-banner');
  const backendBannerMsg = document.getElementById('backend-banner-msg');
  const backendBannerClose = document.getElementById('backend-banner-close');

  function showBackendBanner(level, msg) {
    if (!backendBanner) return;
    backendBanner.dataset.level = level || 'error';
    setText(backendBannerMsg, msg || 'Unknown backend error');
    backendBanner.hidden = false;
  }
  function hideBackendBanner() {
    if (backendBanner) backendBanner.hidden = true;
  }
  if (backendBannerClose) {
    backendBannerClose.addEventListener('click', hideBackendBanner);
  }

  const settingsCheckUpdate = document.getElementById('settings-check-update');
  const settingsUpdateStatus = document.getElementById('settings-update-status');
  const settingsUpdateAction = document.getElementById('settings-update-action');
  const settingsUpdateBanner = document.getElementById('settings-update-banner');
  const settingsApplyUpdate = document.getElementById('settings-apply-update');
  let updateInfoCache = null;

  // ---------- App state ----------------------------------------------------
  const state = {
    peers: [],
    selectedPeerId: null,
    filter: 'favorites',
    mode: 'text',
    queuedFiles: [], // [{ path, name, size }]
    settings: null,
    pendingIncoming: null, // { sessionId, files, totalSize, deadlineAt }
    incomingTimerHandle: null,
    activeTransfer: null, // { sessionId, files: [{ fileId, name, size, bytes }], totalBytes }
    activeReceive: null, // { sessionId } — RX bar keying, independent of TX (2.2.e)
    targetLost: false, // the selected peer vanished from the snapshot (2.2.b)
    displays: [], // monitores del ultimo snapshot (SPEC-displays)
    displaysPending: null, // { deadlineAt } mientras un cambio espera confirmacion (Fase 2)
    displaysBusy: false, // hay un invoke de displays en vuelo: no encimar otro
    displaysProfiles: null, // perfiles guardados (Fase 3); null = todavia no cargados
    displaysTab: 'list', // pestaña activa del modal (Fase 3)
    displaysDraft: [], // borrador del lienzo: monitores activos con su posicion en edicion (Fase 3)
  };

  // ---------- Progress bar segments ----------------------------------------
  const SEGMENTS = 28;
  for (let i = 0; i < SEGMENTS; i++) {
    const s = document.createElement('div');
    s.className = 'seg';
    progressSegments.appendChild(s);
  }
  const segs = [...progressSegments.children];

  function setProgress(pct) {
    const filled = Math.round((pct / 100) * SEGMENTS);
    segs.forEach((s, i) => s.classList.toggle('on', i < filled));
    progressPct.textContent = `${pct}%`;
  }

  // Receiver bar — independent segments/fill from the TX bar above.
  for (let i = 0; i < SEGMENTS; i++) {
    const s = document.createElement('div');
    s.className = 'seg';
    rxProgressSegments.appendChild(s);
  }
  const rxSegs = [...rxProgressSegments.children];

  function setRxProgress(pct) {
    const filled = Math.round((pct / 100) * SEGMENTS);
    rxSegs.forEach((s, i) => s.classList.toggle('on', i < filled));
    rxProgressPct.textContent = `${pct}%`;
  }

  // ---------- Status helpers -----------------------------------------------
  // Priority/TTL so a routine info line (e.g. the ~5s "GRID · N online")
  // can't clobber an error/warning the user still needs to read. Info
  // messages are suppressed while a higher-priority message is within its
  // TTL; warn/err messages always show and arm the TTL window.
  let statusPriorityUntil = 0;
  const STATUS_LEVEL = { info: 0, warn: 1, err: 2 };
  function setStatus(msg, opts) {
    const level = STATUS_LEVEL[(opts && opts.priority) || 'info'];
    const now = Date.now();
    // A conscious user action (opts.force) or any warn/err always shows; a
    // routine info is suppressed only while a higher-priority TTL is live.
    if (level === 0 && !(opts && opts.force) && now < statusPriorityUntil) return;
    setText(statusMsg, msg);
    if (level > 0) {
      const ttl = (opts && opts.ttl) != null ? opts.ttl : 5000;
      statusPriorityUntil = ttl > 0 ? now + ttl : 0;
    } else {
      statusPriorityUntil = 0;
    }
  }
  function showToast(text) {
    toastText.innerHTML = '';
    toastText.textContent = text;
    setToastTitle('TRANSMIT OK');
    toast.hidden = false;
    if (toastHideTimer) clearTimeout(toastHideTimer);
    toastHideTimer = setTimeout(() => (toast.hidden = true), 3800);
  }

  let toastHideTimer = null;

  function setToastTitle(t) {
    const el = document.querySelector('.alert-title');
    if (el) el.textContent = t;
  }

  function showIncomingText(text, alias, fingerprint, senderIp, senderPort) {
    // Incoming text renders into its OWN surface (#incoming-toast), never the
    // shared ACK toast — so a later 'TRANSMIT OK' ACK can't innerHTML='' this
    // node and destroy a received message before the user copies it.
    const titleEl = document.getElementById('incoming-toast-title');
    if (titleEl) titleEl.textContent = `◂ INCOMING FROM ${alias}`;
    incomingToastText.innerHTML = '';

    const body = document.createElement('div');
    body.className = 'incoming-body';
    body.textContent = text;
    incomingToastText.appendChild(body);

    const meta = document.createElement('div');
    meta.className = 'incoming-meta mono';
    meta.textContent = `${text.length} CHARS · ${String(fingerprint || '').slice(0, 16)}... · ${senderIp || '?'}`;
    incomingToastText.appendChild(meta);

    const actions = document.createElement('div');
    actions.className = 'incoming-actions';

    const copyBtn = document.createElement('button');
    copyBtn.className = 'incoming-btn';
    copyBtn.textContent = '⎘ COPY';
    copyBtn.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(text);
        copyBtn.textContent = '✓ COPIED';
        blip(1760, 0.08);
      } catch (e) {
        copyBtn.textContent = 'ERR';
      }
    });
    actions.appendChild(copyBtn);

    const known = isKnownPeer(fingerprint);
    if (!known && senderIp) {
      const saveBtn = document.createElement('button');
      saveBtn.className = 'incoming-btn';
      saveBtn.textContent = '+ SAVE SENDER';
      saveBtn.title = `Register ${alias} (${senderIp}) as a known peer so you can send back`;
      saveBtn.addEventListener('click', async () => {
        saveBtn.disabled = true;
        saveBtn.textContent = '◷ SAVING...';
        try {
          await invoke('add_peer_by_ip', { ip: senderIp, port: senderPort || 53319 });
          saveBtn.textContent = '✓ SAVED';
          blip(1320, 0.08);
        } catch (err) {
          saveBtn.textContent = 'ERR';
          saveBtn.disabled = false;
        }
      });
      actions.appendChild(saveBtn);
    }

    const closeBtn = document.createElement('button');
    closeBtn.className = 'incoming-btn';
    closeBtn.textContent = '✕ CLOSE';
    closeBtn.addEventListener('click', () => {
      incomingToast.hidden = true;
    });
    actions.appendChild(closeBtn);

    incomingToastText.appendChild(actions);
    incomingToast.hidden = false;
  }

  function isKnownPeer(fingerprint) {
    return state.peers.some((p) => p.id === fingerprint);
  }

  // ---------- Typewriter placeholder rotator -------------------------------
  const placeholderLines = [
    'TYPE OR PASTE > TRANSMIT TO PEER...',
    'TEXT, URL, SNIPPET — ANY PAYLOAD.',
    'PRESS CTRL+ENTER TO SEND.',
  ];
  let phTimer = null;
  let phIdx = 0;

  function typePh(line, i = 0) {
    // Starting a fresh line (i===0) cancels any chain still running, so
    // two callers can never leave two concurrent typewriters alive
    // (phTimer only ever tracks one timer).
    if (i === 0 && phTimer) { clearTimeout(phTimer); phTimer = null; }
    textarea.placeholder = line.slice(0, i) + (i < line.length ? '▌' : '');
    if (i <= line.length) {
      phTimer = setTimeout(() => typePh(line, i + 1), 35 + Math.random() * 45);
    } else {
      phTimer = setTimeout(() => {
        textarea.placeholder = line;
        // This inner timer must land in phTimer too — otherwise stopPh()
        // during the 1600ms pause can't cancel the chain and repeated
        // hide/show cycles pile up concurrent typewriters.
        phTimer = setTimeout(() => {
          phIdx = (phIdx + 1) % placeholderLines.length;
          typePh(placeholderLines[phIdx]);
        }, 1600);
      }, 600);
    }
  }
  function stopPh() {
    if (phTimer) { clearTimeout(phTimer); phTimer = null; }
    textarea.placeholder = '';
  }
  // syncFxPaused() below is the single boot entry point for the
  // typewriter — starting it here too would spawn a second chain.
  textarea.addEventListener('focus', () => { if (!textarea.value) stopPh(); });
  textarea.addEventListener('blur', () => {
    if (!textarea.value && !fxDisabled()) { phIdx = 0; typePh(placeholderLines[0]); }
  });

  // ---------- FX governance (fase 0) ----------------------------------------
  // Decorative CSS animations freeze while the window is hidden
  // (html.fx-paused, see styles.css) and the typewriter's setTimeout
  // chain stops too — otherwise the WebView keeps painting in the tray.
  function fxDisabled() {
    return document.documentElement.classList.contains('fx-off');
  }
  function syncFxPaused() {
    document.documentElement.classList.toggle('fx-paused', document.hidden);
    if (document.hidden) {
      stopPh();
    } else if (!textarea.value && document.activeElement !== textarea && !fxDisabled()) {
      phIdx = 0;
      typePh(placeholderLines[0]);
    }
  }
  document.addEventListener('visibilitychange', syncFxPaused);
  syncFxPaused();

  // ---------- Audio (click-clack + blips) ----------------------------------
  let audioCtx = null;
  function ctx() {
    if (!audioCtx) {
      const C = window.AudioContext || window.webkitAudioContext;
      if (C) audioCtx = new C();
    }
    return audioCtx;
  }
  function clack() {
    if (!soundToggle.checked) return;
    const ac = ctx(); if (!ac) return;
    const now = ac.currentTime;
    const buf = ac.createBuffer(1, 0.04 * ac.sampleRate, ac.sampleRate);
    const d = buf.getChannelData(0);
    for (let i = 0; i < d.length; i++) {
      const env = Math.pow(1 - i / d.length, 3);
      d[i] = (Math.random() * 2 - 1) * env * 0.55;
    }
    const src = ac.createBufferSource(); src.buffer = buf;
    const flt = ac.createBiquadFilter();
    flt.type = 'bandpass'; flt.frequency.value = 2400; flt.Q.value = 2.5;
    const g = ac.createGain(); g.gain.value = 0.5;
    src.connect(flt); flt.connect(g); g.connect(ac.destination);
    src.start(now);
  }
  function blip(freq = 880, dur = 0.08) {
    if (!soundToggle.checked) return;
    const ac = ctx(); if (!ac) return;
    const now = ac.currentTime;
    const osc = ac.createOscillator();
    osc.type = 'square'; osc.frequency.value = freq;
    const g = ac.createGain();
    g.gain.setValueAtTime(0.001, now);
    g.gain.exponentialRampToValueAtTime(0.15, now + 0.005);
    g.gain.exponentialRampToValueAtTime(0.001, now + dur);
    osc.connect(g); g.connect(ac.destination);
    osc.start(now); osc.stop(now + dur);
  }
  textarea.addEventListener('keydown', (e) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key.length === 1 || e.key === 'Backspace' || e.key === 'Enter') clack();
  });

  // ---------- Peer rendering -----------------------------------------------
  // Millennium Items — each peer-card icon is one of the 7 Items of the
  // Millennium (Yu-Gi-Oh!). The Puzzle is reserved for the app itself.
  // We keep the original semantic keys (desktop/laptop/phone/...) so peers
  // running older clients still announce a valid icon_type; we just paint
  // them with the Millennium Item visuals.
  const peerIconImg = (key, file, alt) =>
    `<img src="assets/peer-icons/${file}" alt="${alt}" class="peer-item-img" data-icon-key="${key}" />`;
  const ICON_SVG = {
    desktop: peerIconImg('desktop', 'eye.png',   'Millennium Eye'),
    laptop:  peerIconImg('laptop',  'ring.png',  'Millennium Ring'),
    phone:   peerIconImg('phone',   'rod.png',   'Millennium Rod'),
    tablet:  peerIconImg('tablet',  'tauk.png',  'Millennium Tauk'),
    server:  peerIconImg('server',  'key.png',   'Millennium Key'),
    gaming:  peerIconImg('gaming',  'scale.png', 'Millennium Scale'),
    media:   peerIconImg('media',   'tauk.png',  'Millennium Tauk'),
  };
  // Safe lookup: iconType comes from a peer (mDNS TXT / UDP hello, un-
  // validated), so NEVER interpolate it into HTML. This returns a trusted
  // local icon string for a known key, else the desktop default — and uses
  // hasOwnProperty so a key like "constructor"/"__proto__" can't reach a
  // prototype value.
  function iconSvg(key) {
    return Object.prototype.hasOwnProperty.call(ICON_SVG, key)
      ? ICON_SVG[key]
      : ICON_SVG.desktop;
  }
  const ICON_KEYS = ['desktop', 'laptop', 'phone', 'server', 'gaming', 'media'];
  const ICON_LABELS = {
    desktop: 'EYE',
    laptop:  'RING',
    phone:   'ROD',
    server:  'KEY',
    gaming:  'SCALE',
    media:   'TAUK',
  };

  function renderPeers() {
    const filtered = state.peers.filter((p) => {
      if (state.filter === 'favorites') return p.favorite;
      return p.status !== 'offline';
    });

    const onlineCount = state.peers.filter((p) => p.status !== 'offline').length;

    // Empty / placeholder states use a full wipe — fine, they don't flap
    // because there's nothing to keep alive.
    if (state.peers.length === 0 || filtered.length === 0) {
      peerList.innerHTML = '';
      const li = document.createElement('li');
      li.className = 'peer-empty';
      if (state.peers.length === 0) {
        li.innerHTML = '— SCANNING NETWORK —<br><small style="opacity:0.6;letter-spacing:1px;font-size:9px">peers appear within seconds</small>';
      } else if (state.filter === 'favorites') {
        li.innerHTML = '— NO FAVORITES YET —<br><small style="opacity:0.6;letter-spacing:1px;font-size:9px">switch to ALL and click ★ to add one</small>';
      } else {
        li.innerHTML = '— NO PEERS ONLINE —<br><small style="opacity:0.6;letter-spacing:1px;font-size:9px">waiting on the grid</small>';
      }
      peerList.appendChild(li);
    } else {
      // Incremental diff: reuse existing <li> nodes by data-id so the
      // peer rows don't visibly flicker every time the backend pushes
      // a peers-changed snapshot (which can be every 5s from UDP).
      const empty = peerList.querySelector('.peer-empty');
      if (empty) empty.remove();

      const existing = new Map();
      Array.from(peerList.querySelectorAll('li.peer-item')).forEach((li) => {
        if (li.dataset.id) existing.set(li.dataset.id, li);
      });

      const seen = new Set();
      filtered.forEach((p, idx) => {
        seen.add(p.id);
        // Guard each row so one malformed peer can't tumble the whole list
        // (the forEach would otherwise abort and freeze the grid). Next
        // snapshot retries the skipped row.
        try {
          let li = existing.get(p.id);
          if (li) {
            updatePeerItem(li, p);
          } else {
            li = buildPeerItem(p);
          }
          if (peerList.children[idx] !== li) {
            peerList.insertBefore(li, peerList.children[idx] || null);
          }
        } catch (err) {
          console.error('renderPeers: peer render failed', p && p.id, err);
        }
      });

      existing.forEach((li, id) => {
        if (!seen.has(id)) li.remove();
      });
    }

    const favCount = state.peers.filter((p) => p.favorite).length;
    setText(peerCount, String(filtered.length).padStart(2, '0'));
    setText(statusPeers, String(onlineCount).padStart(2, '0'));
    setText(statusFav, String(favCount).padStart(2, '0'));
    setText(filterHint, `${String(filtered.length).padStart(2, '0')} visible`);
  }

  function updatePeerItem(li, p) {
    li.classList.toggle('selected', p.id === state.selectedPeerId);
    li.dataset.status = p.status;
    li.dataset.manual = p.manual ? 'true' : 'false';

    // Don't clobber an in-progress inline rename (only the name is held;
    // status/ip/hex/icons/toggles below still refresh).
    if (li.dataset.renaming !== 'true') {
      setText(li.querySelector('.peer-name'), p.name);
    }
    setText(li.querySelector('.peer-hex'), p.hexId);
    setText(li.querySelector('.peer-ip'), p.ip);

    const iconWrap = li.querySelector('.peer-icon');
    if (iconWrap && iconWrap.dataset.icon !== p.iconType) {
      iconWrap.innerHTML = iconSvg(p.iconType);
      iconWrap.dataset.icon = p.iconType;
    }

    const favBtn = li.querySelector('.fav-toggle');
    if (favBtn) {
      favBtn.classList.toggle('active', !!p.favorite);
      favBtn.setAttribute('aria-pressed', p.favorite ? 'true' : 'false');
    }
    const clipBtn = li.querySelector('.clip-toggle');
    if (clipBtn) {
      clipBtn.classList.toggle('active', !!p.clipboardSync);
      clipBtn.setAttribute('aria-pressed', p.clipboardSync ? 'true' : 'false');
    }

    const statusEl = li.querySelector('.peer-status');
    if (statusEl) {
      // 'reaching' is never emitted by the backend; 'away' has stale CSS we
      // clear defensively. Only 'online'/'offline' are real.
      statusEl.classList.remove('online', 'offline', 'away');
      statusEl.classList.add(p.status);
    }
    const statusLabel = li.querySelector('.status-label');
    if (statusLabel) statusLabel.textContent = String(p.status || 'offline').toUpperCase();
  }

  function buildPeerItem(p) {
    const li = document.createElement('li');
    li.className = 'peer-item';
    if (p.id === state.selectedPeerId) li.classList.add('selected');
    li.dataset.id = p.id;
    li.dataset.status = p.status;
    li.dataset.manual = p.manual ? 'true' : 'false';

    li.innerHTML = `
      <div class="peer-icon"></div>
      <div class="peer-info">
        <div class="peer-name-row">
          <span class="peer-name" title="Double-click to rename"></span>
          <button class="peer-action-btn details-btn" title="Peer details">⋯</button>
        </div>
        <div class="peer-meta">
          <span class="peer-hex mono"></span>
          <span class="peer-ip mono"></span>
        </div>
        <div class="peer-status-row">
          <div class="peer-status">
            <span class="status-dot"></span><span class="status-label"></span>
          </div>
          <div class="peer-toggles">
            <button class="peer-toggle fav-toggle" title="Toggle favorite" aria-pressed="false">★</button>
            <button class="peer-toggle clip-toggle" title="Toggle clipboard sync" aria-pressed="false">📋</button>
          </div>
        </div>
      </div>
    `;

    // Icon: trusted local SVG chosen by a SAFE lookup; data-icon set via
    // dataset (does NOT parse HTML) so a malicious iconType can't inject.
    const iconWrap = li.querySelector('.peer-icon');
    if (iconWrap) {
      iconWrap.innerHTML = iconSvg(p.iconType);
      iconWrap.dataset.icon = p.iconType;
    }

    setText(li.querySelector('.peer-name'), p.name);
    setText(li.querySelector('.peer-hex'), p.hexId);
    setText(li.querySelector('.peer-ip'), p.ip);

    const favBtn = li.querySelector('.fav-toggle');
    if (p.favorite) {
      favBtn.classList.add('active');
      favBtn.setAttribute('aria-pressed', 'true');
    }
    const clipBtn = li.querySelector('.clip-toggle');
    if (p.clipboardSync) {
      clipBtn.classList.add('active');
      clipBtn.setAttribute('aria-pressed', 'true');
    }

    const statusEl = li.querySelector('.peer-status');
    if (statusEl) statusEl.classList.add(p.status);
    const statusLabel = li.querySelector('.status-label');
    if (statusLabel) statusLabel.textContent = String(p.status || 'offline').toUpperCase();

    return li;
  }

  function selectPeer(id) {
    const peer = state.peers.find((p) => p.id === id);
    if (!peer) return;
    state.selectedPeerId = id;
    state.targetLost = false; // conscious re-selection clears the lost state
    setText(targetName, peer.name);
    setText(targetHex, peer.hexId);
    const isOffline = peer.status === 'offline';
    if (sendBtn) sendBtn.disabled = isOffline;
    setStatus(isOffline
      ? `PEER OFFLINE · ${peer.name} (waiting on grid)`
      : `PEER LOCKED · ${peer.name}`, { force: true });
    blip(660, 0.06);
    document.querySelectorAll('.peer-item').forEach((el) => {
      el.classList.toggle('selected', el.dataset.id === id);
    });
  }

  // ---------- Peer list events (delegated) ---------------------------------
  peerList.addEventListener('click', async (e) => {
    const detailsBtn = e.target.closest('.details-btn');
    if (detailsBtn) {
      e.stopPropagation();
      const item = detailsBtn.closest('.peer-item');
      const id = item?.dataset.id;
      if (!id) return;
      const peer = state.peers.find((p) => p.id === id);
      if (peer) openPeerDetails(peer);
      return;
    }
    const favBtn = e.target.closest('.fav-toggle');
    if (favBtn) {
      e.stopPropagation();
      const item = favBtn.closest('.peer-item');
      const id = item?.dataset.id;
      if (!id) return;
      const peer = state.peers.find((p) => p.id === id);
      const newVal = !(peer && peer.favorite);
      try {
        await invoke('toggle_favorite', { peerId: id, value: newVal });
        blip(newVal ? 1320 : 660, 0.06);
      } catch (err) {
        setStatus(`ERR favorite · ${err}`, { priority: 'err' });
      }
      return;
    }
    const clipBtn = e.target.closest('.clip-toggle');
    if (clipBtn) {
      e.stopPropagation();
      const item = clipBtn.closest('.peer-item');
      const id = item?.dataset.id;
      if (!id) return;
      const peer = state.peers.find((p) => p.id === id);
      const newVal = !(peer && peer.clipboardSync);
      try {
        await invoke('set_clipboard_sync', { peerId: id, enabled: newVal });
        blip(newVal ? 1760 : 880, 0.06);
      } catch (err) {
        setStatus(`ERR clipboard sync · ${err}`, { priority: 'err' });
      }
      return;
    }
    const item = e.target.closest('.peer-item');
    if (item && item.dataset.id) selectPeer(item.dataset.id);
  });

  // Double-click on the name also opens inline rename
  peerList.addEventListener('dblclick', (e) => {
    const nameEl = e.target.closest('.peer-name');
    if (!nameEl) return;
    e.stopPropagation();
    const item = nameEl.closest('.peer-item');
    if (item) startInlineRename(item);
  });

  function startInlineRename(item) {
    const nameEl = item.querySelector('.peer-name');
    if (!nameEl || nameEl.querySelector('input')) return;
    // Mark the row so a peers-changed re-render skips updating the name
    // (only the name) and doesn't destroy the <input> mid-type.
    item.dataset.renaming = 'true';
    const id = item.dataset.id;
    const original = nameEl.textContent;
    nameEl.innerHTML = '';
    const input = document.createElement('input');
    input.type = 'text';
    input.className = 'rename-input';
    input.value = original;
    input.maxLength = 64;
    nameEl.appendChild(input);
    input.focus();
    input.select();

    let done = false;
    const finish = async (commit) => {
      if (done) return;
      done = true;
      delete item.dataset.renaming; // clear on EVERY exit (Enter/Escape/blur/catch)
      const newName = input.value.trim();
      nameEl.textContent = commit && newName ? newName : original;
      if (commit && newName && newName !== original) {
        try {
          await invoke('rename_peer', { peerId: id, newName });
          blip(1100, 0.06);
        } catch (err) {
          nameEl.textContent = original;
          setStatus(`ERR rename · ${err}`, { priority: 'err' });
        }
      }
    };

    input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') { e.preventDefault(); finish(true); }
      if (e.key === 'Escape') { e.preventDefault(); finish(false); }
    });
    input.addEventListener('blur', () => finish(true));
  }

  // ---------- Filter buttons -----------------------------------------------
  document.querySelectorAll('.filter-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      document.querySelectorAll('.filter-btn').forEach((b) => b.classList.remove('active'));
      btn.classList.add('active');
      state.filter = btn.dataset.filter;
      renderPeers();
      blip(880, 0.05);
    });
  });

  // ---------- Mode switch (TEXT / FILE) ------------------------------------
  // Activate a mode (TEXT / FILE) programmatically: sets state.mode, toggles the
  // .mode-btn active class and shows/hides the matching .mode-panel. Reused by
  // the click handler and by the drag-drop handler (T2: drops must reveal FILE).
  function activateMode(mode) {
    state.mode = mode;
    document.querySelectorAll('.mode-btn').forEach((b) => {
      b.classList.toggle('active', b.dataset.mode === mode);
    });
    document.querySelectorAll('.mode-panel').forEach((p) => {
      const active = p.id === `mode-${mode}`;
      p.classList.toggle('active', active);
      p.hidden = !active;
    });
  }

  // Move keyboard focus into a modal when it opens (T3), so Tab/Escape reach its
  // controls and screen readers land inside it instead of on the trigger button.
  function focusFirstControl(modal) {
    if (!modal) return;
    const el = modal.querySelector(
      'button, [href], input:not([type="hidden"]), select, textarea, [tabindex]:not([tabindex="-1"])'
    );
    if (el) el.focus();
  }

  document.querySelectorAll('.mode-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      activateMode(btn.dataset.mode);
      blip(550, 0.04);
    });
  });

  // ---------- File dropzone (real: picker + Tauri drag-drop) --------------
  function formatBytes(n) {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
    return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  function renderQueue() {
    fileQueue.textContent = '';
    const n = state.queuedFiles.length;
    if (dropzoneCount) {
      dropzoneCount.hidden = n === 0;
      dropzoneCount.textContent =
        n === 0 ? '' : `${n} archivo${n === 1 ? '' : 's'} listo${n === 1 ? '' : 's'}`;
    }
    if (n === 0) {
      fileQueue.hidden = true;
      return;
    }
    fileQueue.hidden = false;
    // Built with createElement + textContent only — the file name never touches
    // innerHTML, so no escaping helper is needed and no markup can be injected.
    state.queuedFiles.forEach((f, idx) => {
      const li = document.createElement('li');

      const name = document.createElement('span');
      name.className = 'q-name';
      name.textContent = `▸ ${f.name}`;
      name.title = f.name;

      const meta = document.createElement('span');
      meta.className = 'q-meta';

      if (f.size > 0) {
        const size = document.createElement('span');
        size.className = 'q-size';
        size.textContent = formatBytes(f.size);
        meta.appendChild(size);
      }

      const rm = document.createElement('button');
      rm.type = 'button';
      rm.className = 'queue-remove';
      rm.dataset.remove = String(idx);
      rm.setAttribute('aria-label', `Quitar ${f.name}`);
      rm.textContent = '✕';
      meta.appendChild(rm);

      li.appendChild(name);
      li.appendChild(meta);
      fileQueue.appendChild(li);
    });
  }

  fileQueue.addEventListener('click', (e) => {
    const a = e.target.closest('[data-remove]');
    if (!a) return;
    e.preventDefault();
    const idx = parseInt(a.dataset.remove, 10);
    if (!isNaN(idx)) {
      state.queuedFiles.splice(idx, 1);
      renderQueue();
    }
  });

  async function pickFiles() {
    if (!dialog || !dialog.open) {
      setStatus('ERR · dialog plugin not available');
      return;
    }
    try {
      const result = await dialog.open({ multiple: true, directory: false });
      if (!result) return;
      const paths = Array.isArray(result) ? result : [result];
      for (const path of paths) {
        await addPathToQueue(path);
      }
      blip(1100, 0.05);
    } catch (err) {
      setStatus(`ERR picker · ${err}`);
    }
  }

  async function addPathToQueue(path) {
    // Android Storage Access Framework returns content:// URIs from the
    // file picker that tokio::fs can't open directly. The backend's
    // `prepare_file_for_send` command runs them through
    // tauri-plugin-android-fs, which copies the content stream into our
    // app cache and returns a real filesystem path. On desktop and on
    // plain Android paths it's a passthrough.
    let realPath = path;
    if (typeof path === 'string' && path.startsWith('content://')) {
      try {
        setStatus('Reading file from Android storage…');
        realPath = await invoke('prepare_file_for_send', { path });
      } catch (err) {
        setStatus(`ERR · could not read file: ${err}`);
        blip(440, 0.1);
        return;
      }
    }
    const name = realPath.split(/[\\/]/).pop() || 'file';
    state.queuedFiles.push({ path: realPath, name, size: 0 });
    renderQueue();
  }

  dropzone.addEventListener('click', pickFiles);

  // Tauri's webview drag-drop event delivers real filesystem paths.
  listen('tauri://drag-enter', () => {
    dropzone.style.background = 'rgba(0, 240, 255, 0.08)';
  });
  listen('tauri://drag-leave', () => {
    dropzone.style.background = '';
  });
  listen('tauri://drag-drop', async (event) => {
    dropzone.style.background = '';
    const paths = event.payload?.paths || [];
    if (paths.length > 0) activateMode('file');
    for (const path of paths) {
      await addPathToQueue(path);
    }
    if (paths.length > 0) blip(1320, 0.05);
  });

  // ---------- Transmit / send ----------------------------------------------
  async function transmit() {
    if (!state.selectedPeerId) {
      setStatus(
        state.targetLost ? 'ERR · TARGET LOST — select a peer.' : 'ERR · no peer selected.',
        { priority: 'err' }
      );
      blip(220, 0.12);
      return;
    }
    const peer = state.peers.find((p) => p.id === state.selectedPeerId);
    if (!peer || peer.status === 'offline') {
      setStatus('ERR · peer offline');
      blip(220, 0.12);
      return;
    }

    if (state.mode === 'text') {
      await transmitText(peer);
    } else {
      await transmitFiles(peer);
    }
  }

  async function transmitText(peer) {
    if (!textarea.value.trim()) {
      setStatus('ERR · empty payload. Type something first.');
      blip(220, 0.12);
      return;
    }
    const chars = textarea.value.length;
    sendBtn.disabled = true;
    progressBlock.hidden = false;
    setProgress(0);
    progressText.textContent = `TRANSMITTING // ${peer.name}`;
    setStatus(`TX → ${peer.name}...`);
    blip(880, 0.05);

    // Tiny payload — no granular events. Quick animated fill.
    let pct = 0;
    const tick = setInterval(() => {
      pct = Math.min(95, pct + 10 + Math.random() * 10);
      setProgress(pct);
    }, 80);

    try {
      await invoke('send_text', { peerId: peer.id, text: textarea.value });
      clearInterval(tick);
      setProgress(100);
      progressText.textContent = 'COMPLETE';
      blip(1760, 0.12);
      setTimeout(() => blip(2200, 0.16), 130);
      setTimeout(() => {
        progressBlock.hidden = true;
        sendBtn.disabled = false;
        setProgress(0);
        setStatus(`OK · delivered to ${peer.name}.`, { force: true });
        showToast(`${peer.name} · ${chars} CHARS · ACK`);
        textarea.value = '';
      }, 600);
    } catch (err) {
      clearInterval(tick);
      progressBlock.hidden = true;
      sendBtn.disabled = false;
      setStatus(`ERR transmit · ${err}`, { priority: 'err' });
      blip(220, 0.2);
    }
  }

  async function transmitFiles(peer) {
    if (state.queuedFiles.length === 0) {
      setStatus('ERR · queue empty. Drop a file first.');
      blip(220, 0.12);
      return;
    }
    const filePaths = state.queuedFiles.map((f) => f.path);

    sendBtn.disabled = true;
    progressBlock.hidden = false;
    setProgress(0);
    progressText.textContent = `WAITING // ${peer.name}`;
    setStatus(`TX → ${peer.name} · awaiting accept...`);
    blip(880, 0.05);

    // The backend drives the progress events from this call.
    state.activeTransfer = {
      sessionId: null,
      totalBytes: 0,
      bytesSent: 0,
    };

    try {
      const sessionId = await invoke('send_files', {
        peerId: peer.id,
        filePaths,
      });
      state.activeTransfer.sessionId = sessionId;
      setProgress(100);
      progressText.textContent = 'COMPLETE';
      blip(1760, 0.12);
      setTimeout(() => blip(2200, 0.16), 130);
      setTimeout(() => {
        progressBlock.hidden = true;
        sendBtn.disabled = false;
        setProgress(0);
        const count = state.queuedFiles.length;
        setStatus(`OK · ${count} file(s) delivered to ${peer.name}.`, { force: true });
        showToast(`${peer.name} · ${count} FILE(S) · ACK`);
        state.queuedFiles = [];
        renderQueue();
        state.activeTransfer = null;
      }, 700);
    } catch (err) {
      progressBlock.hidden = true;
      sendBtn.disabled = false;
      setStatus(`ERR transmit · ${err}`, { priority: 'err' });
      state.activeTransfer = null;
      blip(220, 0.2);
    }
  }

  sendBtn.addEventListener('click', transmit);
  textarea.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      transmit();
    }
  });

  // ---------- HUD action buttons -------------------------------------------
  document.querySelectorAll('.hud-btn').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const action = btn.dataset.action;
      blip(880, 0.06);
      if (action === 'refresh') {
        setStatus('SCAN · probing the network...');
        try {
          const peers = await invoke('rescan_peers');
          applyPeers(peers, /* initial */ false);
          setStatus(`OK · ${peers.length} peer(s) on the grid.`);
        } catch (err) {
          setStatus(`ERR rescan · ${err}`);
        }
      } else if (action === 'history') {
        openLogModal();
      } else if (action === 'qr') {
        openQrModal();
      } else if (action === 'settings') {
        openSettingsModal();
      } else if (action === 'displays') {
        openDisplaysModal();
      }
    });
  });

  // ---------- Incoming files modal -----------------------------------------
  function openIncomingModal(payload) {
    state.pendingIncoming = payload;
    incomingSenderName.textContent = payload.senderAlias || '—';
    incomingSenderHex.textContent = (payload.senderFingerprint || '').slice(0, 12);
    incomingFileCount.textContent = String(payload.fileCount).padStart(2, '0');
    incomingTotalSize.textContent = formatBytes(payload.totalSize);
    incomingFileList.innerHTML = '';
    (payload.files || []).forEach((f) => {
      const li = document.createElement('li');
      li.className = 'incoming-file-li';
      // Thumbnail comes from the peer. Only accept a data:image/... URL
      // (what thumbnails.rs actually produces); reject anything else
      // (javascript:, data:text/html, ...) and fall back to the icon.
      let thumb;
      if (f.thumbnail && /^data:image\/(png|jpeg|gif|webp);base64,/.test(f.thumbnail)) {
        thumb = document.createElement('img');
        thumb.className = 'incoming-thumb';
        thumb.alt = '';
        thumb.src = f.thumbnail; // validated as data:image/...
      } else {
        thumb = document.createElement('span');
        thumb.className = 'incoming-thumb incoming-thumb-empty';
        thumb.textContent = '📄';
      }
      const nameEl = document.createElement('span');
      nameEl.className = 'file-name';
      nameEl.textContent = f.name;            // textContent, no HTML parsing
      const sizeEl = document.createElement('span');
      sizeEl.className = 'file-size';
      sizeEl.textContent = formatBytes(f.size);
      li.replaceChildren(thumb, nameEl, sizeEl);
      incomingFileList.appendChild(li);
    });

    // First-contact banner — offer to save the sender if we don't know them yet.
    const banner = document.getElementById('incoming-firstcontact');
    if (banner) banner.remove();
    if (payload.senderFingerprint && !isKnownPeer(payload.senderFingerprint) && payload.senderIp) {
      const b = document.createElement('div');
      b.id = 'incoming-firstcontact';
      b.className = 'modal-firstcontact';
      // senderIp/senderPort come from the peer — build with textContent so
      // a malicious value can't inject markup.
      const label = document.createElement('div');
      label.className = 'settings-label';
      label.style.cssText = 'color:var(--neon-magenta);text-shadow:0 0 6px var(--neon-magenta-glow);margin-bottom:6px';
      label.textContent = 'FIRST CONTACT';
      const row = document.createElement('div');
      row.style.cssText = 'display:flex;gap:6px;align-items:center;justify-content:space-between';
      const addr = document.createElement('span');
      addr.className = 'mono';
      addr.style.cssText = 'font-size:11px;color:var(--text-mute)';
      addr.textContent = `${payload.senderIp}:${payload.senderPort || 53319}`;
      const saveBtn = document.createElement('button');
      saveBtn.className = 'modal-btn small';
      saveBtn.id = 'firstcontact-save';
      saveBtn.textContent = '+ SAVE PEER';
      row.append(addr, saveBtn);
      b.replaceChildren(label, row);
      incomingFileList.parentNode.insertBefore(b, incomingFileList);
      b.querySelector('#firstcontact-save').addEventListener('click', async (e) => {
        const btn = e.target;
        btn.disabled = true;
        btn.textContent = '◷ SAVING...';
        try {
          await invoke('add_peer_by_ip', {
            ip: payload.senderIp,
            port: payload.senderPort || 53319,
          });
          btn.textContent = '✓ SAVED';
          blip(1320, 0.08);
        } catch (err) {
          btn.textContent = 'ERR';
          btn.disabled = false;
        }
      });
    }

    incomingModal.hidden = false;
    blip(880, 0.1);
    setTimeout(() => blip(660, 0.08), 130);

    // Countdown — backend timeout is 60 s.
    const deadline = Date.now() + 60_000;
    if (state.incomingTimerHandle) clearInterval(state.incomingTimerHandle);
    state.incomingTimerHandle = setInterval(() => {
      const left = Math.max(0, Math.round((deadline - Date.now()) / 1000));
      incomingTimer.textContent = `awaiting decision · ${left}s`;
      if (left <= 0) clearInterval(state.incomingTimerHandle);
    }, 250);
  }

  function closeIncomingModal() {
    incomingModal.hidden = true;
    state.pendingIncoming = null;
    if (state.incomingTimerHandle) {
      clearInterval(state.incomingTimerHandle);
      state.incomingTimerHandle = null;
    }
  }

  incomingAcceptBtn.addEventListener('click', async () => {
    if (!state.pendingIncoming) return;
    const sid = state.pendingIncoming.sessionId;
    blip(1320, 0.1);
    closeIncomingModal();
    try {
      await invoke('approve_session', { sessionId: sid });
      setStatus('RX · accepting transfer...');
    } catch (err) {
      setStatus(`ERR approve · ${err}`);
    }
  });

  incomingRejectBtn.addEventListener('click', async () => {
    if (!state.pendingIncoming) return;
    const sid = state.pendingIncoming.sessionId;
    blip(220, 0.12);
    closeIncomingModal();
    try {
      await invoke('reject_session', { sessionId: sid });
      setStatus('RX rejected.');
    } catch (err) {
      setStatus(`ERR reject · ${err}`);
    }
  });

  // ---------- Settings modal -----------------------------------------------
  async function openSettingsModal() {
    if (!state.settings) {
      try {
        state.settings = await invoke('get_settings');
      } catch (err) {
        setStatus(`ERR settings · ${err}`);
        return;
      }
    }
    settingsDownloadDir.textContent = state.settings.downloadDir;
    settingsAutoAccept.checked = state.settings.autoAcceptFavorites;
    settingsAutoAcceptLabel.textContent = state.settings.autoAcceptFavorites ? 'ON' : 'OFF';
    if (settingsNotifications) {
      const notifOn = state.settings.notificationsEnabled !== false;
      settingsNotifications.checked = notifOn;
      settingsNotificationsLabel.textContent = notifOn ? 'ON' : 'OFF';
    }
    if (settingsAutostart) {
      const v = !!state.settings.startWithWindows;
      settingsAutostart.checked = v;
      settingsAutostartLabel.textContent = v ? 'ON' : 'OFF';
    }
    if (settingsCloseTray) {
      const v = state.settings.closeToTray !== false;
      settingsCloseTray.checked = v;
      settingsCloseTrayLabel.textContent = v ? 'ON' : 'OFF';
    }
    if (settingsFx) {
      const fxOn = !fxDisabled();
      settingsFx.checked = fxOn;
      settingsFxLabel.textContent = fxOn ? 'ON' : 'OFF';
    }
    // Reset update UI to a known state each time the modal opens
    settingsUpdateStatus.textContent = `current v${(window.__LOCAL_INFO || {}).version || '?'}`;
    settingsUpdateAction.hidden = true;
    settingsApplyUpdate.disabled = false;
    settingsApplyUpdate.textContent = /android/i.test(navigator.userAgent)
      ? '▸ DOWNLOAD & INSTALL'
      : '▸ DOWNLOAD & RESTART';
    settingsModal.hidden = false;
    focusFirstControl(settingsModal);
  }

  function closeSettingsModal() {
    settingsModal.hidden = true;
  }

  settingsCloseBtn.addEventListener('click', closeSettingsModal);

  // ---------- Runtime log modal --------------------------------------------
  const logModal = document.getElementById('log-modal');
  const logPane = document.getElementById('log-pane');
  const logLineCount = document.getElementById('log-line-count');
  const logAutoscroll = document.getElementById('log-autoscroll');
  const logCopyBtn = document.getElementById('log-copy');
  const logExportBtn = document.getElementById('log-export');
  const logClearBtn = document.getElementById('log-clear');
  const logCloseBtn = document.getElementById('log-close');

  // Ring of at most LOG_CAP lines — a flat string that grows forever is
  // a slow RAM leak and makes every repaint O(total history).
  const LOG_CAP = 2000;
  let logRing = [];
  let logLines = 0; // total real (para el "N lines"), NO el tamaño del ring

  function classifyLogLine(line) {
    if (line.includes('[ERR ]') || line.includes('[ERR]')) return 'lvl-err';
    if (line.includes('[WARN]')) return 'lvl-warn';
    return 'lvl-info';
  }

  function appendLogLine(line) {
    logRing.push(line);
    if (logRing.length > LOG_CAP) logRing.shift();
    logLines += 1;
    if (logModal && !logModal.hidden) {
      const span = document.createElement('span');
      span.className = classifyLogLine(line);
      span.textContent = (logPane.childElementCount ? '\n' : '') + line;
      logPane.appendChild(span);
      if (logLineCount) logLineCount.textContent = `${logLines} lines`;
      if (logAutoscroll && logAutoscroll.checked) {
        logPane.scrollTop = logPane.scrollHeight;
      }
    }
  }

  function repaintLog() {
    if (!logPane) return;
    logPane.innerHTML = '';
    for (let i = 0; i < logRing.length; i++) {
      const span = document.createElement('span');
      span.className = classifyLogLine(logRing[i]);
      span.textContent = (i ? '\n' : '') + logRing[i];
      logPane.appendChild(span);
    }
    if (logLineCount) logLineCount.textContent = `${logLines} lines`;
    if (logAutoscroll && logAutoscroll.checked) {
      logPane.scrollTop = logPane.scrollHeight;
    }
  }

  async function openLogModal() {
    // Tell the backend to start emitting live log-line events; while the
    // panel was closed nothing was lost — get_runtime_log below brings
    // the buffered history.
    invoke('set_log_panel_open', { open: true }).catch(() => {});
    try {
      const full = await invoke('get_runtime_log');
      const lines = full ? full.split('\n') : [];
      logRing = lines.slice(-LOG_CAP);
      logLines = lines.length;
    } catch (err) {
      logRing = [`[ui] failed to fetch log: ${err}`];
      logLines = 1;
    }
    logModal.hidden = false;
    focusFirstControl(logModal);
    repaintLog();
  }

  function closeLogModal() {
    logModal.hidden = true;
    invoke('set_log_panel_open', { open: false }).catch(() => {});
  }

  if (logCloseBtn) logCloseBtn.addEventListener('click', closeLogModal);

  // COPY/EXPORT pull the full backend buffer (up to 5000 lines) rather
  // than the 2000-line display ring, so the diagnostic the user pastes
  // back matches the "N lines" counter and isn't silently truncated.
  async function fullLogText() {
    try {
      return (await invoke('get_runtime_log')) || logRing.join('\n');
    } catch (_) {
      return logRing.join('\n');
    }
  }

  if (logCopyBtn) {
    logCopyBtn.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(await fullLogText());
        const orig = logCopyBtn.textContent;
        logCopyBtn.textContent = '✓ COPIED';
        setTimeout(() => { logCopyBtn.textContent = orig; }, 1400);
      } catch (err) {
        setStatus(`ERR copy · ${err}`);
      }
    });
  }

  if (logExportBtn) {
    logExportBtn.addEventListener('click', async () => {
      const blob = new Blob([await fullLogText()], { type: 'text/plain' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      const stamp = new Date().toISOString().replace(/[:.]/g, '-');
      a.href = url;
      a.download = `millennium-log-${stamp}.txt`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
    });
  }

  if (logClearBtn) {
    logClearBtn.addEventListener('click', async () => {
      try {
        await invoke('clear_runtime_log');
        logRing = [];
        logLines = 0;
        repaintLog();
      } catch (err) {
        setStatus(`ERR clear · ${err}`);
      }
    });
  }

  // ---------- QR pairing modal ----------------------------------------------
  const qrModal = document.getElementById('qr-modal');
  const qrCanvas = document.getElementById('qr-canvas');
  const qrPayload = document.getElementById('qr-payload');
  const qrPasteInput = document.getElementById('qr-paste-input');
  const qrAddError = document.getElementById('qr-add-error');
  const qrCopyBtn = document.getElementById('qr-copy-payload');
  const qrAddSubmit = document.getElementById('qr-add-submit');
  const qrCloseBtn = document.getElementById('qr-close');
  let qrCurrentPayload = '';

  async function openQrModal() {
    if (!qrModal) return;
    qrModal.hidden = false;
    focusFirstControl(qrModal);
    setQrTab('show');
    try {
      const data = await invoke('generate_pair_qr');
      // data.svg is produced locally by the qrcode crate (trusted, not
      // peer data) — safe to inject. The error branch below is NOT: err
      // can carry arbitrary text, so use textContent.
      qrCanvas.innerHTML = data.svg || '';
      qrCurrentPayload = data.payload || '';
      setText(qrPayload, qrCurrentPayload);
    } catch (err) {
      qrCanvas.replaceChildren();
      const e = document.createElement('div');
      e.style.cssText = 'padding:24px;color:#ff4d6b';
      e.textContent = String(err);
      qrCanvas.appendChild(e);
      qrCurrentPayload = '';
    }
  }
  function closeQrModal() { if (qrModal) qrModal.hidden = true; }

  function setQrTab(name) {
    document.querySelectorAll('.qr-tab').forEach((t) => {
      t.classList.toggle('active', t.dataset.qrTab === name);
    });
    document.getElementById('qr-pane-show').hidden = name !== 'show';
    document.getElementById('qr-pane-add').hidden = name !== 'add';
    if (qrCopyBtn) qrCopyBtn.hidden = name !== 'show';
    if (qrAddSubmit) qrAddSubmit.hidden = name !== 'add';
    if (qrAddError) qrAddError.hidden = true;
  }
  document.querySelectorAll('.qr-tab').forEach((t) => {
    t.addEventListener('click', () => setQrTab(t.dataset.qrTab));
  });
  if (qrCloseBtn) qrCloseBtn.addEventListener('click', closeQrModal);
  if (qrModal) {
    qrModal.addEventListener('click', (e) => {
      if (e.target === qrModal) closeQrModal();
    });
  }
  if (qrCopyBtn) {
    qrCopyBtn.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(qrCurrentPayload);
        const orig = qrCopyBtn.textContent;
        qrCopyBtn.textContent = '✓ COPIED';
        setTimeout(() => { qrCopyBtn.textContent = orig; }, 1400);
      } catch (err) {
        setStatus(`ERR copy · ${err}`);
      }
    });
  }
  // QR camera scan — Android-only path via tauri-plugin-barcode-scanner.
  // Reveal the scan button when the plugin is available; if not, the
  // "Paste QR contents" textarea remains the only option (desktop case).
  const qrScanBlock = document.getElementById('qr-scan-block');
  const qrScanBtn = document.getElementById('qr-scan-btn');
  const barcodeScanner = window.__TAURI__ && window.__TAURI__.barcodeScanner;
  if (qrScanBlock && barcodeScanner && /android/i.test(navigator.userAgent)) {
    qrScanBlock.hidden = false;
  }
  if (qrScanBtn) {
    qrScanBtn.addEventListener('click', async () => {
      if (!barcodeScanner) {
        setStatus('Camera scanner not available on this device.');
        return;
      }
      try {
        // Make sure the user has granted CAMERA permission first.
        if (barcodeScanner.checkPermissions && barcodeScanner.requestPermissions) {
          const cur = await barcodeScanner.checkPermissions();
          if (cur !== 'granted') {
            const req = await barcodeScanner.requestPermissions();
            if (req !== 'granted') {
              qrAddError.textContent = 'Camera permission denied.';
              qrAddError.hidden = false;
              return;
            }
          }
        }
        qrScanBtn.disabled = true;
        qrScanBtn.textContent = '▸ POINT AT QR…';
        const res = await barcodeScanner.scan({ formats: ['QrCode'] });
        const content = (res && res.content) ? res.content : '';
        if (!content) {
          qrAddError.textContent = 'Empty QR scan.';
          qrAddError.hidden = false;
          return;
        }
        // Try to peek at the alias inside the QR payload so the
        // confirmation prompt mentions the peer by name. Best-effort —
        // a malformed payload still goes through pair_with_qr_payload
        // for validation.
        let confirmLabel = 'this peer';
        try {
          const parsed = JSON.parse(content);
          if (parsed && (parsed.alias || parsed.ip)) {
            confirmLabel = parsed.alias
              ? `${parsed.alias}${parsed.ip ? ` (${parsed.ip})` : ''}`
              : parsed.ip;
          }
        } catch (_) {}
        // Close the modal first so the native confirm() sits on top of
        // the regular app screen, not behind the QR overlay (which is
        // what made the previous flow feel stuck).
        closeQrModal();
        const ok = confirm(`Pair with ${confirmLabel}?\n\nIt will be added as a favourite peer.`);
        if (!ok) {
          setStatus('QR pairing cancelled.');
          return;
        }
        const msg = await invoke('pair_with_qr_payload', { payload: content });
        setStatus(msg);
        // The camera intent backgrounded the WebView while it was
        // scanning, which on some Android builds drops the
        // `peers-changed` event the backend emits after pair. Pull
        // the snapshot directly so the new peer appears without
        // forcing the user to tap SCAN.
        try {
          const peers = await invoke('list_peers');
          applyPeers(peers, /* initial */ false);
        } catch (_) {}
        notify('▣ Paired', confirmLabel);
      } catch (err) {
        qrAddError.textContent = `Scan failed: ${err}`;
        qrAddError.hidden = false;
      } finally {
        qrScanBtn.disabled = false;
        qrScanBtn.textContent = '▸ SCAN WITH CAMERA';
      }
    });
  }

  if (qrAddSubmit) {
    qrAddSubmit.addEventListener('click', async () => {
      const txt = (qrPasteInput.value || '').trim();
      if (!txt) {
        qrAddError.textContent = 'Paste the QR contents first.';
        qrAddError.hidden = false;
        return;
      }
      qrAddSubmit.disabled = true;
      qrAddError.hidden = true;
      try {
        const msg = await invoke('pair_with_qr_payload', { payload: txt });
        setStatus(msg);
        try {
          const peers = await invoke('list_peers');
          applyPeers(peers, /* initial */ false);
        } catch (_) {}
        closeQrModal();
      } catch (err) {
        qrAddError.textContent = String(err);
        qrAddError.hidden = false;
      } finally {
        qrAddSubmit.disabled = false;
      }
    });
  }

  // Wire frontend error catchers to also ship into the backend buffer so
  // a UI crash leaves a trace in the same log the user is going to paste.
  function reportToBackend(level, msg) {
    try {
      invoke('record_frontend_log', { level, msg }).catch(() => {});
    } catch (_) {}
  }
  window.addEventListener('error', (e) => {
    reportToBackend('ERR', `uncaught ${e.message} @ ${e.filename?.split(/[\\/]/).pop()}:${e.lineno}`);
  });
  window.addEventListener('unhandledrejection', (e) => {
    const reason = e.reason?.message || e.reason || 'unknown';
    reportToBackend('ERR', `unhandled rejection: ${reason}`);
  });


  // ---------- Displays modal (SPEC-displays, Fase 2) ------------------------
  // Lista los monitores que reporta Windows y deja prender/apagar uno. Todo
  // cambio entra "a prueba": el backend arranca un watchdog y vuelve al layout
  // anterior si nadie confirma. Este archivo NO revierte por su cuenta — solo
  // dibuja el reloj y le avisa al backend; el que decide siempre es el backend
  // (si la pantalla se apaga y el usuario no puede ni ver la ventana, el
  // rollback tiene que salir igual).
  const displaysBtn = document.getElementById('hud-displays-btn');
  const displaysModal = document.getElementById('displays-modal');
  const displaysList = document.getElementById('displays-list');
  const displaysCount = document.getElementById('displays-count');
  const displaysError = document.getElementById('displays-error');
  const displaysEmpty = document.getElementById('displays-empty');
  const displaysMockWarning = document.getElementById('displays-mock-warning');
  const displaysRefreshBtn = document.getElementById('displays-refresh');
  const displaysCloseBtn = document.getElementById('displays-close');
  const displaysPendingBar = document.getElementById('displays-pending');
  const displaysPendingText = document.getElementById('displays-pending-text');
  const displaysConfirmBtn = document.getElementById('displays-confirm');
  const displaysRevertBtn = document.getElementById('displays-revert');
  // Fase 3: pestañas, perfiles y ajustes.
  const displaysTabs = document.getElementById('displays-tabs');
  const displaysPanes = displaysModal ? Array.from(displaysModal.querySelectorAll('.displays-pane')) : [];
  const displaysProfilesList = document.getElementById('displays-profiles-list');
  const displaysProfilesEmpty = document.getElementById('displays-profiles-empty');
  const displaysProfileName = document.getElementById('displays-profile-name');
  const displaysSaveProfileBtn = document.getElementById('displays-save-profile');
  const displaysProfileConfirm = document.getElementById('displays-profile-confirm');
  const displaysProfileConfirmText = document.getElementById('displays-profile-confirm-text');
  const displaysProfileConfirmYes = document.getElementById('displays-profile-confirm-yes');
  const displaysProfileConfirmNo = document.getElementById('displays-profile-confirm-no');
  const displaysRevertSecsInput = document.getElementById('displays-revert-secs');
  const displaysSettingsSaveBtn = document.getElementById('displays-settings-save');
  // Displays v2, Fase 1: perfil de arranque + interruptor de atajos.
  const displaysStartupSelect = document.getElementById('displays-startup-profile');
  const displaysShortcutsToggle = document.getElementById('displays-shortcuts-enabled');
  const displaysShortcutsToggleLabel = document.getElementById('displays-shortcuts-enabled-label');
  const displaysCanvas = document.getElementById('displays-canvas');
  const displaysCanvasEmpty = document.getElementById('displays-canvas-empty');
  const displaysCanvasApplyBtn = document.getElementById('displays-canvas-apply');
  const displaysCanvasResetBtn = document.getElementById('displays-canvas-reset');

  // El modulo de monitores es Windows-only. En Android el comando existe igual
  // (devuelve Err), asi que esconder el boton es cosmetico, no load-bearing.
  // Se usa el userAgent como el resto del codebase, NO html.is-mobile.
  if (displaysBtn && !/android/i.test(navigator.userAgent)) {
    displaysBtn.hidden = false;
  }

  // La CCD API entrega el refresco en miliherz (60000 = 60 Hz).
  function formatHz(mhz) {
    if (!Number.isFinite(mhz) || mhz <= 0) return '—';
    return `${Math.round((mhz / 1000) * 100) / 100} Hz`;
  }

  // 0x0 no es un bug: es el centinela de un monitor conectado pero sin modo
  // activo (Windows no reporta resolucion para el que esta apagado).
  function formatResolution(d) {
    if (!d.width || !d.height) return '—';
    return `${d.width} × ${d.height}`;
  }

  // El estado activo/detached sale SIEMPRE de d.active, para que el badge no
  // pueda contradecir al estilo .is-detached de la misma fila.
  function displayBadges(d) {
    const badges = [];
    if (d.primary) badges.push({ text: 'PRIMARY', cls: 'primary' });
    badges.push(d.active ? { text: 'ACTIVE', cls: 'active' } : { text: 'DETACHED', cls: 'detached' });
    return badges;
  }

  // --- Cuenta regresiva de confirmacion ------------------------------------
  // UN solo interval para todo el modulo, y se limpia siempre: al confirmar, al
  // revertir, al cerrar el modal y antes de arrancar otro. Un timer huerfano es
  // CPU en reposo, que es justo lo que este proyecto vigila.
  let displaysCountdownTimer = null;
  let displaysLastShownSec = -1;
  // Fase 3: acción destructiva de perfil esperando el OK del banner de confirmación.
  let pendingProfileAction = null; // { type: 'overwrite'|'delete', name }
  // Displays v2, Fase 1: captura de un atajo de teclado para un perfil.
  let shortcutCaptureName = null;  // perfil que está capturando, o null
  let shortcutKeyHandler = null;   // listener one-shot de keydown mientras se captura
  // Displays v2, Fase 1: AJUSTES recién es seguro de guardar cuando loadSettings
  // terminó de poblar los controles. Sin esto, un cambio disparado durante la
  // carga mandaría los defaults vacíos y borraría el perfil de arranque guardado.
  let displaysSettingsLoaded = false;
  // Fase 3: estado del lienzo de arrastre.
  let canvasView = null;   // { scale, minX, minY, offsetX, offsetY } del último render
  let canvasDirty = false; // hay un acomodo local sin aplicar (protege de refrescos)
  let canvasDrag = null;   // arrastre en curso: { m, div, startX, startY, origX, origY, scale }

  function stopDisplaysCountdown() {
    if (displaysCountdownTimer !== null) {
      clearInterval(displaysCountdownTimer);
      displaysCountdownTimer = null;
    }
    displaysLastShownSec = -1;
  }

  function renderDisplaysPending() {
    const pending = state.displaysPending;
    if (displaysPendingBar) displaysPendingBar.hidden = !pending;
    if (!pending) return;
    const remaining = Math.max(0, pending.deadlineAt - Date.now());
    const secs = Math.ceil(remaining / 1000);
    // Escribir en el DOM solo cuando cambia el segundo que se ve: el interval
    // corre mas fino que eso para no atrasarse, no para repintar.
    if (secs !== displaysLastShownSec) {
      displaysLastShownSec = secs;
      setText(displaysPendingText, secs > 0 ? `REVIERTE SOLO EN ${secs}s` : 'REVIRTIENDO…');
    }
    // Llego a cero: el reloj ya no tiene nada que contar. Quien avisa como
    // termino es el evento displays-confirmation del backend.
    if (remaining <= 0) stopDisplaysCountdown();
  }

  function startDisplaysCountdown(remainingMs) {
    stopDisplaysCountdown();
    const ms = Number(remainingMs);
    // Se guarda el DEADLINE, no un contador que se va restando: un setInterval
    // se atrasa (ventana en segundo plano, GC, un apply que trabo el hilo) y el
    // numero terminaria mintiendo sobre cuanto falta de verdad.
    state.displaysPending = { deadlineAt: Date.now() + (Number.isFinite(ms) && ms > 0 ? ms : 0) };
    renderDisplaysPending();
    // El reloj solo tickea con el modal a la vista: cerrado no hay nada que
    // repintar, y el que revierte de verdad vive en el backend.
    if (displaysModal && !displaysModal.hidden && state.displaysPending.deadlineAt > Date.now()) {
      displaysCountdownTimer = setInterval(renderDisplaysPending, 250);
    }
  }

  function clearDisplaysPending() {
    stopDisplaysCountdown();
    state.displaysPending = null;
    renderDisplaysPending();
  }

  function refreshDisplaysUi() {
    renderDisplays();
    renderDisplaysPending();
    // Los botones CARGAR/BORRAR/GUARDAR se apagan mientras hay un cambio en
    // vuelo o pendiente; renderProfiles se ocupa (no-op si no se cargaron).
    renderProfiles();
    // Lienzo: NO se toca en medio de un arrastre (re-renderizar re-escalaría y
    // sacaría el monitor de abajo del cursor). Si el estado ya se asentó (sin
    // pendiente) y no hay un acomodo local sin aplicar, se re-sincroniza con la
    // realidad; si el usuario tiene algo a medio acomodar, se respeta.
    if (state.displaysTab === 'canvas' && !canvasDrag) {
      // Re-sincroniza con la realidad SOLO en un estado asentado: sin arrastre,
      // sin acomodo local sin aplicar, sin cambio pendiente, y **sin un apply en
      // vuelo** — durante el apply `state.displays` todavía tiene lo VIEJO, y
      // re-inicializar ahí pisaría el acomodo recién mandado (se vería el layout
      // viejo justo mientras hay que confirmar o revertir).
      if (!canvasDirty && !state.displaysPending && !state.displaysBusy) initCanvasDraft();
      renderCanvas();
    }
  }

  function buildDisplayItem(d) {
    const li = document.createElement('li');
    li.className = 'display-item';
    li.dataset.id = d.id;
    // Template ESTATICO: cero interpolacion de datos del backend. Los valores
    // entran despues por setText/textContent (mismo patron que buildPeerItem).
    li.innerHTML = `
      <div class="display-glyph">▤</div>
      <div class="display-info">
        <div class="display-name"></div>
        <div class="display-meta mono">
          <span class="display-res"></span>
          <span class="display-sep">·</span>
          <span class="display-hz"></span>
        </div>
      </div>
      <div class="display-badges"></div>
      <div class="display-actions">
        <button class="modal-btn small display-primary-btn" type="button">★ primario</button>
        <button class="modal-btn small display-toggle-btn" type="button"></button>
      </div>`;
    updateDisplayItem(li, d);
    return li;
  }

  function updateDisplayItem(li, d) {
    li.dataset.active = d.active ? 'true' : 'false';
    li.classList.toggle('is-detached', !d.active);
    setText(li.querySelector('.display-name'), d.name);
    setText(li.querySelector('.display-res'), formatResolution(d));
    setText(li.querySelector('.display-hz'), formatHz(d.refreshMhz));

    const btn = li.querySelector('.display-toggle-btn');
    if (btn) {
      const detaching = !!d.active;
      btn.textContent = detaching ? 'DETACH' : 'ATTACH';
      btn.classList.toggle('reject', detaching);
      // canDetach habla SOLO de apagar. No se mira en una fila ya apagada: si
      // el backend alguna vez la marcara false ahi, ATTACH quedaria muerto para
      // siempre y el monitor no se podria volver a prender nunca.
      const lastOne = detaching && d.canDetach === false;
      const waiting = !!state.displaysPending;
      btn.disabled = lastOne || waiting || state.displaysBusy;
      // El title es el unico lugar donde se explica por que esta apagado.
      if (lastOne) {
        btn.title = 'Es el único monitor activo: apagarlo te deja la máquina a ciegas.';
      } else if (waiting) {
        btn.title = 'Hay un cambio esperando confirmación.';
      } else {
        btn.title = detaching ? 'Apagar este monitor' : 'Encender este monitor';
      }
    }

    // ★ primario: solo tiene sentido en un monitor ACTIVO que no sea ya el
    // primario. En una fila apagada o ya primaria se esconde (el badge PRIMARY
    // ya dice cuál es). Se apaga mientras hay un cambio en vuelo o pendiente.
    const primBtn = li.querySelector('.display-primary-btn');
    if (primBtn) {
      const canBePrimary = !!d.active && !d.primary;
      primBtn.hidden = !canBePrimary;
      if (canBePrimary) {
        const waiting = !!state.displaysPending;
        primBtn.disabled = waiting || state.displaysBusy;
        primBtn.title = waiting
          ? 'Hay un cambio esperando confirmación.'
          : 'Hacer este monitor el primario (pasa por la cuenta regresiva).';
      }
    }

    const badgeBox = li.querySelector('.display-badges');
    if (badgeBox) {
      const nodes = displayBadges(d).map((b) => {
        const span = document.createElement('span');
        span.className = `badge display-badge ${b.cls}`;
        span.textContent = b.text;
        return span;
      });
      badgeBox.replaceChildren(...nodes);
    }
  }

  // Render por diff, como renderPeers: se reutiliza el <li> existente por
  // data-id y se borran los que ya no estan.
  function renderDisplays() {
    if (!displaysList) return;
    const items = state.displays || [];
    const existing = new Map();
    Array.from(displaysList.querySelectorAll('li.display-item')).forEach((li) => {
      if (li.dataset.id) existing.set(li.dataset.id, li);
    });
    const seen = new Set();
    items.forEach((d, idx) => {
      seen.add(d.id);
      try {
        let li = existing.get(d.id);
        if (li) {
          updateDisplayItem(li, d);
        } else {
          li = buildDisplayItem(d);
        }
        if (displaysList.children[idx] !== li) {
          displaysList.insertBefore(li, displaysList.children[idx] || null);
        }
      } catch (err) {
        console.error('[displays] fila malformada', err);
      }
    });
    existing.forEach((li, id) => {
      if (!seen.has(id)) li.remove();
    });

    if (displaysEmpty) displaysEmpty.hidden = items.length > 0;
    if (displaysCount) {
      const active = items.filter((d) => d.active).length;
      displaysCount.textContent = `${items.length} monitor(es) · ${active} activo(s)`;
    }
  }

  // Un solo lugar donde se muestra un error de monitores, para que el cartel
  // del modal, la linea de status y el log del backend nunca cuenten historias
  // distintas del mismo problema.
  function showDisplaysError(err) {
    const msg = String(err);
    if (displaysError) {
      // textContent: el mensaje puede venir de Windows, nunca por innerHTML.
      displaysError.textContent = msg;
      displaysError.hidden = false;
    }
    setStatus(`ERR displays · ${msg}`, { priority: 'err' });
    reportToBackend('ERR', `displays: ${msg}`);
  }

  // Todo snapshot que llega del backend (lectura, toggle, confirm o revert)
  // entra por aca. El backend es el dueño del estado: la UI no adivina nada,
  // ni siquiera si sigue habiendo un cambio pendiente.
  function applyDisplaysSnapshot(snapshot) {
    state.displays = Array.isArray(snapshot?.displays) ? snapshot.displays : [];
    if (displaysMockWarning) displaysMockWarning.hidden = snapshot?.source !== 'mock';
    const remainingMs = Number(snapshot?.pending?.remainingMs);
    if (Number.isFinite(remainingMs) && remainingMs > 0) {
      // Rehidratacion: si el usuario cerro y reabrio el modal en el medio, la
      // cuenta reaparece con lo que quedaba segun el reloj del backend.
      startDisplaysCountdown(remainingMs);
    } else {
      clearDisplaysPending();
    }
    refreshDisplaysUi();
  }

  async function loadDisplays() {
    if (displaysError) displaysError.hidden = true;
    try {
      applyDisplaysSnapshot(await invoke('displays_get_snapshot'));
    } catch (err) {
      state.displays = [];
      clearDisplaysPending();
      if (displaysMockWarning) displaysMockWarning.hidden = true;
      refreshDisplaysUi();
      showDisplaysError(err);
    }
  }

  // Prender o apagar un monitor. El comando vuelve con el snapshot ya
  // verificado por el backend (re-enumerado, no "me devolvio 0 asi que anduvo").
  async function toggleDisplay(id) {
    if (state.displaysBusy || state.displaysPending) return;
    const target = (state.displays || []).find((d) => d.id === id);
    if (!target) return;
    state.displaysBusy = true;
    if (displaysError) displaysError.hidden = true;
    refreshDisplaysUi(); // apaga los botones mientras Windows piensa
    blip(660, 0.06);
    setStatus(
      `DISPLAYS · ${target.active ? 'apagando' : 'encendiendo'} ${target.name}…`,
      { force: true }
    );
    try {
      applyDisplaysSnapshot(await invoke('displays_toggle', { displayId: id }));
    } catch (err) {
      // El apply pudo quedar a mitad de camino: lo unico confiable es volver a
      // leer la topologia antes de mostrar nada.
      try { await loadDisplays(); } catch (_) {}
      showDisplaysError(err);
    } finally {
      state.displaysBusy = false;
      refreshDisplaysUi();
    }
  }

  // Hacer primario un monitor (Displays v2, Fase 1). Espejo de toggleDisplay: es
  // un cambio en vivo, así que vuelve con la MISMA red (cuenta regresiva) y el
  // backend ya re-enumeró antes de devolver el snapshot.
  async function setPrimaryDisplay(id) {
    if (state.displaysBusy || state.displaysPending) return;
    const target = (state.displays || []).find((d) => d.id === id);
    if (!target) return;
    state.displaysBusy = true;
    if (displaysError) displaysError.hidden = true;
    refreshDisplaysUi();
    blip(660, 0.06);
    setStatus(`DISPLAYS · haciendo primario ${target.name}…`, { force: true });
    try {
      applyDisplaysSnapshot(await invoke('displays_set_primary', { displayId: id }));
    } catch (err) {
      try { await loadDisplays(); } catch (_) {}
      showDisplaysError(err);
    } finally {
      state.displaysBusy = false;
      refreshDisplaysUi();
    }
  }

  // CONFIRMAR / REVERTIR AHORA: los dos hacen lo mismo salvo el comando.
  async function resolveDisplaysPending(command, doneMsg) {
    if (!state.displaysPending || state.displaysBusy) return;
    state.displaysBusy = true;
    // El reloj se frena ya (la decision esta tomada), pero la barra queda a la
    // vista con el numero congelado hasta que vuelva el snapshot.
    stopDisplaysCountdown();
    if (displaysConfirmBtn) displaysConfirmBtn.disabled = true;
    if (displaysRevertBtn) displaysRevertBtn.disabled = true;
    try {
      applyDisplaysSnapshot(await invoke(command));
      setStatus(`DISPLAYS · ${doneMsg}`, { force: true });
    } catch (err) {
      try { await loadDisplays(); } catch (_) {}
      showDisplaysError(err);
    } finally {
      state.displaysBusy = false;
      if (displaysConfirmBtn) displaysConfirmBtn.disabled = false;
      if (displaysRevertBtn) displaysRevertBtn.disabled = false;
      refreshDisplaysUi();
    }
  }

  // --- Fase 3: pestañas -----------------------------------------------------

  function switchDisplaysTab(tab) {
    state.displaysTab = tab;
    if (displaysTabs) {
      displaysTabs.querySelectorAll('.displays-tab').forEach((b) => {
        b.classList.toggle('is-active', b.dataset.tab === tab);
      });
    }
    displaysPanes.forEach((p) => { p.hidden = p.dataset.pane !== tab; });
    if (displaysError) displaysError.hidden = true;
    hideProfileConfirm();
    cancelShortcutCapture();
    // Los datos de perfiles/ajustes se piden recién al entrar a su pestaña.
    if (tab === 'profiles') loadProfiles();
    else if (tab === 'settings') loadSettings();
    // Lienzo: si hay un acomodo local sin aplicar, se conserva al volver a la
    // pestaña (mismo criterio que la re-sync); si no, se arranca de la realidad.
    else if (tab === 'canvas') { if (!canvasDirty) initCanvasDraft(); renderCanvas(); }
  }

  // --- Fase 3: perfiles -----------------------------------------------------

  function buildProfileItem(p) {
    const li = document.createElement('li');
    li.className = 'displays-profile-item';
    li.dataset.name = p.name;
    // Template ESTATICO; los datos entran por setText (mismo patron que las filas
    // de monitores y de peers: nunca innerHTML con strings del backend).
    li.innerHTML = `
      <div class="displays-profile-info">
        <div class="displays-profile-name-text"></div>
        <div class="mono displays-profile-summary"></div>
      </div>
      <div class="displays-profile-actions">
        <button class="modal-btn small accept displays-profile-load" type="button">CARGAR</button>
        <button class="modal-btn small displays-profile-update" type="button">↻ actualizar</button>
        <button class="modal-btn small displays-profile-shortcut" type="button">⌨ atajo</button>
        <button class="modal-btn small reject displays-profile-delete" type="button">BORRAR</button>
      </div>`;
    updateProfileItem(li, p);
    return li;
  }

  function updateProfileItem(li, p) {
    setText(li.querySelector('.displays-profile-name-text'), p.name);
    setText(li.querySelector('.displays-profile-summary'),
      (typeof p.summary === 'string' && p.summary) ? p.summary : '—');
    const busy = !!state.displaysBusy;
    const pending = !!state.displaysPending;
    const loadBtn = li.querySelector('.displays-profile-load');
    const delBtn = li.querySelector('.displays-profile-delete');
    if (loadBtn) {
      loadBtn.disabled = busy || pending;
      loadBtn.title = pending
        ? 'Hay un cambio esperando confirmación.'
        : 'Aplicar este perfil (vuelve solo si no lo confirmás).';
    }
    if (delBtn) delBtn.disabled = busy;

    // Displays v2, Fase 1: botón actualizar (pisa el perfil con el layout actual)
    // y botón de atajo (muestra la combinación asignada o "atajo").
    const updateBtn = li.querySelector('.displays-profile-update');
    if (updateBtn) {
      updateBtn.disabled = busy || pending;
      updateBtn.title = pending
        ? 'Hay un cambio esperando confirmación.'
        : 'Pisar este perfil con el layout actual (previa confirmación).';
    }
    const scBtn = li.querySelector('.displays-profile-shortcut');
    // Si esta fila está capturando un atajo, no se le pisa el texto (lo maneja
    // el flujo de captura); si no, refleja el atajo guardado.
    if (scBtn && shortcutCaptureName !== p.name) {
      scBtn.disabled = busy;
      // Esta fila NO está capturando: sacar el latido magenta si venía de una
      // captura anterior (renderProfiles reusa el <li>, la clase no se limpia sola).
      scBtn.classList.remove('capturing');
      const sc = (typeof p.shortcut === 'string' && p.shortcut) ? p.shortcut : '';
      scBtn.textContent = sc ? `⌨ ${sc}` : '⌨ atajo';
      scBtn.classList.toggle('has-shortcut', !!sc);
      scBtn.title = sc
        ? `Atajo: ${sc}. Clic para cambiarlo (Supr borra · Esc cancela).`
        : 'Asignar un atajo global para aplicar este perfil.';
    }
  }

  // Render por diff, como renderDisplays: se reusa el <li> por data-name.
  function renderProfiles() {
    if (!displaysProfilesList) return;
    const items = state.displaysProfiles;
    if (items === null) return; // todavia no cargados: no tocar el DOM
    const existing = new Map();
    Array.from(displaysProfilesList.querySelectorAll('li.displays-profile-item')).forEach((li) => {
      if (li.dataset.name) existing.set(li.dataset.name, li);
    });
    const seen = new Set();
    items.forEach((p, idx) => {
      seen.add(p.name);
      try {
        let li = existing.get(p.name);
        if (li) updateProfileItem(li, p);
        else li = buildProfileItem(p);
        if (displaysProfilesList.children[idx] !== li) {
          displaysProfilesList.insertBefore(li, displaysProfilesList.children[idx] || null);
        }
      } catch (err) {
        console.error('[displays] perfil malformado', err);
      }
    });
    existing.forEach((li, name) => { if (!seen.has(name)) li.remove(); });
    if (displaysProfilesEmpty) displaysProfilesEmpty.hidden = items.length > 0;
    if (displaysSaveProfileBtn) displaysSaveProfileBtn.disabled = !!state.displaysBusy;
  }

  async function loadProfiles() {
    if (displaysError) displaysError.hidden = true;
    try {
      const profiles = await invoke('displays_list_profiles');
      state.displaysProfiles = Array.isArray(profiles) ? profiles : [];
    } catch (err) {
      state.displaysProfiles = [];
      showDisplaysError(err);
    }
    renderProfiles();
  }

  // El banner de confirmacion se reusa para pisar y para borrar. Tus perfiles son
  // tus datos: ninguna de las dos corre sin que aprietes SÍ.
  function hideProfileConfirm() {
    pendingProfileAction = null;
    if (displaysProfileConfirm) displaysProfileConfirm.hidden = true;
  }

  function askProfileConfirm(action, message) {
    pendingProfileAction = action;
    if (displaysProfileConfirmText) displaysProfileConfirmText.textContent = message;
    if (displaysProfileConfirm) displaysProfileConfirm.hidden = false;
  }

  async function runPendingProfileAction() {
    const action = pendingProfileAction;
    hideProfileConfirm();
    if (!action) return;
    if (action.type === 'overwrite') await doSaveProfile(action.name);
    else if (action.type === 'delete') await doDeleteProfile(action.name);
  }

  function saveProfileFlow() {
    hideProfileConfirm();
    const typed = (displaysProfileName && displaysProfileName.value || '').trim();
    if (!typed) {
      setStatus('DISPLAYS · poné un nombre para el perfil', { priority: 'warn', ttl: 4000 });
      if (displaysProfileName) displaysProfileName.focus();
      return;
    }
    // Guarda anti-pisado a ciegas: sin la lista cargada no se puede saber si el
    // nombre ya existe, y guardar igual pisaría un perfil sin mostrar el banner.
    if (state.displaysProfiles === null) {
      setStatus('DISPLAYS · esperá, cargando perfiles…', { priority: 'warn', ttl: 3000 });
      return;
    }
    // Pisar un perfil que ya existe = transformar tus datos: se pide OK antes. El
    // match es sin distinguir mayúsculas, PERO se pisa el nombre CANÓNICO ya
    // guardado (no la capitalización tipeada): el backend hace upsert por nombre
    // exacto, así que "trabajo" sobre "Trabajo" tiene que reemplazar "Trabajo",
    // no crear un duplicado.
    const existing = (state.displaysProfiles || []).find(
      (p) => p.name.toLowerCase() === typed.toLowerCase()
    );
    if (existing) {
      askProfileConfirm(
        { type: 'overwrite', name: existing.name },
        `Ya existe "${existing.name}". Guardar reemplaza su layout guardado por el actual.`
      );
      return;
    }
    doSaveProfile(typed);
  }

  async function doSaveProfile(name) {
    if (state.displaysBusy) return;
    state.displaysBusy = true;
    renderProfiles();
    try {
      const profiles = await invoke('displays_save_profile', { name });
      state.displaysProfiles = Array.isArray(profiles) ? profiles : [];
      if (displaysProfileName) displaysProfileName.value = '';
      setStatus(`DISPLAYS · perfil "${name}" guardado`, { force: true });
    } catch (err) {
      showDisplaysError(err);
    } finally {
      state.displaysBusy = false;
      renderProfiles();
    }
  }

  // Cargar = aplicar el layout del perfil, con la MISMA red que el detach: el
  // snapshot vuelve con la cuenta regresiva, y si no confirmás vuelve solo.
  async function loadProfileFlow(name) {
    if (state.displaysBusy || state.displaysPending) return;
    hideProfileConfirm();
    state.displaysBusy = true;
    if (displaysError) displaysError.hidden = true;
    renderProfiles();
    blip(660, 0.06);
    setStatus(`DISPLAYS · cargando "${name}"…`, { force: true });
    try {
      applyDisplaysSnapshot(await invoke('displays_load_profile', { name }));
    } catch (err) {
      try { await loadDisplays(); } catch (_) {}
      showDisplaysError(err);
    } finally {
      state.displaysBusy = false;
      refreshDisplaysUi();
    }
  }

  function deleteProfileFlow(name) {
    if (state.displaysBusy) return;
    askProfileConfirm({ type: 'delete', name }, `¿Borrar el perfil "${name}"? No se puede deshacer.`);
  }

  async function doDeleteProfile(name) {
    if (state.displaysBusy) return;
    state.displaysBusy = true;
    renderProfiles();
    try {
      const profiles = await invoke('displays_delete_profile', { name });
      state.displaysProfiles = Array.isArray(profiles) ? profiles : [];
      setStatus(`DISPLAYS · perfil "${name}" borrado`, { force: true });
    } catch (err) {
      showDisplaysError(err);
    } finally {
      state.displaysBusy = false;
      renderProfiles();
    }
  }

  // --- Displays v2, Fase 1: actualizar perfil + atajos ----------------------

  // "↻ actualizar": pisar el perfil con el layout actual. Es tu dato → se pide
  // OK por el mismo banner que el guardado, y se reusa doSaveProfile (upsert por
  // nombre exacto).
  function updateProfileFlow(name) {
    if (state.displaysBusy || state.displaysPending) return;
    askProfileConfirm(
      { type: 'overwrite', name },
      `Pisar "${name}" con el layout actual (monitores, posiciones y primario de ahora).`
    );
  }

  // Captura de un atajo: al clickear el botón se escucha el próximo keydown y se
  // arma el accelerator ("Ctrl+Shift+1"). Requiere al menos un modificador
  // (Ctrl/Shift/Alt). Esc cancela; Supr/Backspace borra el atajo del perfil.
  function startShortcutCapture(name) {
    if (state.displaysBusy) return;
    cancelShortcutCapture(); // por si había otra fila capturando
    shortcutCaptureName = name;
    if (displaysProfilesList) {
      // Buscar la fila por dataset.name (no por selector con el valor inyectado).
      const li = Array.from(displaysProfilesList.querySelectorAll('li.displays-profile-item'))
        .find((x) => x.dataset && x.dataset.name === name);
      const btn = li ? li.querySelector('.displays-profile-shortcut') : null;
      if (btn) { btn.textContent = '⌨ apretá teclas…'; btn.classList.add('capturing'); }
    }
    setStatus('DISPLAYS · apretá una combinación con Ctrl/Shift/Alt · Supr borra · Esc cancela', { force: true, ttl: 8000 });
    shortcutKeyHandler = onShortcutCaptureKey;
    document.addEventListener('keydown', shortcutKeyHandler, true);
  }

  function cancelShortcutCapture() {
    if (shortcutKeyHandler) {
      document.removeEventListener('keydown', shortcutKeyHandler, true);
      shortcutKeyHandler = null;
    }
    const wasCapturing = shortcutCaptureName !== null;
    shortcutCaptureName = null;
    // Restaurar el texto del botón desde el estado (limpia la clase 'capturing').
    if (wasCapturing) renderProfiles();
  }

  function onShortcutCaptureKey(e) {
    e.preventDefault();
    e.stopPropagation();
    const key = e.key;
    if (key === 'Escape') {
      cancelShortcutCapture();
      setStatus('DISPLAYS · captura cancelada', { ttl: 2500 });
      return;
    }
    if (key === 'Delete' || key === 'Backspace') {
      const name = shortcutCaptureName;
      cancelShortcutCapture();
      if (name) clearShortcut(name);
      return;
    }
    // Un modificador suelto no cierra la captura: se espera una tecla real.
    if (key === 'Control' || key === 'Shift' || key === 'Alt' || key === 'Meta') return;
    const mods = [];
    if (e.ctrlKey) mods.push('Ctrl');
    if (e.shiftKey) mods.push('Shift');
    if (e.altKey) mods.push('Alt');
    if (mods.length === 0) {
      setStatus('DISPLAYS · usá al menos Ctrl, Shift o Alt', { priority: 'warn', ttl: 3000 });
      return; // se sigue capturando
    }
    const token = accelKeyToken(e);
    if (!token) {
      setStatus('DISPLAYS · esa tecla no sirve (usá letras, números o F1–F24)', { priority: 'warn', ttl: 3500 });
      return; // se sigue capturando
    }
    const accelerator = mods.concat(token).join('+');
    const name = shortcutCaptureName;
    cancelShortcutCapture();
    if (name) assignShortcut(name, accelerator);
  }

  // Traduce el keydown a un token que el parser de Tauri acepta ("A","1","F1"…).
  // Se usa e.code (físico) para no depender del layout de teclado.
  function accelKeyToken(e) {
    const code = e.code || '';
    if (/^Key[A-Z]$/.test(code)) return code.slice(3);       // KeyA -> A
    if (/^Digit[0-9]$/.test(code)) return code.slice(5);     // Digit1 -> 1
    if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;  // F1..F24
    return null;
  }

  async function assignShortcut(name, accelerator) {
    if (state.displaysBusy) return;
    state.displaysBusy = true;
    renderProfiles();
    try {
      const profiles = await invoke('displays_set_profile_shortcut', { name, accelerator });
      state.displaysProfiles = Array.isArray(profiles) ? profiles : [];
      setStatus(`DISPLAYS · atajo ${accelerator} → "${name}"`, { force: true });
    } catch (err) {
      // Conflicto (la combinación la usa otro programa) o inválida: el backend
      // ya deshizo el guardado; acá solo se avisa.
      showDisplaysError(err);
    } finally {
      state.displaysBusy = false;
      renderProfiles();
    }
  }

  async function clearShortcut(name) {
    if (state.displaysBusy) return;
    state.displaysBusy = true;
    renderProfiles();
    try {
      const profiles = await invoke('displays_clear_profile_shortcut', { name });
      state.displaysProfiles = Array.isArray(profiles) ? profiles : [];
      setStatus(`DISPLAYS · atajo de "${name}" borrado`, { force: true });
    } catch (err) {
      showDisplaysError(err);
    } finally {
      state.displaysBusy = false;
      renderProfiles();
    }
  }

  // --- Fase 3: ajustes ------------------------------------------------------

  // Rellena el <select> del perfil de arranque desde la lista de perfiles, sin
  // innerHTML (createElement + textContent). Preserva la selección si el perfil
  // sigue existiendo; si no, cae a "ninguno".
  function populateStartupOptions() {
    if (!displaysStartupSelect) return;
    const current = displaysStartupSelect.value;
    const profiles = Array.isArray(state.displaysProfiles) ? state.displaysProfiles : [];
    displaysStartupSelect.replaceChildren();
    const none = document.createElement('option');
    none.value = '';
    none.textContent = '— ninguno —';
    displaysStartupSelect.appendChild(none);
    profiles.forEach((p) => {
      const opt = document.createElement('option');
      opt.value = p.name;
      opt.textContent = p.name;
      displaysStartupSelect.appendChild(opt);
    });
    displaysStartupSelect.value = profiles.some((p) => p.name === current) ? current : '';
  }

  async function loadSettings() {
    if (displaysError) displaysError.hidden = true;
    displaysSettingsLoaded = false; // hasta poblar, saveSettings NO debe correr
    // El selector de startup necesita la lista de perfiles: se pide si no está.
    if (state.displaysProfiles === null) { try { await loadProfiles(); } catch (_) {} }
    populateStartupOptions();
    try {
      const settings = await invoke('displays_get_settings');
      const secs = Number(settings && settings.revertTimeoutSecs);
      if (displaysRevertSecsInput && Number.isFinite(secs)) displaysRevertSecsInput.value = String(secs);
      const startup = settings && settings.startupProfileName;
      if (displaysStartupSelect) displaysStartupSelect.value = (typeof startup === 'string') ? startup : '';
      const shortcutsOn = !!(settings && settings.globalShortcutsEnabled);
      if (displaysShortcutsToggle) displaysShortcutsToggle.checked = shortcutsOn;
      if (displaysShortcutsToggleLabel) displaysShortcutsToggleLabel.textContent = shortcutsOn ? 'ON' : 'OFF';
      displaysSettingsLoaded = true; // ahora sí: los controles reflejan el backend
    } catch (err) {
      showDisplaysError(err);
    }
  }

  // Guarda LOS TRES ajustes juntos (plazo + perfil de arranque + interruptor de
  // atajos). Se manda siempre el objeto completo para no pisar dato del usuario
  // con un default; el backend preserva el resto (mapa de atajos, bases, etc.).
  async function saveSettings() {
    // No guardar hasta que loadSettings haya poblado los controles: si no, se
    // mandarían los defaults vacíos y se borraría el perfil de arranque (dato del
    // usuario). Y no reentrar mientras hay un guardado en vuelo.
    if (state.displaysBusy || !displaysSettingsLoaded) return;
    const raw = Number(displaysRevertSecsInput && displaysRevertSecsInput.value);
    if (!Number.isFinite(raw)) {
      setStatus('DISPLAYS · el plazo tiene que ser un número', { priority: 'warn', ttl: 4000 });
      return;
    }
    // El backend igual pone un piso de 1s; acá se acota a un rango con sentido.
    const secs = Math.min(120, Math.max(3, Math.round(raw)));
    const startupProfileName = (displaysStartupSelect && displaysStartupSelect.value) || null;
    const globalShortcutsEnabled = !!(displaysShortcutsToggle && displaysShortcutsToggle.checked);
    state.displaysBusy = true;
    // Desactivar los tres controles durante el round-trip: cierra la carrera de
    // tocar otro control (o el mismo dos veces) antes de que el guardado vuelva.
    if (displaysSettingsSaveBtn) displaysSettingsSaveBtn.disabled = true;
    if (displaysStartupSelect) displaysStartupSelect.disabled = true;
    if (displaysShortcutsToggle) displaysShortcutsToggle.disabled = true;
    try {
      const settings = await invoke('displays_update_settings', {
        settings: { revertTimeoutSecs: secs, startupProfileName, globalShortcutsEnabled },
      });
      const saved = Number(settings && settings.revertTimeoutSecs);
      const shown = Number.isFinite(saved) ? saved : secs;
      if (displaysRevertSecsInput) displaysRevertSecsInput.value = String(shown);
      // Reflejar lo que quedó (el backend normaliza startup vacío → null).
      const savedStartup = settings && settings.startupProfileName;
      if (displaysStartupSelect) displaysStartupSelect.value = (typeof savedStartup === 'string') ? savedStartup : '';
      const savedShortcuts = !!(settings && settings.globalShortcutsEnabled);
      if (displaysShortcutsToggle) displaysShortcutsToggle.checked = savedShortcuts;
      if (displaysShortcutsToggleLabel) displaysShortcutsToggleLabel.textContent = savedShortcuts ? 'ON' : 'OFF';
      setStatus('DISPLAYS · ajustes guardados', { force: true });
    } catch (err) {
      showDisplaysError(err);
      // Falló el guardado: recargar la verdad para que los controles (y el label
      // ON/OFF) no queden divergentes de lo que quedó en disco.
      try { await loadSettings(); } catch (_) {}
    } finally {
      state.displaysBusy = false;
      if (displaysSettingsSaveBtn) displaysSettingsSaveBtn.disabled = false;
      if (displaysStartupSelect) displaysStartupSelect.disabled = false;
      if (displaysShortcutsToggle) displaysShortcutsToggle.disabled = false;
    }
  }

  // --- Fase 3: lienzo de arrastre (opción A — espejo de Windows) ------------

  function monKey(m) { return `${m.adapterLuid}:${m.targetId}`; }

  // El borrador arranca de los monitores ACTIVOS del snapshot, con su posición
  // actual. Solo se acomodan los activos (los apagados no tienen lugar).
  function initCanvasDraft() {
    const active = (state.displays || []).filter((d) => d.active);
    state.displaysDraft = active.map((d) => ({
      adapterLuid: d.adapterLuid,
      targetId: d.targetId,
      name: d.name,
      primary: !!d.primary,
      w: d.width > 0 ? d.width : 1920,
      h: d.height > 0 ? d.height : 1080,
      x: Number.isFinite(d.positionX) ? d.positionX : 0,
      y: Number.isFinite(d.positionY) ? d.positionY : 0,
    }));
    canvasDirty = false;
  }

  function positionCanvasMonitor(div, m) {
    if (!canvasView) return;
    const { scale, minX, minY, offsetX, offsetY } = canvasView;
    div.style.left = `${offsetX + (m.x - minX) * scale}px`;
    div.style.top = `${offsetY + (m.y - minY) * scale}px`;
    div.style.width = `${Math.max(8, m.w * scale)}px`;
    div.style.height = `${Math.max(8, m.h * scale)}px`;
  }

  function renderCanvas() {
    if (!displaysCanvas) return;
    const mons = state.displaysDraft || [];
    const busy = !!state.displaysBusy;
    const pending = !!state.displaysPending;
    if (displaysCanvasApplyBtn) displaysCanvasApplyBtn.disabled = busy || pending || mons.length === 0;
    if (displaysCanvasResetBtn) displaysCanvasResetBtn.disabled = busy || mons.length === 0;
    if (displaysCanvasEmpty) displaysCanvasEmpty.hidden = mons.length > 0;
    if (mons.length === 0) { displaysCanvas.replaceChildren(); canvasView = null; return; }

    const pad = 16;
    const cw = displaysCanvas.clientWidth || 400;
    const chh = displaysCanvas.clientHeight || 240;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    mons.forEach((m) => {
      minX = Math.min(minX, m.x); minY = Math.min(minY, m.y);
      maxX = Math.max(maxX, m.x + m.w); maxY = Math.max(maxY, m.y + m.h);
    });
    const bw = Math.max(1, maxX - minX);
    const bh = Math.max(1, maxY - minY);
    const availW = Math.max(1, cw - 2 * pad);
    const availH = Math.max(1, chh - 2 * pad);
    const scale = Math.min(availW / bw, availH / bh);
    const offsetX = pad + (availW - bw * scale) / 2;
    const offsetY = pad + (availH - bh * scale) / 2;
    canvasView = { scale, minX, minY, offsetX, offsetY };

    const nodes = mons.map((m) => {
      const div = document.createElement('div');
      div.className = 'displays-canvas-monitor' + (m.primary ? ' is-primary' : '');
      div.dataset.mon = monKey(m);
      positionCanvasMonitor(div, m);
      // textContent, nunca innerHTML: el nombre viene del backend.
      const label = document.createElement('div');
      label.className = 'displays-canvas-label';
      label.textContent = m.name;
      const meta = document.createElement('div');
      meta.className = 'displays-canvas-meta mono';
      meta.textContent = `${m.w}×${m.h}${m.primary ? ' · P' : ''}`;
      div.appendChild(label);
      div.appendChild(meta);
      return div;
    });
    displaysCanvas.replaceChildren(...nodes);
  }

  function onCanvasMouseDown(e) {
    const div = e.target && e.target.closest ? e.target.closest('.displays-canvas-monitor') : null;
    if (!div || state.displaysBusy || state.displaysPending || !canvasView) return;
    const key = div.dataset.mon;
    const m = (state.displaysDraft || []).find((x) => monKey(x) === key);
    if (!m) return;
    e.preventDefault();
    canvasDrag = {
      m, div,
      startX: e.clientX, startY: e.clientY,
      origX: m.x, origY: m.y,
      scale: canvasView.scale,
    };
    div.classList.add('is-dragging');
    document.addEventListener('mousemove', onCanvasMouseMove);
    document.addEventListener('mouseup', onCanvasMouseUp);
  }

  function onCanvasMouseMove(e) {
    if (!canvasDrag) return;
    const { m, div, startX, startY, origX, origY, scale } = canvasDrag;
    // Píxeles de pantalla → coordenadas virtuales, con la escala CONGELADA al
    // empezar el arrastre (no se re-renderiza, así no rescala bajo el cursor).
    m.x = Math.round(origX + (e.clientX - startX) / scale);
    m.y = Math.round(origY + (e.clientY - startY) / scale);
    canvasDirty = true;
    positionCanvasMonitor(div, m);
  }

  function onCanvasMouseUp() {
    document.removeEventListener('mousemove', onCanvasMouseMove);
    document.removeEventListener('mouseup', onCanvasMouseUp);
    if (!canvasDrag) return;
    const { m, div } = canvasDrag;
    div.classList.remove('is-dragging');
    canvasDrag = null;
    snapMonitor(m);   // pegar al borde de un vecino, sin huecos ni superposición
    renderCanvas();   // recomputa escala/centro con la posición final
  }

  function nearest(v, opts) {
    let best = opts[0], bestD = Infinity;
    opts.forEach((o) => { const d = Math.abs(o - v); if (d < bestD) { bestD = d; best = o; } });
    return best;
  }

  function rectsOverlap(ax, ay, aw, ah, bx, by, bw, bh) {
    // Tocarse por el borde (ax+aw === bx) NO es superponerse.
    return ax < bx + bw && ax + aw > bx && ay < by + bh && ay + ah > by;
  }

  // Opción A: al soltar, el monitor se pega flush al borde de algún vecino y se
  // alinea en el eje perpendicular, eligiendo la ubicación más cercana a donde
  // se lo soltó y que no se superponga con nadie.
  function snapMonitor(dragged) {
    const others = (state.displaysDraft || []).filter((x) => x !== dragged);
    if (!others.length) return;
    const dropX = dragged.x, dropY = dragged.y;
    const candidates = [];
    others.forEach((o) => {
      const alignY = nearest(dropY, [o.y, o.y + o.h - dragged.h]);
      const alignX = nearest(dropX, [o.x, o.x + o.w - dragged.w]);
      candidates.push({ x: o.x + o.w, y: alignY });        // a la derecha de O
      candidates.push({ x: o.x - dragged.w, y: alignY });  // a la izquierda
      candidates.push({ x: alignX, y: o.y + o.h });        // abajo
      candidates.push({ x: alignX, y: o.y - dragged.h });  // arriba
    });
    const valid = candidates.filter((c) =>
      !others.some((o) => rectsOverlap(c.x, c.y, dragged.w, dragged.h, o.x, o.y, o.w, o.h))
    );
    const pool = valid.length ? valid : candidates;
    let best = pool[0], bestD = Infinity;
    pool.forEach((c) => {
      const d = Math.hypot(c.x - dropX, c.y - dropY);
      if (d < bestD) { bestD = d; best = c; }
    });
    if (best) { dragged.x = best.x; dragged.y = best.y; }
  }

  function canvasResetDraft() {
    if (state.displaysBusy) return;
    initCanvasDraft();
    renderCanvas();
    setStatus('DISPLAYS · acomodo deshecho', { force: true });
  }

  // APLICAR: el acomodo es un SetDisplayConfig, así que vuelve con la misma red
  // (cuenta regresiva + auto-revert) que el toggle o el cargar perfil.
  async function canvasApplyFlow() {
    if (state.displaysBusy || state.displaysPending) return;
    const mons = state.displaysDraft || [];
    if (!mons.length) return;
    const positions = mons.map((m) => ({
      adapterLuid: m.adapterLuid, targetId: m.targetId, x: m.x, y: m.y,
    }));
    // Se manda al sistema: deja de ser una edición local suelta. Así, cuando la
    // confirmación se resuelva, el lienzo re-sincroniza con la realidad.
    canvasDirty = false;
    state.displaysBusy = true;
    if (displaysError) displaysError.hidden = true;
    refreshDisplaysUi();
    blip(660, 0.06);
    setStatus('DISPLAYS · aplicando el acomodo…', { force: true });
    try {
      applyDisplaysSnapshot(await invoke('displays_apply_layout', { positions }));
    } catch (err) {
      try { await loadDisplays(); } catch (_) {}
      showDisplaysError(err);
    } finally {
      state.displaysBusy = false;
      refreshDisplaysUi();
    }
  }

  async function openDisplaysModal() {
    if (!displaysModal) return;
    // Vaciar antes del await: si no, al reabrir se ven los monitores del
    // snapshot anterior como si fueran los de ahora.
    state.displays = [];
    renderDisplays();
    // Fase 3: cada apertura arranca en LISTA; perfiles/ajustes se cargan recién
    // al entrar a su pestaña, y el nombre a medio tipear no sobrevive.
    state.displaysProfiles = null;
    if (displaysProfilesList) displaysProfilesList.replaceChildren();
    if (displaysProfileName) displaysProfileName.value = '';
    displaysSettingsLoaded = false; // AJUSTES se re-carga al entrar a su pestaña
    hideProfileConfirm();
    // Lienzo: sin borrador ni arrastre a medias de una apertura anterior.
    state.displaysDraft = [];
    canvasDirty = false;
    canvasDrag = null;
    switchDisplaysTab('list');
    if (displaysCount) displaysCount.textContent = 'LEYENDO…';
    // renderDisplays() con la lista vacia prende el "Sin monitores"; durante la
    // lectura todavia no sabemos eso.
    if (displaysEmpty) displaysEmpty.hidden = true;
    displaysModal.hidden = false;
    // Si al cerrar el modal habia un cambio esperando, el reloj vuelve a correr
    // en el acto: el deadline es absoluto, asi que sigue siendo valido aunque
    // nadie lo estuviera mirando. El snapshot que llega en un instante lo
    // corrige con el numero del backend.
    if (state.displaysPending) {
      startDisplaysCountdown(state.displaysPending.deadlineAt - Date.now());
    }
    // A mano, NO focusFirstControl: el primer boton del DOM ahora es CONFIRMAR,
    // que casi siempre vive en la barra oculta — enfocar algo con display:none
    // no hace nada y el foco se queda en el body (adios teclado).
    const firstFocus = (state.displaysPending && displaysConfirmBtn) ? displaysConfirmBtn : displaysRefreshBtn;
    if (firstFocus) firstFocus.focus(); else focusFirstControl(displaysModal);
    await loadDisplays();
  }

  function closeDisplaysModal() {
    if (displaysModal) displaysModal.hidden = true;
    // Si quedó una captura de atajo abierta, cortar su listener global de teclado.
    cancelShortcutCapture();
    // El timer no sobrevive al modal. state.displaysPending SI: guarda el
    // deadline para poder rehidratar al reabrir.
    stopDisplaysCountdown();
  }

  if (displaysCloseBtn) displaysCloseBtn.addEventListener('click', closeDisplaysModal);
  if (displaysRefreshBtn) {
    displaysRefreshBtn.addEventListener('click', async () => {
      blip(880, 0.06);
      await loadDisplays();
    });
  }
  if (displaysConfirmBtn) {
    displaysConfirmBtn.addEventListener('click', () => {
      blip(1100, 0.05);
      resolveDisplaysPending('displays_confirm', 'cambio confirmado');
    });
  }
  if (displaysRevertBtn) {
    displaysRevertBtn.addEventListener('click', () => {
      blip(440, 0.08);
      resolveDisplaysPending('displays_revert', 'cambio revertido');
    });
  }
  // Delegacion: el listener vive en la <ul>, no en cada boton. Con render por
  // diff las filas se reusan y se borran solas; enganchar por fila seria
  // acumular listeners o perderlos.
  if (displaysList) {
    displaysList.addEventListener('click', (e) => {
      const el = e.target;
      if (!el || !el.closest) return;
      const li = el.closest('.display-item');
      const id = li && li.dataset ? li.dataset.id : null;
      if (!id) return;
      const toggleBtn = el.closest('.display-toggle-btn');
      const primBtn = el.closest('.display-primary-btn');
      if (toggleBtn && !toggleBtn.disabled) toggleDisplay(id);
      else if (primBtn && !primBtn.disabled) setPrimaryDisplay(id);
    });
  }

  // --- Fase 3: pestañas, perfiles y ajustes --------------------------------
  if (displaysTabs) {
    displaysTabs.addEventListener('click', (e) => {
      const btn = e.target && e.target.closest ? e.target.closest('.displays-tab') : null;
      if (!btn || !btn.dataset.tab) return;
      blip(720, 0.04);
      switchDisplaysTab(btn.dataset.tab);
    });
  }
  if (displaysSaveProfileBtn) {
    displaysSaveProfileBtn.addEventListener('click', () => { blip(880, 0.05); saveProfileFlow(); });
  }
  if (displaysProfileName) {
    displaysProfileName.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') { e.preventDefault(); saveProfileFlow(); }
    });
  }
  if (displaysProfileConfirmYes) {
    displaysProfileConfirmYes.addEventListener('click', () => { blip(1100, 0.05); runPendingProfileAction(); });
  }
  if (displaysProfileConfirmNo) {
    displaysProfileConfirmNo.addEventListener('click', () => { blip(440, 0.06); hideProfileConfirm(); });
  }
  // Delegacion en la lista de perfiles: CARGAR / BORRAR.
  if (displaysProfilesList) {
    displaysProfilesList.addEventListener('click', (e) => {
      const el = e.target;
      if (!el || !el.closest) return;
      const li = el.closest('.displays-profile-item');
      const name = li && li.dataset ? li.dataset.name : null;
      if (!name) return;
      const loadBtn = el.closest('.displays-profile-load');
      const delBtn = el.closest('.displays-profile-delete');
      const updateBtn = el.closest('.displays-profile-update');
      const scBtn = el.closest('.displays-profile-shortcut');
      if (loadBtn && !loadBtn.disabled) loadProfileFlow(name);
      else if (delBtn && !delBtn.disabled) deleteProfileFlow(name);
      else if (updateBtn && !updateBtn.disabled) updateProfileFlow(name);
      else if (scBtn && !scBtn.disabled) startShortcutCapture(name);
    });
  }
  if (displaysSettingsSaveBtn) {
    displaysSettingsSaveBtn.addEventListener('click', () => { blip(880, 0.05); saveSettings(); });
  }
  if (displaysRevertSecsInput) {
    displaysRevertSecsInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') { e.preventDefault(); saveSettings(); }
    });
  }
  // Displays v2, Fase 1: el selector de startup y el interruptor de atajos
  // guardan al cambiar (no tienen botón GUARDAR propio).
  if (displaysStartupSelect) {
    displaysStartupSelect.addEventListener('change', () => { blip(720, 0.04); saveSettings(); });
  }
  if (displaysShortcutsToggle) {
    displaysShortcutsToggle.addEventListener('change', () => {
      blip(720, 0.04);
      // Reflejar el ON/OFF ya mismo, así el label y el checkbox nunca se
      // contradicen aunque saveSettings se descarte; saveSettings lo re-confirma
      // con lo que devuelve el backend.
      if (displaysShortcutsToggleLabel) {
        displaysShortcutsToggleLabel.textContent = displaysShortcutsToggle.checked ? 'ON' : 'OFF';
      }
      saveSettings();
    });
  }
  // Lienzo: arrastre (delegado en el contenedor) + APLICAR / DESHACER.
  if (displaysCanvas) displaysCanvas.addEventListener('mousedown', onCanvasMouseDown);
  if (displaysCanvasApplyBtn) {
    displaysCanvasApplyBtn.addEventListener('click', () => { blip(880, 0.05); canvasApplyFlow(); });
  }
  if (displaysCanvasResetBtn) {
    displaysCanvasResetBtn.addEventListener('click', () => { blip(440, 0.06); canvasResetDraft(); });
  }

  if (displaysModal) {
    displaysModal.addEventListener('click', (e) => {
      if (e.target === displaysModal) closeDisplaysModal();
    });
  }

  // Close any modal on ESC or click outside the panel (defensive layer
  // in case a stray JS error elsewhere skips listeners).
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      if (!settingsModal.hidden) closeSettingsModal();
      if (!incomingModal.hidden) closeIncomingModal();
      if (!addPeerModal.hidden) closeAddPeerModal();
      if (!peerDetailsModal.hidden) closePeerDetailsModal();
      if (logModal && !logModal.hidden) closeLogModal();
      if (qrModal && !qrModal.hidden) closeQrModal();
      if (displaysModal && !displaysModal.hidden) closeDisplaysModal();
    }
  });

  // ---------- Peer details modal -------------------------------------------
  function openPeerDetails(peer) {
    peerDetailsCurrentId = peer.id;
    peerDetailsTitle.textContent = `◈ ${peer.name}`;
    peerDetailsName.value = peer.name;
    peerDetailsFp.textContent = peer.id.slice(0, 32) + '…';
    peerDetailsFp.title = peer.id;
    peerDetailsAddr.textContent = `${peer.ip || '?'}:${peer.port}`;
    setText(peerDetailsStatus, peer.status === 'online' ? '● ONLINE' : '○ OFFLINE');

    // Tags row: MANUAL · OFFLINE FAVORITE · etc. (the stuff we removed
    // from the card so it doesn't clutter the small layout.)
    const tagsList = [];
    if (peer.manual) tagsList.push('MANUAL');
    if (peer.favorite && peer.status === 'offline') tagsList.push('OFFLINE FAVORITE');
    const tagsEl = document.getElementById('peer-details-tags');
    if (tagsEl) {
      if (tagsList.length) {
        tagsEl.textContent = tagsList.join(' · ');
        tagsEl.hidden = false;
      } else {
        tagsEl.hidden = true;
      }
    }

    // Render the big current icon + the 6-picker.
    const bigIcon = document.getElementById('peer-details-icon-current');
    if (bigIcon) bigIcon.innerHTML = iconSvg(peer.iconType);
    const picker = document.getElementById('peer-details-icon-picker');
    if (picker) {
      picker.innerHTML = ICON_KEYS.map((key) => `
        <button class="icon-pick ${key === peer.iconType ? 'active' : ''}" data-icon="${key}" title="${ICON_LABELS[key]}">
          ${ICON_SVG[key]}
        </button>
      `).join('');
    }

    peerDetailsFav.checked = !!peer.favorite;
    peerDetailsFavLabel.textContent = peer.favorite ? 'ON' : 'OFF';
    peerDetailsClip.checked = !!peer.clipboardSync;
    peerDetailsClipLabel.textContent = peer.clipboardSync ? 'ON' : 'OFF';

    // Forget button is always visible now (used to be only for manuals).
    peerDetailsRemove.hidden = false;
    peerDetailsRemove.textContent = '🗑 FORGET PEER';

    peerDetailsModal.hidden = false;
    focusFirstControl(peerDetailsModal);
  }

  function closePeerDetailsModal() {
    peerDetailsModal.hidden = true;
    peerDetailsCurrentId = null;
  }

  peerDetailsCloseBtn.addEventListener('click', closePeerDetailsModal);
  peerDetailsModal.addEventListener('click', (e) => {
    if (e.target === peerDetailsModal) closePeerDetailsModal();
  });

  async function submitDetailsName() {
    if (!peerDetailsCurrentId) return;
    const peer = state.peers.find((p) => p.id === peerDetailsCurrentId);
    if (!peer) return;
    const newName = peerDetailsName.value.trim();
    if (newName === peer.name) return;
    try {
      await invoke('rename_peer', { peerId: peer.id, newName });
      blip(1100, 0.05);
    } catch (err) {
      peerDetailsName.value = peer.name;
      setStatus(`ERR rename · ${err}`);
    }
  }
  peerDetailsName.addEventListener('blur', submitDetailsName);
  peerDetailsName.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') { e.preventDefault(); peerDetailsName.blur(); }
  });

  peerDetailsFav.addEventListener('change', async () => {
    if (!peerDetailsCurrentId) return;
    const value = peerDetailsFav.checked;
    try {
      await invoke('toggle_favorite', { peerId: peerDetailsCurrentId, value });
      peerDetailsFavLabel.textContent = value ? 'ON' : 'OFF';
      blip(value ? 1320 : 440, 0.05);
    } catch (err) {
      peerDetailsFav.checked = !value;
      setStatus(`ERR fav · ${err}`);
    }
  });

  peerDetailsClip.addEventListener('change', async () => {
    if (!peerDetailsCurrentId) return;
    const enabled = peerDetailsClip.checked;
    try {
      await invoke('set_clipboard_sync', { peerId: peerDetailsCurrentId, enabled });
      peerDetailsClipLabel.textContent = enabled ? 'ON' : 'OFF';
      blip(enabled ? 1320 : 440, 0.05);
    } catch (err) {
      peerDetailsClip.checked = !enabled;
      setStatus(`ERR clipboard · ${err}`);
    }
  });

  peerDetailsRemove.addEventListener('click', async () => {
    if (!peerDetailsCurrentId) return;
    const peer = state.peers.find((p) => p.id === peerDetailsCurrentId);
    if (!peer) return;
    const ok = confirm(
      `Forget peer "${peer.name}"?\n\n` +
      'This clears:\n' +
      '  • manual entry (if any)\n' +
      '  • favorite flag\n' +
      '  • custom name\n' +
      '  • custom icon\n' +
      '  • clipboard sync setting\n' +
      '  • live cache\n\n' +
      'The peer will reappear in ALL if seen on the network again.'
    );
    if (!ok) return;
    try {
      await invoke('forget_peer', { peerId: peer.id });
      blip(440, 0.08);
      setStatus(`Forgot ${peer.name}`);
      closePeerDetailsModal();
    } catch (err) {
      setStatus(`ERR forget · ${err}`);
    }
  });

  // Icon picker — delegated click handler on the picker container.
  document.addEventListener('click', async (e) => {
    const pick = e.target.closest('#peer-details-icon-picker .icon-pick');
    if (!pick || !peerDetailsCurrentId) return;
    const icon = pick.dataset.icon;
    if (!icon) return;
    try {
      await invoke('set_peer_icon', { peerId: peerDetailsCurrentId, icon });
      // Visual update: mark this one active, others inactive.
      pick.parentElement.querySelectorAll('.icon-pick').forEach((b) => {
        b.classList.toggle('active', b === pick);
      });
      // Update the big preview too.
      const bigIcon = document.getElementById('peer-details-icon-current');
      if (bigIcon) bigIcon.innerHTML = iconSvg(icon);
      blip(880, 0.04);
    } catch (err) {
      setStatus(`ERR icon · ${err}`);
    }
  });
  settingsModal.addEventListener('click', (e) => {
    if (e.target === settingsModal) closeSettingsModal();
  });
  incomingModal.addEventListener('click', (e) => {
    if (e.target === incomingModal) {
      // Treat click-outside on incoming as a reject (safer default).
      if (state.pendingIncoming) {
        const sid = state.pendingIncoming.sessionId;
        closeIncomingModal();
        invoke('reject_session', { sessionId: sid }).catch(() => {});
      }
    }
  });

  settingsPickDir.addEventListener('click', async () => {
    if (!dialog || !dialog.open) return;
    try {
      const result = await dialog.open({ directory: true, multiple: false });
      if (!result) return;
      await invoke('set_download_dir', { path: result });
      state.settings.downloadDir = result;
      settingsDownloadDir.textContent = result;
      blip(1100, 0.06);
    } catch (err) {
      setStatus(`ERR change dir · ${err}`);
    }
  });

  settingsCheckUpdate.addEventListener('click', async () => {
    settingsCheckUpdate.disabled = true;
    settingsCheckUpdate.textContent = '◷ ...';
    settingsUpdateAction.hidden = true;
    try {
      const info = await invoke('check_for_update');
      updateInfoCache = info;
      settingsUpdateStatus.textContent = `current v${info.currentVersion} · latest v${info.latestVersion}`;
      if (info.hasUpdate && info.downloadUrl) {
        settingsUpdateBanner.textContent = `Update available: v${info.latestVersion}`;
        settingsUpdateAction.hidden = false;
        blip(1320, 0.06);
      } else if (info.hasUpdate) {
        settingsUpdateBanner.textContent = `New version available, no portable asset found.`;
        settingsUpdateAction.hidden = false;
        settingsApplyUpdate.disabled = true;
      } else {
        settingsUpdateBanner.textContent = '';
        blip(660, 0.05);
      }
    } catch (err) {
      settingsUpdateStatus.textContent = `ERR · ${err}`;
    } finally {
      settingsCheckUpdate.disabled = false;
      settingsCheckUpdate.textContent = '▸ CHECK FOR UPDATE';
    }
  });

  settingsApplyUpdate.addEventListener('click', async () => {
    if (!updateInfoCache || !updateInfoCache.downloadUrl) return;
    const isAndroid = /android/i.test(navigator.userAgent);
    const promptMsg = isAndroid
      ? `Download v${updateInfoCache.latestVersion} and open the installer?`
      : `Download v${updateInfoCache.latestVersion} and restart the app?`;
    const ok = confirm(promptMsg);
    if (!ok) return;
    settingsApplyUpdate.disabled = true;
    settingsApplyUpdate.textContent = '◷ DOWNLOADING...';
    try {
      const result = await invoke('apply_update', {
        downloadUrl: updateInfoCache.downloadUrl,
        // Integrity check: the backend aborts if this is missing/mismatched.
        expectedSha256: updateInfoCache.downloadSha256 || null,
      });
      // On Windows the app exits before we get here. On Android the
      // command returns the public Downloads URI where the APK was
      // published via MediaStore. We don't try to launch the system
      // installer programmatically anymore — every variant of
      // plugin-opener we tried (3 shapes across 2 releases) hit the
      // same OpenArgs deserialization bug. Instead we tell the user
      // exactly where the APK landed and let them tap it from
      // Files / the download-complete notification Android shows
      // for MediaStore inserts.
      if (isAndroid && result && typeof result === 'string') {
        settingsApplyUpdate.textContent = '✓ DOWNLOADED — OPEN IT FROM FILES';
        settingsUpdateBanner.textContent = `APK saved to your Downloads folder. Open it from the Files app (or the download notification) and tap "Install".`;
        setStatus('APK downloaded — open it from Files / Downloads to install.');
        // Also fire a system notification so the user has a quick
        // tap-target even if they navigated away from Settings.
        notify('⬆ Update ready', 'Open Downloads to install the new APK.');
      }
    } catch (err) {
      settingsApplyUpdate.textContent = `ERR · ${err}`;
    }
  });

  settingsAutoAccept.addEventListener('change', async () => {
    const value = settingsAutoAccept.checked;
    try {
      await invoke('set_auto_accept_favorites', { value });
      state.settings.autoAcceptFavorites = value;
      settingsAutoAcceptLabel.textContent = value ? 'ON' : 'OFF';
      blip(value ? 1320 : 440, 0.06);
    } catch (err) {
      settingsAutoAccept.checked = !value;
      setStatus(`ERR toggle · ${err}`);
    }
  });

  if (settingsNotifications) {
    settingsNotifications.addEventListener('change', async () => {
      const value = settingsNotifications.checked;
      try {
        await invoke('set_notifications_enabled', { value });
        if (state.settings) state.settings.notificationsEnabled = value;
        settingsNotificationsLabel.textContent = value ? 'ON' : 'OFF';
        blip(value ? 1320 : 440, 0.06);
        if (value) notify('🔔 Notifications enabled', 'Millennium will now toast on activity.');
      } catch (err) {
        settingsNotifications.checked = !value;
        setStatus(`ERR notif · ${err}`);
      }
    });
  }

  if (settingsAutostart) {
    settingsAutostart.addEventListener('change', async () => {
      const value = settingsAutostart.checked;
      try {
        await invoke('set_start_with_windows', { value });
        if (state.settings) state.settings.startWithWindows = value;
        settingsAutostartLabel.textContent = value ? 'ON' : 'OFF';
        blip(value ? 1320 : 440, 0.06);
      } catch (err) {
        settingsAutostart.checked = !value;
        setStatus(`ERR autostart · ${err}`);
      }
    });
  }

  if (settingsFx) {
    settingsFx.addEventListener('change', () => {
      const fxOn = settingsFx.checked;
      document.documentElement.classList.toggle('fx-off', !fxOn);
      try { localStorage.setItem('fx', fxOn ? 'on' : 'off'); } catch (_) {}
      settingsFxLabel.textContent = fxOn ? 'ON' : 'OFF';
      if (!fxOn) {
        stopPh();
      } else if (!textarea.value && document.activeElement !== textarea) {
        phIdx = 0;
        typePh(placeholderLines[0]);
      }
      blip(fxOn ? 1320 : 440, 0.06);
    });
  }

  if (settingsCloseTray) {
    settingsCloseTray.addEventListener('change', async () => {
      const value = settingsCloseTray.checked;
      try {
        await invoke('set_close_to_tray', { value });
        if (state.settings) state.settings.closeToTray = value;
        settingsCloseTrayLabel.textContent = value ? 'ON' : 'OFF';
        blip(value ? 1320 : 440, 0.06);
      } catch (err) {
        settingsCloseTray.checked = !value;
        setStatus(`ERR close-tray · ${err}`);
      }
    });
  }


  // ---------- Add peer by IP (Fase 8) --------------------------------------
  function openAddPeerModal() {
    addPeerError.hidden = true;
    addPeerError.textContent = '';
    addPeerIp.value = '';
    addPeerPort.value = '53319';
    addPeerSubmit.disabled = false;
    addPeerSubmit.textContent = '▸ REGISTER';
    addPeerModal.hidden = false;
    focusFirstControl(addPeerModal);
    setTimeout(() => addPeerIp.focus(), 0);
  }

  function closeAddPeerModal() {
    addPeerModal.hidden = true;
  }

  addPeerBtn.addEventListener('click', () => {
    blip(880, 0.06);
    openAddPeerModal();
  });

  addPeerModal.addEventListener('click', (e) => {
    if (e.target === addPeerModal) closeAddPeerModal();
  });

  // CANCEL button — was an inline onclick in index.html; wired here now that
  // a strict CSP (Fase 3, Tarea 3.2) blocks inline handlers. The other two
  // modal-close buttons (settings-close, peer-details-close) already had
  // addEventListener handlers, so only this one needed wiring.
  const addPeerCancelBtn = document.getElementById('add-peer-cancel');
  if (addPeerCancelBtn) addPeerCancelBtn.addEventListener('click', closeAddPeerModal);

  const IPV4_RE = /^(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$/;

  async function submitAddPeer() {
    const ip = addPeerIp.value.trim();
    const port = parseInt(addPeerPort.value, 10) || 53319;
    addPeerError.hidden = true;
    addPeerError.textContent = '';
    if (!IPV4_RE.test(ip)) {
      addPeerError.textContent = 'INVALID IP — use IPv4 (eg 192.168.1.42)';
      addPeerError.hidden = false;
      return;
    }
    if (port < 1 || port > 65535) {
      addPeerError.textContent = 'INVALID PORT';
      addPeerError.hidden = false;
      return;
    }
    addPeerSubmit.disabled = true;
    addPeerSubmit.textContent = '◷ PROBING...';
    try {
      const peer = await invoke('add_peer_by_ip', { ip, port });
      blip(1760, 0.1);
      setTimeout(() => blip(2200, 0.12), 130);
      setStatus(`OK · registered ${peer.alias} (${peer.hexId})`);
      closeAddPeerModal();
    } catch (err) {
      addPeerError.textContent = String(err).toUpperCase();
      addPeerError.hidden = false;
      addPeerSubmit.disabled = false;
      addPeerSubmit.textContent = '▸ RETRY';
      blip(220, 0.15);
    }
  }

  addPeerSubmit.addEventListener('click', submitAddPeer);
  addPeerIp.addEventListener('keydown', (e) => { if (e.key === 'Enter') submitAddPeer(); });
  addPeerPort.addEventListener('keydown', (e) => { if (e.key === 'Enter') submitAddPeer(); });

  // ---------- Boot ----------------------------------------------------------
  async function boot() {
    // PANEL_OPEN lives in the backend process, which survives a webview
    // reload (F5 / tauri dev hot-reload). The log modal starts hidden, so
    // reset the flag to match — otherwise the backend keeps emitting a
    // log-line IPC event per line into a closed panel.
    invoke('set_log_panel_open', { open: false }).catch(() => {});
    try {
      const info = await invoke('get_local_info');
      window.__LOCAL_INFO = info;
      hudHost.textContent = info.alias;
      hudIp.textContent = `${info.ip}:${info.port}`;
      hudVersion.textContent = `v${info.version}`;
    } catch (err) {
      hudHost.textContent = 'ERR';
      setStatus(`ERR get_local_info · ${err}`);
      console.error(err);
      return;
    }

    try {
      const peers = await invoke('list_peers');
      applyPeers(peers, /* initial = */ true);
    } catch (err) {
      setStatus(`ERR list_peers · ${err}`);
      console.error(err);
    }

    // Live updates from the mDNS daemon (Fase 3+)
    await listen('peers-changed', (event) => {
      applyPeers(event.payload, /* initial = */ false);
    });

    // Backend failures (HTTPS server bind, etc.) — surface a big
    // persistent banner over the peer list so this can't be missed.
    await listen('backend-error', (event) => {
      const msg = typeof event.payload === 'string' ? event.payload : JSON.stringify(event.payload);
      showBackendBanner('error', msg);
      setStatus(`BACKEND ERR · ${msg}`);
      console.error('[backend-error]', event.payload);
    });


    // Live runtime log lines — append into the in-page buffer so when
    // the user opens the LOG modal they see everything that happened.
    await listen('log-line', (event) => {
      if (typeof event.payload === 'string') {
        appendLogLine(event.payload);
      }
    });

    // System tray menu actions. The tray itself is built in Rust; the
    // menu items emit a `tray-action` event with a string payload so
    // we can react from the same place the in-app buttons do.
    await listen('tray-action', async (event) => {
      const action = event.payload;
      if (action === 'log') {
        openLogModal();
      } else if (action === 'send') {
        // Focus the peer list — user picks a peer manually for now.
        setStatus('Pick a peer to send to.');
      } else if (action === 'toggle-clipboard') {
        // Best-effort: flip clipboard sync for every peer that already has it.
        const onPeers = (state.peers || []).filter((p) => p.clipboardSync);
        if (onPeers.length === 0) {
          setStatus('No peer has clipboard sync enabled.');
        } else {
          for (const p of onPeers) {
            try {
              await invoke('set_clipboard_sync', { peerId: p.id, enabled: false });
            } catch (_) {}
          }
          setStatus(`Clipboard sync disabled for ${onPeers.length} peer(s).`);
        }
      }
    });

    // Incoming text from a peer (Fase 5)
    await listen('incoming-text', (event) => {
      const { text, senderAlias, senderFingerprint, senderIp, senderPort } = event.payload;
      showIncomingText(text, senderAlias, senderFingerprint, senderIp, senderPort);
      blip(1320, 0.12);
      setTimeout(() => blip(1760, 0.1), 130);
      const preview = text.length > 80 ? text.slice(0, 80) + '…' : text;
      notify(`✉ Text from ${senderAlias}`, preview);
    });

    // Incoming file transfer request — show modal (Fase 7)
    await listen('incoming-files-request', (event) => {
      const payload = event.payload;
      if (payload.autoAccepted) {
        setStatus(`RX · auto-accepting ${payload.fileCount} file(s) from ${payload.senderAlias}`);
        notify(
          `⇣ ${payload.senderAlias} — ${payload.fileCount} file(s)`,
          `${formatBytes(payload.totalSize)} · auto-accepted (favorite)`
        );
        return;
      }
      openIncomingModal(payload);
      notify(
        `? ${payload.senderAlias} wants to send ${payload.fileCount} file(s)`,
        `${formatBytes(payload.totalSize)} · waiting for your decision`
      );
    });

    await listen('incoming-files-timeout', () => {
      closeIncomingModal();
      setStatus('RX · timed out (no decision in 60 s)');
    });

    await listen('incoming-files-approved', () => {
      setStatus('RX · accepted, receiving...');
    });

    // Progress events (Fase 7)
    await listen('transfer-progress-sender', (event) => {
      const { bytesSent, total } = event.payload;
      if (total > 0) {
        const pct = Math.min(99, Math.round((bytesSent / total) * 100));
        setProgress(pct);
        progressText.textContent = `TRANSMITTING // ${formatBytes(bytesSent)} / ${formatBytes(total)}`;
      }
    });

    await listen('transfer-progress-receiver', (event) => {
      const { bytesReceived, total, sessionId } = event.payload;
      if (!total || total <= 0) return;
      // Key RX by session so a concurrent send (which drives the TX bar) can't
      // make the two flows fight over one bar.
      if (!state.activeReceive || state.activeReceive.sessionId !== sessionId) {
        state.activeReceive = { sessionId };
      }
      const pct = Math.min(99, Math.round((bytesReceived / total) * 100));
      setRxProgress(pct);
      rxProgressBlock.hidden = false;
      rxProgressText.textContent = `RECEIVING // ${formatBytes(bytesReceived)} / ${formatBytes(total)}`;
      // Info + no TTL barrier: routine RX progress, must keep updating and
      // must not block errors from showing.
      setStatus(`RX · ${formatBytes(bytesReceived)} received`, { priority: 'info', ttl: 0 });
    });

    await listen('file-completed', (event) => {
      const { name, verified } = event.payload;
      if (!verified) {
        setStatus(`WARN · ${name} hash mismatch`);
      }
    });

    await listen('session-completed', (event) => {
      const { senderAlias, fileCount, totalSize, destinationDir } = event.payload;
      // This is a RECEIVER event — clear the RX bar, never the TX bar.
      rxProgressBlock.hidden = true;
      setRxProgress(0);
      state.activeReceive = null;
      // On Android the backend publishes the final file to /Downloads/
      // via MediaStore (see http_server.rs), so the user-visible
      // location is "Downloads" regardless of the app-scoped staging
      // path that came in the event.
      const isAndroid = /android/i.test(navigator.userAgent);
      const visibleLocation = isAndroid ? 'Downloads (open from Files / Gallery)' : destinationDir;
      setStatus(`RX OK · ${fileCount} file(s) from ${senderAlias} saved`);
      showToast(`${senderAlias} → ${fileCount} FILE(S) · ${formatBytes(totalSize)} · saved to ${visibleLocation}`);
      blip(1760, 0.12);
      setTimeout(() => blip(2200, 0.16), 130);
      notify(
        `✓ ${fileCount} file(s) received from ${senderAlias}`,
        `${formatBytes(totalSize)} saved to ${visibleLocation}`
      );
    });

    await listen('session-cancelled', () => {
      // Receiver-side cancel — clear the RX bar, leave the TX bar alone.
      rxProgressBlock.hidden = true;
      setRxProgress(0);
      state.activeReceive = null;
      setStatus('Transfer cancelled.');
    });

    // Clipboard sync is desktop-only. On Android we skip both the
    // notify and the status update so the feature is invisible to
    // mobile users (it's also gated by mutual-consent so peers can't
    // actually push to us if no toggle was ever flipped, which on
    // Android can't even be flipped because the UI hides it).
    const _clipboardIsAndroid = /android/i.test(navigator.userAgent);
    await listen('clipboard-received', (event) => {
      if (_clipboardIsAndroid) return;
      const { senderAlias, text } = event.payload;
      const preview = text.length > 40 ? text.slice(0, 40) + '...' : text;
      setStatus(`📋 ${senderAlias} → clipboard: ${preview}`);
      blip(880, 0.06);
      notify(`📋 Clipboard from ${senderAlias}`, preview);
    });
    await listen('clipboard-image-received', (event) => {
      if (_clipboardIsAndroid) return;
      const { senderAlias, width, height } = event.payload;
      setStatus(`🖼 ${senderAlias} → image clipboard: ${width}×${height}`);
      blip(1320, 0.06);
      notify(`🖼 Image clipboard from ${senderAlias}`, `${width}×${height} ready to paste`);
    });

    // Monitores (SPEC-displays Fase 2). La topologia cambio: puede ser un
    // attach/detach nuestro o un cable que alguien movio. Se relee SOLO con el
    // modal abierto — cerrado, el snapshot no lo ve nadie.
    await listen('displays-changed', () => {
      if (displaysModal && !displaysModal.hidden) loadDisplays();
    });

    // Ciclo de confirmacion. El reloj es del backend; esto solo lo dibuja.
    await listen('displays-confirmation', (event) => {
      const payload = event.payload || {};
      const modalOpen = !!(displaysModal && !displaysModal.hidden);
      if (payload.kind === 'applied') {
        const ms = Number(payload.timeoutMs);
        startDisplaysCountdown(Number.isFinite(ms) && ms > 0 ? ms : 10000);
        refreshDisplaysUi();
        setStatus('DISPLAYS · confirmá el cambio o vuelve solo', { priority: 'warn', ttl: 12000 });
      } else if (payload.kind === 'confirmed') {
        clearDisplaysPending();
        refreshDisplaysUi();
        if (modalOpen) loadDisplays();
        setStatus('DISPLAYS · cambio confirmado', { force: true });
      } else if (payload.kind === 'reverted') {
        clearDisplaysPending();
        refreshDisplaysUi();
        if (modalOpen) loadDisplays();
        // El "por que" importa: revertir a mano y quedarse sin confirmar a
        // tiempo se ven identicos en pantalla, y no son lo mismo.
        const why = payload.reason === 'timeout' ? 'nadie confirmó a tiempo'
          : payload.reason === 'error' ? 'el cambio falló'
          : 'lo revertiste vos';
        setStatus(`DISPLAYS · volvió atrás (${why})`, { priority: 'warn', ttl: 8000 });
      }
    });

    // Preload settings (used by transmit + settings modal)
    try {
      state.settings = await invoke('get_settings');
    } catch (err) {
      console.error('settings load:', err);
    }

    // Surface a previous failed auto-update swap. Pull model (not a pushed
    // event): the backend holds the marker until we ask for it here, once the
    // UI is ready, so a slow webview can never miss the notice.
    try {
      const upErr = await invoke('take_update_failure');
      if (upErr) showBackendBanner('error', upErr);
    } catch (_) {}

    setTimeout(() => {
      if (statusMsg.textContent.startsWith('GRID ONLINE')
          || statusMsg.textContent.startsWith('GRID · waiting')) {
        setStatus('SYS READY · awaiting input');
      }
    }, 2200);
  }

  // Backend is the source of truth for `favorite` (Fase 6 persistence).
  function applyPeers(wirePeers, initial) {
    // Normalize status at ingestion so no downstream consumer has to defend
    // against a missing/unknown status. The backend only ever emits
    // "online"/"offline"; anything else maps to "offline" (never dropped).
    const VALID_STATUS = new Set(['online', 'offline']);
    state.peers = wirePeers.map((p) => {
      const status = VALID_STATUS.has(p.status) ? p.status : 'offline';
      return { ...p, status };
    });

    // The selected peer vanished from the snapshot: DO NOT silently re-point
    // to another peer (you could end up transmitting a secret to the wrong
    // one). Surface TARGET LOST and make the user re-select consciously.
    if (state.selectedPeerId && !state.peers.find((p) => p.id === state.selectedPeerId)) {
      state.selectedPeerId = null;
      state.targetLost = true;
      setText(targetName, 'TARGET LOST');
      setText(targetHex, '—');
      if (sendBtn) sendBtn.disabled = true;
      setStatus('TARGET LOST · peer went offline. Pick another.', { priority: 'warn', ttl: 6000 });
    }

    // Render the list ALWAYS so it stays in sync with state.peers.
    renderPeers();

    // Auto-select the first peer ONLY on the initial load — never on later
    // snapshots, and never right after losing the target.
    if (initial && !state.selectedPeerId && !state.targetLost && state.peers.length > 0) {
      // Auto-select the first peer VISIBLE under the active filter (the UI starts
      // on FAVORITES), not blindly peers[0] — otherwise it can lock onto a peer
      // that isn't shown. Mirrors renderPeers' filter predicate exactly.
      const firstVisible = state.peers.find((p) =>
        state.filter === 'favorites' ? p.favorite : p.status !== 'offline'
      );
      if (firstVisible) {
        state.selectedPeerId = firstVisible.id;
        selectPeer(state.selectedPeerId);
      }
    }

    if (state.peers.length === 0) {
      setStatus('GRID · waiting for peers...');
    } else if (initial) {
      setStatus(`GRID ONLINE · ${state.peers.length} peer(s) detected.`);
    } else {
      setStatus(`GRID · ${state.peers.length} peer(s) online.`);
    }
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot);
  } else {
    boot();
  }
})();
