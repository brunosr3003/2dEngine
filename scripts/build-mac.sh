#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 scripts/check-release-notes.py
[[ $(uname -s) == Darwin ]] || { echo 'Execute este script no Mac.' >&2; exit 1; }
export PATH="$HOME/.cargo/bin:$PATH"
export MMO_API_PADRAO="${MMO_API_PADRAO:-mmo.brunji.com.br:80}"
cargo build --release --bin client
out="$PWD/target/mac"
app="$out/Tempest.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/client "$app/Contents/MacOS/Tempest"
ditto assets "$app/Contents/Resources/assets"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Tempest</string>
<key>CFBundleIdentifier</key><string>com.brunji.tempest</string>
<key>CFBundleName</key><string>Tempest</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>1.1</string>
<key>CFBundleVersion</key><string>147</string>
<key>NSHighResolutionCapable</key><true/>
<key>CFBundleIconFile</key><string>Tempest</string>
</dict></plist>
PLIST
icons="$out/Tempest.iconset"
mkdir -p "$icons"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" assets/ios/AppIcon-1024.png --out "$icons/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" assets/ios/AppIcon-1024.png --out "$icons/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$icons" -o "$app/Contents/Resources/Tempest.icns"

# ASSINATURA E NOTARIZACAO (docs/RELEASE_MAC.md)
#
# O que o dono via: "everytime i download a new version in macos i need to run
# a command with xattr". Isso e' o Gatekeeper. Todo arquivo que um NAVEGADOR
# baixa recebe o atributo `com.apple.quarantine`, e app em quarentena que nao
# esta' NOTARIZADO pela Apple nao abre — no macOS Sequoia nem pelo antigo
# botao direito > Abrir, so' por Ajustes > Privacidade e Seguranca. `xattr -cr`
# apaga o atributo, o que resolve na mao e volta na proxima baixada.
#
# Assinatura ad-hoc (`--sign -`), que era o que havia aqui, NAO resolve: ela
# faz o binario rodar no Apple Silicon, e nada mais. Quem tira a quarentena do
# caminho e' o TICKET de notarizacao grampeado no .app.
#
# Entao: se existir uma identidade "Developer ID Application" no login
# keychain, assina com ela + hardened runtime, manda pra Apple, espera o
# veredito e grampeia o ticket. Sem a identidade, cai no ad-hoc de antes e
# AVISA — build sem notarizacao e' build que vai pedir `xattr` do outro lado.
id_devid="$(security find-identity -v -p codesigning \
    | sed -n 's/.*"\(Developer ID Application:.*\)"/\1/p' | head -1 || true)"
zip="$out/Tempest-Mac-$(uname -m).zip"

if [[ -n "$id_devid" ]]; then
    echo "Assinando com: $id_devid"
    # Por SSH o keychain de login fica TRANCADO e o codesign morre com
    # `errSecInternalComponent` — o mesmo que o build-ios.sh ja' resolvia. Duas
    # coisas: destrancar, e a `set-key-partition-list`, que e' a que autoriza o
    # codesign a USAR a chave sem caixinha de senha. `security import -T` sozinho
    # nao basta em macOS moderno.
    KC="$HOME/Library/Keychains/login.keychain-db"
    if [ -z "${KEYCHAIN_PASSWORD:-}" ] && [ -f "$HOME/.tempest-keychain-pass" ]; then
        KEYCHAIN_PASSWORD="$(cat "$HOME/.tempest-keychain-pass")"
    fi
    if [ -n "${KEYCHAIN_PASSWORD:-}" ]; then
        security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KC"
        security set-key-partition-list -S apple-tool:,apple:,codesign: \
            -s -k "$KEYCHAIN_PASSWORD" "$KC" >/dev/null
    fi
    # `--options runtime` (hardened runtime) e' EXIGIDO pra notarizar, e
    # `--timestamp` tambem: sem carimbo de tempo a Apple recusa.
    codesign --force --deep --timestamp --options runtime \
        --sign "$id_devid" "$app"
    codesign --verify --deep --strict --verbose=2 "$app"
    plutil -lint "$app/Contents/Info.plist"
    if [[ "${TEMPEST_SKIP_NOTARIZATION:-}" == 1 ]]; then
        # Escape hatch for when Apple's notary service refuses the account
        # (e.g. an expired developer agreement): ship signed but NOT
        # notarized. Browser downloads will need System Settings > Privacy &
        # Security > Open Anyway, or `xattr -cr`, on the player's side.
        echo 'WARNING: TEMPEST_SKIP_NOTARIZATION=1 — signed with Developer ID, NOT notarized.' >&2
    else
        # A Apple recebe um zip; o ticket volta pro .app, nao pro zip — por isso
        # o zip final e' feito DEPOIS do staple.
        ditto -c -k --sequesterRsrc --keepParent "$app" "$out/notarizar.zip"
        ENV_FILE="${TEMPEST_ENV:-$HOME/MMORPG/.env}"
        # shellcheck disable=SC1090
        [ -f "$ENV_FILE" ] && source "$ENV_FILE"
        chave="$HOME/.appstoreconnect/private_keys/AuthKey_${ASC_API_KEY_ID:-}.p8"
        if [[ -f "$chave" ]]; then
            xcrun notarytool submit "$out/notarizar.zip" \
                --key "$chave" --key-id "$ASC_API_KEY_ID" \
                --issuer "$ASC_API_ISSUER_ID" --wait
        else
            xcrun notarytool submit "$out/notarizar.zip" \
                --apple-id "$APPLE_ID" --password "$APPLE_APP_PASSWORD" \
                --team-id "$APPLE_TEAM_ID" --wait
        fi
        xcrun stapler staple "$app"
        rm -f "$out/notarizar.zip"
        # A prova real: e' isto que o Gatekeeper responde na maquina de quem baixa.
        spctl --assess --type execute --verbose=2 "$app"
        echo "NOTARIZADO e grampeado — abre sem xattr e sem aviso."
    fi
else
    codesign --force --deep --sign - "$app"
    codesign --verify --deep --strict "$app"
    plutil -lint "$app/Contents/Info.plist"
    echo 'AVISO: sem identidade "Developer ID Application" no keychain.' >&2
    echo 'Build ad-hoc: quem baixar pelo navegador vai precisar de xattr -cr.' >&2
    echo 'Ver docs/RELEASE_MAC.md pra criar o certificado (uma vez so).' >&2
fi

ditto -c -k --sequesterRsrc --keepParent "$app" "$zip"
echo "Build pronta: $app"
