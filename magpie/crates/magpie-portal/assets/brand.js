/* Magpie navigation and branding over the upstream Agentgateway UI.
 * The native router, editors and playgrounds remain intact. */
(() => {
  let session;
  function link(item) {
    const a = document.createElement('a');
    a.href = item.href; a.textContent = item.label;
    a.className = 'magpie-nav-link'; return a;
  }
  function enhance() {
    if (!session) return;
    if (document.title !== 'Magpie Gateway') document.title = 'Magpie Gateway';
    document.querySelectorAll('.brand').forEach(brand => {
      if (brand.dataset.magpie) return;
      brand.dataset.magpie = 'true'; brand.setAttribute('aria-label', 'Magpie Gateway home');
      brand.querySelectorAll('img').forEach(img => img.remove());
      const mark = document.createElement('img'); mark.src = '/assets/magpie/mark.svg'; mark.alt = ''; mark.className = 'magpie-mark';
      const name = document.createElement('span'); name.className = 'magpie-wordmark'; name.textContent = 'Magpie';
      const sub = document.createElement('small'); sub.textContent = 'GATEWAY'; name.append(sub);
      brand.prepend(mark, name);
    });
    document.querySelectorAll('.nav-list, .mobile-nav-menu').forEach(nav => {
      if (nav.querySelector('.magpie-nav')) return;
      const section = document.createElement('section'); section.className = 'magpie-nav';
      const label = document.createElement('h2'); label.textContent = 'MAGPIE GOVERNANCE'; section.append(label);
      session.links.filter(item => ['/australia','/dlp','/policies','/approvals','/access','/audit','/settings'].includes(item.href)).forEach(item => section.append(link(item)));
      nav.append(section);
    });
    const controls = document.querySelector('.topbar-controls');
    if (controls && !controls.querySelector('.magpie-mode')) {
      const mode = link({ href: '/settings', label: session.mode === 'log-only' ? 'Magpie log-only' : 'Magpie enforcement' });
      mode.className = 'magpie-mode'; mode.title = 'Manage enforcement in Magpie Settings'; controls.prepend(mode);
    }
    const main = document.querySelector('main.content');
    if (main && /^\/ui\/?$/.test(location.pathname) && !main.querySelector('.magpie-welcome')) {
      const hero = document.createElement('section'); hero.className = 'magpie-welcome'; hero.setAttribute('aria-label', 'Magpie Gateway');
      const text = document.createElement('div');
      const eyebrow = document.createElement('span'); eyebrow.className = 'magpie-eyebrow'; eyebrow.textContent = 'AUSTRALIAN AI GATEWAY';
      const title = document.createElement('h1'); title.textContent = 'Every connection. A watchful eye.';
      const description = document.createElement('p'); description.textContent = 'Connect your models, agents and tools. Explore with Agentgateway. Govern with Magpie.';
      text.append(eyebrow, title, description);
      const bird = document.createElement('img'); bird.src = '/assets/magpie/magpie-portrait-v1.webp'; bird.alt = 'Australian magpie with a white nape, black chest and silver bill, perched on eucalyptus.';
      hero.append(text, bird); main.prepend(hero);
    }
    if (main && !/^\/ui\/?$/.test(location.pathname)) main.querySelector('.magpie-welcome')?.remove();
  }
  async function refresh() {
    try {
      const response = await fetch('/api/magpie/session', { credentials: 'same-origin' });
      if (response.status === 401) { location.replace('/login'); return; }
      if (!response.ok) return;
      session = await response.json();
      document.querySelectorAll('.magpie-mode').forEach(node => {
        node.textContent = session.mode === 'log-only' ? 'Magpie log-only' : 'Magpie enforcement';
      });
      enhance();
    } catch { /* Native API error handling remains visible. */ }
  }
  let queued = false;
  new MutationObserver(() => {
    if (queued) return; queued = true;
    requestAnimationFrame(() => { queued = false; enhance(); });
  }).observe(document.body, { childList: true, subtree: true });
  refresh();
  setInterval(() => { if (document.visibilityState === 'visible') refresh(); }, 60000);
})();
