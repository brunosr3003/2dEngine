# Manutenção semanal — toda terça, 9h às 11h (Brasília)

Regra desde 13/11/2026: **toda atualização que não for urgente sai só nessa janela.**
Em UTC são 12:00–14:00 (a mmo-vps e o `at` usam UTC).

Urgente, e pode sair fora do ciclo: crash, exploit, perda de item/ouro/TP, pagamento quebrado,
servidor fora do ar. Todo o resto espera a terça.

## Calendário da semana

| Quando | O quê |
|---|---|
| Qua → qui | Mudanças entram no branch de release da próxima terça. Nada vai direto pra prod. |
| **Quinta (até sexta no máximo)** | Fechar a versão: bump do protocolo + build + versionCode, patch notes selados, testes, servidor de teste + Tempest TESTE no iPhone. Mandar o **AAB pra revisão de produção** na Play e o iPhone pra revisão com **"liberar esta versão manualmente"**. |
| Sáb → seg | Conferir se a Play e a Apple aprovaram. Com a **Publicação gerenciada**, a versão aprovada fica parada esperando o clique. Compilar o servidor na mmo-vps (`/opt/tempest-prod/src`), sem trocar o binário que está rodando. Montar o `cutover-NNN.sh` e o `publica_site_NNN.py`. |
| Dom e seg | Avisos no jogo (Panopticon, avisos programados) e post no Discord: "Manutenção terça 9h–11h". |
| **Segunda 18h** | Ponto de corte: se a Play não aprovou, a versão **pula pra semana seguinte**. Nunca subir o servidor sem a Play aprovada, senão o Android fica travado sem atualização. |
| **Terça 8:30** | Avisos de 30, 15 e 5 min (`/opt/tempest-prod/avisos/aviso.sh`). |
| **Terça 9:00** | Servidor fora do ar → backup (`/opt/tempest-prod/backups/release-NNN`) → **"Publicar alterações" na Play** → cutover na mmo-vps (binários, migrações) → site (downloads + `releases.json` com o protocolo novo) → **liberar o iPhone** na App Store Connect. |
| **Terça até 11:00** | Servidor de volta. Conferir login no Android, PC, Mac e iPhone, e o `grep androidpublisher` no binário. Aviso "manutenção concluída" e post no Discord. |

## Configuração que precisa estar ligada
- **Google Play Console** (conta NierTech 6421217034461068724, app 4974878674433802508):
  Visão geral da publicação → **Publicação gerenciada: ligada**.
- **App Store Connect:** em cada versão nova, "Liberação da versão" = **Manualmente**.
- **Panopticon de prod:** aviso programado fixo de toda segunda 20h e terça 8h30
  ("Weekly maintenance Tuesday 9:00–11:00 (Brasília)").

## Próximo passo (opcional)
Automatizar o clique de terça: dar à service account `tempest-ci-uploader` a permissão de lançar
em produção (Play Console → Usuários e permissões). Aí um `at` na terça às 9h publica pela API, junto
com o cutover. Antes, testar se a API respeita a Publicação gerenciada.

## Primeira janela
**Terça 17/11/2026, 9h–11h:** release 199 (Prime Merchant, clã, PK, PvP, mercado e troca refeita).
Como a versão fechou numa sexta (13/11), o AAB e o iPhone vão pra revisão **até sábado, 14/11**. Assim dá
tempo da revisão até o corte de segunda, 16/11 às 18h. A partir da semana seguinte vale o prazo normal da quinta.
