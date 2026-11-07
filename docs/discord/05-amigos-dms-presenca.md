# 05 — Amigos, DMs, grupos, presença e notificações

Legenda: **[DOC]** = fato de documentação oficial consultada (developers/docs) · **[CONH]** = conhecimento geral do produto Discord (não reconfirmado nesta sessão; support.discord.com devolveu 403) · **[INF]** = inferência/proposta para o Telacast.

## 1) Visão geral

No Discord, a camada social é um grafo de **relações** entre usuários (amigo, pendente enviado, pendente recebido, bloqueado) mais **canais privados** (DM 1:1 e DM em grupo) e um canal de eventos em tempo real (gateway) que entrega **presença**, **digitando** e **notificações**. A Home do cliente tem a aba "Amigos" (Online / Todos / Pendentes / Bloqueados / Adicionar amigo) e a coluna "Mensagens diretas" ao lado dos servidores. Presença (status + atividade) é visível para amigos e membros de servidores em comum.

Dependências no Telacast: servidor Rust próprio com protocolo em `proto/src/lib.rs` (já há `FriendRequest`, `FriendAccept`, `FriendRemove`, `OpenDm`, `Friend{status}`, `Dm`). O resto deste documento lista o que um clone fiel precisa além disso.

## 2) Lista de funcionalidades

### 2.1 Amigos e relações
| Func. | O que faz / como o usuário vê | Modelo e regras |
|---|---|---|
| Adicionar amigo | Aba "Adicionar amigo": digita o nome de usuário, envia pedido. [CONH] | Relação com tipo: `friend`, `blocked`, `incoming`, `outgoing`. [CONH] Pedido para quem já enviou pedido a você vira amizade automática. [CONH] |
| Pedidos pendentes | Aba "Pendentes" com recebidos/enviados, aceitar/ignorar/cancelar; badge numérico no ícone Amigos. [CONH] | Contador = só `incoming`. |
| Lista de amigos | Abas Online / Todos, busca, menu de contexto (mensagem, ligar, remover, bloquear). [CONH] | Ordenação alfabética; Online filtra por status != offline/invisível. |
| Remover amigo | Apaga relação nos dois lados, sem notificar. [CONH] | A DM continua existindo. |
| Bloquear | Do perfil, menu da DM ou mensagem. [CONH] | Relação `blocked` unilateral. Efeitos: bloqueado não pode mandar DM nem pedido de amizade; mensagens dele em servidores ficam colapsadas ("mensagem bloqueada, mostrar"); some da lista de amigos; DM 1:1 some/fica inerte. |
| Ignorar | Versão suave do bloqueio (menu de mensagem em servidor, "Ignorar usuário"). [CONH] | Só esconde as mensagens para você; o outro não vê diferença e pode mandar DM. |
| Quem pode me adicionar | Configurações > Conteúdo e Social > "Quem pode te enviar pedido de amizade": todos / amigos de amigos / membros de servidores. [CONH] | Flags por usuário, checadas no servidor ao criar relação. |
| Sugestões / amigos em comum | "Amigos em comum" e "Servidores em comum" no perfil; sugestões de "pessoas que você talvez conheça" na aba Adicionar. [CONH] | Calculado por interseção de amigos/servidores. |
| Contas conectadas | Perfil pode listar conexões (Spotify, GitHub etc.), com visibilidade pública ou só dono. [DOC: `GET /users/@me/connections`, `visibility` 0/1; flags `friend_sync`, `show_activity`] | Fonte de "sincronizar amigos" e de atividade (Spotify). |

