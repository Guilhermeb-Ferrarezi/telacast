# 03 — Canais de texto e mensagens (Discord -> Telacast)

Legenda: **[DOC]** = fato extraído da documentação oficial (docs.discord.com/developers); **[INF]** = inferência / conhecimento geral do produto, não confirmado em fonte consultada nesta rodada. Os valores numéricos de **[DOC]** valem para a API; o cliente pode diferir.

## 1) Visão geral

O Discord organiza conversa em **canais** dentro de servidores (guilds), agrupados por categorias, mais DMs/grupos. Uma **mensagem** é um objeto com autor, conteúdo (texto com markdown), anexos, embeds, reações, referência de resposta, flags e um tipo. Os IDs são *snowflakes* de 64 bits (timestamp embutido), o que dá ordenação e paginação por ID de graça. [DOC]

Pilares da área: (a) tipos de canal; (b) composição (markdown, menções, emoji, anexos, stickers/GIFs); (c) ciclo de vida (editar, excluir, fixar, reagir, responder); (d) leitura (não lidos, menções, digitando); (e) navegação (histórico, busca, threads); (f) proteção (slow mode, rate limits, permissões).

## 2) Funcionalidades

### 2.1 Tipos de canal
Enum [DOC]: 0 texto, 1 DM, 2 voz, 3 grupo DM, 4 categoria, 5 anúncio, 10/11/12 threads (anúncio/pública/privada), 13 estágio, 14 diretório, 15 fórum, 16 mídia.
- **Texto (0)**: padrão. Tópico até 1024 chars (fórum: 4096). [DOC]
- **Anúncio (5)**: mensagens podem ser "publicadas" (crosspost, flag `CROSSPOSTED`) para canais seguidores em outros servidores. [DOC flag; fluxo de seguir = INF]
- **Fórum (15)**: só contém threads ("posts"); cada post nasce com 1ª mensagem. Tags (`available_tags` até 20; nome até 20 chars; 1 emoji; `moderated` restringe a quem tem MANAGE_THREADS), até 5 tags aplicadas por post, `default_reaction_emoji`, `default_sort_order` (0 atividade recente, 1 data de criação), `default_forum_layout` (1 lista, 2 galeria), flags `PINNED` (post fixo no topo) e `REQUIRE_TAG`. [DOC]
- **Mídia (16)**: como fórum, focado em galeria; ainda em desenvolvimento. [DOC]
- **Estágio (13)**: canal de voz com palco/plateia (limite 10.000 ouvintes). Pertence à área de voz, não a esta. [DOC]
- Categorias (4) são só agrupamento/permissões herdadas. [DOC tipo; herança = INF]

### 2.2 Threads
- Públicas (11), privadas (12, só convidados + MANAGE_THREADS), de anúncio (10). [DOC]
- Criar a partir de mensagem: o ID da thread = ID da mensagem (1 thread por mensagem). Sem mensagem: cria thread solta. Em fórum: cria post + 1ª mensagem juntos. [DOC]
- `auto_archive_duration`: 60, 1440, 4320 ou 10080 min de inatividade. Arquivada = somente leitura; enviar mensagem desarquiva; "trancada" (locked) só MANAGE_THREADS desarquiva. [DOC]
- Usuário precisa **entrar** na thread; ao enviar mensagem é adicionado automaticamente. `member_count` para de contar em 50. [DOC]
- Visual (INF): painel lateral ou tela cheia, lista de threads ativas, mensagem "thread iniciada" no canal pai.
- Tipo de mensagem `THREAD_STARTER_MESSAGE` (21) mostra a mensagem original no topo da thread. [DOC]

### 2.3 Markdown e formatação
Sintaxe de mensagem [DOC]: `<@ID>` usuário, `<@&ID>` cargo, `<#ID>` canal, `<:nome:ID>` emoji custom, `<a:nome:ID>` animado, `</cmd:ID>` slash command, `<t:UNIX:estilo>` timestamp com estilos t T d D f F s S R (R = relativo, localizado para o leitor).
Markdown do cliente [INF, conhecimento geral; a página de suporte retornou 403]: **negrito**, *itálico*, __sublinhado__, ~~tachado~~, `código`, blocos ``` com linguagem (realce de sintaxe), `> citação` e `>>> bloco`, `||spoiler||`, `# H1/## H2/### H3`, `-# subtexto`, listas `-`/`1.`, links mascarados `[texto](url)`, escape com `\`.
Regra do modelo: o servidor guarda o **texto bruto** com tokens; o cliente renderiza. Tokens de menção referenciam IDs (renomear usuário não quebra mensagens antigas). [INF]

