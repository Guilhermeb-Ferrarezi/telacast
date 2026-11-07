# R1 - Revisão do ROADMAP: coerência, ordem e dependências

## Veredito geral

O roadmap está bem organizado e a espinha dorsal (fundação, chat, voz, cargos, extras) é defensável. Os problemas não estão na
sequência das fases, e sim em três pontos. Primeiro, a Fase 0 mistura duas coisas diferentes (endurecer para expor na internet
e decidir o modelo de dados) e está inchada para um primeiro marco. Segundo, há inversões pontuais em que a fase N usa algo que
só nasce na fase N+k (tela de Configurações, permissões, HTTP autenticado, `seq`/Resume antes dos eventos incrementais).
Terceiro, faltam itens de engenharia que o roadmap assume sem listar: migrações de banco, a troca do Mutex global do SQLite,
backup/observabilidade/CI. Nada disso exige reescrever o plano; são cerca de 10 ajustes de ordem. Confiança: média-alta. Li o
roadmap inteiro, as seções de prioridade e dependência dos 10 relatórios, e conferi o código (`db.rs` só tem
`CREATE TABLE IF NOT EXISTS`, sem versão de esquema). Não li cada relatório linha a linha, e os números do Discord continuam
"de memória", como o próprio roadmap avisa.

## Problemas, do mais grave ao menos grave

### 1. Não há mecanismo de migração de banco (falta item) - GRAVE
- **Item:** 0.1 ("ainda não há dados reais, migrar depois custa caro").
- **Problema:** o argumento vale só até o primeiro dado real. O `db.rs` usa `CREATE TABLE IF NOT EXISTS` e não tem
  `PRAGMA user_version`. Quando o servidor estiver na internet (Marco A), cada fase seguinte (cargos, FTS5, anexos, reações)
  vai alterar o esquema. Sem migrações versionadas, a primeira alteração depois do Marco A é manual ou destrutiva. Recriar o
  `.db` só é aceitável até o Marco A; depois dele não é.
- **Evidência:** `server/src/db.rs` linhas 19-46 (só `CREATE TABLE IF NOT EXISTS`); roadmap 0.1 e Marco A.
- **Correção:** primeiro item da Fase 0: runner de migrações (`user_version` + lista de `.sql` embutidos) e política "esquema
  só muda por migração a partir do Marco A". O snowflake e as tabelas novas entram como migração 001.

### 2. `seq` + Resume (0.3) é feito antes dos eventos incrementais (Fase 1) - risco de retrabalho - GRAVE
- **Item:** 0.3 "`seq` nos eventos + Resume com buffer" (P1) versus Fase 1 "Eventos incrementais" (L).
- **Problema:** hoje o servidor reenvia Snapshot completo. Um buffer de Resume precisa de eventos discretos para repetir; sobre
  Snapshot ele só faria "reenviar o último snapshot". O desenho do buffer (o que é um evento, qual o escopo do `seq`: por
  conexão, por usuário ou por servidor) depende do catálogo de eventos da Fase 1. Se for feito na 0.3, será refeito.
