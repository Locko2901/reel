// The microphone's noise suppression in words, for the setup and This app's OBS page.
const COST = {
  off: 'the microphone as it comes, at no cost',
  speex: 'takes steady hiss and hum; very little CPU',
  rnnoise: 'a small neural network that also takes keyboard clicks and noise between words; more CPU than Light, still a small share of one core',
  nvidia: 'NVIDIA\'s noise removal on the graphics card (an RTX card with NVIDIA\'s audio effects installed): the strongest, at some of the card\'s time',
};

export function filterNames(filters) {
  return (filters || []).map((f) => f.name || f.filterName || f.id).join(', ');
}

export function noiseHelp(s, status) {
  if (s.noise === 'own') return `the filters of ${s.own_filters_from || 'your own OBS'}, as they are: ${filterNames(s.own_filters)}`;
  if (s.noise === 'nvidia' && status && status.nvidia_noise === false) return 'OBS here does not offer NVIDIA\'s noise removal, so Strong is used';
  return COST[s.noise] || '';
}

export function noiseOptions(select, s, status) {
  for (const o of select.options) {
    if (o.value === 'nvidia') o.hidden = !(status && status.nvidia_noise) && s.noise !== 'nvidia';
    if (o.value === 'own') o.hidden = !(s.own_filters && s.own_filters.length);
  }
  select.value = s.noise;
}

export function ownMicWords(m) {
  return `"${m.name}" in your own OBS's scene collection "${m.collection}" has ${filterNames(m.filters)}; copied as they are, read only`;
}
