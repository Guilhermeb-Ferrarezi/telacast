# Protocolo do Telacast (versão 1)

Guia para quem escreve outro cliente (o app Android, por exemplo). A fonte da verdade é o código:
`proto/src/lib.rs` (tipos), `server/src/main.rs` (comportamento, limites), `server/src/files.rs` e
`server/src/updates.rs` (HTTP). Os **exemplos exatos** de cada mensagem são as fixtures em `proto/fixtures/`,
conferidas byte a byte por `cargo test -p telacast-proto` (ver seção 10).

## 1. Transporte

Tudo conversa com o mesmo servidor (`ws://host:porta` em desenvolvimento, `wss://` em produção).

| Endpoint | Para quê |
|---|---|
| `GET /ws` (upgrade WebSocket) | Gateway: mensagens JSON em quadros de **texto**; áudio de voz em quadros **binários** |
| `GET /publish/{canal}?token=T` (WebSocket) | Quem transmite a tela do canal envia vídeo |
| `GET /watch/{canal}?token=T` (WebSocket) | Quem assiste recebe o vídeo |
| `POST /upload?token=T&channel=ID&name=ARQUIVO` | Envia um anexo (corpo = bytes do arquivo) |
| `GET /files/{id}/{nome}?token=T[&thumb=1]` | Baixa um anexo (ou a miniatura dele, ver 1.3) |
| `GET /update/manifest?platform=P` e `GET /update/file/{sha256}` | Atualização do app desktop |
| `GET /health` | Devolve `ok` |

`T` é o `token` recebido em `Auth` (o mesmo vale para o gateway e para o HTTP; é a sessão). Ele vai na
query string; use `wss://`/`https://` em produção para não expô-lo.

### 1.1 Gateway `/ws`

- Quadros de **texto** = JSON de `C2S` (cliente para servidor) ou `S2C` (servidor para cliente). Um texto que
  não decodifica como `C2S` é **ignorado em silêncio** (sem `Error`). Tamanho máximo de um quadro enviado
  pelo cliente: **64 KiB** (a conexão cai se passar).
- Quadros **binários** = áudio de voz (seção 6). Nunca derrubam a conexão: o que for inválido é descartado.
- Keepalive: o servidor manda `Ping` WebSocket a cada 20 s e encerra a conexão se não receber nada por 60 s
  (qualquer quadro conta, inclusive o `Pong`). Bibliotecas com ping automático (OkHttp) bastam; o desktop
  também desiste se não receber nada por 50 s.
- Limite de conexões simultâneas por IP: 30 (excesso recebe HTTP 429 no upgrade).
- Atrás de proxy no mesmo host o IP real vem de `CF-Connecting-IP` ou `X-Forwarded-For`.

### 1.2 Vídeo `/publish` e `/watch`

Antes do upgrade o servidor confere o token e se a pessoa pode acessar o canal (ou está nesse canal de
voz); senão responde **HTTP 403**. O canal é um id (`i64`).

Formato de cada quadro **binário** (nos dois sentidos):

```
[flag: 1 byte][quadro H.264 em Annex B]
```

`flag = 1` é **quadro-chave** (IDR/I); `flag = 0` é quadro P. O corpo é H.264 Annex B (start codes
`00 00 00 01`). O desktop liga `repeatSPSPPS`, então todo quadro-chave carrega SPS/PPS e dá para começar a
decodificar em qualquer um; o servidor **não inspeciona** o H.264, só olha o primeiro byte. Quadros com menos
de 2 bytes são descartados.

- `publish`: uma transmissão por canal. Se já existir, o servidor manda um quadro de **texto** (`"já há uma
  transmissão neste canal"`) e fecha. Máximo 4 MiB por quadro. Ao abrir e ao fechar, todos os participantes do
  canal recebem um novo `Snapshot` (o campo `live` muda). O servidor guarda o último quadro-chave.
- `watch`: o primeiro quadro recebido é o último quadro-chave guardado; depois chegam os seguintes. Espectador
  lento perde quadros e só volta a receber a partir do próximo quadro-chave (comece a decodificar só em
  `flag = 1`). Sem transmissão, ou com 50 espectadores, o servidor manda um quadro de **texto** explicando e
  fecha. O espectador não precisa enviar nada. Quem assiste e quem transmite recebem `Viewers` pelo gateway.
- Quem sabe que há transmissão: `Snapshot.live` (`channel_id` + `user_id` de quem transmite).

