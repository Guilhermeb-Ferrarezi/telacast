# 08 - Bots, apps e extensibilidade (Discord -> Telacast)

Legenda: **[F]** = fato da documentação pública do Discord (discord.com/developers/docs), **[I]** = inferência/proposta minha.
Aviso: as páginas não foram rebaixadas nesta sessão; os fatos vêm do conhecimento da documentação pública e os números de limite devem ser reconferidos antes de copiar.

## 1) Visão geral

O Discord separa três coisas: (a) **aplicação** (Application) = cadastro de um app no Developer Portal, com id, chave pública, segredo OAuth2, ícone, descrição; (b) **bot user** = usuário especial (flag `bot`) ligado à aplicação, que fala com a Gateway (WebSocket) e a REST API usando um token; (c) **superfícies de interação**: slash commands, componentes (botões/menus), modais, context menus, webhooks, Activities (iframe), Social SDK (jogos). [F]

Dois modos de receber eventos: Gateway (WebSocket persistente, intents) ou **Interactions Endpoint URL** (HTTP POST assinado com Ed25519, sem conexão persistente). [F]

Para o Telacast (servidor Rust próprio, JSON/WebSocket): o ganho real está em **webhooks de entrada** (integrar CI, RSS, scripts) e em **contas de bot com token** usando o mesmo WebSocket/REST do cliente. Slash commands vêm depois. Activities, App Directory, Social SDK e integrações de terceiros são caros e de baixo retorno para um clone pessoal. [I]

## 2) Funcionalidades

### 2.1 Contas de bot e tokens
- **O que faz**: usuário automatizado com badge BOT; entra em servidores via convite OAuth2 (`scope=bot`) com permissões pré-selecionadas; recebe eventos e chama a API. [F]
- **Como**: token secreto estático (`Authorization: Bot <token>`), regenerável no portal; Gateway `IDENTIFY` com `intents`. [F]
- **Intents**: máscara de bits que filtra eventos (GUILDS, GUILD_MEMBERS, GUILD_MESSAGES, MESSAGE_CONTENT, etc.). Alguns são **privilegiados** (GUILD_MEMBERS, GUILD_PRESENCES, MESSAGE_CONTENT): exigem ligar no portal e, acima de 100 servidores, aprovação/verificação. [F]
- **Limites** [F, reconferir]: rate limit por rota com cabeçalhos `X-RateLimit-*` e limite global (~50 req/s); bots não verificados podem estar em até 100 servidores; criação de bots por conta limitada por equipe/2FA.
- **Modelo de dados** [I]: `bots(id, owner_user_id, user_id, token_hash, public_key, flags, created_at)`; `users.is_bot bool`. Guardar apenas hash do token.

### 2.2 Permissões de bot
- Bots usam o mesmo sistema de **cargos + permission bits + overwrites por canal** dos membros; ao ser convidado ganha um cargo gerenciado ("managed role") com as permissões escolhidas, que não pode ser atribuído manualmente. [F]
- Permissões em interações: `app_permissions` vem no payload; comandos podem ter `default_member_permissions` e ser ocultados/restritos por guild/canal/cargo (override de administradores). [F]
- [I] Se o Telacast já tem cargos/permissões, bot = membro com flag e cargo gerenciado: custo marginal baixo.

### 2.3 Slash commands (Application Commands)
- **Tipos** [F]: `CHAT_INPUT` (slash `/`), `USER` e `MESSAGE` (context menu), `PRIMARY_ENTRY_POINT` (Activities).
- **Registro**: REST (`POST /applications/{id}/commands` global, ou `/guilds/{gid}/commands`); `PUT` em massa sobrescreve. Nome minúsculo 1-32 chars, descrição 1-100, até 25 opções, até 100 comandos slash globais por app (reconferir). [F]
- **Opções** [F]: subcomando, grupo de subcomando, string, integer, boolean, user, channel, role, mentionable, number, attachment; `choices` fixas, `autocomplete` (evento em tempo real enquanto digita), `min/max_value`, `min/max_length`, `channel_types`.
- **Localização** (`name_localizations`), `contexts` (guild, DM do bot, DM privada) e `integration_types` (instalado no servidor ou no usuário). [F]
- **UI do cliente**: ao digitar `/` abre picker de comandos agrupado por app; cliente monta formulário com as opções. [F]
- **Modelo** [I]: `commands(id, app_id, guild_id?, type, name, description, options_json, default_perms, version)`.

