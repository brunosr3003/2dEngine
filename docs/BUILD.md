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

## iOS — planejado na Fase 5

### Opção A: `cargo-mobile2`

```sh
cargo install cargo-mobile2
cargo mobile init  # gera projeto Xcode
cargo apple run --release
```

### Opção B: Xcode wrapper manual

Similar ao Android Opção B — `.a` static lib buildada pelo cargo + Xcode
project com SwiftUI bootstrap chamando `ios_main`.

Requer Apple Developer account ($99/ano) para distribuir. TestFlight
para beta.

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
