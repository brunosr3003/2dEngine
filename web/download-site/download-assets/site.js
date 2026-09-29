// A página e os downloads funcionam mesmo sem JavaScript.
fetch('/downloads/releases.json', {cache: 'no-store'})
  .then(response => { if (!response.ok) throw new Error('Sem manifesto'); return response.json(); })
  .then(release => {
    // CACHE-BUSTING. Os zips têm o MESMO nome em toda release e o servidor não
    // manda Cache-Control: o Chrome pode entregar o zip anterior a quem clica
    // "baixar" de novo. O dono baixou "a versão nova" e disse "it still the
    // same". Com ?v=<build> na URL, build nova é URL nova, e cache nenhum
    // serve a antiga. O nome do arquivo baixado não muda.
    const plataformaDoArquivo = {
      'MMORPG-Windows.zip': 'windows', 'MMORPG-Linux.zip': 'linux',
      'MMORPG-Mac.zip': 'mac', 'Tempest-Android.apk': 'android',
    };
    for (const a of document.querySelectorAll('a[href^="/downloads/"]')) {
      const arquivo = a.getAttribute('href').split('/').pop().split('?')[0];
      const plat = plataformaDoArquivo[arquivo];
      const build = release.platforms?.[plat]?.build ?? release.build;
      if (plat && build) a.setAttribute('href', '/downloads/' + arquivo + '?v=' + build);
    }
    for (const node of document.querySelectorAll('[data-size]')) {
      const bytes = release.platforms?.[node.dataset.size]?.bytes;
      const build = release.platforms?.[node.dataset.size]?.build;
      if (typeof bytes === 'number' && bytes > 0) {
        node.textContent = '· ' + new Intl.NumberFormat('pt-BR', {maximumFractionDigits: 1}).format(bytes / 1048576) + ' MB'
          + (build ? ' · versão ' + build : '');
      }
    }
  }).catch(() => {});