### 1.3 Anexos (HTTP)

1. `POST /upload?token=T&channel=ID&name=foto.png` com os bytes no corpo (sem multipart). Respostas: `200` com
   JSON no formato de `Attachment` (`id, name, mime, size, width, height`; fixture `http/upload/UploadResponse.json`);
   `401` token inválido; `400` sem `channel` ou corpo vazio; `403` sem acesso ao canal; `413` se passar do limite (padrão **10 MiB**, `TELACAST_MAX_UPLOAD_MB`); `429` mais de 20 envios por minuto.
   O `mime` vem do conteúdo (PNG, JPEG, GIF, WebP; o resto é `application/octet-stream`), nunca do que o
   cliente diz; `width`/`height` são 0 se não for imagem.
2. Mande o `id` na lista `attachments` de `C2S::Send` (até 10; a mensagem pode ter só anexos). O anexo só
   pode ser usado uma vez, por quem enviou, no canal indicado no upload; senão `Error InvalidInput`.
3. Baixar: `GET /files/{id}/{nome}?token=T` (`{nome}` é decorativo). Só quem lê o canal baixa; `401`/`403`/`404`
   conforme o caso. Imagens vêm `inline`; o resto, `attachment`.
4. **Miniatura**: `GET /files/{id}/{nome}?token=T&thumb=1` devolve uma versão leve da imagem, gerada pelo servidor
   na primeira requisição e guardada em disco (`<TELACAST_FILES_DIR>/thumbs/<sha256 do original>.jpg|png`). Mesma
   permissão do download (`401`/`403`/`404`). Use no chat; baixe o original (sem `thumb`) só quando o usuário abrir a
   imagem ampliada ou pedir para salvar. Regras:
   - Lado maior **<= 1024 px**, proporção mantida, **nunca amplia**. Filtro Lanczos3.
   - **JPEG qualidade 80** (`Content-Type: image/jpeg`) quando a imagem não tem transparência; **PNG** (`image/png`)
     quando algum pixel tem alfa < 255 (um PNG RGBA todo opaco também vira JPEG).
   - Orientação **EXIF** do JPEG é respeitada: a miniatura sai em pé e o EXIF não é copiado. Atenção: `width`/`height`
     do `Attachment` são as dimensões do arquivo cru, então numa foto girada pelo EXIF (orientação 5 a 8) a miniatura
     tem largura e altura trocadas em relação a esses campos; dimensione a vista pela imagem decodificada.
   - Resposta de miniatura traz `X-Telacast-Thumb: 1`, `Cache-Control: private, max-age=31536000, immutable` e o
     `Content-Type` da miniatura. **Sem esse cabeçalho o corpo é o original** (e o `Content-Type` é o do original).
   - **Cai para o original** (sem o cabeçalho, status 200) quando: o tipo não é PNG, JPEG ou WebP (GIF fica de fora
     para não perder a animação; PNG e WebP animados também); o arquivo não decodifica; a imagem passa de
     **64 megapixels** (largura x altura, conferido pelo cabeçalho antes de decodificar) ou de 300 MiB já decodificada;
     a imagem já cabe em 1024 px (reduzir não ajuda); ou a miniatura não ficaria menor que o original.
   - Servidor antigo ignora `thumb=1` e devolve o original sem o cabeçalho: o cliente trata os dois casos igual.
   - Geração: no máximo 2 ao mesmo tempo; pedidos simultâneos da mesma miniatura esperam a primeira. Quando não há
     miniatura possível o servidor grava um marcador vazio (`<hash>.none`) para não tentar de novo.

### 1.4 Atualização do app desktop

`GET /update/manifest?platform=linux-x86_64` devolve um `UpdateManifest` (fixture `http/update/UpdateManifest.json`);
`404` se não há versão publicada para a plataforma, `400` se `platform` for inválida. O binário vem de
`GET /update/file/{sha256}`. A assinatura Ed25519 cobre `update_signing_message(version, platform, sha256, size)`
(`"telacast-update-v1\n{version}\n{platform}\n{sha256}\n{size}\n"`). O app Android não usa isto.

## 2. Regras de serialização (serde, "externamente etiquetado")

O JSON é o que o `serde_json` produz para os enums e structs de `proto/src/lib.rs`:

