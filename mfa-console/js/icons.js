const SVG_ATTRS =
  'xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"';

/** @type {Record<string, string>} */
const PATHS = {
  dashboard: '<path d="M3 10.5 12 3l9 7.5V21a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1v-10.5Z"/>',
  modules:
    '<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>',
  appStore:
    '<path d="M3 9.5 12 3l9 6.5V20a1 1 0 0 1-1 1h-5v-7H9v7H4a1 1 0 0 1-1-1V9.5Z"/><path d="M9 14h6"/>',
  mesh:
    '<circle cx="5" cy="5" r="2"/><circle cx="19" cy="5" r="2"/><circle cx="5" cy="19" r="2"/><circle cx="19" cy="19" r="2"/><circle cx="12" cy="12" r="2.5"/><path d="M7 5h10M5 7v10M19 7v10M7 19h10M7.5 6.5 10.5 10M16.5 6.5 13.5 10M7.5 17.5 10.5 14M16.5 17.5 13.5 14"/>',
  routing:
    '<path d="M4 6h6M14 6h6M4 18h6M14 18h6"/><circle cx="10" cy="6" r="2"/><circle cx="14" cy="18" r="2"/><path d="M10 8v4a2 2 0 0 0 2 2h0a2 2 0 0 0 2-2v-2"/>',
  liquidity:
    '<path d="M12 2v20M17 5H9.5a3.5 3.5 0 0 0 0 7H14a3.5 3.5 0 0 1 0 7H6"/>',
  events:
    '<path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01"/>',
  compliance:
    '<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Z"/><path d="m9 12 2 2 4-4"/>',
  clearing:
    '<rect x="2" y="5" width="20" height="14" rx="2"/><path d="M2 10h20M7 15h.01M11 15h2"/>',
  menu: '<path d="M4 7h16M4 12h16M4 17h16"/>',
  bell:
    '<path d="M18 8a6 6 0 1 0-12 0c0 7-3 8-3 8h18s-3-1-3-8"/><path d="M13.7 21a2 2 0 0 1-3.4 0"/>',
  chat:
    '<path d="M21 11.5a8.4 8.4 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.4 8.4 0 0 1-3.8-.9L3 21l1.9-5.7a8.4 8.4 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.4 8.4 0 0 1 3.8-.9h.5a8.5 8.5 0 0 1 8 8v.5Z"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
  chevronDown: '<path d="m6 9 6 6 6-6"/>',
  chevronRight: '<path d="m9 6 6 6-6 6"/>',
};

/**
 * @param {string} name
 * @param {number} [size]
 */
export function icon(name, size = 18) {
  const body = PATHS[name] ?? PATHS.modules;
  return `<svg ${SVG_ATTRS} width="${size}" height="${size}">${body}</svg>`;
}

/**
 * @param {string} name
 * @param {number} [size]
 */
export function navIcon(name, size = 16) {
  return `<span class="ui-icon">${icon(name, size)}</span>`;
}
