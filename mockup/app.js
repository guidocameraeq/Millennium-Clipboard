// Millennium Clipboard — mockup behavior
// No network code here. Only animations, interactions, mock data.

(() => {
  'use strict';

  // ---------- Typewriter placeholder rotator -------------------------------
  const placeholderLines = [
    'Type or paste here...',
    'Or drag a file onto the next tab.',
    'Press Ctrl+Enter to send.',
    'Your text travels by typewriter ribbon.',
    'No cloud. No account. Just the LAN.',
  ];

  const textarea = document.getElementById('text-composer');
  let placeholderTimer = null;
  let placeholderLineIdx = 0;

  function typewritePlaceholder(line, idx = 0) {
    if (!textarea) return;
    textarea.placeholder = line.slice(0, idx);
    if (idx <= line.length) {
      placeholderTimer = setTimeout(
        () => typewritePlaceholder(line, idx + 1),
        45 + Math.random() * 50,
      );
    } else {
      placeholderTimer = setTimeout(() => {
        placeholderLineIdx = (placeholderLineIdx + 1) % placeholderLines.length;
        typewritePlaceholder(placeholderLines[placeholderLineIdx]);
      }, 2200);
    }
  }

  function stopPlaceholderTypewriter() {
    if (placeholderTimer) {
      clearTimeout(placeholderTimer);
      placeholderTimer = null;
    }
    textarea.placeholder = '';
  }

  typewritePlaceholder(placeholderLines[0]);

  textarea.addEventListener('focus', () => {
    if (!textarea.value) {
      stopPlaceholderTypewriter();
    }
  });

  textarea.addEventListener('blur', () => {
    if (!textarea.value) {
      placeholderLineIdx = 0;
      typewritePlaceholder(placeholderLines[0]);
    }
  });

  // ---------- Character counter --------------------------------------------
  const charCount = document.getElementById('char-count');
  textarea.addEventListener('input', () => {
    charCount.textContent = textarea.value.length;
  });

  // ---------- Click-clack typewriter sound ---------------------------------
  // Synthesized with WebAudio — no asset files needed.
  const soundToggle = document.getElementById('sound-toggle');
  let audioCtx = null;

  function ensureAudioCtx() {
    if (!audioCtx) {
      const Ctor = window.AudioContext || window.webkitAudioContext;
      if (Ctor) audioCtx = new Ctor();
    }
    return audioCtx;
  }

  function clack() {
    if (!soundToggle.checked) return;
    const ctx = ensureAudioCtx();
    if (!ctx) return;
    const now = ctx.currentTime;

    // Short impulse + noise burst for a mechanical "tic"
    const bufferSize = 0.04 * ctx.sampleRate;
    const buffer = ctx.createBuffer(1, bufferSize, ctx.sampleRate);
    const data = buffer.getChannelData(0);
    for (let i = 0; i < bufferSize; i++) {
      const env = Math.pow(1 - i / bufferSize, 3);
      data[i] = (Math.random() * 2 - 1) * env * 0.6;
    }
    const src = ctx.createBufferSource();
    src.buffer = buffer;

    const filter = ctx.createBiquadFilter();
    filter.type = 'bandpass';
    filter.frequency.value = 2200;
    filter.Q.value = 2;

    const gain = ctx.createGain();
    gain.gain.value = 0.5;

    src.connect(filter);
    filter.connect(gain);
    gain.connect(ctx.destination);
    src.start(now);
  }

  textarea.addEventListener('keydown', (e) => {
    // Avoid spam on modifiers
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key.length === 1 || e.key === 'Backspace' || e.key === 'Enter') {
      clack();
    }
  });

  // ---------- Device list interactions -------------------------------------
  const deviceList = document.getElementById('device-list');
  const targetName = document.getElementById('target-name');
  const statusMsg = document.getElementById('status-msg');

  deviceList.addEventListener('click', (e) => {
    const star = e.target.closest('.favorite-star');
    if (star) {
      e.stopPropagation();
      star.classList.toggle('empty');
      const wasFav = !star.classList.contains('empty');
      star.textContent = wasFav ? '★' : '☆';
      const item = star.closest('.device-item');
      if (item) item.dataset.favorite = wasFav ? 'true' : 'false';
      applyFilter();
      return;
    }
    const item = e.target.closest('.device-item');
    if (!item) return;
    document.querySelectorAll('.device-item').forEach((el) => el.classList.remove('selected'));
    item.classList.add('selected');
    const name = item.dataset.name;
    targetName.textContent = name;
    setStatus(`Selected: ${name}`);
  });

  // ---------- Filter All / Favorites ---------------------------------------
  const filterCounter = document.getElementById('device-counter');

  function applyFilter() {
    const mode = document.querySelector('input[name="device-filter"]:checked').value;
    let visible = 0;
    document.querySelectorAll('.device-item').forEach((item) => {
      const isFav = item.dataset.favorite === 'true';
      const show = mode === 'all' || isFav;
      item.style.display = show ? '' : 'none';
      if (show) {
        item.classList.remove('pop');
        // restart animation
        void item.offsetWidth;
        item.classList.add('pop');
        visible++;
      }
    });
    filterCounter.textContent = `${visible} shown`;
  }

  document.querySelectorAll('input[name="device-filter"]').forEach((r) =>
    r.addEventListener('change', applyFilter),
  );

  // ---------- Tab switching -------------------------------------------------
  document.querySelectorAll('[role="tab"]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const target = btn.dataset.tab;
      document.querySelectorAll('[role="tab"]').forEach((b) => {
        const active = b.dataset.tab === target;
        b.setAttribute('aria-selected', active ? 'true' : 'false');
      });
      document.querySelectorAll('.tab-panel').forEach((panel) => {
        const active = panel.id === `tab-${target}`;
        panel.classList.toggle('active', active);
        panel.hidden = !active;
      });
    });
  });

  // ---------- File dropzone (visual only) -----------------------------------
  const dropzone = document.getElementById('dropzone');
  const fileQueue = document.getElementById('file-queue');

  function mockAddFile(name, size) {
    fileQueue.hidden = false;
    const li = document.createElement('li');
    li.textContent = `📄 ${name}  —  ${size}`;
    fileQueue.appendChild(li);
  }

  dropzone.addEventListener('click', () => {
    mockAddFile('proyecto-borrador.docx', '142 KB');
  });

  dropzone.addEventListener('dragover', (e) => {
    e.preventDefault();
    dropzone.style.background = '#dbd0b6';
  });

  dropzone.addEventListener('dragleave', () => {
    dropzone.style.background = '';
  });

  dropzone.addEventListener('drop', (e) => {
    e.preventDefault();
    dropzone.style.background = '';
    [...e.dataTransfer.files].forEach((f) => {
      const kb = Math.max(1, Math.round(f.size / 1024));
      mockAddFile(f.name, `${kb} KB`);
    });
  });

  // ---------- Send simulation ----------------------------------------------
  const sendBtn = document.getElementById('send-btn');
  const progressWrap = document.getElementById('progress-wrap');
  const progressFill = document.getElementById('progress-fill');
  const progressText = document.getElementById('progress-text');
  const toast = document.getElementById('toast');
  const toastText = document.getElementById('toast-text');

  function setStatus(msg) {
    statusMsg.textContent = msg;
  }

  function showToast(text) {
    toastText.textContent = text;
    toast.hidden = false;
    setTimeout(() => (toast.hidden = true), 3500);
  }

  function simulateSend() {
    const activeTab = document.querySelector('[role="tab"][aria-selected="true"]').dataset.tab;
    const payload =
      activeTab === 'text'
        ? textarea.value.trim() || 'Empty message'
        : fileQueue.children.length
        ? `${fileQueue.children.length} file(s)`
        : 'No file selected';

    if (activeTab === 'text' && !textarea.value.trim()) {
      setStatus('Nothing to send. Type something first.');
      return;
    }
    if (activeTab === 'file' && fileQueue.children.length === 0) {
      setStatus('No files queued. Drop a file first.');
      return;
    }

    sendBtn.disabled = true;
    progressWrap.hidden = false;
    progressFill.style.width = '0%';
    progressText.textContent = 'Sending… 0%';
    setStatus(`Sending to ${targetName.textContent}…`);

    let pct = 0;
    const tick = () => {
      pct = Math.min(100, pct + Math.floor(8 + Math.random() * 18));
      progressFill.style.width = `${pct}%`;
      progressText.textContent = `Sending… ${pct}%`;
      if (pct < 100) {
        setTimeout(tick, 180 + Math.random() * 140);
      } else {
        progressText.textContent = 'Done.';
        setTimeout(() => {
          progressWrap.hidden = true;
          sendBtn.disabled = false;
          setStatus(`Sent to ${targetName.textContent}. Ready.`);
          showToast(`Delivered to ${targetName.textContent}: ${payload}`);
          if (activeTab === 'text') {
            textarea.value = '';
            charCount.textContent = '0';
          } else {
            fileQueue.innerHTML = '';
            fileQueue.hidden = true;
          }
        }, 500);
      }
    };
    tick();
  }

  sendBtn.addEventListener('click', simulateSend);

  // Ctrl+Enter shortcut
  textarea.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      simulateSend();
    }
  });

  // ---------- Initial status pulse -----------------------------------------
  let scanned = false;
  setTimeout(() => {
    if (!scanned) {
      setStatus('Scanning network… 4 devices found.');
      scanned = true;
      setTimeout(() => setStatus('Ready.'), 1800);
    }
  }, 600);
})();
