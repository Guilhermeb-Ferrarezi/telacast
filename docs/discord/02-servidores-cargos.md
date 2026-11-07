# 02 — Servidores (guilds), cargos e permissões

Legenda: **[DOC]** = fato confirmado na documentação oficial consultada nesta sessão. **[CONHECIMENTO]** = comportamento conhecido do produto (support.discord.com/uso), não reconfirmado agora. **[INFERÊNCIA]** = sugestão/dedução minha para o Telacast.

## 1. Visão geral

O servidor (guild) é o contêiner central: tem dono, membros, cargos (roles), categorias, canais e configurações. Quase tudo é controlado por **permissões**: um bitfield por cargo, mais overrides por canal/categoria. O dono e quem tem ADMINISTRATOR ignoram tudo. A hierarquia de cargos (posição na lista) limita quem pode moderar/editar quem. Sobre isso há camadas opcionais: Comunidade (regras, onboarding, triagem, descoberta), boost (níveis que liberam ícone animado, banner, mais emojis), expressões (emojis/stickers) e convites.

Ordem de implementação recomendada no Telacast: cargos+permissões (base de tudo) -> categorias/overrides -> moderação (kick/ban/timeout) -> convites avançados -> personalização -> Comunidade.

## 2. Funcionalidades

### 2.1 Criação e configurações do servidor
- **O que faz:** criar com nome e ícone (modelos: "Para mim e meus amigos", "Clube", "Comunidade"...). Em Configurações: visão geral (nome, ícone, canal do sistema, canal AFK + timeout, notificações padrão), segurança, cargos, emojis, stickers, convites, membros, auditoria, integrações, comunidade, etc. [CONHECIMENTO]
- **Modelo [DOC]:** `name` 2–100 chars, `icon`, `splash` (fundo do convite), `banner`, `description`, `owner_id`, `preferred_locale`, `afk_channel_id`, `afk_timeout`, `system_channel_id`, `rules_channel_id`, `public_updates_channel_id`, `safety_alerts_channel_id`, `max_members`, `features[]` (strings), `premium_tier` 0–3, `premium_subscription_count`.
- **Notificações padrão [DOC]:** 0 todas as mensagens, 1 só menções.
- **System channel flags (bitfield) [DOC]:** 1<<0 sem aviso de entrada, 1<<1 sem aviso de boost, 1<<2 sem dicas de setup, 1<<3 sem botão "dar oi com sticker".
- **Transferir propriedade / excluir servidor:** só o dono; exclusão pede confirmação (e 2FA no Discord). [CONHECIMENTO]

### 2.2 Ícone, banner, splash
- Ícone (circular, animado exige boost/feature `ANIMATED_ICON`), banner do servidor (feature `BANNER`, boost nível 2), splash do convite (nível 1). Armazenados por hash de imagem. [DOC para campos; níveis = CONHECIMENTO]
- Usuário vê: ícone na barra lateral esquerda, banner no topo da lista de canais, splash na tela de convite.

### 2.3 Convites e links
- **Campos [DOC]:** `code`, `uses`, `max_uses`, `max_age` (segundos), `temporary` (membro temporário: some ao desconectar se não receber cargo), `expires_at`, canal alvo e criador. Valores 0 = ilimitado [CONHECIMENTO; a doc buscada não explica o 0].
- **UI [CONHECIMENTO]:** diálogo "Convidar amigos" com expiração (30 min, 1h, 6h, 12h, 1 dia, 7 dias, nunca) e máx. de usos (1, 5, 10, 25, 50, 100, sem limite); gerenciamento de convites ativos em Configurações > Convites (quem criou, usos, expira em, revogar).
- **URL vanity** (discord.gg/nome): feature `VANITY_URL` (boost nível 3 / comunidade). **Pausar convites:** feature `INVITES_DISABLED`, editável por quem tem Manage Guild. [DOC para a flag]
- Permissão: CREATE_INSTANT_INVITE (bit 0).

