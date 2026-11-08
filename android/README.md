# Telacast para Android

Cliente Android do Telacast (Kotlin + Jetpack Compose, tema Grafite). Fala com o mesmo servidor do desktop (`docs/PROTOCOL.md`).
Requisitos do aparelho: **Android 8.0 ou mais novo** e processador **arm64** (quase todo celular de 2017 em diante). Aparelhos de
32 bits não são suportados (a ponte do supressor de ruído e o pacote só têm arm64).

## Compilar

Sempre pelo script (ele limita a memória do Gradle; recusa rodar com menos de 3 GB livres):

```bash
scripts/gradle.sh assembleDebug      # APK de teste: app/build/outputs/apk/debug/app-debug.apk
scripts/gradle.sh assembleRelease    # APK final assinado: app/build/outputs/apk/release/telacast-<versão>-arm64.apk
scripts/gradle.sh testDebugUnitTest verifyPaparazziDebug   # testes e imagens
scripts/it.sh                        # testes de integração (sobe um servidor real num banco descartável)
scripts/telas.sh                     # regrava as imagens das telas em docs/telas/
```

A versão mora em **um lugar só**: `version.properties` (`versionName` e `versionCode`). Suba o `versionCode` a cada versão publicada.

## A chave de assinatura (FAÇA BACKUP)

O release é assinado com `~/.config/telacast/android-release.jks` (alias `telacast`, RSA 4096, validade de 30 anos); a senha fica em
`~/.config/telacast/android-signing.properties` (modo 600). **Nada disso vai para o repositório.** O Gradle também aceita as variáveis
`TELACAST_KEYSTORE`, `TELACAST_KEYSTORE_PASSWORD`, `TELACAST_KEY_ALIAS`, `TELACAST_KEY_PASSWORD`. Sem chave nenhuma o `assembleRelease`
ainda roda, mas o APK sai como `telacast-<versão>-arm64-sem-assinatura.apk` (não instala no aparelho).

**Guarde uma cópia do `.jks` e do `.properties` em outro lugar (pendrive, cofre de senhas).** Quem perde a chave nunca mais consegue publicar
uma atualização que o Android aceite por cima da versão instalada: o amigo teria de desinstalar o app (e perder o login) para instalar outra.

## Publicar uma versão (e o amigo se atualiza sozinho)

1. Uma vez, gere a chave do servidor: `telacast-server update-keygen --out ~/.config/telacast/update.key` (ele imprime a chave **pública**, 64 caracteres hex).
   Coloque essa chave pública em `android/update.pub` (um arquivo de texto com só o hex). **Sem esse arquivo o app compila com a atualização
   desligada** (nada é consultado nem instalado). Depois de criar o arquivo, gere o APK de novo.
2. Suba `version.properties`, rode `scripts/gradle.sh assembleRelease`.
3. Publique no servidor:
   `TELACAST_UPDATE_KEY_FILE=~/.config/telacast/update.key telacast-server publish-update --file app/build/outputs/apk/release/telacast-0.2.0-arm64.apk --version 0.2.0 --platform android-arm64 --notes "o que mudou"`
4. Os apps instalados percebem a versão nova ao abrir (e ao voltar para a frente depois de 10 minutos): aparece a faixa **"Nova versão 0.2.0 disponível · Atualizar"**
   e, na primeira vez, a folha "O que mudou". Depois, uma vez por dia, só no Wi-Fi e com o celular carregando, o app baixa e confere o APK sozinho e avisa
   "Nova versão pronta — toque para instalar".

### O que é automático e o que o Android obriga a confirmar

- Automático: consultar a versão, baixar, conferir tamanho, SHA-256 e a assinatura Ed25519 (o app **não instala** nada que falhe numa dessas conferências, nem pacote de outro
  aplicativo, nem versão que não seja maior), e abrir a instalação.
