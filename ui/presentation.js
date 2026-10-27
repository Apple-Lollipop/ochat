/* Presentation only: finite motion, keyboard access and panel dismissal. */
(() => {
  let room = null;
  let connected = false;
  const reduceMotion = () => window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
  function arrive(node, className) {
    if (!node || reduceMotion()) return;
    node.classList.remove(className);
    // Removing the class after the finite animation prevents replay on status updates.
    node.classList.add(className);
    node.addEventListener('animationend', () => node.classList.remove(className), { once:true });
  }
  window.OchatPresentation = Object.freeze({
    messageAdded(node, live) { if (live) arrive(node, 'is-new'); },
    conversationChanged(next) {
      if (next && next !== room) arrive(document.getElementById('timeline'), 'conversation-arrive');
      room = next;
    },
    reset() { room = null; document.getElementById('timeline')?.classList.remove('conversation-arrive'); },
    connect(callbacks) {
      if (connected) return;
      connected = true;
      document.addEventListener('keydown', event => {
        if (event.defaultPrevented || event.repeat || event.isComposing || !(event.ctrlKey || event.metaKey) || !callbacks.canInteract()) return;
        const key = event.key.toLowerCase();
        let action;
        if (key === 'k' && !event.shiftKey) action = callbacks.conversations;
        if (key === 'f' && !event.shiftKey) action = callbacks.search;
        if (key === ',' && !event.shiftKey) action = callbacks.settings;
        if (key === 'enter' && !event.shiftKey) action = callbacks.composer;
        if (key === 'l' && event.shiftKey) action = callbacks.lock;
        if (action) { event.preventDefault(); action(); }
      });
      document.querySelector('.skip-link')?.addEventListener('click', event => {event.preventDefault();if(callbacks.canInteract())callbacks.composer();});
      for (const id of ['friends-button','activity-button']) {
        const button = document.getElementById(id);
        if (!button) continue;
        const update = () => {
          const label = button.textContent.trim();
          button.setAttribute('aria-label', label);
          const count = label.match(/\((\d+)\)/)?.[1];
          if (count) button.dataset.count = Number(count)>99 ? '99+' : count; else delete button.dataset.count;
        };
        update();
        new MutationObserver(update).observe(button, { childList:true, characterData:true, subtree:true });
      }
      const dismissible = new Set(['navigation-dialog','details-dialog','room-actions-dialog','search-dialog','emoji-dialog','message-tools-dialog','reaction-dialog','collection-dialog','activity-dialog','voice-diagnostics-dialog']);
      document.querySelectorAll('dialog').forEach(dialog => {
        dialog.addEventListener('click', event => {
          if (!dialog.open || !dismissible.has(dialog.id) || event.target !== dialog) return;
          const rect = dialog.getBoundingClientRect();
          if (event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom) callbacks.dismiss(dialog);
        });
      });
      document.getElementById('timeline')?.addEventListener('contextmenu', event => {
        if (event.target.closest('a,input,textarea')) return;
        const row = event.target.closest('[data-message-id]');
        if (row && !row.classList.contains('deleted-message') && callbacks.canInteract()) { event.preventDefault(); callbacks.messageMenu(row,event); }
      });
      new MutationObserver(records => {
        records.forEach(record => {
          if (record.target instanceof HTMLButtonElement) record.target.setAttribute('aria-busy', String(record.target.dataset.busy === 'true'));
        });
      }).observe(document.body, { subtree:true, attributes:true, attributeFilter:['data-busy'] });
    }
  });
})();
