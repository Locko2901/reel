import { $ } from './lib.js';
import { size } from './format.js';
import { send } from './upload.js';

function askWorker() {
  return new Promise((resolve) => {
    const sw = navigator.serviceWorker;
    if (!sw?.controller) { resolve([]); return; }
    const done = (files) => { clearTimeout(timer); resolve(files); };
    const timer = setTimeout(() => done([]), 2000);
    sw.addEventListener('message', (e) => done(e.data?.files || []), { once: true });
    sw.controller.postMessage('shared-files');
  });
}

export function pwa() {
  const sw = navigator.serviceWorker;
  if (!sw || window.__TAURI__) return;
  const v = new URL($('script[src*="reel.js"]').src).pathname.split('/')[2] || '';
  sw.register(`/assets/sw.js?v=${v}`, { scope: '/' }).catch(() => {});
}

export async function shareTarget() {
  const list = $('#uploads');
  if (!list || document.body.dataset.page !== 'share-target') return;
  const limit = +list.dataset.limit, gb = list.dataset.limitGb;
  const files = await askWorker();
  const pick = $('#st-pick');
  const refused = () => { $('#st-why').textContent = 'Another one, under the limit?'; pick.hidden = false; };
  if ($('.notice.err', list)) refused();
  const made = [];
  let left = 0;
  const take = (file) => {
    if (file.size > limit) {
      const row = document.createElement('div');
      row.className = 'notice err';
      row.innerHTML = '<span class="grow"></span>';
      row.firstChild.textContent = `${file.name} is ${size(file.size)}: an upload over ${gb} GB is refused; the limit is under Settings, Imports`;
      list.prepend(row);
      refused();
      return;
    }
    left++;
    $('#st-line').textContent = `${file.name}, ${size(file.size)}`;
    send(file, list, (clip) => {
      made.push(clip);
      if (--left === 0 && !$('.notice.err', list)) location.href = made.length === 1 ? `/clips/${made[0].id}` : '/';
    });
  };
  $('#st-file').onchange = (e) => { [...e.target.files].forEach(take); e.target.value = ''; };
  if (files.length) files.forEach(take);
  else pick.hidden = false;
}