| Forma no Rust | JSON |
|---|---|
| variante com campos nomeados | `{"Login":{"username":"a","password":"b","proto":1}}` |
| variante unitária | a **string** `"Logout"` (não é objeto) |
| variante de tupla com um campo (`Snapshot(Snapshot)`, `Message(Message)`, `VoiceUpdate(VoiceState)`) | `{"Snapshot":{...}}`, o conteúdo da struct direto, sem camada extra |
| struct aninhada | objeto com os campos na ordem declarada |
| `Option<T>` | `null` quando vazio (o servidor sempre escreve o campo); ao **ler**, campo ausente também vale `null` |
| enum simples (`Status`, `ErrorCode`, `FriendStatus`) | string com o nome da variante (`"Online"`) |
| `i64`, `u64` | número inteiro JSON |

Consequências para o cliente:

- **Ids são snowflakes de 64 bits** (`112623612862660608`), maiores que 2^53. Ler/escrever como `Long`; nunca
  `Double` nem `Number` de JavaScript. `ts` é milissegundos desde a época Unix (UTC).
- **`#[serde(default)]`**: o campo pode **faltar** ao ler, e o valor padrão vale (0, `false`, `[]`, `""`,
  `Status::Offline`, `ErrorCode::Unknown`). Aplica-se a: `User.status`, `Channel.mentions`, `Dm.mentions`,
  `Snapshot.blocked`, `Snapshot.voice`, `Message.nonce/reactions/kind/pinned/attachments`, `S2C::Error.code`,
  `proto` em `Register/Login/Resume`, `Send.nonce/attachments`, `UpdateManifest.notes/signature`.
  Ao **escrever**, o cliente pode omitir esses campos também (o servidor os preenche com o padrão).
- **Campos desconhecidos são ignorados** (o Rust não usa `deny_unknown_fields`): o servidor pode ganhar campos
  novos sem quebrar clientes antigos. O cliente deve ter a mesma tolerância.
- **`#[serde(other)]`**: `ErrorCode` desconhecido vira `Unknown` e `Status` desconhecido vira `Offline`.
  Só esses dois enums têm valor de reserva. `FriendStatus` **não** tem: um valor novo faz o `Snapshot`
  inteiro falhar (ver `proto/fixtures/extras/`).
- **Variante desconhecida** de `S2C` não decodifica. O cliente deve **descartar o quadro e seguir em frente**
  (o app desktop faz isso). Do outro lado, o servidor ignora em silêncio um `C2S` que não entende.
- Os casos de tolerância estão em `proto/fixtures/extras/` com o resultado esperado em `LEIA-ME.md`.

## 3. Fluxo de login

1. Abra `/ws`. Nada além de `Register`, `Login` e `Resume` vale antes de autenticar; qualquer outra mensagem
   recebe `Error{code: NotAuthenticated}`.
2. Envie **uma** de:
   - `Register{username, password, invite, proto}`: cadastro fechado por convite. Usuário de 3 a 32 caracteres
     (letras, números, `_`, `.`, `-`); senha de 8 a 128 caracteres.
   - `Login{username, password, proto}`: nome comparado sem diferenciar maiúsculas.
   - `Resume{token, proto}`: retoma uma sessão guardada.
   `proto` tem de ser `1` (`PROTO_VERSION`). Sem o campo (lido como 0) ou com outro número, o servidor responde
   `Error{code: VersionMismatch}` e **não** autentica.
3. Sucesso: o servidor manda `Auth{token, me}` e logo depois um `Snapshot`. Guarde o `token` (de forma segura)
   para os próximos `Resume` e para o HTTP. Em `Resume` o token devolvido é o mesmo.
4. Falhas: `InvalidCredentials`, `InvalidInvite`, `UsernameTaken`, `InvalidInput`, `SessionExpired`
   (token revogado ou inexistente), `RateLimited` (10 tentativas por 5 minutos por IP, e por usuário no login),
   `VersionMismatch`, `Internal`. Depois de autenticada, a conexão ignora novos `Register/Login/Resume`.
5. `Logout` revoga o token, tira a pessoa da voz e o servidor fecha a conexão.

### 3.1 O `Snapshot`

Estado completo do usuário: `me`, `servers` (cada um com `channels` e `members`), `friends`, `dms`, `live`
(transmissões em andamento), `blocked` e `voice` (quem está em cada canal de voz). **Não é um diff**: cada novo
`Snapshot` substitui o anterior inteiro. O servidor reenvia o `Snapshot` quando muda presença/status de
alguém relacionado, amizade, bloqueio, servidor, canal, transmissão ou membros. Mensagens novas **não**
passam por ele (chegam como `Message`); `last_message_id`/`last_read_id`/`mentions` valem no instante do envio.

