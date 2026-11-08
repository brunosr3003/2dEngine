plugins {
    id("com.android.application")
}

android {
    namespace = "com.niertech.tempest"
    compileSdk = 36
    ndkVersion = "25.2.9519653"

    signingConfigs {
        getByName("debug") {
            // O keystore de debug deste PC foi criado fora do padrao do AGP
            // (alias "androidebugkey", faltando um "d" do nome canonico
            // "androiddebugkey") — por isso nao da pra usar o signingConfig
            // "debug" automatico do plugin, tem que apontar pra ele na mao.
            storeFile = file(System.getProperty("user.home") + "/.android/debug.keystore")
            storePassword = "android"
            keyAlias = "androidebugkey"
            keyPassword = "android"
        }
        // Chave de release real (a mesma que ja' assinou a build em Teste
        // Fechado no Play Console) — nunca commitada. Vem de
        // ~/.tempest-secrets/keystore.env, lido pelo ambiente que
        // scripts/build-android.sh exporta antes do `./gradlew`. Se as
        // variaveis nao estiverem no ambiente (dev comum, sem --release),
        // esse signingConfig fica incompleto e so' e' usado se alguem
        // pedir a build "release" explicitamente — ver buildTypes abaixo,
        // que cai pro debug nesse caso.
        if (System.getenv("KEYSTORE_PATH") != null) {
            create("release") {
                storeFile = file(System.getenv("KEYSTORE_PATH")!!)
                storePassword = System.getenv("KEYSTORE_PASS")
                keyAlias = System.getenv("KEY_ALIAS")
                keyPassword = System.getenv("KEY_PASS")
            }
        }
    }

    defaultConfig {
        applicationId = "com.niertech.tempest"
        minSdk = 24
        targetSdk = 36
        versionCode = 10
        versionName = "2026110801"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = if (System.getenv("KEYSTORE_PATH") != null) {
                signingConfigs.getByName("release")
            } else {
                signingConfigs.getByName("debug")
            }
        }
    }

    sourceSets {
        getByName("main") {
            // Os .vox/texturas ficam na raiz do repo (lidos do AssetManager
            // em runtime; vox.rs tira o prefixo "assets/" no Android).
            assets.srcDirs("../../../../assets")
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }

    packaging {
        // resources.arsc sem compressao a partir do targetSdk 30 — o
        // zip-align normal do Gradle ja' cuida disso, mantido aqui so' como
        // lembrete do motivo (ver o hack removido de scripts/build-android.sh).
        jniLibs {
            useLegacyPackaging = false
        }
    }
}

dependencies {
    implementation("com.android.billingclient:billing:8.0.0")
}

// --- Ponte com o Rust ---------------------------------------------------
// Builda os dois alvos do client (arm64 = celular, x86_64 = emulador) e
// copia o .so pra dentro do source set de jniLibs ANTES de qualquer build
// do Android. Variaveis de linker/CC pro NDK vem do ambiente (exportadas por
// scripts/build-android.sh); por isso o script roda o gradlew com
// `--no-daemon` — um daemon reaproveitado manteria o ambiente da PRIMEIRA
// invocacao, nao desse build.
val repoRoot = file("../../../..")
val cargoEnv = mapOf(
    "CC_aarch64_linux_android" to System.getenv("CC_aarch64_linux_android"),
    "AR_aarch64_linux_android" to System.getenv("AR_aarch64_linux_android"),
    "CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER" to System.getenv("CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"),
    "CC_x86_64_linux_android" to System.getenv("CC_x86_64_linux_android"),
    "AR_x86_64_linux_android" to System.getenv("AR_x86_64_linux_android"),
    "CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER" to System.getenv("CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER"),
    // Google Play requires native libraries aligned to 16 KB pages (Android
    // 15+ devices with 16 KB pages). NDK r25 links at 4 KB by default and the
    // Play Console refuses the release ("nao e' compativel com tamanhos de
    // pagina de 16 KB").
    "CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS" to "-C link-arg=-Wl,-z,max-page-size=16384",
    "CARGO_TARGET_X86_64_LINUX_ANDROID_RUSTFLAGS" to "-C link-arg=-Wl,-z,max-page-size=16384",
).filterValues { it != null }

tasks.register<Exec>("cargoBuildClientArm64") {
    workingDir = repoRoot
    environment(cargoEnv)
    commandLine("cargo", "build", "--release", "--target", "aarch64-linux-android", "-p", "client")
}

tasks.register<Exec>("cargoBuildClientX86_64") {
    workingDir = repoRoot
    environment(cargoEnv)
    commandLine("cargo", "build", "--release", "--target", "x86_64-linux-android", "-p", "client")
}

tasks.register<Copy>("copyJniLibsArm64") {
    dependsOn("cargoBuildClientArm64")
    from(repoRoot.resolve("target/aarch64-linux-android/release/libclient.so"))
    into("src/main/jniLibs/arm64-v8a")
}

tasks.register<Copy>("copyJniLibsX86_64") {
    dependsOn("cargoBuildClientX86_64")
    from(repoRoot.resolve("target/x86_64-linux-android/release/libclient.so"))
    into("src/main/jniLibs/x86_64")
}

tasks.register("cargoBuildAndroid") {
    dependsOn("copyJniLibsArm64", "copyJniLibsX86_64")
}

tasks.named("preBuild") {
    dependsOn("cargoBuildAndroid")
}
