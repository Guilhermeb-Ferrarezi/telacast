# 06 - Arquitetura de protocolo: Gateway (WebSocket) e REST

Legenda: **[F]** = fato da documentação pública do Discord (conhecimento da doc oficial; as páginas não foram rebaixadas novamente nesta sessão, então confira detalhes numéricos antes de copiar); **[I]** = inferência/recomendação minha.

## 1) Visão geral

O Discord separa o protocolo em duas metades **[F]**:
- **REST (HTTPS, `/api/v10`)**: ações e consultas pontuais (enviar mensagem, buscar histórico, editar, upload). Idempotência fraca, rate limit por rota.
- **Gateway (WebSocket)**: fluxo de eventos em tempo real, servidor para cliente (mensagens novas, presença, digitando), mais poucos comandos cliente para servidor (identify, resume, heartbeat, request guild members, update presence, voice state).
- **CDN**: arquivos/avatares/anexos servidos por host separado (cdn.discordapp.com), URLs com assinatura/expiração para anexos.

Princípio central **[F]**: o cliente carrega o estado inicial uma vez (READY + GUILD_CREATE por guild) e depois aplica **eventos de diferença** (create/update/delete). Nunca reenvia o estado completo, exceto num novo Identify.

Telacast hoje: tudo num WebSocket JSON, enums `C2S`/`S2C`, Snapshot completo após mudança estrutural, sem heartbeat/reconexão/rate limit, ids i64 sequenciais do SQLite. Ou seja, o desenho é simples e funcional para LAN/poucos usuários, mas não aguenta rede instável nem escala.

## 2) Como o protocolo do Discord funciona

### 2.1 Conexão e handshake do Gateway [F]
1. Cliente obtém URL via `GET /gateway/bot` (ou usa `wss://gateway.discord.gg`), com query `?v=10&encoding=json&compress=zlib-stream`.
2. Servidor envia **Opcode 10 Hello** com `heartbeat_interval` (ms).
3. Cliente deve enviar o primeiro heartbeat após `interval * jitter(0..1)` e depois a cada `interval`.
4. Cliente envia **Opcode 2 Identify** (token, `properties` os/browser/device, `intents`, `shard`, `presence`, `large_threshold`, `compress`).
5. Servidor responde evento **READY** (`user`, `guilds` indisponíveis como stubs, `session_id`, `resume_gateway_url`, `application`), depois um **GUILD_CREATE** por guild com canais, cargos, membros parciais, presenças, threads etc.

### 2.2 Opcodes principais [F]
| Op | Nome | Direção | Uso |
|---|---|---|---|
| 0 | Dispatch | S->C | Evento (`t` = nome, `s` = sequência, `d` = dados) |
| 1 | Heartbeat | ambos | Cliente envia último `s`; servidor pode pedir um |
| 2 | Identify | C->S | Autentica nova sessão |
| 3 | Presence Update | C->S | Muda status/atividade |
| 4 | Voice State Update | C->S | Entrar/sair de voz |
| 6 | Resume | C->S | Retoma sessão (token, session_id, seq) |
| 7 | Reconnect | S->C | Servidor manda reconectar |
| 8 | Request Guild Members | C->S | Pede membros (chunking) |
| 9 | Invalid Session | S->C | `d` bool = resumível ou não |
| 10 | Hello | S->C | Intervalo de heartbeat |
| 11 | Heartbeat ACK | S->C | Confirma heartbeat |

Envelope único: `{ "op": n, "d": {...}, "s": seq|null, "t": "EVENT"|null }`.

### 2.3 Heartbeat, ACK e detecção de conexão zumbi [F]
- Se o cliente não recebe ACK (op 11) antes do próximo heartbeat, deve considerar a conexão "zumbi": fechar com código não-1000/1001 e **resumir**.
- Heartbeat carrega o último `s` recebido (ou null).

### 2.4 Resume e Reconnect [F]
- `s` aumenta a cada Dispatch. O cliente guarda `session_id`, `resume_gateway_url` e o último `s`.
- Ao cair: reconecta em `resume_gateway_url`, recebe Hello, envia **Resume** (`token`, `session_id`, `seq`). O servidor reenvia os eventos perdidos desde `seq` e termina com o evento **RESUMED**.
- Se a sessão expirou/buffer estourou: **Invalid Session (d=false)** -> cliente espera 1-5 s aleatório e faz novo Identify (estado completo de novo).
- **Reconnect (op 7)**: servidor pede para reconectar (deploy/balanceamento); cliente reconecta e tenta Resume.
- Close codes relevantes: 4004 (token inválido), 4007 (seq inválido), 4008 (rate limited), 4009 (sessão expirou), 4010-4014 (shard/versão/intents inválidos ou não permitidos). Alguns são não-reconectáveis (4004, 4010-4014).