### 2.4 Cargos (roles) e hierarquia — MODELO DETALHADO
- **Campos de um cargo [CONHECIMENTO + DOC parcial]:** `id`, `name`, `color` (int RGB; 0 = sem cor), `hoist` (exibir separado na lista de membros), `position` (ordem), `permissions` (bitfield como string/u64), `managed` (cargo de bot/integração, não atribuível manualmente), `mentionable`, ícone/emoji do cargo (feature `ROLE_ICONS`, boost 2).
- **@everyone:** cargo implícito com `id == guild_id`, todo membro o tem; seu bitfield é a base. Posição 0, não removível, não deletável. [DOC/CONHECIMENTO]
- **Membro:** lista de IDs de cargo. Cargos exibem cor do nome (usa o cargo mais alto com cor) e agrupam na lista (hoist).
- **Hierarquia [DOC]:** baseada na **maior posição de cargo** do ator. Um ator só pode: criar/editar/reordenar/atribuir cargos **abaixo** do seu mais alto, e somente com permissões que ele mesmo possui; kick, ban e mudar apelido só em quem tem cargo mais alto **abaixo** do dele. Dono ignora a hierarquia. Fora isso, a ordem NÃO decide conflito de permissão: cargos só **somam** (OR); se um nega e outro permite em override de canal, o permitir vence. [DOC]
- **Restrições extras [CONHECIMENTO]:** ninguém edita o próprio cargo mais alto além do que a hierarquia deixa; não se pode dar a outro um cargo igual/acima do seu; MANAGE_ROLES é necessário para tudo isso.
- **Regra de segurança [INFERÊNCIA]:** ao editar bitfield de cargo, rejeitar bits que o ator não tem (exceto dono/admin), no servidor, nunca só no cliente.

### 2.5 Sistema de permissões (bitfield)
Bits (u64; ids 0–52, o 47 é vago) [DOC]:

| Grupo | Flags (bit) |
|---|---|
| Gerais | CREATE_INSTANT_INVITE 0, KICK_MEMBERS 1, BAN_MEMBERS 2, ADMINISTRATOR 3, MANAGE_CHANNELS 4, MANAGE_GUILD 5, VIEW_AUDIT_LOG 7, VIEW_CHANNEL 10, MANAGE_ROLES 28, MANAGE_WEBHOOKS 29, MANAGE_GUILD_EXPRESSIONS 30, VIEW_GUILD_INSIGHTS 19, CHANGE_NICKNAME 26, MANAGE_NICKNAMES 27, MODERATE_MEMBERS (timeout) 40, CREATE_GUILD_EXPRESSIONS 43, MANAGE_EVENTS 33, CREATE_EVENTS 44 |
| Texto | SEND_MESSAGES 11, SEND_TTS 12, MANAGE_MESSAGES 13, EMBED_LINKS 14, ATTACH_FILES 15, READ_MESSAGE_HISTORY 16, MENTION_EVERYONE 17, ADD_REACTIONS 6, USE_EXTERNAL_EMOJIS 18, USE_EXTERNAL_STICKERS 37, USE_APPLICATION_COMMANDS 31, SEND_VOICE_MESSAGES 46, SEND_POLLS 49, PIN_MESSAGES 51, BYPASS_SLOWMODE 52 |
| Threads | MANAGE_THREADS 34, CREATE_PUBLIC_THREADS 35, CREATE_PRIVATE_THREADS 36, SEND_MESSAGES_IN_THREADS 38 |
| Voz | CONNECT 20, SPEAK 21, STREAM 9, MUTE_MEMBERS 22, DEAFEN_MEMBERS 23, MOVE_MEMBERS 24, USE_VAD 25, PRIORITY_SPEAKER 8, REQUEST_TO_SPEAK 32, USE_SOUNDBOARD 42, SET_VOICE_CHANNEL_STATUS 48 |
| Outros | USE_EMBEDDED_ACTIVITIES 39, USE_EXTERNAL_APPS 50, USE_EXTERNAL_SOUNDS 45, VIEW_CREATOR_MONETIZATION_ANALYTICS 41 |