### 2.4 Menções
- `@usuário`, `@cargo`, `@everyone`, `@here`, `#canal`. O objeto Message traz `mentions` (usuários) e `mention_roles`; `mention_everyone`. [DOC]
- `allowed_mentions` controla quem de fato é notificado: `parse` (users/roles/everyone) OU listas `users`/`roles` (até 100 cada), e `replied_user` (se resposta pinga o autor). [DOC]
- Visual (INF): chip colorido, destaque de fundo na mensagem que menciona você, contador vermelho de menções no ícone do servidor/canal.
- Autocompletar ao digitar `@`, `#`, `:` (INF).

### 2.5 Respostas (reply)
- Tipo 19 `REPLY`; campo `message_reference` (message_id, channel_id). Cliente mostra mini-prévia da mensagem original acima, com clique para pular até ela. Resposta pinga o autor salvo `replied_user=false` (toggle "@ ligado/desligado"). [DOC campos; UI = INF]
- Encaminhar (forward) usa `message_snapshots`. [DOC]

### 2.6 Editar, excluir, fixar
- Editar: só o autor; mensagem recebe `edited_timestamp`, UI mostra "(editado)". Na edição (API v10) é preciso reenviar a lista de anexos a manter. [DOC]
- Excluir: autor ou quem tem MANAGE_MESSAGES. Alguns tipos de sistema são não-deletáveis (entrada de DM, mudança de nome/ícone, thread starter). Exclusão em massa: 2-100 IDs, nenhuma com mais de 2 semanas. [DOC]
- Fixar (pin): permissão PIN_MESSAGES; listagem paginada (`before` por `pinned_at`, limite 50 por página). Limite total de fixadas por canal: **não consta nessa página** (conhecimento comum: 50) [INF]. Cria mensagem de sistema "X fixou uma mensagem". [INF]

### 2.7 Reações e emojis
- Reação = emoji + contagem + `me`; `count_details` separa normal vs "burst" (super reação, Nitro). Listar quem reagiu: paginado `after`, limite 1-100 (padrão 25). Apagar todas exige MANAGE_MESSAGES. Emoji custom em endpoint: `nome:id`. [DOC]
- UI (INF): pílulas sob a mensagem, hover mostra quem reagiu, botão "adicionar reação" com seletor de emoji (busca, categorias, recentes, tons de pele). Emoji custom por servidor, animados exigem Nitro para uso externo.
- Emoji Unicode vão como texto; custom como `<:n:id>`. [DOC]

### 2.8 Anexos e uploads
- Upload via multipart; limite padrão **20 MiB por arquivo** (a doc de mensagens cita 25 MiB para o corpo total da requisição); Nitro/Boost aumentam. [DOC — os dois números aparecem em páginas distintas; tratar como "~20-25 MiB base"]
- Anexo tem `description` (alt text até 1024 chars), nome, tipo, tamanho, dimensões de imagem. Embeds só referenciam anexos jpg/jpeg/png/webp/gif via `attachment://`. [DOC]
- Marcar anexo como spoiler (INF: prefixo `SPOILER_` no nome) e mensagens de voz (flag `IS_VOICE_MESSAGE`, 1 áudio, sem texto). [DOC flag]
- UI (INF): arrastar/colar/botão +, pré-visualização antes de enviar com renomear/spoiler/alt, galeria de imagens agrupadas, player de vídeo/áudio inline, bloco de arquivo genérico com download.

### 2.9 Embeds e prévia de links
- Até 10 embeds/mensagem; título 256, descrição 4096, até 25 fields (nome 256, valor 1024), footer 2048, author 256; **soma de textos <= 6000**. Deduplicados por URL. [DOC]
- Prévia automática de links (Open Graph: título, descrição, imagem, vídeo) gerada pelo servidor; autor pode suprimir com flag `SUPPRESS_EMBEDS` (ou `<url>` entre colchetes angulares). [DOC flag; geração por OG = INF]
- Embeds ricos são principalmente de bots/webhooks. Para um clone sem bots, só a prévia de link importa. [INF]

### 2.10 GIFs e stickers
- Stickers: até 3 `sticker_ids` por mensagem; mensagem tem `sticker_items`. [DOC]
- Seletor de GIF (Tenor/Giphy) é do cliente; o GIF enviado vira um link que gera embed. [INF]

### 2.11 Spoilers
`||texto||` oculta até clique; anexos com prefixo `SPOILER_` ficam borrados. Canais podem ter flag `IS_SPOILER_CHANNEL` (não aplicável a estágio). [DOC flag existe; comportamento = INF]