### 2.5 Intents [F]
Bitfield enviado no Identify que escolhe **quais grupos de eventos** o cliente recebe (GUILDS, GUILD_MEMBERS, GUILD_MESSAGES, GUILD_PRESENCES, MESSAGE_CONTENT, DIRECT_MESSAGES, GUILD_MESSAGE_TYPING...). Alguns são "privilegiados" (members, presences, message content). Economiza banda e dá controle de privacidade.

### 2.6 Eventos típicos [F]
READY, RESUMED, GUILD_CREATE/UPDATE/DELETE, CHANNEL_CREATE/UPDATE/DELETE, GUILD_MEMBER_ADD/UPDATE/REMOVE, GUILD_ROLE_*, MESSAGE_CREATE/UPDATE/DELETE/DELETE_BULK, MESSAGE_REACTION_ADD/REMOVE, PRESENCE_UPDATE, TYPING_START (campo `timestamp`; cliente expira o "digitando" em ~10 s), VOICE_STATE_UPDATE, THREAD_*, INTERACTION_CREATE. Cada evento é uma **diferença pequena** ou o objeto completo da entidade alterada, não o estado do mundo.

### 2.7 Sharding [F]
`shard: [id, total]` no Identify; guild vai para o shard `(guild_id >> 22) % total`. Obrigatório acima de 2500 guilds. Para clientes de usuário isso não se aplica; só interessa a bots. **Irrelevante para o Telacast.**

### 2.8 Compressão e encoding [F]
- `compress=zlib-stream` (um único contexto zlib por conexão, mensagens terminam em sufixo `0x0000FFFF`) ou, nas versões mais novas, `zstd-stream`. Também compressão por payload no Identify (`compress: true`).
- `encoding=json` ou `etf` (Erlang term format). Cliente web oficial usa JSON + zlib-stream.

### 2.9 Snowflake [F]
ID de 64 bits: `timestamp_ms desde 2015-01-01 (42 bits) | worker (5) | processo (5) | incremento (12)`. Ordenável por tempo, gerado sem coordenação, serializado como **string** em JSON (evita perda de precisão em JS). `id >> 22` + epoch dá a data de criação, usado até para paginação por tempo.

### 2.10 REST: paginação, nonce, erros [F]
- `GET /channels/{id}/messages?limit=1..100&before|after|around=<snowflake>` (mutuamente exclusivos). Retorna do mais novo para o mais antigo. Para cima/baixo no histórico usa-se o id da última mensagem recebida como cursor.
- `nonce` (até 25 chars) e `enforce_nonce: true` no POST de mensagem: o mesmo nonce em ~poucos minutos não duplica; o MESSAGE_CREATE devolve o nonce, e o cliente reconcilia a mensagem otimista local.
- Erros: HTTP status + JSON `{ "code": 50035, "message": "...", "errors": {...} }` (códigos JSON próprios, ex.: 10003 canal desconhecido, 50013 sem permissão, 50035 corpo inválido).
- Versionamento na URL (`/api/v10`, `?v=10` no gateway); versões antigas são depreciadas com cronograma.

### 2.11 Rate limits [F]
- Por rota + "major parameter" (channel_id/guild_id/webhook_id), em **buckets**. Headers: `X-RateLimit-Limit`, `-Remaining`, `-Reset`, `-Reset-After`, `-Bucket`, `-Scope` (user/global/shared).
- 429: corpo `{ "message", "retry_after", "global": bool }` + `Retry-After`. Limite global ~50 req/s por bot. Reincidência leva a ban temporário por IP (Cloudflare).
- Gateway: 120 comandos/60 s por conexão; 1 identify a cada 5 s por shard; excedeu = close 4008.

### 2.12 CDN [F]
Avatares/ícones/emojis em `cdn.discordapp.com/<tipo>/<id>/<hash>.<ext>` (png, jpg, webp, gif; `?size=` potência de 2). Anexos de mensagens têm URLs assinadas (`ex`, `is`, `hm`) com expiração. Upload via `multipart/form-data` no POST de mensagem (limite por plano).

