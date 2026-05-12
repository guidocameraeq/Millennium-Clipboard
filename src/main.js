// Millennium Clipboard // GRID — mockup behavior
// No network code. Animations, interactions, and mock data only.

(() => {
  'use strict';

  // ---------- Refs ----------------------------------------------------------
  const textarea = document.getElementById('text-composer');
  const charCount = document.getElementById('char-count');
  const peerList = document.getElementById('peer-list');
  const targetName = document.getElementById('target-name');
  const targetHex = document.getElementById('target-hex');
  const statusMsg = document.getElementById('status-msg');
  const peerCount = document.getElementById('peer-count');
  const filterHint = document.getElementById('filter-hint');
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
  const hudUptime = document.getElementById('hud-uptime');

  // ---------- Build the segmented progress bar -----------------------------
  const SEGMENTS = 28;
  for (let i = 0; i < SEGMENTS; i++) {
    const s = document.createElement('div');
    s.className = 'seg';
    progressSegments.appendChild(s);
  }
  const segs = [...progressSegments.children];

  // ---------- Uptime ticker -------------------------------------------------
  const t0 = Date.now();
  function tickUptime() {
    const s = Math.floor((Date.now() - t0) / 1000);
    const hh = String(Math.floor(s / 3600)).padStart(2, '0');
    const mm = String(Math.floor((s % 3600) / 60)).padStart(2, '0');
    const ss = String(s % 60).padStart(2, '0');
    hudUptime.textContent = `${hh}:${mm}:${ss}`;
  }
  setInterval(tickUptime, 1000);
  tickUptime();

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
    if (!textarea) return;
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

  textarea.addEventListener('focus', () => {
    if (!textarea.value) stopPh();
  });

  textarea.addEventListener('blur', () => {
    if (!textarea.value) {
      phIdx = 0;
      typePh(placeholderLines[0]);
    }
  });

  // ---------- Character counter (0000 format) ------------------------------
  function updateCharCount() {
    const n = textarea.value.length;
    charCount.textContent = String(n).padStart(4, '0');
  }
  textarea.addEventListener('input', updateCharCount);

  // ---------- Click-clack synth (only synth — retro audio cue) ------------
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
    const ac = ctx();
    if (!ac) return;
    const now = ac.currentTime;
    const buf = ac.createBuffer(1, 0.04 * ac.sampleRate, ac.sampleRate);
    const d = buf.getChannelData(0);
    for (let i = 0; i < d.length; i++) {
      const env = Math.pow(1 - i / d.length, 3);
      d[i] = (Math.random() * 2 - 1) * env * 0.55;
    }
    const src = ac.createBufferSource();
    src.buffer = buf;
    const flt = ac.createBiquadFilter();
    flt.type = 'bandpass';
    flt.frequency.value = 2400;
    flt.Q.value = 2.5;
    const g = ac.createGain();
    g.gain.value = 0.5;
    src.connect(flt); flt.connect(g); g.connect(ac.destination);
    src.start(now);
  }

  function blip(freq = 880, dur = 0.08) {
    if (!soundToggle.checked) return;
    const ac = ctx();
    if (!ac) return;
    const now = ac.currentTime;
    const osc = ac.createOscillator();
    osc.type = 'square';
    osc.frequency.value = freq;
    const g = ac.createGain();
    g.gain.setValueAtTime(0.001, now);
    g.gain.exponentialRampToValueAtTime(0.15, now + 0.005);
    g.gain.exponentialRampToValueAtTime(0.001, now + dur);
    osc.connect(g); g.connect(ac.destination);
    osc.start(now);
    osc.stop(now + dur);
  }

  textarea.addEventListener('keydown', (e) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key.length === 1 || e.key === 'Backspace' || e.key === 'Enter') {
      clack();
    }
  });

  // ---------- Peer list interactions ---------------------------------------
  function selectPeer(item) {
    document.querySelectorAll('.peer-item').forEach((el) => el.classList.remove('selected'));
    item.classList.add('selected');
    targetName.textContent = item.dataset.name;
    targetHex.textContent = item.dataset.hex;
    setStatus(`PEER LOCKED · ${item.dataset.name}`);
    blip(660, 0.06);
  }

  peerList.addEventListener('click', (e) => {
    const fav = e.target.closest('.fav-btn');
    if (fav) {
      e.stopPropagation();
      const isFav = fav.dataset.favorite === 'true';
      const next = !isFav;
      fav.dataset.favorite = next ? 'true' : 'false';
      fav.textContent = next ? '★' : '☆';
      const item = fav.closest('.peer-item');
      if (item) item.dataset.favorite = next ? 'true' : 'false';
      applyFilter();
      blip(next ? 1320 : 440, 0.05);
      return;
    }
    const item = e.target.closest('.peer-item');
    if (item) selectPeer(item);
  });

  // ---------- Filter buttons (ALL / FAVORITES) -----------------------------
  function applyFilter() {
    const active = document.querySelector('.filter-btn.active');
    const mode = active ? active.dataset.filter : 'all';
    let visible = 0;
    document.querySelectorAll('.peer-item').forEach((item) => {
      const isFav = item.dataset.favorite === 'true';
      const show = mode === 'all' || isFav;
      item.style.display = show ? '' : 'none';
      if (show) visible++;
    });
    filterHint.textContent = `${String(visible).padStart(2, '0')} visible`;
    const total = document.querySelectorAll('.peer-item').length;
    peerCount.textContent = String(total).padStart(2, '0');
  }

  document.querySelectorAll('.filter-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      document.querySelectorAll('.filter-btn').forEach((b) => b.classList.remove('active'));
      btn.classList.add('active');
      applyFilter();
      blip(880, 0.05);
    });
  });

  // ---------- Mode switch (TEXT / FILE) ------------------------------------
  document.querySelectorAll('.mode-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const mode = btn.dataset.mode;
      document.querySelectorAll('.mode-btn').forEach((b) => b.classList.remove('active'));
      btn.classList.add('active');
      document.querySelectorAll('.mode-panel').forEach((p) => {
        const active = p.id === `mode-${mode}`;
        p.classList.toggle('active', active);
        p.hidden = !active;
      });
      blip(550, 0.04);
    });
  });

  // ---------- File dropzone (visual only) ----------------------------------
  function mockAddFile(name, size) {
    fileQueue.hidden = false;
    const li = document.createElement('li');
    li.textContent = `▸ ${name}  //  ${size}`;
    fileQueue.appendChild(li);
  }

  dropzone.addEventListener('click', () => {
    mockAddFile('proyecto-borrador.docx', '142 KB');
    blip(1100, 0.05);
  });

  dropzone.addEventListener('dragover', (e) => {
    e.preventDefault();
    dropzone.style.background = 'rgba(0, 240, 255, 0.08)';
  });
  dropzone.addEventListener('dragleave', () => { dropzone.style.background = ''; });
  dropzone.addEventListener('drop', (e) => {
    e.preventDefault();
    dropzone.style.background = '';
    [...e.dataTransfer.files].forEach((f) => {
      const kb = Math.max(1, Math.round(f.size / 1024));
      mockAddFile(f.name.toUpperCase(), `${kb} KB`);
    });
    blip(1320, 0.05);
  });

  // ---------- Send / transmit ----------------------------------------------
  function setStatus(msg) { statusMsg.textContent = msg; }

  function showToast(text) {
    toastText.textContent = text;
    toast.hidden = false;
    setTimeout(() => (toast.hidden = true), 3800);
  }

  function setProgress(pct) {
    const filled = Math.round((pct / 100) * SEGMENTS);
    segs.forEach((s, i) => s.classList.toggle('on', i < filled));
    progressPct.textContent = `${pct}%`;
  }

  function simulateTransmit() {
    const activeMode = document.querySelector('.mode-btn.active').dataset.mode;
    let payload;

    if (activeMode === 'text') {
      if (!textarea.value.trim()) {
        setStatus('ERR · empty payload. Type something first.');
        blip(220, 0.12);
        return;
      }
      payload = `${textarea.value.length} CHARS`;
    } else {
      if (fileQueue.children.length === 0) {
        setStatus('ERR · queue empty. Drop a file first.');
        blip(220, 0.12);
        return;
      }
      payload = `${fileQueue.children.length} FILE(S)`;
    }

    sendBtn.disabled = true;
    progressBlock.hidden = false;
    setProgress(0);
    progressText.textContent = `TRANSMITTING // ${targetName.textContent}`;
    setStatus(`TX → ${targetName.textContent}...`);
    blip(880, 0.05);

    let pct = 0;
    const tick = () => {
      pct = Math.min(100, pct + Math.floor(4 + Math.random() * 12));
      setProgress(pct);
      if (soundToggle.checked && pct < 100) blip(660 + pct * 6, 0.02);
      if (pct < 100) {
        setTimeout(tick, 90 + Math.random() * 110);
      } else {
        progressText.textContent = 'COMPLETE';
        blip(1760, 0.12);
        setTimeout(() => blip(2200, 0.16), 130);
        setTimeout(() => {
          progressBlock.hidden = true;
          sendBtn.disabled = false;
          setProgress(0);
          setStatus(`OK · delivered to ${targetName.textContent}.`);
          showToast(`${targetName.textContent} · ${payload} · ACK`);
          if (activeMode === 'text') {
            textarea.value = '';
            updateCharCount();
          } else {
            fileQueue.innerHTML = '';
            fileQueue.hidden = true;
          }
        }, 700);
      }
    };
    tick();
  }

  sendBtn.addEventListener('click', simulateTransmit);

  textarea.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      simulateTransmit();
    }
  });

  // ---------- HUD action buttons -------------------------------------------
  document.querySelectorAll('.hud-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const action = btn.dataset.action;
      blip(880, 0.06);
      if (action === 'refresh') {
        setStatus('SCAN · probing 192.168.1.0/24...');
        document.querySelectorAll('.peer-item').forEach((item, i) => {
          item.style.opacity = '0';
          item.style.transform = 'translateY(-6px)';
          setTimeout(() => {
            item.style.transition = 'all 0.25s ease-out';
            item.style.opacity = '';
            item.style.transform = '';
          }, 200 + i * 100);
        });
        setTimeout(() => setStatus('OK · 4 peers locked on grid.'), 900);
      } else if (action === 'history') {
        setStatus('LOG · 0 records (TODO)');
      } else if (action === 'settings') {
        setStatus('CONF · panel TBD');
      }
    });
  });

  // ---------- Init pulse ----------------------------------------------------
  setTimeout(() => setStatus('GRID ONLINE · 4 peers detected.'), 500);
  setTimeout(() => setStatus('SYS READY · awaiting input'), 2400);
  updateCharCount();
})();
