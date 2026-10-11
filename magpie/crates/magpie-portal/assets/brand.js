/* Magpie navigation and branding over the upstream Agentgateway UI.
 * The native router, editors and playgrounds remain intact. */
(() => {
  let session;
  function enhance() {
    if (!session) return;
    if (document.title !== 'Magpie Gateway') document.title = 'Magpie Gateway';
    document.querySelectorAll('.brand').forEach(brand => {
      if (brand.dataset.magpie) return;
      brand.dataset.magpie = 'true'; brand.setAttribute('aria-label', 'Magpie Gateway home');
      brand.querySelectorAll('img').forEach(img => img.remove());
      const mark = document.createElement('img'); mark.src = '/assets/magpie/magpie-emblem-v1.webp'; mark.alt = ''; mark.className = 'magpie-mark';
      const name = document.createElement('span'); name.className = 'magpie-wordmark'; name.textContent = 'Magpie';
      const sub = document.createElement('small'); sub.textContent = 'GATEWAY'; name.append(sub);
      brand.prepend(mark, name);
    });
    const main = document.querySelector('main.content');
    if (main && /^\/ui\/?$/.test(location.pathname) && !main.querySelector('.magpie-welcome')) {
      const hero = document.createElement('section'); hero.className = 'magpie-welcome'; hero.setAttribute('aria-label', 'Magpie Gateway');
      const banner = document.createElement('img'); banner.src = '/assets/magpie/magpie-banner-v1.webp';
      banner.width = 2172; banner.height = 724;
      banner.alt = 'Magpie Gateway. Australian ready. Safe agents. Sovereign AI. Real outcomes. SSO portal, policy and guardrails, Australian DLP, sovereign routing, audit and observability.';
      hero.append(banner); main.prepend(hero);
    }
    if (main && !/^\/ui\/?$/.test(location.pathname)) main.querySelector('.magpie-welcome')?.remove();
  }
  async function refresh() {
    try {
      const response = await fetch('/api/magpie/session', { credentials: 'same-origin' });
      if (response.status === 401) return;
      if (!response.ok) return;
      session = await response.json();

      enhance();
    } catch { /* Native API error handling remains visible. */ }
  }
  let queued = false;
  new MutationObserver(() => {
    if (queued) return; queued = true;
    requestAnimationFrame(() => { queued = false; enhance(); });
  }).observe(document.body, { childList: true, subtree: true });
  if (!/^\/ui\/(login|accept-invitation|reset-password)/.test(location.pathname)) refresh();
  setInterval(() => { if (document.visibilityState === 'visible') refresh(); }, 60000);
})();
