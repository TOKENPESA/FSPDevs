import { icon } from "./icons.js";

/** @param {string} selector @param {import("./icons.js").IconName | string} name @param {number} [size] */
function mount(selector, name, size = 18) {
  document.querySelectorAll(selector).forEach((el) => {
    el.innerHTML = icon(name, size);
  });
}

mount(".ui-icon-menu", "menu", 18);
mount(".ui-icon-bell", "bell", 18);
mount(".ui-icon-chat", "chat", 20);
mount(".search-icon", "search", 16);