### 2.2 DMs 1:1 e grupo
| Func. | Descrição | Regras |
|---|---|---|
| Abrir DM | Clicar no usuário ou na lista de DMs. [DOC: `POST /users/@me/channels` com `recipient_id`; se já existir, devolve a existente] | Canal idempotente por par de usuários. Discord avisa para não abrir DMs em massa (rate limit). [DOC] |
| Lista de DMs | Coluna à esquerda ordenada por última atividade, com avatar+status, nome, prévia/atividade e botão de fechar (esconder). [CONH] | Fechar não apaga o histórico; reabre ao chegar nova mensagem. |
| DM em grupo | Até 10 pessoas (hoje; já foi 10, confirmar), nome e ícone editáveis, dono pode remover membros, qualquer membro pode adicionar amigos. [CONH] `POST /users/@me/channels` também cria grupo, mas é legado do GameBridge, limite 10 grupos ativos. [DOC] | Canal tipo `GROUP_DM`, lista de `recipients`, `owner_id`, `name`, `icon`. |
| Solicitações de mensagem | Mensagens de não-amigos sem servidor em comum ou suspeitas caem em "Solicitações de mensagem" (aceitar/recusar). [CONH] | Controlado por configuração "filtro de spam de DM" (3 níveis) e por "permitir DM de membros do servidor". |
| Privacidade de DM | Por servidor: "Permitir mensagens diretas de membros do servidor". Global: filtro de conteúdo explícito em DMs (todos / só desconhecidos / desligado). [CONH] | Flag por (usuário, servidor). |
| Chamadas em DM | Botões de ligar voz/vídeo/compartilhar tela; toca e aparece "Chamada recebida" com atender/recusar; em grupo, "iniciar chamada" toca para todos. [CONH] | Estado de chamada por canal: `ringing`, lista de participantes, quem foi tocado; timeout de toque (~1 min, [INF]). Depende da área de voz (outro mapeamento). |
| Digitando | "Fulano está digitando..." [DOC: evento Typing Start com `channel_id`, `user_id`, `timestamp`] | Efêmero, expira ~10 s no cliente. [CONH] |
| Fixar/histórico/anexos | Mesmos recursos de mensagens de canal (outra área). | — |
| Marcar como não lida / lida | Lido ao abrir; menu "Marcar como lida". Marcador "Novas mensagens" (linha vermelha) no ponto do último lido. [CONH] | Guardar `last_read_message_id` por (usuário, canal). |

### 2.3 Presença e status
| Func. | Descrição | Regras |
|---|---|---|
| Status | Online, Ausente, Não perturbe, Invisível (+ Offline derivado). [DOC: valores `online`, `idle`, `dnd`, `invisible`, `offline`] | Invisível aparece como offline para os outros. [DOC] Escolha manual do usuário sobrepõe o automático. |
| Ausente automático | Após ~10 min sem input, o cliente envia `idle`. [CONH] [DOC: payload de Update Presence tem `since` (ms em que ficou ausente), `afk`] | Cliente decide; servidor só armazena. |
| Por plataforma | `client_status` com `desktop`, `mobile`, `web`; ausente se offline/invisível. [DOC] | Status exibido = "melhor" entre as sessões (online > idle > dnd, [INF]). |
| Atividade / rich presence | "Jogando X", "Ouvindo Spotify", "Transmitindo", "Assistindo", "Competindo" aparecem na lista de membros, perfil e DM. [DOC: tipos de atividade 0 Playing, 1 Streaming, 2 Listening, 3 Watching, 4 Custom, 5 Competing] | Lista `activities` por usuário; cada uma com `name`, `type`, `state`, `details`, timestamps, assets (ícone), botões. [CONH] |
| Status personalizado | Texto + emoji, com expiração (30 min, 1 h, 4 h, hoje, nunca). [CONH] | É uma atividade `type 4` (Custom) com `state` e `emoji`. [DOC tipo / CONH expiração] |
| Detecção de jogo | Cliente varre processos e casa com banco de jogos detectáveis. [CONH] | Para o Telacast: opcional, ver prioridade. |
| Spotify | Conta conectada -> atividade "Ouvindo" com faixa, artista, progresso; "Ouvir junto" para amigos. [CONH] | Via conexão (`show_activity`). [DOC campo] |
| Privacidade de atividade | "Compartilhar atividade detectada" liga/desliga; por servidor também. [CONH] | Flag global. |
| Quem recebe presença | Amigos + membros de servidores em comum; no gateway exige intent `GUILD_PRESENCES`. [DOC] | Presença é fan-out: ao mudar, o servidor empurra para amigos e salas abertas. |

