# 01 — Contas, autenticação e perfil (mapa do Discord para o Telacast)

Legenda: **[F]** = fato confirmado em documentação oficial consultada nesta sessão; **[C]** = conhecimento geral do produto, não reconfirmado agora; **[I]** = inferência/sugestão minha para o Telacast.

## 1) Visão geral da área

O Discord separa **identidade** (username único + display name livre + apelidos por servidor/amigo), **segurança da conta** (senha, e-mail, MFA, sessões) e **perfil** (avatar, banner, cor, bio, pronomes, conexões, badges, status). Tudo isso alimenta o resto do produto: lista de membros, DMs, menções, presença. Para o Telacast (servidor próprio, sem dependência do Discord), o núcleo útil é: nome de exibição, avatar/banner/bio, status manual + status personalizado, sessões/dispositivos, 2FA TOTP, recuperação de senha, privacidade básica e exclusão de conta. Nitro e conexões externas são periféricos.

## 2) Funcionalidades

### 2.1 Cadastro e verificação
- **O que faz:** cria conta com e-mail, display name, username, senha, data de nascimento. [C]
- **Regras:** conta não verificada pode ter restrições; no sistema de usernames novo, conta não verificada recebe 5 dígitos aleatórios no fim do username após 7 dias [F]. Existem "contas provisórias" (FAQ oficial citada na lista de artigos) [F: só o título].
- **Modelo [I]:** `users(id, email, email_verified_at, password_hash, created_at, birthdate)`.

### 2.2 Username (identificador único)
- **Regras [F]:** 2–32 caracteres; minúsculas forçadas (case-insensitive); só a-z, 0-9, `_` e `.`; sem dois `.` seguidos; único globalmente; sem personificar Discord/staff/outros. Pode ser alterado até **2 vezes por semana** [F]. API: não pode conter `@ # : \`` nem "discord", nem ser `everyone`/`here` [F].
- **Histórico [F]:** antes de 2024 existia discriminador `#0000` (case-sensitive); removido.
- **Como o usuário vê:** Configurações > Minha conta; aparece em busca de usuário, pedidos de amizade e menções.

### 2.3 Display name e apelidos
- **Display name [F]:** 1–32 caracteres, qualquer caractere (espaços, emoji, não-latinos), não único, alterável sempre. É o nome principal em DMs, servidores e pedidos de amizade.
- **Apelido de servidor [F]:** definido em Perfis > Perfil por servidor; tem prioridade sobre display name naquele servidor.
- **Apelido de amigo [F]:** privado, só quem definiu vê; prioridade sobre display name em DMs.
- **Precedência [F]:** apelido de amigo / de servidor > display name > username. Menção pode ser buscada por display name ou username.

### 2.4 Login, senha e recuperação
- **Login [C]:** e-mail/telefone + senha; novo dispositivo/local pode exigir verificação por e-mail; QR code de login a partir do app móvel; login sem senha com passkey.
- **Recuperação de senha [C]:** link por e-mail com token de uso único e expiração curta; ao trocar a senha, todas as sessões são invalidadas.
- **Troca de e-mail/senha [C]:** exige senha atual (e código MFA se ativo). Artigos oficiais existem para ambos [F: títulos].
- **Modelo [I]:** `password_resets(token_hash, user_id, expires_at, used_at)`; rate limit por IP/conta.

### 2.5 MFA / 2FA
- **Tipos [F]:** Security keys/Passkeys (recomendado, resistente a phishing), app autenticador TOTP, SMS (só habilitável **depois** de ter app autenticador; não recomendado), e **códigos de backup** (uso único).
- **Regras [F]:** o suporte do Discord **não consegue remover MFA** da conta; códigos de backup só valem para login/gestão de identidade, não para registrar passkey; usuário pode visualizar/baixar os códigos e ver quais já foram usados.
- **Detalhes [C]:** 8 códigos de backup, regeneráveis; TOTP RFC 6238 de 6 dígitos; ativar/desativar exige senha. Algumas ações sensíveis exigem MFA para moderadores/admins de servidor.
- **Modelo [I]:** `mfa_totp(user_id, secret_enc, confirmed_at)`, `mfa_backup_codes(user_id, code_hash, used_at)`, `webauthn_credentials(...)` (P3).

### 2.6 Sessões e dispositivos
- **O que faz [C]:** lista de dispositivos autenticados (SO, navegador, localização aproximada, último uso), com "Sair" por dispositivo e "Sair de todos". Mudança de senha encerra todas.
- **Modelo [I]:** `sessions(id, user_id, token_hash, device_name, os, ip, created_at, last_seen_at, revoked_at)`; o Telacast já tem token, falta metadado + revogação por item.
- **Alternar contas [F título]:** "Account Switcher" permite várias contas salvas no cliente.