Tipos de canal (`Channel.kind`): `0` texto, `1` voz, `4` categoria (`2` anúncio e `3` palco reservados). `parent`
é a categoria (ou `null`). DMs não aparecem em `servers`; ficam em `dms` (`channel_id` próprio). `User.online` =
conectada e visível; `Status::Invisible` só aparece para a própria pessoa (os outros a veem `Offline`).

## 4. Reconexão

Não existe sessão de conexão a retomar: reconectar é abrir `/ws` de novo e mandar `Resume{token, proto:1}`.
O desktop espera 1, 2, 4, 8 e depois no máximo 15 s entre tentativas (recuo exponencial). Se a resposta for
`SessionExpired`, `NotAuthenticated`, `VersionMismatch` ou `InvalidCredentials`, a sessão não vale mais:
volte para a tela de login. Outros erros (`RateLimited`, `Internal`) são passageiros: tente de novo depois.
Após `Auth`/`Snapshot`, **recarregue o histórico do canal aberto** (`History`), porque mensagens podem ter
chegado enquanto estava fora. A voz e a transmissão precisam ser reabertas (o estado de voz some quando a
conexão cai). O servidor não reenvia o que se perdeu.

## 5. Mensagens, `nonce` e envio otimista

- `Send{channel_id, content, reply_to, nonce, attachments}`. O servidor aplica `trim` no texto; vazio sem
  anexos, ou acima de 4000 caracteres, é **descartado sem erro** (valide antes). O servidor responde para
  **todos os participantes do canal, inclusive o autor**, com `S2C::Message`.
- **`nonce`**: número que o cliente escolhe (0 = sem). O servidor o devolve na `Message` do autor (0 para
  os outros e no histórico). Fluxo otimista: o cliente mostra a mensagem como "enviando" com um `nonce` novo
  e, quando chega uma `Message` com o mesmo `nonce` (do próprio `author_id`), troca a provisória pela definitiva
  (`id` real). Reenviar o **mesmo** `nonce` (por exemplo depois de reconectar sem saber se a primeira chegou) não
  duplica a mensagem: o servidor ignora o repetido (guarda os últimos 4096 pares usuário+`nonce`). Como o
  reenvio ignorado não gera resposta, depois de reconectar confira com `History`. O campo é `u64` no Rust;
  use valores positivos abaixo de 2^63 para caber num `Long` (o desktop monta `(milissegundos << 8) | contador` e usa o `nonce` negativo como id provisório da mensagem).
- `Search.nonce` é outra coisa: volta em `SearchResults.nonce` para casar pedido e resposta.
- Mensagem de resposta: `reply_to` = id da original; a `Message` traz `reply{id, author, content}` com o começo
  (até 120 caracteres) do texto original. Original apagada ou inexistente: `Error{NotFound}`.
- `History{channel_id, before}` devolve até **50** mensagens em ordem cronológica, as mais novas
  anteriores a `before` (ou as mais recentes se `null`). Menos de 50 = chegou ao começo. Sem acesso ao canal,
  a requisição é ignorada.
- Reações vêm no histórico/busca (`Message.reactions`); mudanças ao vivo chegam como `S2C::Reaction` (some ou
  soma 1 ao `count`; `me` = `user_id` é você).
- `Message.kind`: `0` normal, `2` alguém entrou no servidor (`content` = `"entrou"`), `3` alguém fixou uma
  mensagem (`content` = id da fixada, em texto).
- Menções no texto são `@usuario`; `Channel.mentions`/`Dm.mentions` contam as não lidas (zeram com `Ack`).

## 6. Voz (quadros binários)

1. `VoiceJoin{channel_id}` (canal `kind = 1`, só membros). Um canal por vez: entrar noutro move a pessoa.
   Todos do servidor recebem `VoiceUpdate(VoiceState)` e, ao sair, `VoiceLeft`. `VoiceSet{muted, deafened}`
   muda o estado próprio (surdo implica mudo). Cair a conexão também gera `VoiceLeft`.
2. Áudio: Opus mono, 48 kHz, quadros de **20 ms**, um pacote Opus por quadro, no máximo `VOICE_MAX_PACKET = 400`
   bytes. `VOICE_FRAME = 1` é o primeiro byte de todo quadro de áudio.

