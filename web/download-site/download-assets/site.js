// A página e os downloads funcionam mesmo sem JavaScript.
fetch('/downloads/releases.json', {cache: 'no-store'})
  .then(response => { if (!response.ok) throw new Error('Sem manifesto'); return response.json(); })
  .then(release => {
    for (const node of document.querySelectorAll('[data-size]')) {
      const bytes = release.platforms?.[node.dataset.size]?.bytes;
      if (typeof bytes === 'number' && bytes > 0) {
        node.textContent = '· ' + new Intl.NumberFormat('pt-BR', {maximumFractionDigits: 1}).format(bytes / 1048576) + ' MB';
      }
    }
  }).catch(() => {});