### 2.7 Avatar, banner e cor de destaque
- **Avatar [F]:** campo `avatar` (hash); API aceita imagem para alterar. **Banner [F]:** `banner` (hash). **Cor de destaque [F]:** `accent_color` (inteiro RGB).
- **Limites [C]:** avatar ~10 MB, formatos PNG/JPG/WebP/GIF; GIF animado e banner de perfil são recursos de Nitro. Avatar por servidor e banner por servidor também (Nitro).
- **Decoração de avatar e nameplate [F]:** `avatar_decoration_data` (hash + SKU) e `collectibles` (nameplate) — cosméticos pagos/loja.
- **Modelo [I]:** `users.avatar_id`, `users.banner_id` apontando para tabela de mídia; redimensionar no servidor (ex.: 128/512 px) e servir por hash.

### 2.8 Bio, pronomes e perfil
- **Bio [C]:** "Sobre mim", até 190 caracteres (Nitro: mais), com markdown limitado. **Pronomes [C]:** campo curto (~40 caracteres) exibido ao lado do nome. **Tema de perfil [C]:** duas cores (Nitro). **Perfis por servidor [F]:** apelido (e, com Nitro, avatar/banner/bio) diferentes por servidor.
- **Tag de servidor [F]:** `primary_guild` (identity guild, tag de até 4 caracteres, badge) — "etiqueta" ao lado do nome.
- **Cartão de perfil [C]:** mostra badges, membro desde, cargos no servidor, amigos/servidores em comum, atividades, nota privada sobre o usuário.
- **Notas privadas [C]:** texto visível só para quem escreveu.

### 2.9 Presença e status personalizado
- **Status manual [F]:** Online (verde), Ausente/Idle (automático por inatividade; manual fica indefinidamente), Não perturbe (vermelho; silencia notificações desktop), Invisível (aparece offline, usa tudo normal).
- **Status personalizado [F título; detalhes C]:** emoji + texto até 128 caracteres, com expiração: não limpar, 30 min, 1 h, 4 h, hoje.
- **Atividade [F título "Activity Sharing"]:** jogo/Spotify/etc. mostrado no perfil (periférico, ver outras áreas).
- **Modelo [I]:** `presence(user_id, status_manual, custom_emoji, custom_text, custom_expires_at)`; broadcast ao mudar; expirar via job/checagem preguiçosa.

### 2.10 Badges e flags públicos
- **Fato [F]:** `public_flags` é bitfield: Staff, Partner, HypeSquad (Events/Bravery/Brilliance/Balance), Bug Hunter 1/2, Early Nitro Supporter, Verified Bot, Early Verified Bot Developer, Moderator Programs Alumni, etc. `flags` e `public_flags` separados.
- **[I]:** para o Telacast, só "Admin/Staff", "Apoiador", "Bot" fazem sentido.

### 2.11 Conexões externas
- **Fato [F]:** objeto Connection: `id, name, type, revoked, integrations, verified, friend_sync, show_activity, two_way_link, visibility` (0 = só o usuário, 1 = todos). Existe "role connection" por aplicação (`platform_name` ≤50, `platform_username` ≤100, metadata chave-valor ≤100 chars) para cargos vinculados.
- **[C]:** Spotify, Steam, YouTube, Twitch, GitHub, X, etc. via OAuth; exibidas no perfil; Spotify permite "ouvir junto".

### 2.12 Nitro (diferencial pago)
- **Fato [F]:** `premium_type`: 0 nenhum, 1 Nitro Classic, 2 Nitro, 3 Nitro Basic.
- **Vantagens [C]:** upload maior, emoji/stickers em qualquer servidor, avatar/banner animado, bio maior, perfil por servidor, tema de perfil, streaming melhor, boosts de servidor.
- **[I]:** Telacast pode ter um único "plano apoiador" opcional, ou nada.

### 2.13 Configurações de conta e privacidade
- **Minha conta [C]:** username, e-mail, telefone, senha, MFA, "remover/desativar conta", exclusão.
- **Privacidade [C]:** quem pode enviar DM (membros de servidores), quem pode enviar pedido de amizade (todos / amigos de amigos / membros de servidores), filtro de conteúdo explícito em DMs, bloquear usuários, exibir atividade, pedido de exportação de dados (pacote em ZIP) e exclusão. Exclusão tem prazo de carência de ~14 dias [C]. Idade mínima: 13 anos; verificação de idade em alguns países [C].
- **Outras [C]:** idioma (`locale` [F]), tema claro/escuro, acessibilidade, notificações, teclado, voz, aparência.
- **Modelo [I]:** `user_settings(user_id, json)` e `blocks(user_id, blocked_id)`; flags de privacidade em colunas próprias para consultar no servidor.

