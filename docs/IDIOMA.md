# O jogo em dois idiomas

Pedido do dono (27/09/2026): *"a full translate to english and a language
settings"*.

O jogo nasceu em português com o texto escrito **direto no código** — rótulo de
HUD, nome de item, fala de NPC, aviso do servidor, tudo literal no meio da
chamada que desenha. São ~2.900 frases em 162 arquivos.

## A decisão: a chave é a própria frase em português

O caminho de sempre seria trocar cada literal por uma chave (`t!("hud.bolsa")`).
Foi recusado: são **726 pontos de chamada** reescritos, e cada reescrita é uma
chance de mexer na lógica sem querer.

Aqui a chave é a **frase em português**, e quem traduz é o **desenho**:

```rust
// O código não mudou. Continua assim:
estilo::texto(x, y, "Bolsa", 14, COR);

// E dentro de hud_estilo::desenha_texto:
let s = &shared::idioma::tr(s);   // "Bolsa" → "Bag"
```

Isso só é barato porque todo texto do cliente passa por **um gargalo**: as
funções de `crates/client/src/hud_estilo.rs`. Traduzir lá dentro pega de uma vez
o rótulo escrito no código, o nome de item que veio do **banco** e o aviso que o
**servidor** mandou — sem tocar no protocolo, sem coluna nova, sem migração.

### O que isso implica

| | |
|---|---|
| A lógica continua em português | A tradução é no último instante, ao desenhar e ao medir. Nada compara, salva ou envia inglês. Nenhuma comparação de nome quebra. |
| Frase sem verbete aparece em português | Não há `panic!` nem `"???"`. Falta de tradução degrada pra língua original, que é legível. |
| O servidor nunca traduz | Ele manda português; o cliente traduz ao desenhar. Por isso um servidor serve os dois idiomas ao mesmo tempo. |

## Onde mexer

| O quê | Onde |
|---|---|
| O motor (o `Idioma`, o `tr`, o casamento de modelo) | `crates/shared/src/idioma.rs` |
| O dicionário, em cinco partes | `crates/shared/src/idioma/en/` |
| O gargalo do desenho | `crates/client/src/hud_estilo.rs` |
| A escolha do jogador | `crates/client/src/config_interface.rs` (Menu › Sistema › Interface) |
| Onde a escolha fica salva | `crates/client/src/lembranca.rs` (`tempest.prefs`, no aparelho) |
| O site | `web/download-site/download-assets/site-i18n.js` |
| Os e-mails da conta | `crates/web/src/email.rs` |

## Acrescentei uma frase no jogo. E agora?

1. Escreva em português, como sempre.
2. Ponha o par em `crates/shared/src/idioma/en/<parte>.rs`.
3. `cargo test -p shared --test idioma`.

O teste cobra o que o compilador não vê:

- **buraco tem que bater**: `"Nível {n}"` → `"Level {n}"`. Buraco a mais, a
  menos ou renomeado é erro;
- **buraco nomeado pode trocar de ordem**, anônimo não. Tradução que reordena é
  obrigada a nomear — `"{a} de {b}"` → `"{b}'s {a}"` funciona, `"{} de {}"` →
  `"{}'s {}"` sairia trocado;
- **chave repetida é erro** (a segunda nunca seria usada);
- **modelo sem texto fixo com letra não entra**: `"{a} {b}"` casaria qualquer
  frase com um espaço e passaria na frente do verbete certo.

### A frase é montada com `format!`?

Ela chega no desenho já preenchida ("Nível 5 de 12"), então o verbete guarda os
pedaços fixos e o `Modelo` casa o que sobrou. Funciona sozinho — só respeite as
regras de buraco acima.

### A frase é quebrada em linhas antes de desenhar?

Aí **traduza na entrada da quebra**, não no desenho: a quebra entrega *pedaços*,
e pedaço de frase não casa com verbete. Já é assim em `menu_missoes::quebrar_linhas`,
`missoes::quebra`, `novidades::quebra`, `personagens::texto_linhas` e
`hud_estilo::texto_ajustado` (que corta com `…`).

## Conferir na tela, sem abrir o jogo

`MMO_IDIOMA` ganha do arquivo de preferências — é como o teste e a captura
pedem uma língua sem mexer na escolha de ninguém:

```sh
env DISPLAY=:1 MMO_IDIOMA=en MMO_PREVIA_FICHA=1 \
    MMO_PREVIA_SAIDA=/tmp/en ./target/debug/client
```

> O idioma é definido **antes dos ramos de prévia**, em `main.rs`. Já esteve
> depois, e como cada prévia termina em `return`, a captura em inglês saía
> **idêntica** à em português, byte por byte, sem nada acusar. O teste
> `definir_vale_na_hora_e_para_quem_desenhar_depois` guarda esse caso.

Dois scripts de apoio (no scratchpad da sessão, não versionados) mediram a
cobertura e a largura; o que eles ensinaram está aqui:

- **rótulo em CAIXA ALTA é texto de tela**, não nome de const. A primeira
  varredura jogou fora todo literal `[A-Z0-9_]+` e perdeu `ATRIBUTOS`,
  `COMBATE`, `BOLSA`, o menu inteiro. Só apareceu numa captura, com a ficha
  meio em inglês e meio em português;
- **inglês é mais comprido**. Onde o português abrevia (`Red. dano`,
  `Inv. Pet`), a caixa é apertada e o inglês também tem que abreviar
  (`Dmg. red.`, `Pet Summ.`).

## O que NÃO foi traduzido, e por quê

| | |
|---|---|
| A trilha do `jogadorbot` (`bots.jsonl`) | `~/tempest-prod/analisa-bots.py` lê o campo `detalhe`: corta em `·`, tira a palavra `em` e procura `MORTO`. Traduzir a trilha quebraria a análise em silêncio. As **ações** (`atacou`, `coletou`, `quest_aceita`) são chaves de dado, não texto — nunca se traduzem. |
| O panóptico | Painel de operador, e estava em edição em paralelo. |
| SQL, caminho de asset, chave de cache | Não são texto de tela. |
| Nome próprio (`Tempest`, `Morganeers`, `Peacemain`, `Matteo`) | Não se traduz nome. |

Os logs de servidor e a saída das ferramentas de linha de comando **foram**
passados pro inglês na fonte: ninguém alterna a língua de um `tracing::info!`.
