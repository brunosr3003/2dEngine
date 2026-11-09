# Próximas atualizações

Lista de trabalho do Tempest. **Nada aqui está publicado.** Quando um item for
entrar numa build, ele vira nota em inglês em `docs/PATCHNOTES.txt` (genérica,
para o jogador) e detalhe em `docs/CHANGELOG.md` (para o time).

## Conteúdo e jogo

_Para o Bruno preencher: regiões, bosses, dungeons, sistemas, eventos._

- [x] **Tamanho mínimo de toque para TUDO que é clicável.** Ícones e botões
      estão pequenos demais. Regra única para o jogo inteiro (HUD, menus,
      lojas, listas, abas, fechar, setas, slots da bolsa…): nenhum alvo
      clicável abaixo de um mínimo confortável, **independente do tamanho e da
      resolução da tela** (medir em tamanho físico, tipo ~9–10 mm / 44–48 pt,
      não em pixels). Onde o desenho for menor, a área de toque cresce além do
      desenho. Vale também para a escala de interface: mesmo no 100%, nada
      fica abaixo do mínimo.
- [x] **Ícone do app instalado = ícone da loja.** O ícone da Play Store está
      bom, mas o que aparece no celular depois de instalar é outro, feio.
      Usar sempre o mesmo ícone da loja (Android: ícone adaptativo/mipmaps do
      launcher; conferir iOS, Mac e Windows também).
- [x] **Rankings.** Lista de rank de **poder** e de **nível**, com recorte
      **por classe** e **geral** (e outros recortes que fizerem sentido).

- [ ] **Redesenhar a tela de Ofertas Relâmpago.** Está feia. Tem que ficar
      bonita, limpa e dar vontade de comprar: arte de destaque por oferta
      (cartão grande, ícones dos itens em vitrine, brilho/moldura de raridade),
      o "de R$ X" riscado e o preço em destaque, cronômetro com urgência
      visual, selo de "economia de N%", botão de comprar chamativo, animação
      de entrada no popup quando a oferta abre. Seguir o estilo da loja de TP
      (`loja_tp.rs`, botão dourado). Código atual: `client/src/ofertas_ui.rs`.

### Entraram na release de 09/11/2026 (protocolo 191)
- [x] Ofertas relâmpago configuráveis no banco (docs/OFERTAS_RELAMPAGO.md), menu próprio com bolinha vermelha e ícone no menu rápido.
- [x] Troca direta entre jogadores.
- [x] Esquiva da DES para todas as classes; Duas Pistolas com o dobro.
- [x] Missão da chave do Porão do Naufrágio e tarja do portal abrem o Craft na aba de chaves.
- [x] Criação de personagem com aparência aleatória + botão Randomize.
- [x] Cliente desatualizado no Android vai direto pra Google Play.
- [x] Compra de TP repetida no Android (o pacote ficava "possuído" pra sempre): servidor agora consome.

## Pendências conhecidas (levantadas no lançamento de 08/11/2026)

### Lojas e pagamento
- [ ] **iOS: compra de TP com StoreKit** + conferência do recibo no servidor
      (igual ao Google Play). Depende da migração da conta Apple para NierTech e
      do Paid Apps Agreement ativo.
- [ ] **iOS: reenviar à App Store** com build nova, prints e descrição novos, e
      resposta à rejeição 4.3 (Spam) explicando motor próprio + mundo voxel.
- [ ] **PC (Windows/Mac/Linux): meio de pagamento real** para TP. Hoje a compra
      no PC é recusada (o simulado saiu do ar no lançamento).
- [x] **Android: só confirmar a compra no Google depois que o servidor creditar**
      — hoje o app confirma (`acknowledgePurchase`) por conta própria; se o
      servidor falhar, o Google não estorna e o jogador fica sem o TP.

### Play Console
- [ ] Enviar os **símbolos de depuração nativos** e o arquivo de desofuscação
      junto do AAB (os dois avisos de toda versão; ajudam a ler crashes).
- [ ] Declarar o **status de comerciante** (prazo 01/02/2027).
- [ ] Resolver o aviso **"problema urgente com sua conta para pagamentos"**.

### Código
- [ ] `main.rs`: `previa_porta` só some com `debug_assertions`, mas o módulo
      também é desligado no Android/iOS — build **debug** do Android quebra.
- [ ] Fazer merge da branch `release-2026-11-08` no `main`.
- [ ] Devolver o cargo de admin (staff) na conta nova do Bruno depois do reset.

### Investigar
- [ ] Personagem que às vezes "não anda" mesmo com o toque no chão mostrando o
      destino (voltou após sair e entrar; ver se o vigia de reconexão de 8 s
      resolve de vez).