## 3) Telacast: já tem vs falta

| Já tem | Falta |
|---|---|
| Cadastro e login usuário+senha (argon2) | E-mail, verificação, recuperação de senha, troca de senha/e-mail |
| Token de sessão | Lista de sessões com metadados, revogar uma/todas, expirar |
| Presença online/offline | Idle, DND, invisível, status personalizado (emoji + texto + expiração) |
| Avatar com inicial colorida | Upload de avatar, banner, cor de destaque |
| — | Display name separado do username; apelido por servidor/amigo |
| — | Bio, pronomes, cartão de perfil, nota privada |
| — | 2FA TOTP + códigos de backup |
| — | Bloqueio, privacidade de DM/amizade, exportar/excluir conta |
| — | Badges, conexões externas, Nitro-like |

## 4) Prioridade sugerida [I]

| Pri | Item | Esforço | Motivo |
|---|---|---|---|
| P0 | Display name separado do username (regras 2.2/2.3) | S | Base de identidade; evita retrabalho de schema |
| P0 | Status manual (online/idle/DND/invisível) + idle automático | S | Barato, muito visível, depende só de presença |
| P0 | Upload de avatar (redimensionar, hash, limite de tamanho) | M | Maior ganho visual; exige armazenamento de mídia |
| P0 | Lista de sessões + revogar + encerrar todas ao trocar senha | M | Segurança básica; token já existe |
| P1 | Status personalizado (emoji+texto+expiração) | S | Aproveita presença do P0 |
| P1 | Bio, pronomes, banner, cor de destaque, cartão de perfil | M | Expressão; schema simples |
| P1 | Troca de senha/e-mail com senha atual | S | Higiene de conta |
| P1 | Bloqueio de usuários e privacidade de DM/amizade | M | Segurança social |
| P1 | Recuperação de senha por e-mail | M | Exige envio de e-mail (SMTP) no servidor |
| P2 | 2FA TOTP + códigos de backup | M | Seguro e padrão aberto; fácil com crate `totp-rs` [I] |
| P2 | Apelido por servidor e por amigo | M | Depende da camada de servidores/amigos |
| P2 | Exportar dados e excluir conta (carência) | M | Boa prática/LGPD |
| P2 | Perfil por servidor (avatar/bio) | M | Só depois de apelidos |
| P2 | Nota privada sobre usuário | S | Trivial |
| P3 | Passkeys/WebAuthn | L | Complexo no egui; TOTP basta |
| P3 | SMS MFA, verificação telefone | L | Custo e fragilidade |
| P3 | Conexões externas (OAuth), role connections | L | Dependem de terceiros |
| P3 | Nitro, decorações, nameplates, tag de servidor | L | Monetização/cosmético; sem valor para o clone |
| P3 | Badges de HypeSquad/Bug Hunter etc. | S | Não aplicável; manter só Admin/Bot |

## 5) Fontes consultadas

- https://docs.discord.com/developers/resources/user (antigo discord.com/developers/docs/resources/user; redireciona) — User object, flags, premium_type, Connection, PATCH /users/@me, role connection. [F]
- https://support.discord.com/hc/en-us/articles/12620128861463-New-Usernames-Display-Names — regras de username, display name, apelidos, 2 trocas/semana. [F]
- https://support.discord.com/hc/en-us/articles/219576828-Setting-up-Two-Factor-Authentication — tipos de MFA, backup codes. [F]
- https://support.discord.com/hc/en-us/articles/227779547-Changing-Your-Online-Status-and-Custom-Status — status manual. [F]
- Títulos de artigos relacionados vistos na barra lateral do suporte (provisional accounts, trocar e-mail, reset de senha, excluir conta, logout, account switcher). Só títulos.
- Tentativas bloqueadas (HTTP 403 via WebFetch; contornado com o fetch de página pública): páginas de privacidade e exclusão de conta não foram lidas; seus detalhes são [C]/[I]. Página de exclusão (ID 212177037) devolveu 404.
- Cliente instalado em /snap/discord/current/usr/share/discord: só listado (app.asar presente), nada extraído; seção de interface vem de [C].
- Limitação: números como 190 caracteres de bio, 128 do status, 8 códigos de backup, 14 dias de exclusão vêm de memória e devem ser tratados como aproximados.
