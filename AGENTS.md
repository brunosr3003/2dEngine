# Publicação do Tempest

O usuário pediu que as atualizações do jogo sejam compiladas e publicadas no
site como parte da entrega, sem pedir novamente autorização para publicar.
Atualize Windows, Android, Linux, Mac e iOS/TestFlight pelos scripts existentes;
registre qualquer plataforma bloqueada sem anunciar que ela foi publicada.

Antes de empacotar, atualize `docs/PATCHNOTES.txt`, incremente `BUILD` em
`crates/client/src/atualizacao.rs` e rode `scripts/check-release-notes.py --seal`.
Publique os pacotes antes do manifesto `downloads/releases.json`. Cada entrada
de plataforma deve registrar o `build` realmente disponível e seu `update_url`.
Copie o mesmo manifesto para `WEB_STATIC/releases.json` na API de produção:
`/api/client-release` é o que os clientes consultam antes do login.
Desktop abre o site; mobile abre a loja/TestFlight. Enquanto Android for
distribuído por APK, mantenha o link do APK, sem apontar a uma loja inexistente.
Verifique os hashes dos downloads públicos e preserve uma cópia da versão anterior.
