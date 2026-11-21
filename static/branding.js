(async () => {
  try {
    const response = await fetch('/api/site-config');
    if (!response.ok) return;
    const {group_name: name} = await response.json();
    if (typeof name !== 'string' || !name) return;
    // Treat the configured name as text, including when it contains HTML characters.
    const brand = document.querySelector('.brand');
    if (brand) {
      const label = document.createElement('span');
      label.className = 'group-name';
      label.textContent = name;
      brand.replaceChildren(brand.querySelector('span'), label);
    }
    if (name === 'Secret Santa') return;
    document.title += ' · ' + name;
    const heading = document.querySelector('h1');
    if (heading && !document.querySelector('main.admin')) {
      const subtitle = document.createElement('em');
      subtitle.textContent = 'Secret Santa';
      heading.replaceChildren(document.createTextNode(name), subtitle);
    }
    for (const selector of ['meta[property="og:title"]', 'meta[property="og:site_name"]', 'meta[name="twitter:title"]']) {
      const tag = document.querySelector(selector);
      if (tag) tag.content = name + ' · Secret Santa';
    }
  } catch {
    // The generic Secret Santa branding remains usable if configuration cannot load.
  }
})();
