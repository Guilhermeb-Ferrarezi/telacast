# Telacast Android: progresso

Plano: `docs/MOBILE-PLAN.md` (raiz do repositório). Este arquivo é o estado da execução: é dele que se retoma.

## Flag

`ANDROID_INSTALL=0`  (nunca rodar `adb install` nem tocar no celular/emulador; só mudar para 1 quando o usuário mandar)

## Passo atual

**A1 (esqueleto + tema): feito, aguardando revisão e commit.** Próximo: **A2 (contrato do protocolo)**.

## Feito

- A1: projeto Gradle (Kotlin DSL) em `android/`, módulo `app` (`app.telacast`, minSdk 26, compile/targetSdk 34).
- Tema Grafite em Compose (`app/src/main/java/app/telacast/tema/`): `Grafite` (cores iguais a `app/src/theme.rs`), `Formas` (raios 8/9/10/12/14), `Tipografia` (Inter e Newsreader variáveis, pesos por `FontVariation.weight`), `GrafiteTheme` (Material3 só como base, mapeado para o Grafite; só escuro).
- Componentes de amostra (`ui/Componentes.kt`): avatar com presença, botão primário/quieto/perigo, campo de texto, selo de não lido, linha de canal.
- `PaletaScreen` (`ui/PaletaScreen.kt`) e teste Paparazzi (`PaletaSnapshotTest`) que gera `docs/paleta.png`.
- Verificado: `assembleDebug` gera o APK; PNG lido e cores conferidas pixel a pixel contra `theme.rs`.

## Bloqueado

Nada.

## Comandos exatos (rodar dentro de `android/`)

Todos passam pela receita de pouca RAM (`scripts/gradle.sh`, que recusa rodar com menos de 3 GB disponíveis e usa `systemd-run` com MemoryMax=2500M, `nice -n 19`, `--no-daemon --max-workers=2`). Um build por vez.

```bash
free -m                                   # precisa de >= 3000 MB em "available"
scripts/gradle.sh assembleDebug           # APK: app/build/outputs/apk/debug/app-debug.apk
scripts/gradle.sh verifyPaparazziDebug    # teste de snapshot: compara com app/src/test/snapshots (falha se a tela mudou)
scripts/paleta.sh                         # regrava o snapshot e copia para docs/paleta.png (rodar quando o tema mudar de propósito)
```

Equivalente sem o script (a receita crua do plano):

```bash
systemd-run --user --scope -q -p MemoryMax=2500M -p MemorySwapMax=0 nice -n 19 env ANDROID_HOME=$HOME/Android/Sdk GRADLE_OPTS=-Xmx512m ~/Android/gradle-8.9/bin/gradle --no-daemon --max-workers=2 assembleDebug
```

Não há `gradlew`: usa-se o Gradle 8.9 de `~/Android/gradle-8.9` (evita baixar outra versão). `local.properties` (sdk.dir) é local e ignorado pelo git.

## Versões fixadas

AGP 8.5.2, Kotlin 2.0.21 (plugin do compilador Compose), Gradle 8.9, JDK 21 (alvo 17), Compose BOM 2024.09.03 (foundation, ui, animation, material3), activity-compose 1.9.2, core-ktx 1.13.1, lifecycle-runtime-ktx 2.8.6, Paparazzi 1.3.5, JUnit 4.13.2.

## Notas para os próximos passos

- Paparazzi reduz o PNG para no máximo 1000 px se `useDeviceResolution` for falso; o teste da paleta liga isso e usa um Pixel 5 com 4700 px de altura (se a paleta crescer, aumentar `screenHeight`). `RenderingMode.V_SCROLL` deu imagem de 151 px de largura com Compose: não usar.
- Fontes variáveis funcionam no layoutlib do Paparazzi (pesos 300 a 700 visivelmente diferentes). Itálico do Newsreader é sintetizado (não há arquivo itálico), igual ao egui.
- Tráfego claro `ws://` no Android 9+ exige exceção só no build debug (A3).
