# Banco e tamanho da bolsa

O **banco** fica com o **Estivador** da vila (toda ilha tem um, no porto).
Tocar nele abre o painel Banco: a bolsa a' esquerda, o banco a' direita; tocar
num item passa ele pro outro lado (`VaultDeposit` / `VaultWithdraw`, que o
servidor so' aceita perto do Estivador — `perto_do_banco`). Se o Estivador
tiver missao pra oferecer, o dialogo vem antes e o banco abre quando ele
fecha. No Menu, **Comércio → Banco** leva ate' o Estivador (com o botao
Teleportar, se houver pergaminho). O banco e' do PERSONAGEM e e' o mesmo em
toda ilha (mesma tabela `vault`).

## Expandir

Bolsa e banco comecam com 40 espacos e sobem de 10 em 10, pagando **ouro**
(`shared::armazem`):

| | teto | custo da proxima expansao |
|---|---|---|
| Bolsa | 100 (6 expansoes) | 2.000, 4.000, 8.000 … 64.000 (dobra) |
| Banco | 160 (12 expansoes) | 1.000 × n² (1.000, 4.000, 9.000 …) |

O botao **+10 espaços** fica no pe' da bolsa (de qualquer lugar) e embaixo de
cada lado no painel do Banco (o do banco, so' perto do Estivador).
`ClientMessage::ExpandirArmazem { banco }` → `ServerMessage::Armazem` (tambem
no login). As expansoes compradas moram em `characters.bolsa_extra` /
`banco_extra`; o tamanho e' conta. A grade da bolsa e a do banco ROLAM
(dedo, roda ou barra), entao o painel nunca passa do tamanho da tela.

## Pilhas

Pocoes empilham ate' 999 (eram 20). No login, pilhas separadas do mesmo item
viram uma so' (`juntar_pilhas`), na bolsa e no banco.

## Moedas: ouro, cobre e darksteel

Nenhuma moeda ocupa espaço na grade. O **ouro** é um saldo (`gold`); o que
entrar como item (baú, correio, prêmio antigo) vira saldo. **Cobre** e
**darksteel** moram na **carteira**: os dois últimos espaços da lista da
bolsa (`armazem::CARTEIRA`), depois da grade. `add_to_inventory` e
`por_empilhavel` põem essas moedas direto lá (sem teto de pilha, mesmo com a
bolsa cheia), e `arrumar_bolsa` — no login, ao expandir e antes de todo envio
da bolsa — recolhe moeda solta na grade. Como a carteira faz parte da lista,
as contas de craft, forja, oficina e loja (`tem`/`consumir`) seguem iguais. A
bolsa mostra as três moedas no cabeçalho.