- **Evidência:** [R06] seção de prioridades: "(2) seq+Resume+re-sync; (3) incrementais"; o próprio roadmap admite em Fase 1
  que o Snapshot amplifica a carga ([R07] #12).
- **Correção:** tirar `seq`/Resume da Fase 0 (e do critério do Marco A, ver item 7). Fazer o re-sync por `after` (barato,
  depende só de snowflake e paginação) no lugar do Resume na Fase 0/1, e `seq`+Resume junto dos eventos incrementais.
  Para o Marco A bastam heartbeat, reconexão e re-sync por `after`.

### 3. Segurança de HTTP e de upload entra sem estar na Fase 0 - GRAVE
- **Item:** Fase 1 "Anexos: upload por HTTP"; Fase 4 "upload de avatar"; Fase 5 "webhooks" e API de bot.
- **Problema:** toda a segurança da Fase 0 (rate limit, tokens, ticket de uso único, TLS) é descrita para WebSocket. O primeiro
  endpoint HTTP autenticado aparece na Fase 1 e nenhum item diz como ele autentica, aplica rate limit, limita tamanho de
  corpo ou serve arquivo (cabeçalhos `Content-Disposition`, tipo por magic bytes, [R07] P3 "filtro de mídia"). Também a
  tabela de tokens por hash/expiração (0.2) precisa valer para HTTP (`Authorization: Bearer`), não só para o frame WS.
- **Evidência:** [R07] P3 "Filtro de mídia/validação de upload"; [R03] linha 123 ("segurança de tipos"); roadmap 0.2 só cita
  `Send`, `JoinServer`, login.
- **Correção:** em 0.2, definir a camada de autenticação **comum** (extractor axum que valida token por hash) e o rate limit
  genérico (middleware/`tower_governor`), usável por WS e HTTP. Mover a "validação de upload" de P3 para requisito do item
  Anexos (Fase 1), não opcional. Decidir o limite de tamanho e o diretório de armazenamento (decisão em aberto nº 3) antes
  de começar a Fase 1, não depois.

### 4. A Fase 0 está inchada para um primeiro marco - GRAVE (de planejamento)
- **Item:** toda a Fase 0 (aprox. 20 itens: 8 M, ~8 S, 1 M "DM vira canal").
- **Problema:** o Marco A ("pode expor na internet") exige só segurança + robustez de conexão, mas a fase também carrega
  decisões de modelo de dados (snowflake, `kind`/`parent_id`, DM como canal, 7 tabelas "previstas") e itens de cliente que não
  bloqueiam exposição (instância única, keyring, `PROTO_VERSION`). Resultado: o marco fica longe e a única coisa que o
  usuário vê é "nada mudou na tela".
- **Correção:** dividir em **0a Endurecer** (migrações, testes, TLS, rate limit, tokens, cadastro por convite, limites,
  Mutex/`spawn_blocking`, heartbeat, reconexão, nonce, ErrorCode, backup) = Marco A; e **0b Modelo** (snowflake, `display_name`,
  `channels.kind/parent_id`, `channel_members`) que vira o início da Fase 1. Rebaixar instância única e `PROTO_VERSION` para
  "quando conveniente"; o keyring pode ir com tokens-com-hash, que é a mesma discussão.

### 5. Criar o esquema de 7 tabelas "para as fases seguintes" (0.1) - risco de retrabalho
- **Item:** 0.1 "`read_state`, `roles`, `member_roles`, `channel_overwrites`, `bans`, `invites`, `audit` - criar o esquema junto".
- **Problema:** esquema sem código que o use é especulativo e tende a mudar quando a fase real chega (por exemplo, `bans` por
  servidor, `audit` com campo de alvo, `read_state` com contagem de menções e silenciar, que só se define depois de
  Menções). É retrabalho certo, e sem migrações (item 1) ainda dói. Além disso `invites` já é usada na própria Fase 0
  (cadastro por convite da instância) e "Convites avançados" reaparece na Fase 3: item duplicado.
- **Evidência:** roadmap 0.2 (convites 128 bits, expiração, limite de usos, revogável) versus Fase 3 "Convites avançados:
  expiração, usos máximos, revogar, listar" (mesmas funções); [R07] separa só uma vez (P1).
- **Correção:** criar cada tabela na fase que a usa, via migração. Manter na 0.1 apenas o que muda a forma de **todas** as
  outras (snowflake, DM como canal). Unificar convites: a Fase 0 entrega a tabela e o CLI; a Fase 3 só adiciona UI/listagem
  para convites de *servidor*, e o texto deve deixar claro que são dois tipos de convite (instância e guild).

### 6. Tela de Configurações (Fase 4) é exigida pela Fase 2 - inversão
- **Item:** Fase 2 "seleção de dispositivos, volume por usuário, push-to-talk"; Fase 4 P0 "tela de Configurações (Voz...)".
- **Problema:** dispositivos de áudio, PTT e VAD precisam de uma tela de configurações e de persistência de preferências
  (config local em arquivo), que só existem na Fase 4. A voz seria entregue sem como escolher microfone, ou com uma tela
  provisória que será jogada fora.
- **Evidência:** [R10] linha 103 ("Configuracoes... so 'Sair'... Persistir em arquivo (serde + dir de config)"); roadmap Fase 2 P1.
- **Correção:** puxar para a Fase 1 (junto da "Base de UI") um esqueleto de Configurações com seções vazias e persistência de
  preferências local; a Fase 2 preenche a seção Voz, a Fase 4 preenche Conta/Aparência. O mesmo vale para o canal de
  notificações (som, DND), que a Fase 1 já precisa para "menção".

### 7. Critério do Marco A incoerente e sem moderação mínima
- **Item:** "Marco A: quando 0.2 e 0.3 (P0/P1) estiverem prontos".
- **Problemas:** (a) 0.3 P1 inclui `seq`+Resume (M), que pelo item 2 não deve estar. (b) O Marco A expõe o servidor com
  cadastro fechado, mas [R07] #8 ("sem kick/ban/sair do servidor... irremediável") é Alta e só entra na Fase 3, depois de
  cargos (L). Kick/ban/sair/apagar mensagem **não dependem de cargos**: o dono é suficiente ([R07] P1 diz "é o mínimo de moderação").
  (c) "Sair do servidor" é S e está colado a "editar/excluir servidor" na Fase 3 sem motivo.
- **Correção:** criar "moderação mínima do dono" (kick, ban com tabela `bans`, sair do servidor, apagar mensagem de qualquer
  um) na Fase 1, antes de cargos. Quando houver cargos, a checagem passa a ser `compute_permissions`, trocando uma linha. É
  a ordem de menor retrabalho porque o ponto de checagem (`can(member, perm, channel)`) já existe como função, mesmo que de
  início só olhe `owner_id`.

### 8. Fixar mensagem e apagar mensagem dos outros dependem de permissão, mas vêm antes dela
- **Item:** Fase 1 "fixadas", "editar e excluir" versus Fase 3 "cargos".
- **Problema:** [R03] linha 126 diz explicitamente "Fixar... depende de permissões". Sem cargos, fixar vira "só o dono" ou
  "qualquer um"; depois muda para `MANAGE_MESSAGES`. Se a Fase 1 espalhar `if user == owner` pelo código, a Fase 3 terá de
  caçar todos.
- **Correção:** na Fase 1 introduzir já a função única `can(member, Permission, channel) -> bool`, com implementação provisória
  (dono = tudo, autor = editar/apagar o próprio, demais = básico). Fase 3 só troca o corpo. Também o enum `Permission` e a
  decisão do bitfield (u64 em `i64`, [R02] linha 57) devem ser definidos agora.

### 9. Mutex global do SQLite e async não estão no roadmap
- **Item:** ausente (aparece como [R07] #16, Média).
- **Problema:** o roadmap pede rate limit, `spawn_blocking` para Argon2, canais limitados, FTS5, uploads e fan-out de
  eventos, tudo sobre um `Mutex<Connection>` usado dentro de handlers async, com `.lock().unwrap()` que envenena. Isso
  limita justamente o que se promete na Fase 1 (busca FTS5, paginação, eventos). Se for resolvido tarde, mexe em todas as chamadas de `db.rs`.
- **Correção:** item da Fase 0a: encapsular o banco numa camada (`Db` com `async fn` que roda em `spawn_blocking` ou pool
  r2d2/deadpool de leitura + 1 escritor, WAL já está ligado), sem `unwrap` em lock. Isso também viabiliza os testes (item 10).

### 10. Testes e CI chegam tarde e são pequenos demais
- **Item:** 0.3 último item "Testes automáticos do servidor... S/M [minha decisão]".
- **Problema:** é o último da lista, mas é a rede de proteção das demais refatorações da Fase 0 (snowflake mexe em todas as
  tabelas; rate limit; tokens). "S/M" subestima: como `main.rs` tem 522 linhas com `AppState` global, precisa de uma
  forma de subir o servidor em porta efêmera com banco em memória e um cliente de teste do protocolo. A Fase 3 cobra testes
  de `compute_permissions`, mas não há infra para eles.
- **Correção:** colocar como **primeiro** item executável da Fase 0a, **M**: harness (servidor em memória + cliente WS de
  teste), 6 a 8 testes de fumaça (cadastro, login, criar servidor, enviar, história, amizade, DM) e CI local simples
  (`cargo test`, `cargo clippy`). Observação: respeitar a RAM limitada, rodando os testes só do servidor.

### 11. Estimativas de esforço discrepantes
- **Snowflake: S no roadmap, M no [R06] (linha 133).** Ele toca todas as tabelas, todas as FKs, `proto` (tipos de id), o
  app inteiro (1207 linhas em `main.rs`) e precisa tratar relógio que volta e estado do nó/seq sob o lock. Sem dados é
  mais fácil, mas ainda é **M**.
- **"DM vira canal com `channel_members`": entra dentro de um M junto de `kind`/`parent_id`.** Muda a tabela de mensagens
  (hoje separa DM de canal), `dm_members`, `Snapshot`, `OpenDm`, não lidos. Realista: **M-L** sozinho.
- **Eventos incrementais: L, correto**, mas é o item de maior risco da Fase 1 e está listado primeiro sem sub-etapas.
  Sugestão: 3 sub-entregas (presença+membros, mensagens, estrutura) com `RequestSnapshot` mantido; o [R06] fala em
  "começando por presença e membros", o roadmap diz "presença e mensagens"; escolher um dos dois.
- **Markdown M-L:** razoável. **Busca FTS5 M:** razoável, mas exige triggers e migração (item 1) e respeito a permissões
  de canal (depende do item 8).
- **Cargos P0 L + Hierarquia M + Categorias/canais privados M + UI L:** o total de cargos (aprox. L+M+M+L) está
  bem dimensionado; **TLS M** pode ser **S** se ficar com Caddy (decisão tomada), mas o cliente rustls e o pinning/recusa de
  `ws://` mantêm M. **Rate limit M**: razoável.
- **Backup do `.db` sem estimativa:** S, mas precisa existir (item 12).

### 12. Faltam itens de operação (deploy, backup, observabilidade)
- **Item:** menção solta no Marco A ("plano da VPS da Oracle (Caddy + systemd + DuckDNS + backup do .db)"); nada nas fases.
- **Problema:** expor um servidor na internet sem backup testado, sem logs estruturados, sem métricas mínimas, sem
  política de retenção e sem log de segurança ([R07] #18, #17) é incoerente com o grau de cuidado do resto da Fase 0.
  Backup de SQLite em WAL precisa de `VACUUM INTO`/API de backup, não cópia do arquivo.
- **Correção:** adicionar à 0a: `tracing` com log estruturado + log de segurança (login falho, ban, convite usado), endpoint
  `/healthz`, backup agendado via `VACUUM INTO` com restauração testada, unidade systemd, e uma linha sobre `Origin` ([R07] #19).
  Também o atalho `telacast-server invite` do CLI precisa de um subcomando `backup`.

### 13. Notificações e silenciar: pedaço da Fase 1 definido na Fase 4
- **Item:** Fase 1 "Não lidos... contagem de menções" versus Fase 4 P1 "silenciar servidor/canal/DM" e "notificação desktop".
- **Problema:** `read_state` e o badge de menção precisam saber se o canal está silenciado; modelar depois obriga a mudar o
  `read_state` e o cálculo de badge. Também "bloquear usuário" (Fase 4 P0) altera o que a Fase 1 mostra (esconder mensagens).
- **Correção:** ao definir `read_state` (Fase 1), já incluir `muted_until`/nível de notificação (ainda que a UI venha na Fase 4).
  Mover "bloquear" para logo após as DMs/menções da Fase 1 ou garantir que o esquema de `blocks` entre antes de mensagens em
  grupo.

### 14. Avatares e mensagem: refazer o layout duas vezes
- **Item:** Fase 1 (Markdown, respostas, reações, anexos, barra de hover) versus Fase 4 P0 "upload de avatar".
- **Problema:** a linha de mensagem do egui será desenhada e reescrita para cada recurso. Avatares (e cor de cargo na Fase
  3) mudam altura e agrupamento de cada linha, e a virtualização de altura variável ([R10] linha 123) precisa de cache
  de alturas. Fazer isso por último implica mexer em um código já complexo.
- **Correção:** na "Base de UI" da Fase 1, definir já o componente `MessageRow` com espaço para avatar (inicial colorida como
  placeholder), cabeçalho com cor de nome, e cache de alturas. O upload real do avatar fica na Fase 4, depois do
  armazenamento de anexos (dependência já correta).

### 15. Menores
- **Voz e vídeo no mesmo WebSocket:** a Fase 2 põe áudio Opus no WS atual, junto com quadros H.264 de até 16 MiB
  ([R07] #14). Um quadro grande atrasa o áudio atrás dele (bloqueio de cabeça de fila). Mitigar já na Fase 0 com limite de
  quadro, e na Fase 2 usar conexão WS separada para áudio ou priorizar a fila de saída. O roadmap não menciona.
- **Fase 4 "chamadas em DM" (P3) e "estado de chamada":** correto depender da Fase 2, mas exige o `channel_members` da 0.1;
  ok, apenas registrar.
- **Permissão de transmitir:** [R07] #10 diz que qualquer membro pode publicar; deveria ser a permissão `STREAM` já na Fase 3
  (não só "revalidar acesso").
- **Decisão em aberto nº 2 (libopus) bloqueia a Fase 2:** resolver antes de começar a Fase 2, de preferência com um
  spike pequeno (captura + Opus em loopback) feito durante a Fase 1 para reduzir risco.
- **Observação do roadmap sobre captura em processo separado** (Fase 4, "Arquitetura") deveria ser decisão da Fase 2, não
  da 4: o áudio do sistema na transmissão mexe nessa mesma parte.
- **Fase 5 webhooks (S):** depende de HTTP autenticado e token de bot (item 3); coerente, mas webhook sem bot só pode
  entrar depois da camada HTTP.
- **Dois "P0" no cliente (keyring e instância única) na Fase 0** com a menor ligação com o Marco A: ver item 4.

## Ordem sugerida, resumida

1. **0a (Marco A):** migrações, harness de testes, camada de banco async, TLS, rate limit/auth comum a WS+HTTP, tokens por
   hash, cadastro por convite, limites, heartbeat, reconexão, nonce, ErrorCode, re-sync por `after`, log/backup/healthz.
2. **Fase 1:** 0b (snowflake, `display_name`, canais com `kind`, DM como canal) -> `can()` provisório + moderação mínima do
   dono -> eventos incrementais com `seq`/Resume -> editar/apagar, paginação, base de UI com Configurações e `MessageRow` ->
   não lidos (com `muted_until`) -> markdown, menções, respostas -> reações, busca, fixadas -> anexos.
3. **Fases 2 a 5** como estão, com a seção Voz de Configurações, `STREAM` como permissão e o spike de Opus antecipado.

## O que está bom

- A decisão de começar pela segurança, antes de expor, e a ligação explícita entre cada item e o risco numerado do [R07].
- Separar "fatos do Discord" de "decisões de produto" e a seção "A confirmar": evita que números de memória virem requisito.
- Voz em duas etapas (WS agora, UDP depois) com o mesmo formato de quadro: evita retrabalho de protocolo de áudio.
- Cadastro fechado por convite com CLI resolve de uma vez spam e a criação da primeira conta.
- Reconhecer que cargos bloqueiam moderação/canais privados, e que voz não depende de cargos (ordem trocável).
- Anexos por HTTP, armazenamento por hash e avatar reaproveitando esse armazenamento: dependência correta.
- Tirar o token da URL do vídeo e `Unknown` nos enums: baratos e evitam quebra de cliente antigo.
- Lista clara de "fora do escopo", sem pretensão de monetização ou compatibilidade com a API do Discord.
- Função única `compute_permissions` com testes: a decisão certa (só falta antecipar a interface `can()`, item 8).
