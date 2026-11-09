// Two independent decoders: bounded drift correction, not frame-exact evidence.
// Keep media identity, authorization and jurisdiction checks on the server.
for (const root of document.querySelectorAll('[data-comparison]')) {
  const find = selector => root.querySelector(selector);
  const processed = find('.processed-video'), original = find('.original-video');
  const stage = find('.compare-stage'), divider = find('.compare-divider');
  const toggle = find('[data-compare-toggle]'), play = find('[data-play]');
  const seek = find('[data-seek]'), status = find('.player-status');
  const shield = find('.compare-shield'), labels = find('.compare-labels');
  let wanted = false, syncing = false, starting = false, generation = 0, timeout;
  const active = () => toggle.checked ? [processed, original] : [processed];
  const pauseBoth = () => { processed.pause(); original.pause(); };
  const setMessage = message => { if (status.textContent !== message) status.textContent = message; };
  const updatePlay = () => { play.textContent = wanted ? 'Pause' : 'Play'; play.setAttribute('aria-label', play.textContent); };
  const reveal = value => {
    const percent = Math.max(0, Math.min(100, value));
    stage.style.setProperty('--split', `${percent}%`);
    divider.setAttribute('aria-valuenow', String(Math.round(percent)));
    divider.setAttribute('aria-valuetext', `${Math.round(percent)}% original`);
  };
  function fail(message) {
    generation++; starting = false; syncing = false; wanted = false;
    clearTimeout(timeout); pauseBoth(); toggle.checked = false;
    stage.classList.remove('comparing'); shield.hidden = true;
    divider.hidden = labels.hidden = true;
    original.removeAttribute('src'); original.load();
    updatePlay(); setMessage(message);
  }
  function hold() {
    if (!syncing) {
      syncing = true; generation++; starting = false;
      timeout = setTimeout(() => fail('Video unavailable or too slow. Retry Compare original, or reload the page.'), 10000);
    }
    pauseBoth();
    shield.hidden = !toggle.checked;
    setMessage('Aligning video pair…');
  }
  function compatible() {
    return Number.isFinite(processed.duration) && Number.isFinite(original.duration)
      && Math.abs(processed.duration - original.duration) <= 1 / 24
      && processed.videoWidth === original.videoWidth
      && processed.videoHeight === original.videoHeight;
  }
  function reconcile() {
    const videos = active();
    if (videos.some(v => v.readyState < 1)) return;
    if (toggle.checked && !compatible()) {
      fail('These videos do not match. Compare is unavailable; use the anonymized preview.'); return;
    }
    // Always seek the follower to the processed (master) timeline, never vice versa.
    if (toggle.checked && syncing && !processed.seeking && !original.seeking
        && Math.abs(processed.currentTime - original.currentTime) > 0.04) {
      hold(); original.currentTime = processed.currentTime; return;
    }
    if (videos.some(v => v.seeking || v.readyState < 3)) return;
    if (syncing) {
      syncing = false; clearTimeout(timeout); shield.hidden = true; setMessage('');
    }
    if (!wanted || starting || videos.every(v => !v.paused)) return;
    const ticket = generation;
    starting = true;
    Promise.all(videos.map(v => v.play())).then(() => {
      if (ticket !== generation) return;
      starting = false;
      if (!wanted) pauseBoth();
    }).catch(() => {
      if (ticket === generation) fail('Playback could not start. Press Play to retry.');
    });
  }
  processed.controls = false;
  find('.player-controls').hidden = false;
  // The original has no src until explicit comparison opt-in.
  original.muted = true;
  toggle.addEventListener('change', () => {
    generation++; starting = false; pauseBoth();
    stage.classList.toggle('comparing', toggle.checked);
    divider.hidden = labels.hidden = !toggle.checked;
    if (toggle.checked) {
      reveal(50); hold(); original.src = original.dataset.source; original.load();
    } else {
      syncing = false; clearTimeout(timeout); shield.hidden = true;
      original.removeAttribute('src'); original.load(); setMessage(''); reconcile();
    }
  });
  play.addEventListener('click', () => {
    wanted = !wanted; updatePlay();
    if (!wanted) { generation++; starting = false; pauseBoth(); return; }
    if (processed.ended) { hold(); processed.currentTime = 0; }
    setMessage(''); reconcile();
    if (active().some(v => v.readyState < 3)) hold();
  });
  seek.addEventListener('input', () => {
    if (!Number.isFinite(processed.duration)) return;
    hold(); processed.currentTime = Math.min(Number(seek.value), processed.duration);
    if (toggle.checked && original.readyState >= 1) original.currentTime = processed.currentTime;
    reconcile();
  });
  find('[data-speed]').addEventListener('change', event => {
    processed.playbackRate = original.playbackRate = Number(event.target.value);
  });
  find('[data-volume]').addEventListener('input', event => { processed.volume = Number(event.target.value); });
  find('[data-fullscreen]').addEventListener('click', async () => {
    try {
      if (document.fullscreenElement === root) await document.exitFullscreen();
      else if (root.requestFullscreen) await root.requestFullscreen();
      else setMessage('Fullscreen comparison is not supported in this browser.');
    } catch { setMessage('Fullscreen was unavailable. You can continue reviewing here.'); }
  });
  for (const video of [processed, original]) {
    for (const event of ['loadedmetadata', 'canplay', 'seeked']) video.addEventListener(event, reconcile);
    video.addEventListener('waiting', () => { if (active().includes(video)) hold(); });
    video.addEventListener('seeking', () => { if (active().includes(video)) hold(); });
    video.addEventListener('error', () => {
      if (active().includes(video)) fail('Could not load video. Compare is off. Retry or reload the page.');
    });
  }
  processed.addEventListener('ended', () => { wanted = false; pauseBoth(); updatePlay(); });
  // Leaving the tab pauses review rather than allowing hidden-tab decoder drift.
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) { wanted = false; generation++; starting = false; pauseBoth(); updatePlay(); }
  });
  let pointer = null;
  const drag = event => {
    const bounds = stage.getBoundingClientRect();
    reveal((event.clientX - bounds.left) / bounds.width * 100);
  };
  divider.addEventListener('pointerdown', event => {
    if (event.button !== 0) return;
    pointer = event.pointerId; divider.setPointerCapture(pointer); divider.focus(); drag(event);
  });
  divider.addEventListener('pointermove', event => { if (event.pointerId === pointer) drag(event); });
  for (const event of ['pointerup', 'pointercancel', 'lostpointercapture']) {
    divider.addEventListener(event, () => { pointer = null; });
  }
  divider.addEventListener('keydown', event => {
    const current = Number(divider.getAttribute('aria-valuenow'));
    const step = event.shiftKey ? 10 : 1;
    const values = { ArrowLeft: current - step, ArrowRight: current + step, Home: 0, End: 100 };
    if (event.key in values) { event.preventDefault(); reveal(values[event.key]); }
  });
  const clock = value => `${Math.floor(value / 60)}:${String(Math.floor(value % 60)).padStart(2, '0')}`;
  function tick() {
    if (!root.isConnected) return;
    if (Number.isFinite(processed.duration)) {
      seek.max = processed.duration;
      if (document.activeElement !== seek) seek.value = processed.currentTime;
      find('[data-time]').textContent = `${clock(processed.currentTime)} / ${clock(processed.duration)}`;
    }
    // Pause both and realign if playback drifts by more than ~2 frames at 24 fps.
    if (toggle.checked && !syncing && Math.abs(processed.currentTime - original.currentTime) > 0.08) hold();
    reconcile(); requestAnimationFrame(tick);
  }
  tick();
}
