# Skills por arma

São 12 skills ativas, três para cada conjunto. A arma equipada escolhe as
skills disponíveis; o nível do personagem libera a primeira no **nível 1**,
a segunda no **5** e a terceira no **10**. Esses níveis são os valores
iniciais do playtest. Não há pontos para comprar skills, ranks ou passivas.

## Catálogo

| Arma | Lv | Skill | Efeito | Mana | Recarga |
|---|---:|---|---|---:|---:|
| Espada e escudo | 1 | Investida | Avança até o alvo com colisão e atinge inimigos no caminho | 10 | 8 s |
| Espada e escudo | 5 | Golpe Largo | Corte em cone voltado para o alvo | 15 | 6 s |
| Espada e escudo | 10 | Muralha | Reduz o dano recebido em 50% durante 5 s | 25 | 20 s |
| Katana | 1 | Saque | Corte em linha voltado para o alvo | 8 | 6 s |
| Katana | 5 | Dança | Atinge o alvo próximo e inimigos ao redor dele | 18 | 10 s |
| Katana | 10 | Vento Cortante | Onda cortante que atinge o alvo | 22 | 12 s |
| Duas pistolas | 1 | Tiro Certeiro | Disparo poderoso no alvo | 8 | 4 s |
| Duas pistolas | 5 | Rajada | Disparos em cone voltado para o alvo | 16 | 9 s |
| Duas pistolas | 10 | Barril | Arremessa barril e explode no alvo | 24 | 16 s |
| Anel mágico | 1 | Bênção | Cura o próprio personagem | 14 | 10 s |
| Anel mágico | 5 | Aura | Cura o personagem e aliados ao seu redor | 26 | 18 s |
| Anel mágico | 10 | Julgamento | Impacto mágico no alvo e inimigos próximos | 30 | 14 s |

Valores de mana, dano, cura, área e recarga são iniciais e continuam editáveis
no banco. `shared::skills::playtest()` é a origem do cadastro inicial;
`DESTRAVA_EM` define os níveis e `Skill::impacto_em()` define o tempo do gesto.

## Uso

Atalhos **1, 2 e 3**, ou clique na barra inferior. A barra mostra nome, mana,
recarga e nível de desbloqueio. Passar o mouse mostra a descrição.
Trocar de arma troca os três botões automaticamente, preservando as recargas
de cada skill.

Toda skill ofensiva exige um inimigo selecionado, vivo, no alcance e visível.
O alvo é uma entidade: a skill acompanha sua posição até o impacto. Trocar a
seleção durante a preparação mantém o destinatário original. Sem alvo válido,
o servidor rejeita o uso sem cobrar mana ou iniciar recarga.
Áreas ofensivas ficam centradas no alvo; cones e linhas se orientam para ele.
Dança exige um alvo próximo (alcance igual ao raio, inicialmente 2,5).
Bênção, Muralha e Aura usam o próprio personagem como alvo; Aura também cura
os aliados próximos. Não há mira no chão ou direção escolhida pelo cursor.

## Execução

O servidor valida arma, nível, mana, recarga e estado do personagem. Uma skill
aceita cobra mana uma vez, inicia a recarga e envia o início da animação.
Ataques básicos ficam suspensos durante o gesto.

Dano, cura e proteção só são aplicados no impacto. Vento Cortante e Tiro
Certeiro atingem a entidade selecionada, com rastro e efeito de impacto
sincronizados; outro mob cruzando a trajetória não intercepta essas skills.
Alcance, linha de visão, zona segura, vida e regras
de PvP são respeitados ao resolver os efeitos. Aura cura o próprio personagem
e aliados elegíveis da mesma facção ou grupo; não ressuscita personagens.

Movimento mantido durante a preparação pode cancelar a skill. Trocar de arma,
morrer ou ficar incapacitado antes do impacto também cancela. Se o alvo morrer,
sumir, sair do alcance ou ficar atrás de um obstáculo, o cast é cancelado.
Cancelamento
antes do efeito devolve mana e remove a recarga; depois do impacto não há
reembolso. A Investida usa a colisão do movimento normal.

