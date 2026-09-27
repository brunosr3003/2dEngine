#!/usr/bin/env bash
# Build iOS do cliente Rust (macroquad) + .ipa assinado + upload pro TestFlight.
# Roda NO MAC (Xcode + rustup com aarch64-apple-ios).
#
# Nao ha projeto Xcode: o binario do cargo ja' e' o app (a miniquad chama
# UIApplicationMain). O script monta o Tempest.app na mao, compila o icone com
# actool, assina com o profile de App Store e envia com altool.
#
# Credenciais NUNCA ficam aqui: le de $TEMPEST_ENV (padrao ~/MMORPG/.env):
#   APPLE_TEAM_ID, APP_BUNDLE_ID, ASC_API_KEY_ID, ASC_API_ISSUER_ID
# e a chave em ~/.appstoreconnect/private_keys/AuthKey_<KEY_ID>.p8.
#
# Uso:
#   scripts/build-ios.sh                 build + ipa + upload
#   scripts/build-ios.sh --skip-upload   so' gera o .ipa
#   scripts/build-ios.sh --iphone        instala DIRETO no iPhone pareado
#   scripts/build-ios.sh --version 1.1 --build 2609141530
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
python3 scripts/check-release-notes.py
export PATH="$HOME/.cargo/bin:$PATH"

ENV_FILE="${TEMPEST_ENV:-$HOME/MMORPG/.env}"
[ -f "$ENV_FILE" ] || { echo "ERRO: $ENV_FILE nao existe"; exit 1; }
# shellcheck disable=SC1090
source "$ENV_FILE"

VERSAO="1.1"
BUILD="$(date -u +%y%m%d%H%M)"
SKIP_UPLOAD=0
IPHONE=0
while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSAO="$2"; shift 2;;
    --build) BUILD="$2"; shift 2;;
    --skip-upload) SKIP_UPLOAD=1; shift;;
    --iphone) IPHONE=1; SKIP_UPLOAD=1; shift;;
    *) echo "flag desconhecida: $1"; exit 2;;
  esac
done

BUNDLE="${APP_BUNDLE_ID:-com.brunji.tempest}"
TEAM="${APPLE_TEAM_ID:?APPLE_TEAM_ID}"
MIN_OS="15.0"
SAIDA="$ROOT/target/ios"
APP="$SAIDA/Payload/Tempest.app"

echo "==> Tempest iOS $VERSAO ($BUILD) — $BUNDLE"

# ── 1. binario ────────────────────────────────────────────────────────────────
rustup target add aarch64-apple-ios >/dev/null
IPHONEOS_DEPLOYMENT_TARGET="$MIN_OS" cargo build --release --target aarch64-apple-ios --bin client

rm -rf "$SAIDA"
mkdir -p "$APP"
cp "$ROOT/target/aarch64-apple-ios/release/client" "$APP/Tempest"

# ── 2. assets lidos do disco em runtime (o resto e' include_bytes) ────────────
mkdir -p "$APP/assets"
cp -R "$ROOT/assets/vox" "$APP/assets/vox"

# ── 3. icone (actool gera Assets.car + as chaves CFBundleIcons) ───────────────
ICONE="$ROOT/assets/ios/AppIcon-1024.png"
[ -f "$ICONE" ] || { echo "ERRO: falta $ICONE (python3 tools/ios/gerar_icone.py)"; exit 1; }
XC="$SAIDA/Assets.xcassets"
mkdir -p "$XC/AppIcon.appiconset"
cp "$ICONE" "$XC/AppIcon.appiconset/icon-1024.png"
cat > "$XC/Contents.json" <<'EOF'
{ "info" : { "author" : "xcode", "version" : 1 } }
EOF
cat > "$XC/AppIcon.appiconset/Contents.json" <<'EOF'
{
  "images" : [
    { "filename" : "icon-1024.png", "idiom" : "universal", "platform" : "ios", "size" : "1024x1024" }
  ],
  "info" : { "author" : "xcode", "version" : 1 }
}
EOF
xcrun actool "$XC" --compile "$APP" --platform iphoneos \
  --minimum-deployment-target "$MIN_OS" --app-icon AppIcon \
  --target-device iphone --target-device ipad \
  --output-partial-info-plist "$SAIDA/icone-partial.plist" >/dev/null

