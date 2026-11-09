# Build

## Desktop (Windows, macOS, Linux)

Pré-requisitos:
- Rust 1.82+ (instalado via `rustup`).
- Dependências nativas:
  - **Linux:** `libx11-dev libxrandr-dev libxi-dev libgl1-mesa-dev libasound2-dev`
    (Ubuntu/Debian) — mais os equivalentes para seu distro.
  - **macOS:** Xcode CLI tools (`xcode-select --install`).
  - **Windows:** Visual Studio Build Tools ou instalar com MSVC toolchain.

Compilar tudo:

```sh
cargo build --workspace
```

Rodar cliente e servidor (terminais separados):

```sh
cargo run --bin server
cargo run --bin client
```

Release (otimizado):

```sh
cargo build --release --workspace
```

Os binários ficam em `target/release/{client,server}`.

---

## Web (wasm) — planejado na Fase 5

Pré-requisitos:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

Build:

```sh
cd crates/client
wasm-pack build --target web --out-dir ../../web/pkg
```

Servir:

```sh
python3 -m http.server 8080 --directory web
# abrir http://localhost:8080
```

**Pendências:**
- `net_client.rs` precisa de impl `cfg(target_family = "wasm")` usando
  `web_sys::WebSocket`.
- Página HTML com `<canvas>` e bootstrap do wasm.
- wgpu em WebGPU requer navegador moderno; fallback WebGL2 via feature flag.

---

## Android (APK)

Roda neste PC, sem root: JDK 17 em `~/opt/jdk-17`, SDK em `~/Android/Sdk`
(NDK 25.2, plataformas 34 e 36) e o AVD `tempest` (Pixel 6, x86_64, GPU do host).

```bash
scripts/android-sdk-setup.sh           # uma vez: JDK + SDK + NDK + emulador + AVD (~6 GB)
scripts/run-android-emulator.sh --instalar   # sobe o emulador ao lado do terminal e abre o app
scripts/build-android.sh --abrir       # build + install + abre no aparelho/emulador conectado
```

**Build via Gradle** (migrado de `cargo-quad-apk` em 03/11/2026, pra' poder
usar a Play Billing Library — ver docs/LOJA.md): o projeto Android de verdade
fica em `crates/client/android/` (módulo `app`, `build.gradle.kts`,
`gradlew`). `scripts/build-android.sh` so' exporta as variáveis do NDK (linker
e `CC_*`/`AR_*` por alvo, lidas pelas tasks `cargoBuildClientArm64`/
`cargoBuildClientX86_64` do `app/build.gradle.kts`) e chama
`./gradlew --no-daemon assembleDebug` — **sempre `--no-daemon`**: um daemon
reaproveitado manteria o ambiente da primeira chamada, não da atual. O APK
sai em `crates/client/android/app/build/outputs/apk/debug/app-debug.apk`,
com `arm64-v8a` (celular) e `x86_64` (emulador), assinado com a chave de
debug (keystore deste PC com alias `androidebugkey`, apontado na mão em
`signingConfigs.debug` do `app/build.gradle.kts` porque não é o alias padrão
`androiddebugkey` que o plugin espera).

Config principal: `crates/client/android/app/build.gradle.kts` (pacote
`com.niertech.tempest`, compileSdk/targetSdk 36, minSdk 24, Billing Library
7.1.1) e `AndroidManifest.xml` escrito à mão (paisagem, `INTERNET`, e a
`<queries>` da Play Store que a Billing Library precisa pra bindar o
serviço). O `[package.metadata.android]` do `crates/client/Cargo.toml`
**não é mais lido por nada** — é o Gradle quem manda agora; ficou só como
histórico de quais flags existiam (ícone, orientação, SDK mínima).

O `cdylib` sai do MESMO `src/main.rs` do binário desktop: o crate ganhou um
`[lib]` com `crate-type = ["cdylib", "rlib"]` apontando pro mesmo path do
`[[bin]]`. O Android não roda um executável — carrega uma lib compartilhada
via JNI (`MainActivity.java` chama `System.loadLibrary("client")`) — e os
símbolos `Java_quad_1native_...` que a Activity invoca vêm do miniquad e do
`crates/client/src/billing_android.rs`, exportados automaticamente por
qualquer cdylib que inclua essas dependências (não tem nada de especial no
`main.rs` em si). Conferir com
`llvm-nm -D libclient.so | grep Java_` depois de qualquer mudança grande no
grafo de dependências do cliente.

Armadilhas já resolvidas (ver comentários nos arquivos):

- Android 12+ exige `android:exported` na activity.
- `.vox` no Android vêm do AssetManager, cuja raiz já é `assets/`: o `vox.rs`
  tira esse prefixo.
