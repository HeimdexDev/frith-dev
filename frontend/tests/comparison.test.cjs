// Controller tests with deterministic media events; browser checks cover actual decoders.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const source = fs.readFileSync(`${__dirname}/../static/comparison.js`, 'utf8');
function harness() {
  const nodes = new Map();
  const element = () => ({
    listeners: {}, attrs: {}, dataset: {source:'/original'}, style: {setProperty() {}},
    classList: {add(){}, remove(){}, toggle(){}}, textContent:'', hidden:true,
    checked:false, readyState:4, duration:8, videoWidth:960, videoHeight:540,
    currentTime:0, paused:true, seeking:false, ended:false, isConnected:true,
    addEventListener(name, fn) { (this.listeners[name] ??= []).push(fn); },
    emit(name, event={}) { for (const fn of this.listeners[name] ?? []) fn({target:this,...event}); },
    setAttribute(name,value) { this.attrs[name]=value; },
    getAttribute(name) { return this.attrs[name]; },
    removeAttribute(name) { delete this.attrs[name]; if(name==='src') this.src=''; },
    play() { this.paused=false; return Promise.resolve(); }, pause() { this.paused=true; },
    load(){}, focus(){}, setPointerCapture(){},
    getBoundingClientRect() { return {left:0,width:100}; },
  });
  const root = element(); root.querySelector = selector => {
    if(!nodes.has(selector)) nodes.set(selector,element());
    return nodes.get(selector);
  };
  const document = element(); document.querySelectorAll = () => [root];
  let frame, timeout;
  vm.runInNewContext(source, {document, requestAnimationFrame: fn => {frame=fn;}, setTimeout: fn=>{timeout=fn;return 1;}, clearTimeout:()=>{timeout=null;}, console});
  return {get:root.querySelector, frame:()=>frame(), timeout:()=>timeout(), document};
}
test('original is lazy, divider does not seek, and disabling releases original', () => {
  const h=harness(), raw=h.get('.original-video'), video=h.get('.processed-video'), toggle=h.get('[data-compare-toggle]');
  assert.equal(raw.src,undefined);
  toggle.checked=true; toggle.emit('change'); h.frame();
  assert.equal(raw.src,'/original'); assert.equal(raw.muted,true);
  video.currentTime=raw.currentTime=3;
  const slider=h.get('.compare-divider');
  slider.emit('keydown',{key:'ArrowRight',preventDefault(){}});
  assert.equal(slider.attrs['aria-valuenow'],'51');
  slider.emit('keydown',{key:'End',preventDefault(){}});
  assert.equal(slider.attrs['aria-valuenow'],'100'); assert.equal(video.currentTime,3);
  toggle.checked=false; toggle.emit('change');
  assert.equal(raw.src,''); assert.equal(raw.paused,true); assert.equal(slider.hidden,true);
});
test('buffering pauses both; seeks and drift align the follower before playback', async () => {
  const h=harness(), raw=h.get('.original-video'), video=h.get('.processed-video'), toggle=h.get('[data-compare-toggle]');
  toggle.checked=true; toggle.emit('change'); h.frame();
  h.get('[data-play]').emit('click'); await Promise.resolve();
  assert.equal(video.paused,false); assert.equal(raw.paused,false);
  raw.readyState=2; raw.emit('waiting');
  assert.equal(video.paused,true); assert.equal(raw.paused,true);
  assert.equal(h.get('.compare-shield').hidden,false);
  raw.readyState=4; raw.emit('canplay'); await Promise.resolve();
  const seek=h.get('[data-seek]');seek.value='4';seek.emit('input');
  assert.equal(video.currentTime,4);assert.equal(raw.currentTime,4);
  video.currentTime=5;raw.currentTime=4.5;h.frame();
  assert.equal(video.paused,true);assert.equal(raw.currentTime,5);
  raw.emit('seeked');await Promise.resolve();
  assert.equal(h.get('.compare-shield').hidden,true);
});
test('mismatch, media failure and timeout fail closed to anonymized-only paused mode', () => {
  for(const mode of ['mismatch','error','timeout']) {
    const h=harness(), raw=h.get('.original-video'), toggle=h.get('[data-compare-toggle]');
    toggle.checked=true;toggle.emit('change');
    if(mode==='mismatch') {raw.duration=9;raw.emit('loadedmetadata');}
    if(mode==='error') raw.emit('error');
    if(mode==='timeout') h.timeout();
    assert.equal(toggle.checked,false); assert.equal(raw.src,'');
    assert.equal(h.get('.processed-video').paused,true);
    assert.notEqual(h.get('.player-status').textContent,'');
    assert.equal(h.get('.compare-divider').hidden,true);
  }
});