### 2.4 Interações, componentes e modais
- **Interaction** = payload `{id, application_id, type, data, guild_id, channel_id, member, user, token, version, app_permissions, locale}`. Tipos: PING, APPLICATION_COMMAND, MESSAGE_COMPONENT, APPLICATION_COMMAND_AUTOCOMPLETE, MODAL_SUBMIT. [F]
- **Resposta**: precisa responder em **3 s** (callback inicial); pode "deferir" (tipo 5/6) e então editar via webhook por **15 min** usando o `interaction token`. Respostas podem ser **efêmeras** (só quem chamou vê, flag `64`). [F]
- **Componentes** [F]: Action Row (até 5 por mensagem), Button (5 estilos + link + premium), Select Menus (string, user, role, mentionable, channel; 1-25 opções), Text Input (só em modal). Mais recentes "Components v2" (Container, Section, Text Display, Media Gallery, File, Separator) com flag `IS_COMPONENTS_V2`.
- **Modais** [F]: popup com até 5 componentes (text inputs curto/parágrafo); retorna MODAL_SUBMIT.
- **Segurança do modo HTTP** [F]: verificar `X-Signature-Ed25519` + `X-Signature-Timestamp` com a chave pública do app; responder PING com PONG.
- **Modelo** [I]: mensagem ganha `components_json`; `interaction_tokens(token, app_id, message_id, expires_at)`. O servidor entrega o evento ao bot via WS (`INTERACTION_CREATE`) e aceita callback REST/WS.

### 2.5 Webhooks
- **Incoming webhook** [F]: URL `/webhooks/{id}/{token}` que posta mensagem num canal sem bot nem sessão; aceita `content`, `username`/`avatar_url` por requisição, `embeds` (até 10), `components`, `allowed_mentions`, `thread_name`, arquivos multipart, `?wait=true`, `?thread_id=`. Mensagem aparece com badge BOT. Editável/apagável via `.../messages/{id}`.
- **Tipos** [F]: Incoming, Channel Follower (seguir canal de anúncios), Application (usados por interações).
- **Limites** [F, reconferir]: ~5 req/2 s por webhook; 2000 chars no content; 10 embeds, 6000 chars totais de embed; 25 MB por arquivo (varia).
- **Webhook events (saída)** [F]: o app registra uma URL e recebe `APPLICATION_AUTHORIZED`, `ENTITLEMENT_CREATE`, etc., com assinatura Ed25519 (diferente do webhook de entrada).
- **Modelo** [I]: `webhooks(id, channel_id, creator_id, name, avatar, token_hash)`.

### 2.6 OAuth2 e escopos
- Fluxos [F]: Authorization Code (+PKCE), Implicit, Client Credentials (token do dono do app), Bot authorization (`scope=bot` com `permissions=`).
- Escopos principais [F]: `identify`, `email`, `guilds`, `guilds.join`, `guilds.members.read`, `connections`, `bot`, `applications.commands`, `webhook.incoming`, `rpc` (privado), `messages.read` (RPC local), `activities.write`, `role_connections.write`, `dm_channels.read` (restrito).
- Endpoints: `/oauth2/authorize`, `/api/oauth2/token`, `/oauth2/token/revoke`. Tokens com expiração + refresh token. [F]
- **Instalação** [F]: "Installation contexts" (guild install vs user install) e link de instalação padrão com escopos/permissões.
- [I] Para o Telacast, OAuth2 só faz sentido se houver apps de terceiros que atuam como o usuário. Para bots basta token estático.

### 2.7 Activities e Embedded App SDK
- **O que faz** [F]: app web em iframe dentro da chamada de voz/canal (jogos, Watch Together); o SDK JS (`@discord/embedded-app-sdk`) conversa com o cliente por postMessage/RPC (commands: `authorize`, `getInstanceConnectedParticipants`, `setActivity`, eventos de voz/participantes). Backend do app faz OAuth e mantém estado; tráfego passa por proxy `*.discordsays.com` com CSP restrita. Lançamento por comando `PRIMARY_ENTRY_POINT`.
- [I] Custo alto: sandbox de iframe/webview dentro do egui (egui não tem webview nativo), proxy, SDK, sincronização multiusuário. Baixa prioridade.

