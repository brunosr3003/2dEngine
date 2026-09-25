# Onde obter

No MIR4, todo material pedido tem um atalho para "onde consigo isso". Aqui é
a **lupa** ao lado do item: abre um popup com de onde ele sai e um botão para
ir até lá. Nada abre por tecla; é só tocar na lupa.

Código: `crates/client/src/onde_obter.rs` (lógica pura + popup),
`crates/server/src/economy.rs` (`fontes_de_itens`),
`crates/server/src/world/chefes.rs` (`onde_obter_snapshot`, `itens_do_chefe`).

## Onde a lupa aparece

| Lugar | Em quê |
|---|---|
| Craft | cada ingrediente da receita selecionada |
| Forja | Darksteel e Cobre do próximo refino |
| Bolsa | cartão de qualquer item (menos ouro) |
| Diário de missões | objetivo de juntar/entregar item ainda não cumprido |
| Diárias | diária de juntar item |
| Mercado | ícone de cada anúncio de item (toque no ícone; TP não tem) e o item escolhido na aba Vender |
| Barra de itens | configurador (Menu → Sistema → Barra): cada consumível da lista e cada espaço preenchido |

Com o popup aberto, os painéis de baixo ficam parados (não desenham), para o
toque no popup não cair num botão deles. O X volta ao painel.

## De onde vêm as fontes

O servidor monta tudo das tabelas reais e manda no login e a cada recarga da
economia (`ServerMessage::ResourceSources`, protocolo 96). Só o que existe:

| Fonte | Tirada de | Mostra |
|---|---|---|
| **Coleta** | `farm_node_drops`: árvore (linha 1) e pedra. A tabela da pedra é por cor do **material**; a pedra de cada cor entrega as cores na proporção de `RENDIMENTO_DA_PEDRA`, então a fonte é a pedra, com a chance já multiplicada | tipo de nó, quantidade, chance por coleta |
| **Bicho** | `loot_drops` dos kinds comuns e de praia (sem chefes, sem equipamento) | nome, nível da zona, chance por morte e as **ilhas** onde nasce |
| **Chefe do mundo** | `itens_do_chefe` (o mesmo que `loot_de_chefe` rola; teste garante) | nome, nível, chance e a **ilha** |
| **Vendedor** | lojas que existem num NPC da vila (hoje a do Alquimista) | NPC, preço |
| **Craft** | receitas (`craft_recipes`) pelo item que sai | receita, nível mínimo |
| **Craft de material** | sínteses de `shared::combinar` para aço, platina e os demais materiais coloridos | 10 da cor anterior e os custos da síntese |
| **Missão** | recompensas de `QUESTS` (item 1 e 2) | título, se é diária |
| **Dungeon** | chaves de craft (Escama/Garra/Chifre/Couro) | **Abrir**: a janela das Dungeons (Menu → Aventura → Dungeons). A Caçada (raid) segue em breve |
| **Mercado** | o cliente acrescenta para todo item **não vinculado** | abre o Mercado buscando o nome |

## O que o "Ir" faz

| Fonte | Ir |
|---|---|
| Coleta | vai à região mais perto daquele tipo na ilha e liga o **auto coleta só daquele tipo** (o mesmo "Ir" do mapa) |
| Bicho | vai à zona mais perto onde ele nasce com chance boa (≥ 15%) e liga o **auto combate** |
| Chefe | vai até o chefe e liga o auto combate. **Nível do chefe > seu nível + 5**: só chega (nada liga sozinho) e avisa |
| Vendedor | vai até o NPC e fala com ele (a loja abre) |
| Craft | abre o Craft na receita |
| Craft de material | abre a aba Materiais do Craft na síntese correspondente |
| Mercado | abre o Mercado na aba Comprar, buscando o item |
| Missão | sem Ir |
| Dungeon | abre a janela das Dungeons |

O Ir fecha os painéis, desliga o que brigaria pela rota (como o mapa) e diz no
chat "Indo: …"; a faixa do HUD mostra "INDO · … · N m".

## Outra ilha

Bicho e chefe trazem **em que ilha** existem (`ilhas_dos_bichos` e a zona do
chefe em `world/chefes.rs`). O bicho conta nas ilhas cuja faixa de nível o
sorteia com chance ≥ 15% (mesma conta do spawn, `quests::chance_do_kind`);
caranguejo, em toda ilha. Se a fonte não está na ilha atual, a linha diz
"Ilha: Geleira" (ou "Ilhas: Ermo, Planalto") e fica **sem Ir**, com "Outra
ilha": a viagem é pelo menu Viajar do Capitão do Porto (ilhas que a história
já liberou).
Coleta e vendedor não precisam: toda ilha tem árvore (o deserto, rala), as
quatro cores de pedra (a cor sai da altura relativa ao pico) e o Alquimista
na vila.

Zona com nível mínimo acima do seu nível + 5 aparece em vermelho, depois das
opções do seu nível.

## Ordem

1. Coleta e bicho do seu nível, do mais perto.
2. Chefe do seu nível e vendedor.
3. Craft.
4. Bicho acima do nível, chefe muito acima.
5. Mercado.
6. Missão.
7. Dungeon (abre a janela; raid em breve).

## Fora do escopo por enquanto

- **Loja de facção**: o NPC de facção vem do mapfile legado e o cliente não
  trata `FactionShopOpen` nem manda `FactionShopBuy` — o jogador não tem como
  comprar nela. Entra quando a loja voltar ao cliente.
- **Pesca**: o servidor ainda sabe pescar (`FishingCast`), mas o cliente nunca
  manda o pedido; peixe não tem de onde sair para o jogador. Entra junto da
  pesca no cliente.
- **Ir para outra ilha**: só quando existir viagem livre entre ilhas.
- **Barra do HUD**: o toque longo no espaço já abre o configurador (onde há a
  lupa); uma segunda ação no mesmo gesto brigaria com o arrasto do AUTO.
- A Caçada (raid) entra quando existir.
