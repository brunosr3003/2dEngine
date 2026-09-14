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

## Android — planejado na Fase 5

### Opção A: `cargo-apk` (simples)

```sh
cargo install cargo-apk
cargo apk run -p client --target aarch64-linux-android
```

Requer Android NDK instalado e `ANDROID_NDK_HOME` setado.

### Opção B: projeto Gradle + NativeActivity (produção)

- Criar módulo Android Gradle com `app/build.gradle`.
- `app/src/main/jniLibs/{arm64-v8a,armeabi-v7a,x86_64}/libclient.so` —
  artefato do `cargo build --target aarch64-linux-android`.
- `AndroidManifest.xml` com NativeActivity pointing to `android_main`.
- Mais trabalho, mas permite UI nativa (splash, login, configurações) +
  Google Play.

**Pendências:**
- Adicionar `[lib] crate-type = ["cdylib"]` ao `crates/client`.
- `android_main` entry point no `client`.
- Input de touch no `engine::input` (`WindowEvent::Touch`).

---

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
a partir de `assets/ios/AppIcon-1024.png` (`python3 tools/ios/gerar_icone.py`),
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
