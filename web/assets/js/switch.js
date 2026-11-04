// Inside a form a switch keeps its hidden field (data-input) in step, so the form posts it.
export const isOn = (el) => el.getAttribute('aria-checked') === 'true';

export function setOn(el, on) {
  el.setAttribute('aria-checked', on ? 'true' : 'false');
  const field = el.dataset.input && document.getElementById(el.dataset.input);
  if (field) field.disabled = !on;
}

export function switches() {
  if (document.documentElement.dataset.switches) return;
  document.documentElement.dataset.switches = 'on';
  document.addEventListener('click', (e) => {
    const s = e.target.closest('button[role="switch"]');
    if (!s || s.disabled) return;
    setOn(s, !isOn(s));
    s.dispatchEvent(new Event('change', { bubbles: true }));
  });
}