### 2.12 Mensagens do sistema
Campo `type`: DEFAULT 0, REPLY 19, THREAD_STARTER_MESSAGE 21, POLL_RESULT 46, além de RECIPIENT_ADD/REMOVE, CALL, CHANNEL_NAME_CHANGE, CHANNEL_ICON_CHANGE etc. [DOC; valores numéricos dos demais não listados na página]. No UI (INF): linha centrada com ícone ("X entrou no servidor", "X fixou uma mensagem", "X impulsionou o servidor", chamada perdida). Várias não são deletáveis. [DOC]

### 2.13 Enquetes (polls)
Campo `poll` na criação; ao encerrar gera mensagem tipo 46 com embed `poll_result`. [DOC]

### 2.14 "Digitando…"
Evento de gateway `TYPING_START` por canal; cliente envia ping a cada ~8-10 s enquanto digita e expira em ~10 s. UI: "X está digitando…" sobre a caixa de texto (até 3 nomes, depois "várias pessoas"). Nesta rodada **não** consultei a página do gateway; tudo [INF].

### 2.15 Leitura, não lidos, indicadores
[INF, a partir do conhecimento do produto]: o servidor guarda por usuário+canal o `last_read_message_id` e contagem de menções. Indicadores: ponto branco/negrito no canal, badge vermelho com número de menções no canal e no ícone do servidor, barra "novas mensagens desde..." com divisor vermelho na linha onde parou, botão "marcar como lido" (Esc) e "ir para o início de não lidas". Silenciar canal/servidor suprime indicadores (só menções contam conforme configuração).

### 2.16 Histórico e paginação
`GET messages`: `limit` 1-100 (padrão 50); `before`, `after`, `around` exclusivos entre si; ordem do mais novo ao mais antigo. Requer VIEW_CHANNEL + READ_MESSAGE_HISTORY. `around` permite "pular para mensagem" (reply, busca, link). [DOC]
Cliente (INF): rolagem infinita para cima carregando lotes; ao pular para mensagem antiga, mostra "você está vendo mensagens antigas" + botão "ir para o presente".

### 2.17 Busca de mensagens
API de busca em guild: `limit` 1-25, `offset` até 9975; resposta 202 significa "índice não pronto, tente depois (`retry_after`)"; resultado pode ter menos itens que o limite, então não paginar pelo tamanho. [DOC]
Filtros do cliente (INF): `from:`, `mentions:`, `has:` (link, embed, file, image, video, sound), `in:`, `before:`/`during:`/`after:`, `pinned:`; resultados com contexto e botão "pular".

### 2.18 Slow mode e rate limits
- `rate_limit_per_user` (segundos, 0-21600): intervalo mínimo entre mensagens por usuário no canal; bots e quem tem BYPASS_SLOWMODE (ou MANAGE_MESSAGES/CHANNEL, INF) são isentos; vale também para criar threads. Em threads, `default_thread_rate_limit_per_user` copia na criação. [DOC]
- UI (INF): contador regressivo na caixa de texto, ícone de relógio no cabeçalho do canal.
- Rate limit HTTP: baseado em buckets com cabeçalhos (`X-RateLimit-*`), limite global; a página de referência apenas remete à página de Rate Limits (não consultada). [DOC parcial / detalhes INF]
- Idempotência: `nonce` (até 25 chars) ecoado no evento de criação; `enforce_nonce=true` evita duplicata do mesmo autor. [DOC] Excelente para o clone (envio otimista).

### 2.19 Outros relevantes
- Flags: `SUPPRESS_NOTIFICATIONS` ("@silent"), `EPHEMERAL` (só interações/bots). [DOC]
- Snowflake: `(id >> 22) + 1420070400000` = timestamp ms; permite derivar `before/after` por data. [DOC]
- Componentes (botões/selects, `IS_COMPONENTS_V2`) são de bots: ignorar. [DOC]

## 3) Telacast: já tem vs falta

Já tem (informado): canais de texto, enviar/receber em tempo real, histórico das últimas 50, agrupamento por autor, não lido em negrito. No protocolo atual (`proto/src/lib.rs`): `C2S::Send{channel_id, content}` e `C2S::History{channel_id, before}` (já existe `before`, então paginação para trás é só UI) .

Falta (resumo): markdown/renderização; menções e autocomplete; respostas; editar/excluir; fixar; reações/emoji; anexos/imagens; prévia de links; "digitando…"; contagem de não lidos/menções e marcador de "onde parei"; busca; threads/fórum/anúncio; mensagens de sistema; slow mode; nonce/envio otimista; permissões por canal (pré-requisito de várias); `around` para pular a mensagem; GIF/stickers; spoilers.

