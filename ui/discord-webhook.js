/* Only status crosses back from the encrypted vault; stored webhook tokens stay native. */
(() => {
  let api, url, enabled, status, save, test, remove, configured = false, loaded = false, epoch = 0, busy = false, savedEnabled = false, loading = null;
  const make = (tag, text, className) => {
    const value = document.createElement(tag);
    if (text) value.textContent = text;
    if (className) value.className = className;
    return value;
  };
  function valid(generation, requestEpoch) {
    const context = api.context();
    return context.unlocked && context.generation === generation && epoch === requestEpoch;
  }
  function controls() {
    url.disabled = enabled.disabled = save.disabled = !loaded || busy;
    test.disabled = !loaded || busy || !configured || !!url.value.trim();
    remove.disabled = !loaded || busy || !configured;
    url.placeholder = configured ? 'Webhook saved securely — paste to replace' : 'https://discord.com/api/webhooks/…';
  }
  function changed() { return loaded && (!!url.value.trim() || enabled.checked !== savedEnabled); }
  function refreshDirty() { document.getElementById('settings-dialog').dispatchEvent(new Event('change', { bubbles: true })); }
  function fill() {
    const request = load(); loading = request;
    return request.finally(() => { if (loading === request) loading = null; });
  }
  async function load() {
    const generation = api.context().generation, requestEpoch = ++epoch;
    url.value = ''; url.type = 'password'; loaded = false; busy = false; controls(); status.textContent = 'Loading webhook settings…';
    try {
      const result = await api.invoke('discord_webhook_status');
      if (!valid(generation, requestEpoch)) return;
      configured = !!result.configured; enabled.checked = savedEnabled = !!result.enabled;
      loaded = true; status.textContent = configured ? 'A webhook is saved on this account.' : 'No Discord webhook configured.';
    } catch (_) { if (valid(generation, requestEpoch)) status.textContent = 'Could not load webhook settings. Close Settings and try again.'; }
    finally { if (valid(generation, requestEpoch)) controls(); }
  }
  async function perform(kind, propagate = false, announce = true) {
    if (!api.context().unlocked || !loaded || busy) {
      if (propagate) throw Error('Wait for the current webhook action, then save settings again.');
      return;
    }
    if (kind === 'test' && url.value.trim()) { status.textContent = 'Save the new webhook before testing.'; return; }
    const generation = api.context().generation, requestEpoch = epoch;
    busy = true; controls(); status.textContent = kind === 'test' ? 'Sending test announcement…' : 'Saving webhook…';
    try {
      if (kind === 'test') {
        await api.invoke('notify_discord_online', { test: true });
        if (valid(generation, requestEpoch)) status.textContent = 'Test announcement sent to Discord.';
      } else {
        const result = await api.invoke('save_discord_webhook', {
          enabled: kind === 'remove' ? false : enabled.checked,
          url: kind === 'remove' ? '' : url.value.trim() || null
        });
        if (!valid(generation, requestEpoch)) return;
        configured = !!result.configured; enabled.checked = savedEnabled = !!result.enabled; url.value = '';
        status.textContent = kind === 'remove' ? 'Webhook removed.' : 'Webhook settings saved.';
        refreshDirty();
        if (result.enabled && announce) accountOpened();
      }
    } catch (error) {
      // Native errors are deliberately sanitized and never include the webhook URL.
      if (valid(generation, requestEpoch)) status.textContent = String(error);
      if (propagate) throw error;
    } finally {
      if (valid(generation, requestEpoch)) { busy = false; controls(); }
    }
  }
  async function savePending() {
    if (loading) await loading;
    if (busy) throw Error('Wait for the current webhook action, then save settings again.');
    if (changed()) await perform('save', true, false);
  }
  async function accountOpened() {
    if (!api.context().unlocked) return;
    try { await api.invoke('notify_discord_online', { test: false }); }
    catch (_) { /* An external service must never interrupt account unlock or chat. */ }
  }
  function clear() {
    epoch++; url.value = ''; url.type = 'password'; enabled.checked = false;
    loaded = configured = busy = savedEnabled = false; loading = null; status.textContent = ''; controls();
  }
  window.OchatDiscordWebhook = {
    init(callbacks) {
      api = callbacks;
      const section = make('section', '', 'discord-webhook-settings'); section.id = 'discord-webhook-settings'; section.dataset.settingsIndependent = '';
      section.append(make('h3', 'Discord online announcement'));
      const label = make('label', '', 'checkbox-row');
      enabled = make('input'); enabled.id = 'discord-webhook-enabled'; enabled.type = 'checkbox';
      label.append(enabled, make('span', 'Announce when I open Ochat'));
      const urlLabel = make('label', 'Discord webhook URL'); urlLabel.htmlFor = 'discord-webhook-url';
      url = make('input'); url.id = 'discord-webhook-url'; url.type = 'password'; url.maxLength = 512; url.autocomplete = 'off'; url.spellcheck = false; url.dataset.privatePassword = '';
      url.oninput = controls;
      save = make('button', 'Save webhook', 'secondary'); test = make('button', 'Send test', 'secondary'); remove = make('button', 'Remove', 'text-button danger');
      for (const [button, kind] of [[save, 'save'], [test, 'test'], [remove, 'remove']]) { button.type = 'button'; button.onclick = () => perform(kind); }
      const actions = make('div', '', 'webhook-actions'); actions.append(save, test, remove);
      status = make('p', '', 'field-hint'); status.id = 'discord-webhook-status'; status.setAttribute('role', 'status');
      section.append(label, urlLabel, url, make('p', 'Example: “Your name is online on Ochat”. Sent once per app launch after unlocking, except while Invisible. Your name and online activity are shared with Discord and the webhook channel. Uses Tor when enabled.', 'field-hint'), actions, status, make('p', 'Use Save below to save all settings, including this announcement. Save webhook saves just this section before a test. Leave the URL empty to keep your saved webhook. The URL stays encrypted and is never shared with peers.', 'field-hint'));
      document.querySelector('[data-settings-panel="account"]').append(section);
      document.getElementById('settings-dialog').addEventListener('close', clear); controls();
    },
    accountOpened, fill, savePending, isDirty: changed, reset: clear
  };
})();