```
cliente -> servidor : [0x01][seq: u32 big-endian][pacote Opus]
servidor -> cliente : [0x01][remetente: i64 big-endian][seq: u32 big-endian][pacote Opus]
```

   Exemplos: `proto/fixtures/binary/voz_cliente_para_servidor.hex` e `voz_servidor_para_cliente.hex`. O servidor
   descarta quadros menores que 6 bytes, com primeiro byte diferente de 1, de quem está mudo/surdo ou fora de
   voz, e acima de ~100 quadros por segundo (balde de fichas); não repassa a quem está surdo nem ao remetente.
   Quem está em silêncio (VAD) não envia nada; quem recebe considera "falando" quem mandou pacote nos últimos ~250 ms.
   A fila de saída guarda ~2 s (100 quadros) por conexão e descarta o excedente.

## 7. Limites conhecidos

Todos aplicados no servidor; excesso gera `Error{code: RateLimited}` (ou erro de validação) salvo onde dito.

| O quê | Limite |
|---|---|
| Texto de mensagem (`Send`, `Edit`) | 1 a 4000 caracteres (após `trim`); fora disso é descartado em silêncio |
| `Send` | 12 por 10 s; até 10 anexos |
| `Edit` | 20 por 30 s (só o autor) |
| `Delete` | 30 por 30 s (autor ou dono do servidor) |
| Busca (`Search`) | `text` até 200 caracteres; 20 por 10 s; 25 resultados por página (`more` = há outra) |
| `Pin` | 20 por 30 s; 50 fixadas por canal |
| `React` | 30 por 10 s; emoji de 1 a 8 caracteres, sem letras/números ASCII, espaço ou controle |
| `Ack` | 120 por 10 s (excesso ignorado em silêncio) |
| `Typing` | 20 por 10 s (excesso ignorado em silêncio); reenviar a cada poucos segundos |
| `PushRegister` / `PushUnregister` | 20 por minuto (juntos); token de 1 a 4096 bytes, até 10 por conta |
| `SetFocus` | 60 por 10 s (excesso ignorado em silêncio) |
| `SetStatus` | 30 por 60 s; `Offline` não é aceito (use `Invisible`) |
| `Block` | 20 por 60 s |
| `FriendRequest` | 20 por hora |
| `CreateServer` | 5 por hora; nome 1 a 50; até 20 servidores próprios |
| `JoinServer` | 10 por 10 min |
| `CreateChannel` | 20 por hora; nome 1 a 32 (vira minúsculas, espaços viram `-`); até 200 por servidor; só o dono; `kind` 0, 1 ou 4 |
| Voz (`VoiceJoin`, `VoiceSet`) | 30 por 10 s; áudio ~100 quadros/s, pacote até 400 bytes |
| Login/cadastro | 10 por 5 min por IP (e por usuário no login); troca de senha 5 por 5 min |
| Senha / usuário | 8 a 128 caracteres / 3 a 32 (`[A-Za-z0-9_.-]`) |
| Upload | 10 MiB (padrão), 20 por minuto |
| Quadro WebSocket do cliente em `/ws` | 64 KiB; quadro de vídeo em `/publish` 4 MiB |
| Espectadores por transmissão | 50 |
| Conexões por IP | 30 |
| Histórico | 50 mensagens por página |

## 8. Variantes de `C2S` (cliente para servidor)

Fixtures em `proto/fixtures/c2s/<Variante>.json` (casos extras: `<Variante>.<caso>.json`).