### 2.4 Notificações
| Func. | Descrição | Regras |
|---|---|---|
| Nível por servidor | Todas as mensagens / só @menções / nada. [CONH] | Campo `message_notifications` (0 todas, 1 menções, 2 nada) por servidor, com padrão do servidor. [CONH] |
| Sobrescrita por canal | Mesmo seletor por canal, ou "usar categoria". [CONH] | Tabela `(user, channel) -> nível`. |
| Silenciar | Servidor/canal/DM/grupo por 15 min, 1 h, 3 h, 8 h, 24 h ou até religar. [CONH] | `muted` + `mute_until`. Silenciado: sem badge de menção na sidebar (só indicador discreto) e sem som/push. |
| Suprimir @everyone/@here, cargos | Toggles por servidor. [CONH] | Flags. |
| Menções | Badge vermelho com contagem em canal, servidor e DM. [CONH] | `mention_count` por canal; mensagem de DM sempre conta como menção. [CONH] |
| Badge de não lida | Bolinha branca no servidor/canal com mensagem nova; número vermelho nas menções; na DM, número vermelho. Ícone do app/barra de tarefas mostra total. [CONH] | Derivado de `last_message_id > last_read_message_id` e menções. |
| Notificação desktop | Toast do SO ao receber DM/menção quando a janela não está em foco. Opções: ativar, som, "nunca notificar com foco". [CONH] | Cliente usa API de notificação do SO. |
| Som | Sons de mensagem, entrada/saída de voz, chamada tocando, com volume. [CONH] | Assets locais. |
| Push móvel | Enviado se o usuário está inativo no desktop por ~2 min. [CONH] | Fora do escopo (sem app móvel). |
| Não perturbe | Status `dnd` silencia todos os sons/toasts/push. [CONH] | Cliente aplica: se `dnd`, não dispara toast nem som. |
| Notificações por threads, eventos, respostas | Níveis próprios. [CONH] | Menor prioridade. |
| Sinal de "Silenciar até" da conta | Horário silencioso móvel. [CONH] | Fora. |

## 3) Telacast: já tem vs falta

Estado atual (inferido do protocolo `proto/src/lib.rs`): `FriendRequest{username}`, `FriendAccept`, `FriendRemove`, `OpenDm`, `Friend{status}`, `Dm`, presença online/offline, contador de pedidos pendentes. [INF a partir de `grep`; não foi feita revisão completa do app]

| Área | Já tem | Falta |
|---|---|---|
| Amigos | pedido, aceitar, remover, contador | recusar/cancelar pedido enviado (confirmar), abas Online/Todos/Pendentes/Bloqueados, busca, bloquear, ignorar, amigos em comum, sugestões, privacidade de pedido |
| DMs | DM 1:1 | DM em grupo, fechar/esconder DM, ordenação por atividade, não lidas e marcador "novas mensagens", solicitações de mensagem, privacidade de DM, indicador digitando |
| Presença | online/offline | ausente (automático), não perturbar, invisível, status por sessão, atividade, status personalizado com expiração |
| Chamadas | — | chamada em DM (depende da voz), toque, atender/recusar |
| Notificações | — | contador de não lidas/menções, silenciar, níveis por servidor/canal, toast desktop, som, DND respeitado, badge no ícone |
| Privacidade | — | quem me adiciona, quem me manda DM, filtro de spam |

## 4) Prioridade

Esforço: S = poucos dias/uma função; M = ~1 semana, toca servidor+cliente; L = várias semanas ou depende de outra área.