- `cargo rustc --crate-type cdylib` sozinho dá "cannot mix `bin` crate type
  with others" num crate que só tem `[[bin]]` — por isso o `[lib]` separado,
  em vez de tentar forçar o crate-type na hora de buildar.
- O keystore de debug deste PC tem o alias com um "d" faltando
  (`androidebugkey`); o `signingConfigs.debug` do Gradle aponta pra ele na
  mão em vez de usar o default do plugin.
- **O AVD nasce em paisagem** (`hw.initialOrientation = landscape`), porque o
  jogo é paisagem. Se nascesse em pé, a janela mostraria o quadro deitado
  dentro do quadro retrato e o emulador mapearia o toque pela geometria
  retrato — **o clique do mouse não acerta nada**, enquanto `adb shell input
  tap` continua funcionando (ele entra direto no Android, sem passar pela
  janela). Essa diferença é o diagnóstico: **se o `tap` funciona e o mouse
  não, é a janela**.
- **Não use a tela cheia do emulador, e evite `adb emu rotate`.** Cada mudança
  de tamanho/orientação faz o emulador **recentrar** a janela, e ela vai parar
  fora da tela (y negativo). De lá não há volta: mover a janela de fora não
  funciona (`movewindowpixel` e regra de `move` não surtem efeito), e a
  posição do `emulator-user.ini` é aplicada antes da rotação. A recuperação é
  reiniciar o emulador. O script ainda tem a rede de segurança que gira até as
  orientações casarem, mas com o AVD em paisagem ela não dispara.
- A janela também **não pode ser tilada**: o Hyprland a redimensiona depois do
  mapa e o emulador continua mapeando o toque pelo tamanho que ele escolheu. A
  regra de janela tem que existir **antes do mapa** (float/resize em runtime
  devolvem "ok" e não mudam nada), então o script a registra por `hyprctl eval`
  — e não no arquivo de config, porque recarregar a config derruba o stream do
  Moonlight. Mesma armadilha da janela do QEMU.

No celular o cliente vai direto no web de produção (`api.rs`), igual ao iOS.
Ainda falta no Android: abrir URL (login Google), tela sempre acesa e área
segura (`nativo.rs`), e assinatura de release pra Play Store.

## iOS (TestFlight)

Sem projeto Xcode: o binário do cargo já é o app (a miniquad chama
`UIApplicationMain`). No Mac:

```sh
rustup target add aarch64-apple-ios   # uma vez
scripts/build-ios.sh                  # build + .ipa + upload pro TestFlight
scripts/build-ios.sh --skip-upload    # só o .ipa em target/ios/
```

O script compila `--target aarch64-apple-ios` (deployment target 15.0), monta
`target/ios/Payload/Tempest.app` com `assets/vox` (o único asset lido do disco;
fontes e ícones são `include_bytes!`), gera o `Assets.car` do ícone com `actool`
a partir de `assets/ios/AppIcon-1024.png` (`python3 tools/icone_app/gerar.py`,
que gera TODOS os ícones — iOS, Mac, Android, Windows e o da janela — a partir
do ícone da Google Play em `assets/icone/tempest-loja-512.png`),
escreve o `Info.plist` (paisagem, tela cheia, `com.brunji.tempest`, versão
`1.1`, build = `aammddHHMM` UTC), acha o profile de App Store do bundle,
assina com a identidade "Apple Distribution" do time e envia com `altool`.

Credenciais ficam fora do repo: `~/MMORPG/.env` (`APPLE_TEAM_ID`,
`APP_BUNDLE_ID`, `ASC_API_KEY_ID`, `ASC_API_ISSUER_ID`) e a chave em
`~/.appstoreconnect/private_keys/`.

**Assinar por SSH:** o keychain de login fica trancado numa sessão SSH e o
`codesign` falha com `errSecInternalComponent` (o `xcodebuild -exportArchive`
com a API key também assina localmente e falha igual). Rodando no Terminal
do próprio Mac funciona sem nada. Por SSH, o script destranca o keychain se
achar a senha do Mac em `$KEYCHAIN_PASSWORD` ou em `~/.tempest-keychain-pass`
(fora do repo, `chmod 600`).

No iOS o cliente usa o web de produção `mmo.brunji.com.br:80` (sem variável
de ambiente) e abre em tela cheia com `high_dpi`.

---

## Servidor em produção

### Docker (esboço)

```dockerfile
FROM rust:1.82-alpine AS builder
WORKDIR /app
COPY . .
RUN apk add --no-cache musl-dev
RUN cargo build --release --bin server

FROM alpine:3.20
COPY --from=builder /app/target/release/server /usr/local/bin/server
EXPOSE 9000
ENV BIND_ADDR=0.0.0.0:9000
CMD ["server"]
```