Os gestos e efeitos visuais são procedurais no cliente. A katana mantém as
duas mãos no cabo durante suas três skills. Os efeitos de início, impacto e
cancelamento vêm do servidor. Cliente e servidor usam o protocolo 81.

Os gestos têm preparação legível, ação rápida e 0,36 s de recuperação.
Pistolas apontam para o alvo no disparo e recuam depois; a Dança liga dois
cortes de katana sem girar o tronco uma volta inteira. Os rastros seguem a
lâmina; disparos saem dos canos e magia, das mãos. Barril tem volume 3D e
trajetória em arco a partir do arremesso. Os efeitos são desenhados no mundo,
com perspectiva, em vez de linhas sobrepostas à tela.

As 12 skills usam brilho aditivo, partículas com rastros e ondas de impacto.
Espada tem energia dourada e barreira hexagonal; katana tem cortes largos
cianos com núcleo branco; pistolas têm clarão nos canos, rastros alaranjados
e explosão de fogo no barril. Anel tem selos e espirais verdes nas curas,
e uma coluna violeta com raios no Julgamento. O brilho desaparece gradualmente
por 0,85–1,6 s após o impacto; Muralha acompanha os 5 s do buff. Essa duração
visual não muda o instante do dano nem o tempo de recuperação.

Prévia local: `MMO_PREVIA_SKILLS=1 ./scripts/run-client.sh`. Setas escolhem
a skill, espaço pausa e Esc sai; não conecta ao servidor nem muda personagens.
Acrescente `MMO_PREVIA_HUD=1` para conferir a interface de combate.

## HUD e uso automático

Skills ficam em botões circulares no canto inferior direito, com ícones
próprios, teclas 1–3, recarga radial e marca AUTO. Clique e solte para usar;
arraste para cima e solte para ativar o uso automático, ou para baixo para
desativar. O gesto não dispara uma skill. Cada arma mantém suas marcações
durante a conexão; todas começam em manual ao reconectar.

O uso automático respeita nível, mana, recarga, ação em curso e alcance do
alvo selecionado. Curas esperam a vida cair abaixo de 85%; Muralha espera um
alvo a até 8 unidades. Rejeições e cancelamentos impõem intervalo antes de
tentar novamente. A validação final e o instante do dano continuam no servidor.

O botão COMBATE, ou Z, liga/desliga o auto combate. Procura apenas monstros
vivos em até 24 unidades do ponto de ativação, aproxima, ataca e troca de
alvo após a morte. Sem monstros, aguarda; após 8 segundos sem aproximação ou
dano, ignora o alvo por 15 segundos. Usa apenas as skills marcadas AUTO.
Andar (teclado ou clique no chão) NÃO desliga: a área acompanha o personagem
e a busca de alvo espera ele parar por 0,4 s. Clicar num monstro troca o
alvo sem sair do modo. Esc, Z ou abrir o inventário interrompem;
morte/incapacitação, troca de mapa e desconexão também o desligam.

A ficha mantém poder abaixo do nível; alvo, localização, chat e EXP usam
painéis compactos. Em janelas estreitas o alvo desce para não sobrepor a
ficha. Segurar F3 mostra os diagnósticos de rede e renderização.

## Boss para teste

O comando administrativo `admin_cli spawn_boss <x> <z> <hp>` cria um
Guardião de Treino de nível 20 perto das coordenadas, em chão acessível e
fora da zona segura. Por exemplo, `spawn_boss 74 -139 50000` cria um boss
com 50.000 HP. Ele é temporário: desaparece quando o servidor reinicia.

O comando usa `MMORPG_WS_URL` e exige `MMORPG_ADMIN_SECRET` igual ao do
servidor. O CLI confirma o ID e a posição após o servidor criar o boss.
Não altera o cadastro nem a força dos mobs comuns. A vida permitida vai de
1.000 a 60.000, dentro do limite transmitido pelo protocolo.