| Variante | O que faz |
|---|---|
| `Register{username, password, invite, proto}` | Cadastro por convite; responde `Auth` + `Snapshot` ou `Error` |
| `Login{username, password, proto}` | Entra com senha |
| `Resume{token, proto}` | Retoma sessão pelo token |
| `Logout` | Revoga a sessão e fecha a conexão |
| `VoiceJoin{channel_id}` | Entra num canal de voz (depois valem os quadros binários) |
| `VoiceLeave` | Sai da voz |
| `VoiceSet{muted, deafened}` | Mudo/surdo próprios |
| `ChangePassword{old, new}` | Troca a senha; encerra as outras sessões; responde `PasswordChanged` + `Sessions` |
| `ListSessions` | Pede as sessões da conta; responde `Sessions` |
| `RevokeSession{id}` | Encerra uma sessão; responde `Sessions` |
| `RevokeOtherSessions` | Encerra todas menos a atual; responde `Sessions` |
| `CreateServer{name}` | Cria servidor (nasce com a categoria "canais de texto" e o canal "geral"); responde `Snapshot` e `ServerJoined` |
| `JoinServer{code}` | Entra por código de convite; responde `ServerJoined` (ou `Error InvalidInvite`) |
| `CreateChannel{server_id, name, kind, parent_id}` | Cria canal (0 texto, 1 voz) ou categoria (4); só o dono; todos recebem `Snapshot` |
| `Send{channel_id, content, reply_to, nonce, attachments}` | Envia mensagem; todos recebem `Message` |
| `Edit{message_id, content}` | Edita (só o autor); todos recebem `MessageEdited` |
| `Delete{message_id}` | Apaga (autor ou dono); todos recebem `MessageDeleted` |
| `SetStatus{status}` | Status escolhido, salvo na conta; `Snapshot` atualizado |
| `SetIdle{idle}` | "Ausente" automático, só desta conexão |
| `Search{nonce, channel_id, server_id, from, text, before}` | Busca em um canal/DM ou em um servidor; responde `SearchResults` |
| `Block{user_id}` / `Unblock{user_id}` | Bloqueia/desbloqueia; `Snapshot` atualizado dos dois |
| `Pin{message_id, pin}` | Fixa/desafixa; todos recebem `Pinned` (e `Message` kind 3 ao fixar) |
| `Pins{channel_id}` | Pede as fixadas; responde `Pins` |
| `React{message_id, emoji, add}` | Reage ou tira a reação; todos recebem `Reaction` |
| `Ack{channel_id, message_id}` | Marca o canal como lido até a mensagem |
| `Typing{channel_id}` | "Está digitando…"; os outros recebem `Typing` |
| `History{channel_id, before}` | Pede mensagens anteriores; responde `History` |
| `FriendRequest{username}` | Pede amizade pelo nome; `Snapshot` dos dois; erros `NotFound`, `InvalidInput` (si mesmo, bloqueado, já são amigos) |
| `FriendAccept{user_id}` | Aceita um pedido recebido |
| `FriendRemove{user_id}` | Remove amigo, recusa ou cancela pedido |
| `OpenDm{user_id}` | Abre (ou acha) a DM; responde `DmOpened` e `Snapshot` |
| `PushRegister{token, platform}` | Registra o token de push do aparelho (`platform` = `"fcm"`); sem resposta em caso de sucesso (seção 11) |
| `PushUnregister{token}` | Apaga o token (mande antes do `Logout`); sem resposta |
| `SetFocus{focused}` | Este app está em primeiro plano e visível? Estado por conexão, padrão `false`; sem resposta (seção 11) |

## 9. Variantes de `S2C` (servidor para cliente)

Fixtures em `proto/fixtures/s2c/<Variante>.json`.

| Variante | O que significa |
|---|---|
| `Auth{token, me}` | Login ou retomada deu certo; vem antes do primeiro `Snapshot` |
| `Viewers{channel_id, users}` | Quem assiste à transmissão do canal (lista vazia = ninguém); vai ao transmissor e aos espectadores |
| `VoiceUpdate(VoiceState)` | Alguém entrou numa voz ou mudou mudo/surdo (inclui você) |
| `VoiceLeft{user_id, channel_id}` | Alguém saiu da voz (ou a conexão dele caiu) |
| `Sessions{list}` | Sessões da conta (`SessionInfo`: `device` = IP, `created`/`last_used` em ms, `current`) |
| `PasswordChanged` | A senha foi trocada |
| `Error{code, msg}` | Falha; decida pelo `code` (estável), mostre `msg` (texto em português) |
| `Snapshot(Snapshot)` | Estado completo, substitui o anterior |
| `Message(Message)` | Mensagem nova (ou de sistema, `kind` 2 e 3) |
| `History{channel_id, messages}` | Resposta de `History`, em ordem cronológica |
| `SearchResults{nonce, hits, more}` | Resposta de `Search`, mais novas primeiro |
| `Pinned{channel_id, message_id, pinned}` | A mensagem foi fixada ou desafixada |
| `Pins{channel_id, messages}` | Fixadas do canal, a mais recente primeiro |
| `Reaction{channel_id, message_id, emoji, user_id, add}` | Alguém reagiu (`add`) ou tirou a reação |
| `MessageEdited{channel_id, message_id, content}` | Texto novo da mensagem (marque como editada) |
| `MessageDeleted{channel_id, message_id}` | Mensagem apagada |
| `Typing{channel_id, user_id}` | Alguém está digitando (dura poucos segundos; nunca é enviado de volta a quem digita) |
| `DmOpened{channel_id}` | Resposta de `OpenDm` |
| `ServerJoined{server_id}` | Resposta de `CreateServer`/`JoinServer` (abra o servidor) |