### 2.8 Social SDK e Rich Presence
- **Social SDK** [F]: SDK C++/Unreal/Unity para jogos usarem amigos, DMs, chat, convites e voz do Discord dentro do jogo; usa contas Discord ou contas provisórias. **Rich Presence** (Activity via Gateway/RPC/SDK): estado do jogo, party, timestamps, botões, `assets` com imagens. Game Activity detectado do processo também. [F]
- [I] Rich Presence simples ("Jogando X", "Ouvindo Y") é barata: um campo `activity` na presença. SDK completo não vale.

### 2.9 Integrações (Twitch/YouTube/Spotify etc.)
- **Connections** [F]: o usuário vincula contas externas (Spotify, Twitch, YouTube, Steam, GitHub, Xbox, Reddit...) via OAuth do próprio Discord; exibidas no perfil (`/users/@me/connections`). Spotify alimenta Rich Presence ("Ouvindo Spotify") e permite "Listen Along".
- **Integrações de servidor** [F]: Twitch/YouTube ligadas por *assinante* do canal geram cargos pagos/sync de assinantes; o servidor também recebe notificação de live. Há ainda "Linked Roles" (`role_connections`): app define metadados e o usuário ganha cargo por verificação externa.
- [I] Não replicar OAuth de terceiros. Alternativa mínima: webhooks de entrada + bot de RSS/live feito por terceiros cumprem o papel de "notificações".

### 2.10 Emojis e stickers customizados
- **Emoji** [F]: imagem PNG/JPEG/GIF (animado) até **256 KiB**, redimensionada para 128x128; nome 2-32 chars `[a-zA-Z0-9_]`; sintaxe na mensagem `<:nome:id>` e `<a:nome:id>` (animado); cota por servidor por nível de boost (50/100/150/250 cada tipo, estático e animado); `roles` podem restringir uso; "Nitro" libera uso entre servidores. Também emojis de aplicação (`/applications/{id}/emojis`).
- **Sticker** [F]: PNG/APNG/Lottie (JSON)/GIF até **512 KiB**, 320x320; nome 2-30, descrição, tag de emoji associada; cota 5/15/30/60 por nível; enviados via `sticker_ids` na mensagem (1-3 por msg).
- **Modelo** [I]: `emojis(id, guild_id, name, animated, mime, path, creator_id, role_ids[])`; `stickers(id, guild_id, name, tags, format, path)`. Cliente: atlas de texturas egui com cache, parser de `<:n:id>` no renderer de mensagens, picker com busca. Reações já poderiam usar emoji custom.

### 2.11 App Directory / sistema de apps
- **App Directory** [F]: catálogo público de apps verificados (descrição, tags, imagens, link de instalação); exige verificação do app (identidade, política, uso de intents). Developer Portal: Teams (apps pertencem a equipes), Installation, OAuth2, Bot, Rich Presence, App Testers, Monetização (SKUs, entitlements, premium apps).
- **Verificação** [F]: obrigatória acima de 100 servidores para manter bot e intents privilegiados.
- [I] Para o Telacast: um painel simples "Meus apps" (criar app, copiar token, ver webhooks) basta; diretório público desnecessário.

## 3) Proposta mínima para o Telacast [I]

Ordem sugerida, cada etapa usável sozinha:

