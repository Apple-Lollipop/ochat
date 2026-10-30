/* Metadata checks only; release downloads remain in the user's browser. */
(() => {
  const RELEASES = 'https://github.com/Artist6689/ochat/releases/latest';
  let api, checked = false, pending = null, epoch = 0;
  let status, checkButton, automatic, releaseButton, notice, noticeText;
  const node = (tag, text, className) => {
    const element = document.createElement(tag);
    if (text) element.textContent = text;
    if (className) element.className = className;
    return element;
  };
  const valid = (generation, requestEpoch) => {
    const context = api.context();
    return context.unlocked && context.generation === generation && epoch === requestEpoch;
  };
  async function openRelease() {
    if (!api.context().unlocked) return;
    try { await api.openExternalLink(RELEASES); notice.hidden = true; }
    catch (error) { api.toast(error); }
  }
  async function check(manual = false) {
    const context = api.context();
    if (!context.unlocked || pending || (!manual && (checked || context.settings?.automatic_update_checks === false))) return;
    checked = true;
    const generation = context.generation, requestEpoch = epoch;
    checkButton.disabled = true;
    status.textContent = 'Checking for updates…';
    const request = Promise.resolve().then(() => api.invoke('check_for_updates'));
    pending = request;
    try {
      const result = await request;
      if (!valid(generation, requestEpoch)) return;
      releaseButton.hidden = !result.available;
      if (result.available) {
        status.textContent = `Ochat ${result.latest} is available. You have ${result.current}.`;
        noticeText.textContent = status.textContent;
        notice.hidden = false;
      } else {
        status.textContent = result.latest ? `Ochat ${result.current} is up to date.` : 'No published release is available yet.';
        notice.hidden = true;
      }
    } catch (_) {
      if (valid(generation, requestEpoch)) status.textContent = manual
        ? 'Could not check for updates. Check your connection and try again later.'
        : 'Update check unavailable. You can retry here.';
    } finally {
      if (pending === request) pending = null;
      if (valid(generation, requestEpoch)) checkButton.disabled = false;
    }
  }
  function init(callbacks) {
    api = callbacks;
    const section = node('section', '', 'update-settings');
    section.id = 'update-settings';
    const version = node('p', 'Ochat', 'field-hint');
    version.id = 'installed-version';
    status = node('p', '', 'field-hint'); status.id = 'update-status'; status.setAttribute('role', 'status');
    automatic = node('input'); automatic.type = 'checkbox'; automatic.id = 'automatic-update-checks';
    const label = node('label', '', 'checkbox-row');
    label.append(automatic, node('span', 'Check for updates after unlocking'));
    checkButton = node('button', 'Check for updates', 'secondary'); checkButton.type = 'button'; checkButton.id = 'check-for-updates';
    checkButton.onclick = () => check(true);
    releaseButton = node('button', 'View latest release', 'text-button'); releaseButton.type = 'button'; releaseButton.hidden = true; releaseButton.onclick = openRelease;
    section.append(node('h3', 'App updates'), version, label, node('p', 'Checks GitHub for a new stable version. Uses Ochat’s Tor connection when enabled. Downloads and installation are manual.', 'field-hint'), checkButton, releaseButton, status);
    document.querySelector('[data-settings-panel="account"]').append(section);
    notice = node('aside', '', 'update-notice'); notice.id = 'update-notice'; notice.hidden = true;
    notice.setAttribute('aria-label', 'App update'); notice.setAttribute('aria-live', 'polite');
    noticeText = node('p');
    const later = node('button', 'Not now', 'secondary'); later.type = 'button'; later.onclick = () => { notice.hidden = true; };
    const open = node('button', 'View release', 'primary'); open.type = 'button'; open.onclick = openRelease;
    const actions = node('div', '', 'update-actions'); actions.append(later, open);
    notice.append(node('strong', 'Update available'), noticeText, actions); document.body.append(notice);
    api.invoke('app_version').then(value => { version.textContent = `Ochat ${value}`; }).catch(() => {});
  }
  window.OchatUpdates = {
    init,
    accountOpened: () => check(),
    fill: settings => { automatic.checked = settings.automatic_update_checks !== false; },
    read: () => ({ automatic_update_checks: automatic.checked }),
    reset: () => {
      epoch++; pending = null;
      notice.hidden = true; noticeText.textContent = ''; status.textContent = '';
      releaseButton.hidden = true; checkButton.disabled = false;
    }
  };
})();
