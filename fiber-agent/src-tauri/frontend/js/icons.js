const SVG_ATTRS =
  'xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"';

const PATHS = {
  dashboard: '<path d="M3 10.5 12 3l9 7.5V21a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1v-10.5Z"/>',
  modules:
    '<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>',
  appStore:
    '<path d="M3 9.5 12 3l9 6.5V20a1 1 0 0 1-1 1h-5v-7H9v7H4a1 1 0 0 1-1-1V9.5Z"/><path d="M9 14h6"/>',
  dicoba:
    '<path d="M3 21h18M5 21V7l7-4 7 4v14M9 21v-6h6v6M9 9h.01M15 9h.01M9 13h.01M15 13h.01"/>',
  mobile:
    '<rect x="7" y="2.5" width="10" height="19" rx="2"/><path d="M11 18h2"/>',
  savings:
    '<path d="M19 7c0 2.2-1.8 4-4 4H7.5a2.5 2.5 0 1 0 0 5H13a4 4 0 0 1 0 8H6"/><path d="M16 7V5a3 3 0 0 0-6 0v2"/>',
  loans:
    '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8l-6-6Z"/><path d="M14 2v6h6M8 13h8M8 17h5"/>',
  float:
    '<path d="M2 12h2l3-9 4 18 3-9h2"/><path d="M16 8h6M19 5v6"/>',
  fees:
    '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8l-6-6Z"/><path d="M14 2v6h6M8 11h8M8 15h5"/>',
  menu: '<path d="M4 7h16M4 12h16M4 17h16"/>',
  bell:
    '<path d="M18 8a6 6 0 1 0-12 0c0 7-3 8-3 8h18s-3-1-3-8"/><path d="M13.7 21a2 2 0 0 1-3.4 0"/>',
  funding:
    '<path d="M12 3v18"/><path d="M7 8h10"/><path d="M5 12h14"/><path d="M7 16h10"/><circle cx="12" cy="12" r="9"/>',
  chat:
    '<path d="M21 11.5a8.4 8.4 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.4 8.4 0 0 1-3.8-.9L3 21l1.9-5.7a8.4 8.4 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.4 8.4 0 0 1 3.8-.9h.5a8.5 8.5 0 0 1 8 8v.5Z"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
  chevronDown: '<path d="m6 9 6 6 6-6"/>',
  chevronRight: '<path d="m9 6 6 6-6 6"/>',
};

/** @typedef {keyof typeof PATHS} IconName */

/** @param {IconName | string} name @param {number} [size] @returns {string} */
export function icon(name, size = 18) {
  const body = PATHS[/** @type {IconName} */ (name)] ?? PATHS.modules;
  return `<svg ${SVG_ATTRS} width="${size}" height="${size}">${body}</svg>`;
}

/** @param {IconName | string} name @param {number} [size] @returns {string} */
export function navIcon(name, size = 16) {
  return `<span class="ui-icon">${icon(name, size)}</span>`;
}