| Etapa | Entrega | Detalhe | Esforço |
|---|---|---|---|
| 1 | **Webhooks de entrada** | Tabela `webhooks`; `POST /api/webhooks/{id}/{token}` com JSON `{content, username, avatar_url, embeds[]}`; mensagem sem autor-usuário (autor "webhook" com badge BOT); rate limit simples; UI do cliente: criar/copiar/excluir URL nas configurações do canal | **S** |
| 2 | **Embeds** (se ainda não existirem) | título, descrição, cor, campos, rodapé, imagem; cliente renderiza em egui | **S-M** |
| 3 | **Contas de bot + token** | `users.is_bot`, tela "Aplicativos" (criar bot, mostrar token uma vez, regenerar), auth do WS por `Bot <token>`, convite a servidor (link/ação de admin) com cargo gerenciado; badge BOT no cliente | **M** |
| 4 | **Rate limits e documentação da API** | Cabeçalhos/erros padronizados + doc do protocolo JSON/WS + crate Rust cliente `telacast-bot` (reaproveitando tipos do protocolo) | **M** |
| 5 | **Slash commands** | Registro REST/WS de comandos por guild, picker `/` no input (autocomplete local), evento `INTERACTION_CREATE` entregue ao bot, resposta com callback (3 s, deferir, efêmera) e edição por token | **L** |
| 6 | **Botões/menus/modais** | `components` na mensagem, clique gera interação; select menus; modal com campos de texto | **L** |
| 7 | **Emoji/sticker custom** | upload, cota, parser `<:n:id>`, picker, cache de texturas | **M** (emoji) / **M** (sticker) |
| 8 | **Rich Presence simples** | campo `activity` (texto+ícone) setável por bot/cliente | **S** |
| - | Activities, Social SDK, App Directory, OAuth2 completo, integrações Twitch/YouTube/Spotify | Não fazer agora | **L-XL** |

Notas de projeto [I]:
- Reaproveitar o canal WS interno: bot = cliente autenticado por token com conjunto reduzido de eventos (filtro tipo intents; começar sem privilegiados, ou só um flag "ler conteúdo de mensagens").
- Modelo de interação: guardar `interaction_token` por 15 min; resposta inicial falhando em 3 s mostra "o aplicativo não respondeu" no cliente.
- Para testes: bot mock que responde `/ping`, no espírito de só testar com mocks.
- Segurança: token hasheado (SHA-256), mostrado uma única vez; webhooks com token na URL = segredo; bloquear `@everyone` por padrão (`allowed_mentions`); limitar tamanho de embeds e anexos.

## 4) Prioridade

| Prio | Item | Motivo |
|---|---|---|
| **P1** | Webhooks de entrada (+ embeds) | Esforço mínimo, valor imediato (notificações de CI/scripts/RSS), não exige mudança no cliente além de renderizar autor/badge |
| **P1** | Contas de bot + token + API documentada | Base de todo o ecossistema; permite scripts próprios do usuário/dono do servidor |
| **P2** | Emoji customizado | Muito visível ao usuário final e relativamente barato; stickers depois |
| **P2** | Slash commands | Principal UX de bots, mas exige UI de picker, interações e API estável (depende de bot e embeds prontos) |
| **P2** | Rich Presence simples | Barato, dá vida à lista de membros |
| **P3** | Botões/menus/modais | Valor alto para bots sofisticados, esforço L; só após slash |
| **P3** | OAuth2 de terceiros, Linked Roles | Só com demanda de apps externos |
| **P3** | Activities/Embedded App SDK, Social SDK, App Directory, integrações Twitch/YouTube/Spotify | Custo desproporcional (webview, proxy, SDK nativo, parcerias OAuth); não faz sentido para um clone próprio sem comunidade de devs |
| **P0** | (nenhum) | Nada nesta área é bloqueador para chat/voz básicos |

## 5) Fontes (documentação pública, não rebaixadas nesta sessão)
- https://discord.com/developers/docs/intro
- https://discord.com/developers/docs/topics/oauth2 (escopos e fluxos)
- https://discord.com/developers/docs/topics/gateway (intents)
- https://discord.com/developers/docs/topics/permissions
- https://discord.com/developers/docs/interactions/overview
- https://discord.com/developers/docs/interactions/application-commands
- https://discord.com/developers/docs/interactions/receiving-and-responding
- https://discord.com/developers/docs/components/overview (botões, selects, modais)
- https://discord.com/developers/docs/resources/webhook
- https://discord.com/developers/docs/events/webhook-events
- https://discord.com/developers/docs/topics/rate-limits
- https://discord.com/developers/docs/resources/emoji e /resources/sticker
- https://discord.com/developers/docs/activities/overview e https://discord.com/developers/docs/developer-tools/embedded-app-sdk
- https://discord.com/developers/docs/discord-social-sdk/overview
- https://discord.com/developers/docs/rich-presence/overview
- https://discord.com/developers/docs/resources/user#connection-object e /topics/role-connections-metadata
- https://discord.com/developers/docs/resources/application