Algoritmo de cálculo [DOC]:
1. **Base (nível servidor):** dono => tudo. Senão `perms = @everyone.permissions`; `perms |= role.permissions` para cada cargo do membro; se `perms & ADMINISTRATOR` => tudo.
2. **Overrides de canal** (pulados se admin): (a) override do @everyone: `perms &= ~deny; perms |= allow`; (b) overrides dos cargos do membro: **agrega** todos os `deny` num só e todos os `allow` num só, depois `perms &= ~denyAgg; perms |= allowAgg`; (c) override do membro específico: `&= ~deny; |= allow`.
3. Ordem de precedência efetiva: base < @everyone-canal < cargos-canal < membro-canal. Dentro dos cargos, allow vence deny.
- **Overwrite** = `{id, type (0 cargo, 1 membro), allow, deny}`; um bit em allow é "✓", em deny "✗", em nenhum "/" (herdar). Allow e deny do mesmo bit são mutuamente exclusivos (UI tri-estado).
- **Categorias e sincronização [CONHECIMENTO]:** canal filho que está "sincronizado" copia os overrides da categoria; ao alterar manualmente deixa de estar sincronizado (sem flag guardada: compara-se com o pai). Canais herdam ao serem movidos para a categoria quando sincronizados. [INFERÊNCIA: guardar `parent_id` e calcular "sincronizado" por igualdade de overwrites.]
- **Implícitas [DOC]:** negar VIEW_CHANNEL torna o resto irrelevante naquele canal; negar SEND_MESSAGES invalida MENTION_EVERYONE, SEND_TTS, ATTACH_FILES, EMBED_LINKS; negar CONNECT em voz invalida o restante de voz. [Implementar como máscara final: se !VIEW_CHANNEL => 0 (para canal).]
- **Permissões de Timeout [CONHECIMENTO]:** membro em timeout fica só com VIEW_CHANNEL e READ_MESSAGE_HISTORY até `communication_disabled_until`.
- **Persistência [INFERÊNCIA]:** u64 em SQLite exige cuidado (i64 assinado); armazenar como INTEGER usando bit 63 livre (máx. bit 52, cabe). Enviar como string/u64 no protocolo.
- **Visualizar como cargo / testar permissões:** "Ver canal como cargo" no editor. [CONHECIMENTO]

### 2.6 Categorias e canais
- Categoria agrupa canais, recolhível, com overrides que os filhos herdam. Tem `position`; canais têm `parent_id` e `position`; reordenação por arrastar. Permissão: MANAGE_CHANNELS. Canais privados = override negando VIEW_CHANNEL ao @everyone. [CONHECIMENTO]
- Detalhes de tipos de canal (voz, fórum, etc.) pertencem a outra área do mapeamento.

### 2.7 Membros, apelidos e lista de membros
- **Membro [DOC]:** `user`, `nick` (apelido por servidor), `avatar`/`banner` por servidor, `roles[]`, `joined_at`, `premium_since` (boost), `deaf`, `mute`, `communication_disabled_until` (timeout), `pending` (triagem), `flags` (ex.: COMPLETED_ONBOARDING, BYPASSES_VERIFICATION).
- **Apelido:** CHANGE_NICKNAME (próprio), MANAGE_NICKNAMES (de outros, respeita hierarquia). Exibição: apelido > nome global > username. [DOC permissões; ordem = CONHECIMENTO]
- **Lista de membros [CONHECIMENTO]:** barra à direita; agrupa por cargo `hoist` (ordenado por posição), depois "Online" e "Offline"; mostra coroa para o dono, cor do cargo, avatar, status, atividade. O Discord carrega a lista por faixas (lazy) ao rolar; só mostra quem pode ver o canal. [INFERÊNCIA: no Telacast, filtrar por VIEW_CHANNEL do canal selecionado.]
- **Moderação:** expulsar (KICK, 1), banir (BAN, 2; opção de apagar mensagens recentes até 7 dias; lista de banidos com desbanir), timeout (MODERATE_MEMBERS: 60 s, 5 min, 10 min, 1 h, 1 dia, 1 semana), mutar/ensurdecer em voz. Auditoria registra tudo. [CONHECIMENTO]
- **Perfil de membro:** clicar abre popout com cargos, "membro desde", botões de gerenciar cargos (+). [CONHECIMENTO]

### 2.8 Log de auditoria
- Registro de ações (criar/editar/apagar canal, cargo, convite, kick, ban, mudança de permissões), com autor, alvo, motivo opcional; retenção 45 dias; filtros por usuário e tipo. Permissão VIEW_AUDIT_LOG. [CONHECIMENTO] Útil e barato num servidor próprio. [INFERÊNCIA]

### 2.9 Segurança do servidor
- **Nível de verificação [DOC]:** 0 nenhum, 1 e-mail verificado, 2 conta com >5 min, 3 membro há >10 min, 4 telefone verificado.
- **Filtro de conteúdo explícito [DOC]:** 0 desligado, 1 membros sem cargo, 2 todos.
- **MFA para moderação [DOC]:** 0/1.
- **AutoMod [CONHECIMENTO]:** regras de palavras-chave/spam/menções com ação (bloquear, alertar, timeout). **Raid protection** (`RAID_ALERTS_DISABLED`), pausa de convites e DMs. Parcial [DOC].

