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
  const charCount = document.getElementById('char-count');
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
  const hudUptime = document.getElementById('hud-uptime');
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

  // ---------- Uptime ticker -------------------------------------------------
  const t0 = Date.now();
  setInterval(() => {
    const s = Math.floor((Date.now() - t0) / 1000);
    const hh = String(Math.floor(s / 3600)).padStart(2, '0');
    const mm = String(Math.floor((s % 3600) / 60)).padStart(2, '0');
    const ss = String(s % 60).padStart(2, '0');
    hudUptime.textContent = `${hh}:${mm}:${ss}`;
  }, 1000);

  // ---------- Typewriter placeholder rotator -------------------------------
  const placeholderLines = [
    'TYPE OR PASTE > TRANSMIT TO PEER...',
    'TEXT, URL, SNIPPET — ANY PAYLOAD.',
    'PRESS CTRL+ENTER TO SEND.',
    'NO CLOUD. NO ACCOUNT. JUST THE GRID.',
    'mDNS DISCOVERY · TLS PINNED · LAN ONLY.',
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

  // ---------- Character counter --------------------------------------------
  function updateCharCount() {
    charCount.textContent = String(textarea.value.length).padStart(4, '0');
  }
  textarea.addEventListener('input', updateCharCount);

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
      iconWrap.innerHTML = ICON_SVG[p.iconType] || ICON_SVG.desktop;
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
      <div class="peer-icon" data-icon="${p.iconType}">${ICON_SVG[p.iconType] || ICON_SVG.desktop}</div>
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
  document.querySelectorAll('.mode-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      state.mode = btn.dataset.mode;
      document.querySelectorAll('.mode-btn').forEach((b) => b.classList.remove('active'));
      btn.classList.add('active');
      document.querySelectorAll('.mode-panel').forEach((p) => {
        const active = p.id === `mode-${state.mode}`;
        p.classList.toggle('active', active);
        p.hidden = !active;
      });
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
    fileQueue.innerHTML = '';
    if (state.queuedFiles.length === 0) {
      fileQueue.hidden = true;
      return;
    }
    fileQueue.hidden = false;
    state.queuedFiles.forEach((f, idx) => {
      const li = document.createElement('li');
      li.innerHTML = `<span>▸ ${escapeHtml(f.name)}</span><span style="opacity:.7"> · ${formatBytes(f.size)} · <a href="#" data-remove="${idx}" style="color:var(--neon-magenta);text-decoration:none">[X]</a></span>`;
      fileQueue.appendChild(li);
    });
  }

  function escapeHtml(s) {
    const div = document.createElement('div');
    div.textContent = s;
    return div.innerHTML;
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
        updateCharCount();
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
      const thumb = f.thumbnail
        ? `<img class="incoming-thumb" src="${f.thumbnail}" alt="" />`
        : `<span class="incoming-thumb incoming-thumb-empty">📄</span>`;
      li.innerHTML = `
        ${thumb}
        <span class="file-name">${escapeHtml(f.name)}</span>
        <span class="file-size">${formatBytes(f.size)}</span>
      `;
      incomingFileList.appendChild(li);
    });

    // First-contact banner — offer to save the sender if we don't know them yet.
    const banner = document.getElementById('incoming-firstcontact');
    if (banner) banner.remove();
    if (payload.senderFingerprint && !isKnownPeer(payload.senderFingerprint) && payload.senderIp) {
      const b = document.createElement('div');
      b.id = 'incoming-firstcontact';
      b.className = 'modal-firstcontact';
      b.innerHTML = `
        <div class="settings-label" style="color:var(--neon-magenta);text-shadow:0 0 6px var(--neon-magenta-glow);margin-bottom:6px">FIRST CONTACT</div>
        <div style="display:flex;gap:6px;align-items:center;justify-content:space-between">
          <span class="mono" style="font-size:11px;color:var(--text-mute)">${payload.senderIp}:${payload.senderPort || 53319}</span>
          <button class="modal-btn small" id="firstcontact-save">+ SAVE PEER</button>
        </div>
      `;
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
    setQrTab('show');
    try {
      const data = await invoke('generate_pair_qr');
      qrCanvas.innerHTML = data.svg || '';
      qrCurrentPayload = data.payload || '';
      setText(qrPayload, qrCurrentPayload);
    } catch (err) {
      qrCanvas.innerHTML = `<div style="padding:24px;color:#ff4d6b">${err}</div>`;
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
    if (bigIcon) bigIcon.innerHTML = ICON_SVG[peer.iconType] || ICON_SVG.desktop;
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
      if (bigIcon) bigIcon.innerHTML = ICON_SVG[icon] || ICON_SVG.desktop;
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
      settingsCheckUpdate.textContent = '▸ CHECK';
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

    updateCharCount();
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
      state.selectedPeerId = state.peers[0].id;
      selectPeer(state.selectedPeerId);
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