`ErrorCode`: `InvalidCredentials`, `WrongPassword`, `InvalidInvite`, `UsernameTaken`, `SessionExpired`,
`NotAuthenticated`, `RateLimited`, `Forbidden`, `NotFound`, `InvalidInput`, `VersionMismatch`, `Internal`,
`Unknown` (reserva para códigos novos).

## 10. Contrato por fixtures

- `proto/fixtures/c2s/` e `s2c/`: um arquivo por variante (`<Variante>.json`) mais casos (`<Variante>.<caso>.json`,
  por exemplo `Send.simples.json`, com `Option` vazio). JSON compacto, sem quebra de linha final.
- `proto/fixtures/http/`: corpos HTTP (`UploadResponse`, `UpdateManifest`).
- `proto/fixtures/binary/`: quadros de voz em hexadecimal.
- `proto/fixtures/extras/`: entradas de tolerância e resultado esperado (`LEIA-ME.md`).
- `cargo test -p telacast-proto -j2` compara o que o Rust gera com os arquivos, lê cada arquivo de volta e falha
  se houver fixture órfã. Adicionar variante ao enum quebra a compilação do teste até a fixture ser criada.
- Mudou o protocolo: `UPDATE_FIXTURES=1 cargo test -p telacast-proto -j2`, confira o diff, e o teste Kotlin
  (que lê os mesmos arquivos) mostra o que o cliente precisa acompanhar. Mudança que quebra cliente antigo
  sobe `PROTO_VERSION`.

## 11. Notificações push (FCM)

Para o app Android receber aviso com o aplicativo fechado. O servidor só envia se a credencial do Firebase
estiver configurada (`docs/DEPLOY.md`); sem ela o push fica desligado e os três comandos abaixo continuam
sendo aceitos (o token fica guardado). Mudança **só de adição**: não sobe `PROTO_VERSION`; um servidor
antigo ignora em silêncio os três comandos, um cliente antigo nunca os manda.

### 11.1 Comandos

- `PushRegister{token, platform}`: `platform` é `"fcm"` (outro valor dá `Error InvalidInput`). Mande a cada
  abertura do app depois do `Auth` e sempre que o FCM trocar o token (`onNewToken`). **Idempotente**: repetir
  o mesmo token só renova. Um token novo do mesmo aparelho convive com o antigo; a conta guarda no máximo
  **10** tokens e os mais antigos (pelo último registro) saem. Um token pertence a uma conta: se outra conta
  registrar o mesmo, ele **muda de dono**. Token vazio, com espaço ou controle, ou com mais de **4096 bytes**:
  `Error InvalidInput`. Sucesso é **silencioso** (o protocolo não tem `S2C` de confirmação; use um comando com
  resposta, como `ListSessions`, se precisar saber que foi aplicado: as mensagens de uma conexão são tratadas em
  ordem). 20 comandos de push por minuto.
- `PushUnregister{token}`: apaga o token **da própria conta** (token desconhecido não é erro). Faça isso antes do
  `Logout`: o servidor **não** apaga tokens ao revogar a sessão (nem no `Logout`, nem em `RevokeSession`), então
  sem o `PushUnregister` o aparelho continua recebendo avisos da conta que saiu.
- `SetFocus{focused}`: "o app está em primeiro plano **e** visível". Estado **por conexão** WebSocket e começa
  `false` em toda conexão (mais seguro: melhor avisar de mais do que perder). Mande `true` ao ir para primeiro
  plano e `false` ao ir para segundo plano ou apagar a tela. Some quando a conexão fecha. Enquanto **alguma**
  conexão da conta estiver com `true`, a conta não recebe push (quem está olhando o app já vê a mensagem).
  O app Android em segundo plano mantém o socket por alguns minutos com `focused=false` e **recebe** o push.

### 11.2 O que é enviado: `PushData`

