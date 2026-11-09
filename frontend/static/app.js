// Progressive enhancement only. Dataset state and authorization live in the Rust adapter.
document.querySelectorAll('[data-open]').forEach(button => button.addEventListener('click', () => document.getElementById(button.dataset.open).showModal()));
document.querySelectorAll('[data-close]').forEach(button => button.addEventListener('click', () => document.getElementById(button.dataset.close).close()));
document.getElementById('go-back')?.addEventListener('click', () => history.back());
const picker = document.getElementById('videos');
function choose(names) {
  document.getElementById('files').value = names.join('\n');
  document.getElementById('file-list').textContent = names.join(' · ');
  document.getElementById('create-button').disabled = names.length === 0 || names.length > 50;
}
picker?.addEventListener('change', () => choose(Array.from(picker.files, file => file.name)));
document.getElementById('use-sample')?.addEventListener('click', () => { picker.value = ''; choose(['warehouse_01.mp4', 'warehouse_02.mp4']); });