| Pri | Item | Esf. | Motivo |
|---|---|---|---|
| **P0** | Não lidas por canal/DM (`last_read`, badge, linha "novas mensagens") | M | Sem isso o chat não é usável com vários canais; base de todas as notificações. |
| **P0** | Status manual: online / ausente / não perturbe / invisível | S | Enum + broadcast; invisível exige o servidor mascarar como offline. |
| **P0** | Bloquear usuário (e recusar/cancelar pedido) | M | Segurança mínima de qualquer rede social; define regras de entrega de DM. |
| **P1** | Contagem de menções (@usuário) com badge vermelho | M | Distingue "importante" de "ruído"; depende do parser de menções (outra área). |
| **P1** | Silenciar servidor/canal/DM com duração | S | Tabela simples; alto valor percebido. |
| **P1** | Notificação desktop + som, respeitando DND e foco | M | Usa API do SO (ex. `notify-rust`); esforço é preferências. |
| **P1** | Abas Online/Todos/Pendentes/Bloqueados + busca na lista de amigos | S | Só UI. |
| **P1** | Indicador "digitando" | S | Evento efêmero, sem persistência. |
| **P1** | Ausente automático por inatividade | S | Timer no cliente + mensagem de presença. |
| **P2** | DM em grupo (criar, nomear, adicionar/remover, sair) | L | Muda modelo de `Dm` de par para lista de membros; migração. |
| **P2** | Status personalizado (texto+emoji+expiração) | S | Campo na presença + job de expiração. |
| **P2** | Níveis de notificação (todas/menções/nada) por servidor e canal | M | Tabela de preferências + UI. |
| **P2** | Privacidade: quem me adiciona, DM de membros do servidor | M | Flags e checagens no servidor. |
| **P2** | Ignorar (esconde mensagens localmente) | S | Filtro de cliente. |
| **P2** | Fechar/esconder DM; ordenar por última atividade | S | Flag por usuário/DM. |
| **P3** | Chamadas em DM (toque, atender/recusar) | L | Depende da voz/WebRTC. |
| **P3** | Atividade/rich presence ("jogando X"; Spotify) | L | Detecção de processos + contas externas; valor social, mas alto custo. Começar por atividade manual via API do Telacast. |
| **P3** | Solicitações de mensagem e filtro de spam | M | Só faz sentido com base maior de usuários. |
| **P3** | Sugestões de amigos / amigos em comum | M | Interseção de grafos; baixa urgência. |
| **P3** | Push móvel, horário silencioso | L | Sem app móvel. |

Ordem sugerida de entrega: P0 (não lidas -> status -> bloqueio), depois P1 em bloco "notificações" (menção, mute, toast/som), depois grupo de DM.

## 5) Notas de modelagem [INF]

- `relationships(user_id, other_id, kind)` com `kind in {friend, incoming, outgoing, blocked}`; bloquear remove a amizade.
- `read_state(user_id, channel_id, last_read_id, mention_count)`; badge = `last_msg_id > last_read_id`.
- `notif_prefs(user_id, scope(server|channel|dm), scope_id, level, muted_until)`.
- `presence(user_id, status, activities[], updated_at)` em memória no servidor, com fan-out para amigos e salas abertas; `invisible` armazenado mas transmitido como `offline`.
- Canais de DM: unificar com canais normais (`kind = dm | group_dm`) e tabela `channel_members` evita retrabalho ao criar grupos.

## 6) Fontes consultadas

- https://docs.discord.com/developers/resources/user (antigo discord.com/developers/docs/resources/user): criação de DM, conexões, flags de usuário. Não cobre relações de amizade (essas ficam só no cliente/API não documentada).
- https://docs.discord.com/developers/events/gateway-events: Presence Update, tipos de atividade, `client_status`, valores de status, Update Presence (opcode 3), Typing Start.
- Tentadas e bloqueadas (HTTP 403): support.discord.com "Notifications Settings 101" e "Blocking & Privacy Settings". Por isso as regras de UI, bloqueio, privacidade e notificações estão marcadas [CONH] e devem ser revalidadas no cliente real.
- Cliente instalado em /snap/discord/current/usr/share/discord: apenas listado (`app.asar` presente); não foi aberto nesta sessão por restrição de RAM.
- Código do Telacast: `grep` em `proto/src/lib.rs` (tipos `Friend`, `FriendStatus`, `Dm`, mensagens `FriendRequest/Accept/Remove`, `OpenDm`).