### 2.10 Boost
- Membros "impulsionam" o servidor (assinatura paga); `premium_tier` 0–3 pelo número de boosts (2/7/14 no desenho conhecido [CONHECIMENTO]). Níveis liberam: mais slots de emoji (50/100/150/250), qualidade de áudio/stream, banner, splash, ícone animado, URL vanity, ícone de cargo, upload maior. Membro tem `premium_since`, selo de boost ao lado do nome.
- **Sem dinheiro no Telacast [INFERÊNCIA]:** pode virar apenas "nível definido pelo dono" ou ignorar (P3).

### 2.11 Comunidade, regras, triagem, onboarding
- **Comunidade [DOC feature `COMMUNITY`]:** ativada por Administrator; exige nível de verificação >= 1... (requisitos exatos: e-mail verificado, filtro de conteúdo, 2FA para mods [CONHECIMENTO]), canal de regras (`rules_channel_id`) e de atualizações (`public_updates_channel_id`). Libera anúncios, fóruns, palco, descoberta, insights.
- **Triagem de membros (Membership Screening) [CONHECIMENTO]:** ao entrar, o usuário vê regras e precisa aceitar; enquanto isso `pending = true` e não pode falar/ver. Após aceitar, `pending = false`.
- **Welcome Screen [DOC]:** `description` + até 5 `welcome_channels` (channel_id, description, emoji).
- **Onboarding [DOC]:** `enabled`, `default_channel_ids`, `mode`, `prompts[]` (tipo multi/dropdown, `single_select`, `required`, `in_onboarding`) com `options` que atribuem `channel_ids` e `role_ids`. Restrição: ao ativar exige >= 7 canais padrão, 5 dos quais com envio liberado para @everyone. Flags de membro: STARTED_ONBOARDING/COMPLETED_ONBOARDING.
- **Cargos automáticos por perguntas** = onboarding vira atribuição de cargos [INFERÊNCIA útil para Telacast].

### 2.12 Descoberta (Server Discovery)
- Diretório público (`DISCOVERABLE`, só Comunidade, requisitos de tamanho/atividade, aprovação do Discord). Servidores parceiros/verificados têm selos (`PARTNERED`, `VERIFIED`). [DOC: features; requisitos = CONHECIMENTO]
- **Telacast [INFERÊNCIA]:** servidor próprio => diretório local opcional; baixa prioridade.

### 2.13 Emojis e stickers do servidor
- Emoji: nome, imagem (PNG/GIF/WEBP até 256 KB), `animated`, `roles` (restrito a cargos), uso entre servidores via USE_EXTERNAL_EMOJIS. Slots por nível de boost. Stickers: PNG/APNG/Lottie, nome, tags, descrição. Permissões: CREATE_GUILD_EXPRESSIONS / MANAGE_GUILD_EXPRESSIONS. [DOC permissões; limites = CONHECIMENTO]
- Soundboard (sons) entra no mesmo grupo de permissões [DOC]; ignorar.

### 2.14 Eventos agendados, widget e outros do servidor
- Eventos agendados (CREATE_EVENTS/MANAGE_EVENTS), widget do servidor (`widget_channel_id`), webhooks (MANAGE_WEBHOOKS), integrações/bots (cargos `managed`), insights (VIEW_GUILD_INSIGHTS), tag de servidor, servidores "em destaque" no perfil. [DOC campos; funcionalidades = CONHECIMENTO]
- Lista de servidores do usuário: barra esquerda com ícones, ordenação arrastável, pastas de servidores, indicadores de não lido/menção, silenciar servidor, notificações por servidor/canal/categoria. [CONHECIMENTO] (Notificações podem ser tratadas por outra área.)

## 3. O que o Telacast já tem vs falta

**Já tem (informado pelo briefing):** criar servidor; convite por código; entrar por convite; canais de texto criados só pelo dono; lista de membros online/offline com marca de dono.

**Falta (resumo):**
- Cargos, @everyone, bitfield de permissões, hierarquia, cores/hoist na lista.
- Overrides por canal e categoria; categorias; canais privados.
- Moderação (kick/ban/timeout), apelidos por servidor, auditoria.
- Configurações do servidor (nome, ícone, banner, transferir dono, excluir, sair).
- Convites com expiração/usos máximos/revogação e gerenciamento.
- Segurança (verificação, filtro, AutoMod), triagem/regras/onboarding, Comunidade.
- Emojis/stickers do servidor, boost, descoberta, eventos, webhooks.