- A instalação usa o `PackageInstaller` com "ação do usuário não necessária". O Android **só dispensa o diálogo** quando todas estas condições valem: Android 12 ou mais novo,
  o Telacast é quem instalou a versão atual (o instalador de registro) e o novo APK tem a mesma assinatura. Se qualquer uma falhar (por exemplo, a primeira versão
  foi instalada pelo navegador ou por `adb`), o Android mostra a tela "Deseja atualizar este app?" e a pessoa toca em **Atualizar**. Em Android 8 a 11 o diálogo sempre aparece.
- **Uma vez**, o Android pede para liberar "instalar apps desconhecidos" para o Telacast (Configurações). O app mostra uma folha em português explicando o caminho e continua sozinho quando a pessoa volta.
- Reiniciar o app para a versão nova é sempre feito pelo Android.

## A primeira instalação do amigo

1. O amigo abre `https://telecast.guilhermebf.dev/download/android-arm64` no celular (o servidor entrega o APK mais recente já com nome e tipo certos).
2. O Android pede para permitir "instalar apps desconhecidos" para o **navegador** (uma vez): ele libera, toca no arquivo e em "Instalar".
3. Abre o Telacast: o servidor `telecast.guilhermebf.dev` já vem preenchido (o campo fica em "Avançado"). Entra com o convite que o dono gerou
   (`telacast-server invite --uses 1 --days 2 --label amigo`).
4. As permissões só são pedidas quando usadas: notificações ao entrar, microfone ao entrar numa chamada de voz, captura de tela ao transmitir.

## Notificações (push) pelo Firebase

Sem configuração o app funciona normalmente, mas **sem push com o app fechado** (as notificações só chegam enquanto o app está aberto ou na chamada de voz). Para ligar:

1. No console do Firebase crie um projeto e um app Android com o pacote `app.telacast`; baixe o `google-services.json` e coloque em `android/app/google-services.json`
   (o arquivo é ignorado pelo git). O Gradle aplica o plugin sozinho quando o arquivo existe.
2. No servidor configure a credencial do FCM (conta de serviço): `docs/DEPLOY.md`, seção "Notificações push".
3. Gere o APK de novo. O app registra o token (`PushRegister`) ao entrar e o servidor passa a mandar as mensagens diretas, menções e pedidos de amizade.
   Os dados da notificação (autor, canal, trecho do texto) passam pelos servidores do Google.

Para provar a exibição e o toque sem FCM, no build **debug**:
`adb shell am broadcast -n app.telacast/app.telacast.push.ReceptorDeTesteDePush -a app.telacast.debug.PUSH_DE_TESTE --es kind dm --es channel_id <id> --es title "fulano" --es body "oi" --es author_id 2 --ez forcar true`.

## Segurança do release

- Só `https`/`wss` (texto claro bloqueado pelo Android; o debug tem uma exceção própria em `src/debug`). Domínio sem esquema vira `https` automaticamente.
- `allowBackup=false`: o token da sessão fica no Keystore do Android e não vai para backup.
- R8 ligado (código e recursos enxutos) com regras em `app/proguard-rules.pro`; sem logs de depuração no release.
- Relatório de crash: a pilha (sem mensagens, usuários, tokens ou senhas) é gravada num arquivo privado; ao abrir de novo o app oferece "Copiar" e "Compartilhar".

## O que NÃO está pronto

- Voz e transmissão/assistência de tela foram provadas em **um aparelho** (Redmi, Android 17) e contra o servidor real; falta variedade de aparelhos, Bluetooth e redes ruins.
- Sem UDP: voz e vídeo vão pelo WebSocket (TCP); em rede ruim isso adiciona atraso.
- Só arm64 e Android 8+. Sem iOS, sem tablet otimizado, sem tema claro.
- Sem áudio do sistema na transmissão de tela (o desktop também não envia).
- O push depende de você criar o projeto Firebase (acima); sem ele não há notificação com o app fechado.
- A atualização pelo app depende de `update.pub` e de o primeiro APK ter sido gerado com ele.
