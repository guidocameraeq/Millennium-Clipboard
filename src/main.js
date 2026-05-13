// Millennium Clipboard // GRID — frontend (Fase 7)

(() => {
  'use strict';

  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;
  const dialog = window.__TAURI__.dialog;

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
  const toast = document.getElementById('toast');
  const toastText = document.getElementById('toast-text');
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

  const addPeerBtn = document.getElementById('add-peer-btn');
  const addPeerModal = document.getElementById('add-peer-modal');
  const addPeerIp = document.getElementById('add-peer-ip');
  const addPeerPort = document.getElementById('add-peer-port');
  const addPeerError = document.getElementById('add-peer-error');
  const addPeerSubmit = document.getElementById('add-peer-submit');

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

  // ---------- Status helpers -----------------------------------------------
  function setStatus(msg) { statusMsg.textContent = msg; }
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

  function showIncomingText(text, alias, fingerprint) {
    if (toastHideTimer) {
      clearTimeout(toastHideTimer);
      toastHideTimer = null;
    }
    setToastTitle(`◂ INCOMING FROM ${alias}`);
    toastText.innerHTML = '';

    const body = document.createElement('div');
    body.className = 'incoming-body';
    body.textContent = text;
    toastText.appendChild(body);

    const meta = document.createElement('div');
    meta.className = 'incoming-meta mono';
    meta.textContent = `${text.length} CHARS · ${fingerprint.slice(0, 16)}...`;
    toastText.appendChild(meta);

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

    const closeBtn = document.createElement('button');
    closeBtn.className = 'incoming-btn';
    closeBtn.textContent = '✕ CLOSE';
    closeBtn.addEventListener('click', () => {
      toast.hidden = true;
    });
    actions.appendChild(closeBtn);

    toastText.appendChild(actions);
    toast.hidden = false;
    // Persistent — user must dismiss with COPY or CLOSE.
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
    textarea.placeholder = line.slice(0, i) + (i < line.length ? '▌' : '');
    if (i <= line.length) {
      phTimer = setTimeout(() => typePh(line, i + 1), 35 + Math.random() * 45);
    } else {
      phTimer = setTimeout(() => {
        textarea.placeholder = line;
        setTimeout(() => {
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
  typePh(placeholderLines[0]);
  textarea.addEventListener('focus', () => { if (!textarea.value) stopPh(); });
  textarea.addEventListener('blur', () => {
    if (!textarea.value) { phIdx = 0; typePh(placeholderLines[0]); }
  });

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
  const ICON_SVG = {
    desktop: `<svg viewBox="0 0 24 24" width="22" height="22" stroke="currentColor" stroke-width="1.5" fill="none"><rect x="3" y="4" width="18" height="12" rx="1" /><line x1="2" y1="20" x2="22" y2="20" /><line x1="10" y1="16" x2="10" y2="20" /><line x1="14" y1="16" x2="14" y2="20" /></svg>`,
    phone: `<svg viewBox="0 0 24 24" width="22" height="22" stroke="currentColor" stroke-width="1.5" fill="none"><rect x="7" y="2" width="10" height="20" rx="2" /><line x1="10" y1="19" x2="14" y2="19" /></svg>`,
    tablet: `<svg viewBox="0 0 24 24" width="22" height="22" stroke="currentColor" stroke-width="1.5" fill="none"><rect x="3" y="3" width="18" height="18" rx="2" /><line x1="10" y1="18" x2="14" y2="18" /></svg>`,
  };

  function renderPeers() {
    const filtered = state.peers.filter((p) => {
      if (state.filter === 'favorites') return p.favorite;
      // ALL = peers currently on the network
      return p.status !== 'offline';
    });

    peerList.innerHTML = '';

    const onlineCount = state.peers.filter((p) => p.status !== 'offline').length;

    if (state.peers.length === 0) {
      const li = document.createElement('li');
      li.className = 'peer-empty';
      li.innerHTML = '— SCANNING NETWORK —<br><small style="opacity:0.6;letter-spacing:1px;font-size:9px">peers appear within seconds</small>';
      peerList.appendChild(li);
    } else if (filtered.length === 0) {
      const li = document.createElement('li');
      li.className = 'peer-empty';
      if (state.filter === 'favorites') {
        li.innerHTML = '— NO FAVORITES YET —<br><small style="opacity:0.6;letter-spacing:1px;font-size:9px">switch to ALL and click ★ to add one</small>';
      } else {
        li.innerHTML = '— NO PEERS ONLINE —<br><small style="opacity:0.6;letter-spacing:1px;font-size:9px">waiting on the grid</small>';
      }
      peerList.appendChild(li);
    } else {
      filtered.forEach((p) => peerList.appendChild(buildPeerItem(p)));
    }

    const favCount = state.peers.filter((p) => p.favorite).length;
    // HUD peer-count badge reflects what the current tab is showing.
    peerCount.textContent = String(filtered.length).padStart(2, '0');
    // Bottom-strip PEERS = online on the network right now.
    statusPeers.textContent = String(onlineCount).padStart(2, '0');
    statusFav.textContent = String(favCount).padStart(2, '0');
    filterHint.textContent = `${String(filtered.length).padStart(2, '0')} visible`;
  }

  function buildPeerItem(p) {
    const li = document.createElement('li');
    li.className = 'peer-item';
    if (p.id === state.selectedPeerId) li.classList.add('selected');
    li.dataset.id = p.id;
    li.dataset.status = p.status;
    li.dataset.manual = p.manual ? 'true' : 'false';

    li.innerHTML = `
      <div class="peer-icon">${ICON_SVG[p.iconType] || ICON_SVG.desktop}</div>
      <div class="peer-info">
        <div class="peer-name-row">
          <span class="peer-name"></span>
          <button class="fav-btn" aria-label="Toggle favorite"></button>
        </div>
        <div class="peer-meta">
          <span class="peer-hex mono"></span>
          <span class="peer-ip mono"></span>
        </div>
        <div class="peer-status">
          <span class="status-dot"></span><span class="status-label"></span>
        </div>
      </div>
    `;

    li.querySelector('.peer-name').textContent = p.name;
    li.querySelector('.peer-hex').textContent = p.hexId;
    li.querySelector('.peer-ip').textContent = p.ip;

    const favBtn = li.querySelector('.fav-btn');
    favBtn.textContent = p.favorite ? '★' : '☆';
    favBtn.dataset.favorite = p.favorite ? 'true' : 'false';

    const statusEl = li.querySelector('.peer-status');
    statusEl.classList.add(p.status);
    li.querySelector('.status-label').textContent = p.status.toUpperCase();

    return li;
  }

  function selectPeer(id) {
    const peer = state.peers.find((p) => p.id === id);
    if (!peer) return;
    state.selectedPeerId = id;
    targetName.textContent = peer.name;
    targetHex.textContent = peer.hexId;
    const isOffline = peer.status === 'offline';
    sendBtn.disabled = isOffline;
    setStatus(isOffline
      ? `PEER OFFLINE · ${peer.name} (waiting on grid)`
      : `PEER LOCKED · ${peer.name}`);
    blip(660, 0.06);
    document.querySelectorAll('.peer-item').forEach((el) => {
      el.classList.toggle('selected', el.dataset.id === id);
    });
  }

  // ---------- Peer list events (delegated) ---------------------------------
  peerList.addEventListener('click', async (e) => {
    const favBtn = e.target.closest('.fav-btn');
    if (favBtn) {
      e.stopPropagation();
      const item = favBtn.closest('.peer-item');
      const id = item?.dataset.id;
      if (!id) return;
      const peer = state.peers.find((p) => p.id === id);
      if (!peer) return;
      const next = !peer.favorite;
      try {
        await invoke('toggle_favorite', { peerId: id, value: next });
        peer.favorite = next;
        renderPeers();
        blip(next ? 1320 : 440, 0.05);
      } catch (err) {
        setStatus(`ERR toggle_favorite · ${err}`);
      }
      return;
    }
    const item = e.target.closest('.peer-item');
    if (item && item.dataset.id) selectPeer(item.dataset.id);
  });

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
    // Derive name + size on Rust side would be ideal; for MVP we infer
    // name from path and fetch size via fs metadata when sending.
    const name = path.split(/[\\/]/).pop() || 'file';
    state.queuedFiles.push({ path, name, size: 0 });
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
      setStatus('ERR · no peer selected.');
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
        setStatus(`OK · delivered to ${peer.name}.`);
        showToast(`${peer.name} · ${chars} CHARS · ACK`);
        textarea.value = '';
        updateCharCount();
      }, 600);
    } catch (err) {
      clearInterval(tick);
      progressBlock.hidden = true;
      sendBtn.disabled = false;
      setStatus(`ERR transmit · ${err}`);
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
        setStatus(`OK · ${count} file(s) delivered to ${peer.name}.`);
        showToast(`${peer.name} · ${count} FILE(S) · ACK`);
        state.queuedFiles = [];
        renderQueue();
        state.activeTransfer = null;
      }, 700);
    } catch (err) {
      progressBlock.hidden = true;
      sendBtn.disabled = false;
      setStatus(`ERR transmit · ${err}`);
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
        setStatus('LOG · panel TBD (Fase 8+)');
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
      li.innerHTML = `<span class="file-name">${escapeHtml(f.name)}</span><span class="file-size">${formatBytes(f.size)}</span>`;
      incomingFileList.appendChild(li);
    });
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
    settingsModal.hidden = false;
  }

  function closeSettingsModal() {
    settingsModal.hidden = true;
  }

  settingsCloseBtn.addEventListener('click', closeSettingsModal);

  // Close any modal on ESC or click outside the panel (defensive layer
  // in case a stray JS error elsewhere skips listeners).
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      if (!settingsModal.hidden) closeSettingsModal();
      if (!incomingModal.hidden) closeIncomingModal();
      if (!addPeerModal.hidden) closeAddPeerModal();
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
    try {
      const info = await invoke('get_local_info');
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

    // Incoming text from a peer (Fase 5)
    await listen('incoming-text', (event) => {
      const { text, senderAlias, senderFingerprint } = event.payload;
      showIncomingText(text, senderAlias, senderFingerprint);
      blip(1320, 0.12);
      setTimeout(() => blip(1760, 0.1), 130);
    });

    // Incoming file transfer request — show modal (Fase 7)
    await listen('incoming-files-request', (event) => {
      const payload = event.payload;
      if (payload.autoAccepted) {
        // Backend auto-accepted (favorite + auto-accept enabled) — show brief toast.
        setStatus(`RX · auto-accepting ${payload.fileCount} file(s) from ${payload.senderAlias}`);
        return;
      }
      openIncomingModal(payload);
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
      const { bytesReceived, total } = event.payload;
      if (total > 0) {
        const pct = Math.min(99, Math.round((bytesReceived / total) * 100));
        setProgress(pct);
        progressBlock.hidden = false;
        progressText.textContent = `RECEIVING // ${formatBytes(bytesReceived)} / ${formatBytes(total)}`;
        setStatus(`RX · ${formatBytes(bytesReceived)} received`);
      }
    });

    await listen('file-completed', (event) => {
      const { name, verified } = event.payload;
      if (!verified) {
        setStatus(`WARN · ${name} hash mismatch`);
      }
    });

    await listen('session-completed', (event) => {
      const { senderAlias, fileCount, totalSize, destinationDir } = event.payload;
      progressBlock.hidden = true;
      setProgress(0);
      setStatus(`RX OK · ${fileCount} file(s) from ${senderAlias} saved`);
      showToast(`${senderAlias} → ${fileCount} FILE(S) · ${formatBytes(totalSize)} · saved to ${destinationDir}`);
      blip(1760, 0.12);
      setTimeout(() => blip(2200, 0.16), 130);
    });

    await listen('session-cancelled', () => {
      progressBlock.hidden = true;
      setProgress(0);
      setStatus('Transfer cancelled.');
    });

    // Preload settings (used by transmit + settings modal)
    try {
      state.settings = await invoke('get_settings');
    } catch (err) {
      console.error('settings load:', err);
    }

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
    state.peers = wirePeers.map((p) => ({ ...p }));

    // Drop selection if the selected peer vanished.
    if (state.selectedPeerId && !state.peers.find((p) => p.id === state.selectedPeerId)) {
      state.selectedPeerId = null;
      targetName.textContent = '—';
      targetHex.textContent = '—';
      sendBtn.disabled = true;
    }

    // If nothing selected and peers exist, pick the first.
    if (!state.selectedPeerId && state.peers.length > 0) {
      state.selectedPeerId = state.peers[0].id;
      selectPeer(state.selectedPeerId);
    } else {
      renderPeers();
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
