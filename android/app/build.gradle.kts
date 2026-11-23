import org.gradle.internal.os.OperatingSystem

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

/* ------------------------------------------------------------------ */
/* Rust core (../core) → libkuro_ffi.so + Kotlin bindingy (UniFFI)     */
/* ------------------------------------------------------------------ */

val coreDir = rootProject.file("../core")
val rustJniDir = layout.buildDirectory.dir("rustJniLibs")
val uniffiOutDir = layout.buildDirectory.dir("generated/uniffi")
val abis = listOf("arm64-v8a", "armeabi-v7a", "x86_64")

val rustSources = fileTree(coreDir) {
    include("Cargo.toml", "Cargo.lock", "*/Cargo.toml", "*/src/**", "*/uniffi.toml", "*/main.rs")
    exclude("target/**")
}

android {
    namespace = "app.kuro"
    compileSdk = 36
    ndkVersion = "29.0.14206865"

    defaultConfig {
        applicationId = "app.kuro"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
    }

    splits {
        abi {
            isEnable = true
            reset()
            include(*abis.toTypedArray())
            isUniversalApk = false
        }
    }

    buildTypes {
        debug {
            applicationIdSuffix = ".debug"
        }
        release {
            isMinifyEnabled = false
            // Soukromý projekt: podepsáno debug klíčem, vlastní keystore lze doplnit později.
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    sourceSets["main"].apply {
        java.srcDir(uniffiOutDir)
        jniLibs.srcDir(rustJniDir)
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
        optIn.add("androidx.compose.material3.ExperimentalMaterial3Api")
    }
}

/** Přeloží Rust core pro všechny ABI do build/rustJniLibs/<abi>/libkuro_ffi.so. */
val cargoNdk by tasks.registering(Exec::class) {
    group = "rust"
    description = "cargo ndk build --release -p kuro-ffi"
    inputs.files(rustSources)
    outputs.dir(rustJniDir)
    workingDir = coreDir
    environment("ANDROID_NDK_HOME", android.ndkDirectory.absolutePath)
    commandLine(
        listOf("cargo", "ndk") +
            abis.flatMap { listOf("-t", it) } +
            listOf("-o", rustJniDir.get().asFile.absolutePath, "build", "--release", "-p", "kuro-ffi"),
    )
}

/** Knihovna pro hostitele (Windows/macOS/Linux) — z ní čte UniFFI metadata. */
val cargoHost by tasks.registering(Exec::class) {
    group = "rust"
    description = "cargo build --release -p kuro-ffi (hostitel)"
    inputs.files(rustSources)
    outputs.dir(File(coreDir, "target/release"))
    workingDir = coreDir
    commandLine("cargo", "build", "--release", "-p", "kuro-ffi")
}

/** Vygeneruje Kotlin bindingy z knihovny přeložené pro hostitele. */
val uniffiBindings by tasks.registering(Exec::class) {
    group = "rust"
    description = "uniffi-bindgen generate → build/generated/uniffi"
    dependsOn(cargoHost)
    inputs.files(rustSources)
    outputs.dir(uniffiOutDir)
    workingDir = coreDir
    val os = OperatingSystem.current()
    val hostLib = when {
        os.isWindows -> "kuro_ffi.dll"
        os.isMacOsX -> "libkuro_ffi.dylib"
        else -> "libkuro_ffi.so"
    }
    commandLine(
        "cargo", "run", "--release", "-p", "uniffi-bindgen", "--",
        "generate", "--library", "target/release/$hostLib",
        "--language", "kotlin", "--no-format",
        "--out-dir", uniffiOutDir.get().asFile.absolutePath,
    )
}

tasks.named("preBuild") { dependsOn(cargoNdk, uniffiBindings) }

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.navigation.compose)

    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.compose.material3)
    implementation(libs.compose.material.icons)
    debugImplementation(libs.compose.ui.tooling)

    implementation(libs.work.runtime)
    implementation(libs.datastore.preferences)
    implementation(libs.coil.compose)
    implementation(libs.coil.okhttp)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.mpv)
    implementation(libs.cast.framework)
    implementation(libs.mediarouter)

    // JNA jako AAR — obsahuje nativní libjnidispatch.so pro Android.
    implementation("${libs.jna.get().module}:${libs.jna.get().version}@aar")
}
