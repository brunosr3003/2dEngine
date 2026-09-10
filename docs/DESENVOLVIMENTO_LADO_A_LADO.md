# Terminal e jogo lado a lado (Hyprland)

Como rodar o cliente sem que a janela do jogo cubra o terminal.

## Resumo

```sh
cd ~/2dEngine
./scripts/run-client.sh            # ou --build pra compilar antes
```

Resultado: jogo em `10,75 945x995`, terminal em `965,75 945x995`.

## A causa: window swallowing

O `~/.config/hypr/config/misc.lua` liga o **swallow**:

```lua
enable_swallow = true,
swallow_regex  = "(kitty|ghostty|[Kk]onsole|Alacritty|gnome-terminal|xfce[0-9]?-terminal)",
```

Quando um app gráfico nasce **a partir de um terminal** que casa com esse
regex, o compositor *engole* o terminal: esconde a janela dele e entrega o tile
inteiro pro app novo. É comportamento desejado pra `mpv video.mp4` — e é
exatamente o que faz o jogo parecer que "abre por cima".

O sintoma no `hyprctl clients` engana, porque parece layout quebrado:

```
kitty     at=[10, 75]  size=[945, 995]
tempest   at=[10, 75]  size=[1900, 995]     ← mesmo `at`, largura cheia
```

Não é o cliente. Um `kitty --class teste` lançado do terminal reproduz igual.
E abrir um terminal pela keybind funciona normal, porque aí não existe terminal
pai pra ser engolido.

## A solução

`scripts/run-client.sh` desliga o swallow, sobe o cliente e religa na saída:

```sh
swallow() { hyprctl eval "hl.config({ misc = { enable_swallow = $1 } })"; }
trap 'swallow true' EXIT INT TERM
swallow false
```

O `trap` cobre Ctrl+C, crash e kill — o swallow nunca fica desligado por
acidente.

O caminho mais elegante seria `misc:swallow_exception_regex`, que existe nesta
versão mas **não surtiu efeito**: testado casando a classe (`^(tempest)$`) e o
título (`^(Tempest)$`) da janela do jogo, o swallow continuou acontecendo.

## O que o cliente faz da sua parte

Em `crates/client/src/main.rs`, no `window_conf()`:

- `linux_wm_class = "tempest"` — sem isso a janela se anuncia como
  `miniquad-application`, o nome genérico do framework, e toda regra teria que
  casar isso.
- `wayland_decorations = ServerOnly` — o default do miniquad carrega libdecor e
  desenha decoração do lado do cliente. O Hyprland já decora pelo hyprbars.
- `linux_backend = WaylandWithX11Fallback` — Wayland nativo quando existir.
- Tamanho inicial `940x980`, ajustável por `MMO_WIN=LARGURAxALTURA`.

E a regra, registrada pelo script (também persistida no fim do
`~/.config/hypr/config/windowrules.lua`):

```lua
hl.window_rule({
    name             = "tempest-cliente",
    match            = { class = "^(tempest)$" },
    content          = "none",
    float            = false,
    fullscreen       = false,
    fullscreen_state = 0,
})
```

`content = "none"` é obrigatório: as regras de Gaming no topo do
`windowrules.lua` casam por conteúdo detectado e mandariam a janela pro
`name:gaming` em `fullscreen_state = 2`, cobrindo a tela.

**Não recarregue a config do Hyprland** — o reload derruba o stream do
Moonlight. Pra mexer numa regra em runtime, reregistre com o mesmo `name` via
`hyprctl eval`.

## O que foi testado e NÃO era o problema

Registro pra ninguém repetir a caçada:

| Tentativa | Resultado |
|---|---|
| X11 (Xwayland) | janela com largura cheia |
| Wayland nativo | igual |
| Tilado no workspace especial | o especial some quando outra ws recebe foco |
| `pseudo = true` | centraliza no **monitor**, não no tile (`at=[492,100]`) |
| Flutuante com `size`/`move` em pixels | posiciona certo, mas o terminal volta a ocupar a largura toda |
| `size = { "monitor_w*0.48", ... }` | janela **não mapeada** — some do `hyprctl clients` |
| `wayland_decorations = ServerOnly` | não mudou (mas está certo assim) |
| `request_new_screen_size` após o mapa | não mudou |
| `hl.dsp.layout("togglesplit")` pra forçar relayout | não mudou |
| `swallow_exception_regex` (classe e título) | sem efeito |

## Dispatchers em runtime

Nesta versão do Hyprland os dispatchers viraram Lua sob `hl.dsp`.
`hyprctl dispatch <nome> <args>` com vírgula quebra o parser. O `eval` descarta
o valor de retorno, então pra inspecionar escreva num arquivo:

```sh
hyprctl eval "local f=io.open('/tmp/api.txt','w')
  for k,v in pairs(hl.dsp.window) do f:write(k..'\n') end f:close()"
```

Passar argumento errado de propósito mostra a assinatura:

```
hl.window.move: unrecognized arguments.
  Expected one of: direction, x+y(+relative), workspace, into_group, out_of_group
```

Aviso: os dispatchers de janela agem na **janela ativa** quando não recebem
alvo. `hl.dsp.window.close()` fecha o terminal da sessão.