## 4) Prioridade sugerida

Esforço: S = dias, M = 1-2 semanas, L = várias semanas.

| Pri | Funcionalidade | Esf. | Motivo |
|---|---|---|---|
| P0 | Editar e excluir próprias mensagens (+ "(editado)") | S | Básico esperado; exige evento S2C de update/delete e coluna `edited_at`. |
| P0 | Markdown básico (negrito, itálico, código, bloco de código, citação, links, spoiler) | M | Em egui exige parser + layout rico; maior ganho visual. |
| P0 | Nonce / envio otimista / erros de envio | S | [DOC] nonce; evita duplicata e latência percebida. |
| P0 | Paginação para trás ao rolar (usar `before` existente) | S | Backend quase pronto. |
| P0 | Não lidos com contagem de menções + divisor "novas mensagens" (`last_read_message_id`) | M | Completa o indicador atual; base para notificações. |
| P1 | Menções `@usuário` / `#canal` com autocomplete e destaque | M | Notificação direcionada; token com ID [DOC formato]. |
| P1 | Respostas com prévia e pular para mensagem (`around`) | M | Contexto de conversa; uso diário alto. |
| P1 | Anexos de imagem/arquivo (upload, limite configurável ~10-25 MiB, prévia inline) | L | Exige armazenamento, texturas no egui, segurança de tipos. |
| P1 | "Digitando…" (efêmero, sem persistência) | S | Barato (evento volátil com expiração ~10 s) e dá sensação de vida. |
| P1 | Reações com emoji Unicode | M | Alto engajamento; modelo simples (msg, emoji, user). |
| P1 | Fixar mensagens + lista de fixadas | S | Modelo simples, depende de permissões. |
| P1 | Busca por texto (FTS5 no SQLite) com filtros `from:`/`in:`/`has:` | M | SQLite já é usado; FTS barato; UI de resultados é o custo. |
| P1 | Mensagens de sistema (entrou, fixou) | S | Tipo `type` na mensagem; reutiliza lista. |
| P2 | Slow mode por canal | S | Valor 0-21600 s [DOC]; checagem no servidor; útil para moderação. |
| P2 | Threads (pública, auto-arquivo) | L | Muda modelo (canal filho de mensagem); bastante UI. |
| P2 | Prévia de link (Open Graph) | M | Servidor precisa buscar URLs (risco SSRF, cache); fazer com cuidado. |
| P2 | Emoji custom por servidor | M | Upload, armazenamento e picker. |
| P2 | Timestamps `<t:...>` , spoiler em anexo, alt text | S | Pequenos acabamentos. |
| P2 | Canal de anúncio (sem crosspost) | S | Só restrição de quem posta; crosspost é P3. |
| P2 | Fórum (posts + tags) | L | Só depois de threads. |
| P3 | Polls | M | Baixa prioridade. |
| P3 | Stickers e GIFs (Tenor/Giphy) | M-L | GIF dependeria de API externa; na prática colar link + prévia basta. |
| P3 | Embeds ricos de bots/webhooks, componentes V2, slash commands, mensagens de voz, canal mídia/diretório, super reações, crosspost entre servidores, forwarding | L | Sem ecossistema de bots/Nitro, pouco retorno. |

Observações de design (INF): usar IDs snowflake (ou i64 monotônico, já usado) para ordenação; guardar texto bruto e renderizar no cliente; rate limit por usuário no servidor (token bucket) desde o início para proteger o próprio servidor; limitar tamanho de mensagem (Discord usa 2000 chars, 4000 com Nitro: INF conhecimento geral).

## 5) Fontes consultadas

- https://docs.discord.com/developers/resources/message (antes discord.com/developers/docs/resources/message): tipos, flags, embeds, anexos, allowed_mentions, reações, pins, bulk delete, paginação, busca, nonce, polls, stickers.
- https://docs.discord.com/developers/resources/channel: tipos de canal, slow mode, threads, fórum/mídia, estágio.
- https://docs.discord.com/developers/reference: snowflake, sintaxe de formatação/menções/timestamps, limites de upload.
- Não obtidas: support.discord.com (Markdown 101 e Slowmode FAQ retornaram 403); página de Rate Limits e de Gateway Events não consultadas; cliente instalado em /snap/discord só listado (sem inspeção do JS), logo tudo marcado [INF] sobre UI vem de conhecimento geral do produto.
- Telacast: /home/guilherme/telacast/proto/src/lib.rs (somente leitura).