### 2.13 Consistência e "sem perder mensagens" [F + I]
- [F] Gateway só garante entrega **dentro de uma sessão resumível**. Fora disso, o cliente refaz Identify e deve **re-sincronizar via REST** (busca mensagens `after=<último id visto>`).
- [I] O desenho depende de ids ordenáveis: `after=<último id>` é o cursor natural para preencher o buraco.

## 3) Lições para o Telacast

Formato: item - ação (adotar / simplificar / corrigir) - esforço (S/M/L). Todo o conteúdo desta seção é **[I]**.

### 3.1 Corrigir no protocolo atual
1. **Heartbeat + detecção de zumbi** - Servidor manda `Hello{heartbeat_interval_ms}`; cliente envia `Heartbeat{seq}` e espera `HeartbeatAck`; sem ACK -> fecha e reconecta. Servidor derruba clientes sem heartbeat em 2x o intervalo (libera presença "online" falsa). Mais simples que o Discord: sem jitter obrigatório. **S**
2. **Reconexão automática com backoff** - Cliente egui reconecta (exponencial 1 s -> 30 s, com jitter) mantendo token. Mostrar banner "reconectando" na UI. **S**
3. **Sequência + Resume** - Cada evento S2C ganha `seq: u64` por sessão. Servidor guarda buffer em anel (ex.: últimos 1000 eventos ou 5 min) por sessão. `Resume{session_id, seq}` reenvia o que faltou; se não der, `InvalidSession` -> Identify completo. **M**
4. **Fallback de re-sync por REST/histórico** - Após Identify novo, para cada canal aberto, pedir `after=<último id local>` e fundir. Garante zero perda mesmo sem buffer. Depende do item 3.4 abaixo. **M**
5. **Snapshots incrementais em vez de completos** - Manter `Ready` (estado inicial) uma única vez e trocar o reenvio de Snapshot por eventos: `ChannelCreate/Update/Delete`, `MemberAdd/Update/Remove`, `RoleUpdate`, `GuildUpdate`, `PresenceUpdate`, `MessageCreate/Update/Delete`. O cliente aplica no seu store. Reduz banda, evita "piscar" da UI e perda de scroll/estado. Migração pode ser gradual: primeiro presença/mensagens (já são eventos?), depois estrutura. Manter um `RequestSnapshot` explícito como válvula de escape (equivale ao Identify). **L**
6. **Versionamento do protocolo** - Constante `PROTO_VERSION` no `telacast-proto`; cliente envia `v` no Identify/Login; servidor responde `Error{code: UnsupportedVersion}` e fecha com código dedicado. Usar `#[serde(other)]`/variante `Unknown` nos enums para ignorar eventos desconhecidos em vez de quebrar parse. **S**
7. **Envelope com `op`/`t`/`s`** - Hoje enums Rust serializados (tag do serde). Manter o enum, mas adicionar `seq` no envelope S2C (`{seq, event}`). Não vale copiar o formato `op/d/t` do Discord: o enum tipado é mais seguro. **S**