# ── 4. Info.plist ─────────────────────────────────────────────────────────────
SDK_VER="$(xcrun --sdk iphoneos --show-sdk-version)"
SDK_BUILD="$(xcrun --sdk iphoneos --show-sdk-build-version)"
XCODE_VER="$(xcodebuild -version | awk '/Xcode/{print $2}' | awk -F. '{printf "%d%d%d", $1, ($2==""?0:$2), ($3==""?0:$3)}')"
XCODE_BUILD="$(xcodebuild -version | awk '/Build version/{print $3}')"
OS_BUILD="$(sw_vers -buildVersion)"
PL="$APP/Info.plist"
cat > "$PL" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>pt_BR</string>
  <key>CFBundleExecutable</key><string>Tempest</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>Tempest</string>
  <key>CFBundleDisplayName</key><string>Tempest</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSAO</string>
  <key>CFBundleVersion</key><string>$BUILD</string>
  <key>CFBundleSupportedPlatforms</key><array><string>iPhoneOS</string></array>
  <key>LSRequiresIPhoneOS</key><true/>
  <key>MinimumOSVersion</key><string>$MIN_OS</string>
  <key>UIDeviceFamily</key><array><integer>1</integer><integer>2</integer></array>
  <key>UIRequiredDeviceCapabilities</key><array><string>arm64</string></array>
  <key>UIRequiresFullScreen</key><true/>
  <key>UIStatusBarHidden</key><true/>
  <key>UILaunchScreen</key><dict/>
  <key>UISupportedInterfaceOrientations</key>
  <array><string>UIInterfaceOrientationLandscapeLeft</string><string>UIInterfaceOrientationLandscapeRight</string></array>
  <key>UISupportedInterfaceOrientations~ipad</key>
  <array><string>UIInterfaceOrientationLandscapeLeft</string><string>UIInterfaceOrientationLandscapeRight</string></array>
  <key>ITSAppUsesNonExemptEncryption</key><false/>
  <key>DTPlatformName</key><string>iphoneos</string>
  <key>DTPlatformVersion</key><string>$SDK_VER</string>
  <key>DTSDKName</key><string>iphoneos$SDK_VER</string>
  <key>DTSDKBuild</key><string>$SDK_BUILD</string>
  <key>DTPlatformBuild</key><string>$SDK_BUILD</string>
  <key>DTXcode</key><string>$XCODE_VER</string>
  <key>DTXcodeBuild</key><string>$XCODE_BUILD</string>
  <key>DTCompiler</key><string>com.apple.compilers.llvm.clang.1_0</string>
  <key>BuildMachineOSBuild</key><string>$OS_BUILD</string>
</dict>
</plist>
EOF
/usr/libexec/PlistBuddy -c "Merge $SAIDA/icone-partial.plist" "$PL" >/dev/null
plutil -lint "$PL" >/dev/null

# ── 5. profile de App Store + assinatura ──────────────────────────────────────
PROFILE=""
for f in "$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles/"*.mobileprovision \
         "$HOME/Library/MobileDevice/Provisioning Profiles/"*.mobileprovision; do
  [ -f "$f" ] || continue
  P="$(security cms -D -i "$f" 2>/dev/null)" || continue
  ID="$(echo "$P" | plutil -extract Entitlements.application-identifier raw -o - - 2>/dev/null || true)"
  GTA="$(echo "$P" | plutil -extract Entitlements.get-task-allow raw -o - - 2>/dev/null || true)"
  DEV="$(echo "$P" | plutil -extract ProvisionedDevices raw -o - - 2>/dev/null || true)"
  if [ "$ID" = "$TEAM.$BUNDLE" ] && [ "$GTA" = "false" ] && [ -z "$DEV" ]; then
    PROFILE="$f"; echo "$P" | plutil -extract Entitlements xml1 -o "$SAIDA/entitlements.plist" -
    break
  fi
done
[ -n "$PROFILE" ] || { echo "ERRO: profile de App Store pra $TEAM.$BUNDLE nao encontrado"; exit 1; }
cp "$PROFILE" "$APP/embedded.mobileprovision"

# Por SSH o keychain de login fica TRANCADO ("User interaction is not
# allowed") e o codesign falha com errSecInternalComponent. Rodando no
# Terminal do proprio Mac (sessao grafica) nao precisa de nada. Por SSH, a
# senha do Mac vem de $KEYCHAIN_PASSWORD ou de ~/.tempest-keychain-pass (fora
# do repo, chmod 600).
KC="$HOME/Library/Keychains/login.keychain-db"
if [ -z "${KEYCHAIN_PASSWORD:-}" ] && [ -f "$HOME/.tempest-keychain-pass" ]; then
  KEYCHAIN_PASSWORD="$(cat "$HOME/.tempest-keychain-pass")"
fi
if [ -n "${KEYCHAIN_PASSWORD:-}" ]; then
  security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KC"
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KEYCHAIN_PASSWORD" "$KC" >/dev/null
fi

IDENT="$(security find-identity -v -p codesigning | awk -v t="($TEAM)" '/Apple Distribution/ && index($0,t){print $2; exit}')"
[ -n "$IDENT" ] || { echo "ERRO: identidade Apple Distribution do time $TEAM nao encontrada"; exit 1; }
# A instalacao direta sera' reassinada com o profile de desenvolvimento mais
# abaixo. O servidor de timestamp da Apple pode estar indisponivel; nesse
# caminho a assinatura intermediaria nao depende dele.
if [ "$IPHONE" -eq 1 ]; then
  codesign --force --timestamp=none --sign "$IDENT" --entitlements "$SAIDA/entitlements.plist" "$APP"
