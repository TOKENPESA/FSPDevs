export const eventsPanel = {
  id: "mfa-events",
  title: "Event Log",
  navLabel: "Live feed",
  navIcon: "events",
  badge: "monitor",
  navDescription: "Heartbeat, heal, payment, and liquidity events from the MFA monitor stream.",
  render() {
    return `
      <div class="workspace-card">
        <div class="workspace-card-head">
          <h2>Supervisor event log</h2>
          <p class="panel-hint">Mirrors monitor WebSocket traffic and local actions</p>
        </div>
        <ul id="event-log-visible" class="event-log-panel" data-event-log-visible></ul>
      </div>`;
  },
  /**
   * @param {HTMLElement} root
   */
  mount(root) {
    const visible = root.querySelector("[data-event-log-visible]");
    const bridge = document.getElementById("event-log");
    if (!visible || !bridge) return;

    const mirror = () => {
      visible.innerHTML = bridge.innerHTML;
    };

    const observer = new MutationObserver(mirror);
    observer.observe(bridge, { childList: true, subtree: true });
    mirror();
  },
};