## 4. Prioridades

| Prio | Item | Esforço | Motivo |
|---|---|---|---|
| P0 | Cargos + @everyone + bitfield u64 + cálculo de permissões no servidor (base + overrides) | L | Tudo depende disso; hoje só o dono age. Implementar uma função única `compute_permissions(member, channel)` testável. |
| P0 | Hierarquia (posição) e checagem "só concede o que tem" | M | Falha de segurança se faltar. |
| P0 | Categorias e `parent_id`; canais privados (deny VIEW_CHANNEL) | M | Organização mínima e uso de overrides. |
| P0 | Sair do servidor, editar nome/ícone, excluir (dono) | S | Básico de configuração. |
| P1 | Editor de cargos na UI (cor, hoist, mencionável, permissões tri-estado) | L | Torna o sistema utilizável. |
| P1 | Atribuir cargos a membros; lista de membros agrupada por cargo hoist, cor do nome | M | Visibilidade do sistema. |
| P1 | Kick, ban (+ lista de banidos), apelidos | M | Moderação essencial. |
| P1 | Convites: expiração, usos máx., revogar, lista; pausar convites | M | Controle de entrada. |
| P1 | Overrides por canal/categoria na UI + sincronização com categoria | M | Completa o modelo. |
| P1 | Filtro da lista de membros por VIEW_CHANNEL | S | Evita vazamento de quem vê o canal. |
| P2 | Timeout (MODERATE_MEMBERS), log de auditoria | M | Moderação e rastreio. |
| P2 | Regras + triagem (`pending`), nível de verificação simples | M | Comunidades maiores. |
| P2 | Emojis do servidor (upload, nome, uso no chat) | M | Personalização muito visível. |
| P2 | Banner, splash, canal do sistema, mensagens de boas-vindas | S | Barato e visual. |
| P2 | Transferir propriedade | S | Segurança operacional. |
| P2 | Onboarding com perguntas/cargos automáticos, Welcome Screen | L | Só após cargos maduros. |
| P2 | Stickers, eventos agendados, webhooks | M cada | Úteis, não essenciais. |
| P3 | Boost pago, níveis, descoberta pública, parceiro/verificado, insights, monetização, soundboard, tag de servidor | M–L | Dependem de infraestrutura/pagamentos/escala que o clone não tem. |
| P3 | AutoMod avançado, vanity URL | M | Só se houver demanda. |

Notas de modelagem [INFERÊNCIA]:
- Tabelas: `roles(id, guild_id, name, color, hoist, position, permissions INTEGER, mentionable, managed)`, `member_roles(guild_id, user_id, role_id)`, `channel_overwrites(channel_id, target_id, target_type, allow, deny)`, `invites(code, guild_id, channel_id, creator, max_uses, uses, expires_at, temporary)`, `bans(guild_id, user_id, reason)`.
- @everyone: role com `id == guild_id`; criar junto ao servidor com permissões padrão (VIEW_CHANNEL, SEND_MESSAGES, READ_MESSAGE_HISTORY, ADD_REACTIONS, EMBED_LINKS, ATTACH_FILES, CONNECT, SPEAK, USE_VAD, CREATE_INSTANT_INVITE, CHANGE_NICKNAME...).
- Invalidar cache de permissões ao mudar cargo/override/membro e emitir eventos ao cliente para atualizar a UI (canais ocultos somem).
- Dono é imutável via flag `owner_id` e nunca depende de cargos.

## 5. Fontes consultadas
- https://docs.discord.com/developers/topics/permissions (antes discord.com/developers/docs/topics/permissions): lista de bits e algoritmo de cálculo, hierarquia, permissões implícitas. [consultada nesta sessão]
- https://docs.discord.com/developers/resources/guild: campos de Guild, Member, Welcome Screen, Onboarding, features, flags. [consultada]
- https://docs.discord.com/developers/resources/invite: campos de convite (a página não detalha o significado de 0). [consultada]
- Cliente instalado em /snap/discord/current/usr/share/discord: apenas listado (sem inspeção; é Electron, sem conteúdo útil sem ferramentas pesadas).
- Itens marcados [CONHECIMENTO] (níveis de boost, UI de convite, tempos de timeout, requisitos de Comunidade, limites de emoji, retenção da auditoria) vêm de conhecimento geral do produto/support.discord.com e **não foram reverificados** nesta sessão; validar antes de copiar números exatos.