Mensagem de **dados** do FCM (sem bloco `notification`: o app monta a notificação), prioridade alta, validade de
1 hora (`ttl`), `collapse_key` = `ch-<channel_id>` (ou `friend-<author_id>` no pedido de amizade). O objeto
`data` tem **só strings** (exigência do FCM); ids são decimais em texto; campos que não se aplicam vêm `""`.
Exemplos exatos: `proto/fixtures/push/PushData.{dm,mention,friend_request}.json` (tipo `PushData` em `proto/src/lib.rs`).

| Campo | Valor |
|---|---|
| `kind` | `dm`, `mention`, `friend_request` (e `message`, reservado: hoje o servidor não manda). Valor desconhecido: trate como `message` |
| `channel_id` | canal da mensagem; `""` em `friend_request` |
| `message_id` | a mensagem; `""` em `friend_request` |
| `server_id` | servidor do canal; `""` em DM e em `friend_request` |
| `title` | DM: nome do autor. Canal: `autor em #canal (servidor)`. Amizade: `Pedido de amizade` |
| `body` | texto simples (abaixo), no máximo **180 caracteres**; amizade: `<nome> quer ser seu amigo` |
| `author_id` | quem escreveu (ou pediu amizade) |
| `ts` | milissegundos desde 1970 (para a mensagem, o mesmo `ts` do `Message`) |

Exemplo (o objeto `data` de uma menção; o corpo HTTP completo para o FCM é
`{"message":{"token":"...","data":{...},"android":{"priority":"HIGH","ttl":"3600s","collapse_key":"ch-112623700000000012"}}}`):

```json
{"kind":"mention","channel_id":"112623700000000012","message_id":"112624001234567905","server_id":"112623700000000001","title":"marina.souza em #geral (Turma do Telacast)","body":"@guilherme olha isso: spoiler oculto 📎 foto da praia.png","author_id":"112623612862660617","ts":"1762512435678"}
```

`body`, a partir do texto cru da mensagem (mesmo Markdown do chat, `telacast-md`): sem marcas de formatação;
`||spoiler||` vira `spoiler oculto`; `<@id>` vira `@nome` (id desconhecido: `@usuário`); código, citação, lista e
título viram texto; quebras de linha e espaços repetidos viram um espaço; cada anexo vira `📎 <nome do arquivo>`
(vários: `📎 primeiro.png (+2)`); acima de 180 caracteres corta em fronteira de **caractere** (nunca de byte) e
termina em `…`. Mensagens de sistema (entrou, fixou) não geram push.

### 11.3 Quem é avisado

Para cada pessoa que **tem token**, **não é o autor** e **não tem nenhuma conexão com `focused=true`**, e que
não está em **Não perturbe** nem **bloqueou o autor**:

- **DM**: sempre (`kind = dm`).
- **Canal de servidor**: só se a mensagem a **menciona** (`<@id>`), **responde a uma mensagem dela**, ou usa
  `@everyone` (todos que veem o canal) ou `@here` (só quem tem alguma conexão aberta): `kind = mention`.
  Canal comum sem menção **não** avisa. (Ponto de extensão: `server/src/push.rs`, `ChannelNotify`; o chamador
  usa sempre `MentionsOnly` até existirem preferências por canal/servidor e "silenciar".)
- **Pedido de amizade** novo (não o aceite automático de um pedido cruzado): `kind = friend_request`.

Quem fica online só com conexões `focused=false` recebe. Não existe confirmação de leitura: se o app abrir e
ler o canal (`Ack`), cabe ao app cancelar a notificação que já mostrou.

### 11.4 Anti-spam e entrega

- **Agrupamento**: por (pessoa, canal), o primeiro aviso abre uma janela de **3 s**; os que chegam nela
  substituem o conteúdo (o **último vence**) e a janela fecha com **um** envio. Logo, todo push sai 3 s depois
  da mensagem. Cada pedido de amizade tem a sua janela.
- No máximo **20 pushes por minuto** por pessoa; o excesso é descartado em silêncio.
- O envio é feito por uma fila em tarefa própria (não trava o gateway), com 1000 avisos no máximo (o mais antigo
  sai) e no máximo 4 envios simultâneos. Erro 5xx/429 do FCM: **uma** nova tentativa e depois descarta. Token
  recusado (`404`/`UNREGISTERED`, ou `INVALID_ARGUMENT` que cita o token) é apagado do banco.
- A entrega é "melhor esforço": não há reenvio depois do descarte nem aviso de perda; ao reabrir, o app
  recarrega o `Snapshot`/`History` como sempre.