### systemd (VPS single-host)

```ini
[Unit]
Description=2dEngine MMO server
After=network.target

[Service]
Type=simple
User=mmo
Environment=BIND_ADDR=0.0.0.0:9000
Environment=RUST_LOG=info
ExecStart=/opt/mmo/server
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

### Proxy + TLS

WebSocket em produção precisa de TLS (`wss://`). Colocar Caddy ou Nginx
na frente:

```caddy
game.seudominio.com {
    reverse_proxy /ws/* localhost:9000
}
```

Cliente conecta em `wss://game.seudominio.com/ws/`.

### Android: fechamento após login no shader da água (26/09/2026)

O driver do aparelho reportou `shader da agua (gpu): CompilationError`,
`Typename expected, found 'theta'`. A macro `ONDA` usava continuação de
linha, recusada em GLSL ES 1.00 pelo compilador do dispositivo. O erro também
foi reproduzido com `glslangValidator`. Foi substituída por uma função GLSL,
conservando as três ondas, suas amplitudes e a atenuação na costa.

Validação dos shaders reais (vertex + fragment e ligação):
`python3 scripts/check-water-shaders.py` (requer `glslangValidator`).
Testes da geometria: `cargo test -p client agua::`.
A prévia Android com `MMO_PREVIA_PERSONAGENS=1 scripts/build-android.sh`
exercita a cena de seleção com água sem login; gere novamente sem essa
variável antes de distribuir o APK normal.

### Validação de todos os shaders Android

`python3 scripts/check-shaders.py` descobre os shaders GLSL do cliente,
inclusive os inline, e compila/liga cada par vertex/fragment. A geração do
APK chama essa verificação antes de compilar Rust e para se houver erro.
Requer `glslangValidator` no PATH. A revisão de 26/09 validou os três pares
do cliente (mundo/sombras, água e efeitos/auras) e os dois pares internos da
macroquad 0.4.16. Isso detecta erros GLSL; não substitui testes em cada driver.

### Patch notes obrigatórios nas builds do cliente

Antes de distribuir uma mudança, escreva a data e as novidades para o jogador
em `docs/PATCHNOTES.txt`. Esse texto é o painel "What's New in Tempest": **sempre em inglês**, nunca
em português, e genérico para o jogador (uma entrada por dia de atualização, com
tópicos curtos: região nova, boss novo, dungeon nova, sistema novo, balanceamento,
correções). O detalhe técnico vai em `docs/CHANGELOG.md`, em português, para o time. Depois rode `python3 scripts/check-release-notes.py --seal`
e versione o texto e `docs/patchnotes-release.json` junto com o código.
Os scripts Android, iOS, Mac e Windows bloqueiam o empacotamento se o cliente
ou os assets mudaram sem notas atualizadas. Rebuild do mesmo código reutiliza
as mesmas notas. O texto fica embutido no app e pode ser reaberto nas telas
de servidor e login pelo botão **Novidades**.

Windows: `bash scripts/build-windows.sh` gera `target/windows/MMORPG-Windows.zip`
com executável x64 e assets. Requer o target Rust `x86_64-pc-windows-gnu` e MinGW.
O script aceita MinGW no PATH ou em `~/.local/share/tempest-mingw/usr/bin`.

A página pública de download está em `web/download-site/`; ela não precisa de
bundler e usa `/download-assets/` para estilos e imagens. Os links apontam aos
pacotes em `/downloads/`. A conta/cadastro anterior deve ser preservada como
`conta.html` ao atualizar a página inicial em produção.

Linux: `bash scripts/build-linux-client.sh` empacota o cliente x86_64 com
assets em `target/linux/MMORPG-Linux.zip`, apontando para a API de produção.

### Aviso de cliente desatualizado

`crates/client/src/atualizacao.rs::BUILD` é o número crescente da release,
independente do protocolo. Incremente antes de selar as notas. O cliente
consulta `/api/client-release` nas telas de servidor/login, inclusive antes
do login automático. Uma falha na consulta não impede entrar.

O endpoint lê `WEB_STATIC/releases.json`. Publique esse mesmo JSON em
`/downloads/releases.json` no site, somente depois dos pacotes. Dentro de
`platforms`, cada plataforma (`windows`, `linux`, `mac`, `android`, `ios`)
informa `build` e `update_url`, além de arquivo, tamanho e SHA-256 quando há
download. Não avance `ios` antes de a build estar disponível no TestFlight.
Desktop abre o site; mobile usa o endereço publicado. O TestFlight por
convite abre com `itms-beta://`; Android ainda distribuído por APK usa o site.