### 3.2 Adotar
- **Snowflake próprio** - 64 bits `ms desde epoch Telacast | 10 bits nó | 12 bits seq`; gerado no servidor, serializado como string no JSON (ou i64 se o cliente é só Rust, mas string protege futuros clientes web). Ganhos: ordem temporal sem consultar o banco, cursores `before/after/around` que funcionam mesmo com import/replicação, sem colisão se houver mais de um servidor. Migração: coluna nova TEXT/INTEGER, ids antigos mapeados mantendo a ordem (ids pequenos já são menores que qualquer snowflake novo). **M**
- **Paginação por cursor completa** - Adicionar `after` e `around` além de `before`, limite 1..100, resposta newest-first, `has_more` opcional. `around` permite "pular para mensagem" (respostas, busca, links). **S**
- **Nonce / idempotência** - Cliente gera nonce (uuid curto) no envio; servidor ecoa no `MessageCreate` e deduplica por (usuário, nonce) por alguns minutos. Cliente mostra mensagem otimista "enviando" e troca pela real. Resolve duplicata após reconexão. **S**
- **Rate limit** - Token bucket por usuário no servidor: ex. 5 msgs/5 s por canal, 1 typing/8 s, 120 comandos/min por conexão. Responder `RateLimited{retry_after_ms, scope}` (equivalente ao 429) e o cliente desabilita o envio até lá. Em vez dos headers HTTP, campos no erro. **S**
- **Códigos de erro estáveis** - `enum ErrorCode` numérico (`UnknownChannel=10003`-estilo, `MissingPermission`, `InvalidBody`, `RateLimited`, `UnsupportedVersion`) em `Error{code, message}`; o cliente decide por código, não por texto. **S**
- **Intents simplificados** - Cliente informa quais grupos quer (mensagens, presença, digitando). Útil para economizar tráfego de presença/typing em canais grandes. Só vale quando houver muitos usuários. **S** (baixo valor agora)
- **Typing com expiração** - `TypingStart{channel, user}` com expiração no cliente (~8-10 s), sem evento de "parou". Já o servidor limita a 1 por 8 s por usuário. **S**
- **Anexos/avatares por URL (CDN própria)** - Rota HTTP estática `/files/<tipo>/<id>/<hash>.<ext>`, nome com hash de conteúdo (cache imutável, `ETag`), upload via multipart na REST em vez de bytes no WebSocket. Assinatura/expiração opcional só se houver canais privados expostos. **M**
- **Separar REST de gateway** - Mover ações pontuais (histórico, upload, edição) para HTTP (axum) e deixar o WebSocket só para eventos + comandos leves. Facilita cache, retry, rate limit por rota e ferramentas externas. Pode ser feito rota por rota. **L**

### 3.3 Simplificar / não copiar
- **Sharding**: ignorar (servidor único). **-**
- **ETF**: ignorar; JSON (ou MessagePack/bincode no futuro) basta. **-**
- **Compressão**: começar com `permessage-deflate` do próprio WebSocket (flag em tungstenite/axum), que dá a maioria do ganho sem contexto próprio. zstd-stream só se o Snapshot/READY ficar grande. **S**
- **Buckets de rate limit com headers detalhados**: bastam limites fixos por tipo de ação. **-**
- **Guild chunking/large_threshold**: só necessário com milhares de membros por servidor; adiar. **-**
- **Opcodes numéricos**: manter enums Rust tipados. **-**

## 4) Prioridade

| P | Item | Esforço |
|---|---|---|
| P0 | Heartbeat + ACK + detecção de zumbi | S |
| P0 | Reconexão automática com backoff + banner na UI | S |
| P0 | Nonce / deduplicação de envio | S |
| P0 | Códigos de erro estáveis + variante Unknown nos enums | S |
| P1 | `seq` nos eventos + Resume com buffer | M |
| P1 | Re-sync por `after` (cursor) após reconexão | M |
| P1 | Rate limit no servidor + `RateLimited` no protocolo | S |
| P1 | Versão do protocolo no login | S |
| P1 | Paginação `after`/`around` | S |
| P2 | Eventos incrementais substituindo Snapshot completo (por entidade, gradual) | L |
| P2 | Snowflake próprio | M |
| P2 | permessage-deflate | S |
| P2 | Typing com expiração/limite | S |
| P3 | Upload/CDN própria por HTTP | M |
| P3 | REST separado do gateway | L |
| P3 | Intents, compressão própria, zstd | S |

Ordem sugerida: (1) P0 de uma vez, pois formam a base de robustez; (2) `seq`+Resume+re-sync; (3) incrementais por entidade, começando por presença e membros, que são os que mais disparam Snapshot.

## 5) Fontes (documentação pública do Discord)

- https://discord.com/developers/docs/events/gateway (conexão, Hello, Identify, Resume, Heartbeat, intents, sharding, compressão)
- https://discord.com/developers/docs/events/gateway-events (lista de eventos e payloads)
- https://discord.com/developers/docs/topics/opcodes-and-status-codes (opcodes, close codes, códigos JSON de erro)
- https://discord.com/developers/docs/topics/rate-limits (buckets, headers, 429, limites do gateway)
- https://discord.com/developers/docs/reference (versionamento, snowflakes, ids como string, CDN, formatação de imagens)
- https://discord.com/developers/docs/resources/message (paginação before/after/around, nonce, enforce_nonce, anexos)
- https://discord.com/developers/docs/topics/permissions (contexto de permissões em eventos)

Observação: nenhuma conta ou servidor do Discord foi contatado; os números exatos (limites, tamanhos) vêm da doc como lembrada e devem ser reconferidos antes de virarem requisito.