else
  codesign --force --timestamp --sign "$IDENT" --entitlements "$SAIDA/entitlements.plist" "$APP" ||
    codesign --force --timestamp=none --sign "$IDENT" --entitlements "$SAIDA/entitlements.plist" "$APP"
fi
codesign --verify --deep --strict "$APP"

# ── 6. ipa ────────────────────────────────────────────────────────────────────
IPA="$SAIDA/Tempest-$VERSAO-$BUILD.ipa"
(cd "$SAIDA" && zip -qry "$IPA" Payload)
echo "==> IPA: $IPA ($(du -h "$IPA" | cut -f1))"

# ── 7. upload ─────────────────────────────────────────────────────────────────
if [ "$SKIP_UPLOAD" -eq 0 ]; then
  : "${ASC_API_KEY_ID:?ASC_API_KEY_ID}" "${ASC_API_ISSUER_ID:?ASC_API_ISSUER_ID}"
  echo "==> upload TestFlight..."
  xcrun altool --upload-app --type ios --file "$IPA" \
    --apiKey "$ASC_API_KEY_ID" --apiIssuer "$ASC_API_ISSUER_ID"
fi
# ── 8. instalar direto no iPhone ──────────────────────────────────────────────
#
# SEM TESTFLIGHT. O limite diario da Apple (erro 90382) barrava o envio, e o
# dono perguntou se nao dava pra mandar por AirDrop: nao da', o iOS nao instala
# .ipa solto. O que da' e' instalar pelo aparelho PAREADO, que aqui alcanca
# pelo Tailscale.
#
# O app tem que ser REASSINADO: o build normal usa profile de App Store, e o
# iOS recusa com "Attempted to install a Beta profile without the proper
# entitlement". Precisa de profile de DESENVOLVIMENTO (get-task-allow=true,
# com o aparelho registrado) e da identidade Apple Development.
#
# A copia vai pra DevPayload/ e o Payload/ original fica intacto — e' dele que
# sai o .ipa do TestFlight.
if [ "$IPHONE" -eq 1 ]; then
  echo "==> reassinando pra desenvolvimento..."
  DEV_PROF=""
  for f in "$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles/"*.mobileprovision \
           "$HOME/Library/MobileDevice/Provisioning Profiles/"*.mobileprovision; do
    [ -f "$f" ] || continue
    P="$(security cms -D -i "$f" 2>/dev/null)" || continue
    ID="$(echo "$P" | plutil -extract Entitlements.application-identifier raw -o - - 2>/dev/null || true)"
    GTA="$(echo "$P" | plutil -extract Entitlements.get-task-allow raw -o - - 2>/dev/null || true)"
    if [ "$ID" = "$TEAM.$BUNDLE" ] && [ "$GTA" = "true" ]; then DEV_PROF="$f"; break; fi
  done
  [ -n "$DEV_PROF" ] || { echo "ERRO: profile de DESENVOLVIMENTO pra $TEAM.$BUNDLE nao encontrado"; exit 1; }

  rm -rf "$SAIDA/DevPayload" && mkdir -p "$SAIDA/DevPayload"
  cp -R "$APP" "$SAIDA/DevPayload/"
  cp "$DEV_PROF" "$SAIDA/DevPayload/Tempest.app/embedded.mobileprovision"
  security cms -D -i "$DEV_PROF" | plutil -extract Entitlements xml1 -o "$SAIDA/dev-entitlements.plist" -
  DEV_IDENT="$(security find-identity -v -p codesigning | awk '/Apple Development/{print $2; exit}')"
  [ -n "$DEV_IDENT" ] || { echo "ERRO: identidade Apple Development nao encontrada"; exit 1; }
  codesign --force --timestamp=none --sign "$DEV_IDENT" \
    --entitlements "$SAIDA/dev-entitlements.plist" "$SAIDA/DevPayload/Tempest.app"
  codesign --verify --strict "$SAIDA/DevPayload/Tempest.app"

  # O aparelho: o primeiro pareado. Com mais de um, passe DEVICE_ID no ambiente.
  DEV_ID="${DEVICE_ID:-$(xcrun devicectl list devices 2>/dev/null | awk '/available \(paired\)/{print $3; exit}')}"
  [ -n "$DEV_ID" ] || { echo "ERRO: nenhum aparelho pareado (destranque o iPhone e tente de novo)"; exit 1; }
  echo "==> instalando em $DEV_ID..."
  # O iPhone precisa estar DESTRANCADO: travado, o mount da imagem de
  # desenvolvedor falha com kAMDMobileImageMounterDeviceLocked.
  xcrun devicectl device install app --device "$DEV_ID" "$SAIDA/DevPayload/Tempest.app"
fi
echo "==> pronto: Tempest $VERSAO ($BUILD)"
