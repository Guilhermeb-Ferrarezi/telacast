# Telacast

Clone **próprio** do Discord em Rust: cliente desktop em egui, servidor axum + PostgreSQL 18, vídeo H.264 (captura X11 + NVENC) por WebSocket. Não usa a API nem os servidores do Discord. Idioma do produto e dos commits: **português do Brasil**.

Planos: `ROADMAP.md` (fases 0 a 5, decisões, ordem), `docs/MOBILE-PLAN.md` (Android, **só depois do desktop terminar**), `docs/ui-spec/` (10 especificações de interface extraídas do cliente web do Discord; leia a da área em que for mexer), `docs/discord/` e `docs/review/` (pesquisa e pareceres).

## Estrutura

| Pasta | O que é |
|---|---|
| `proto/` | protocolo JSON do gateway (`C2S`, `S2C`, `Snapshot`...). Mudou aqui, mudam servidor e app |
| `server/` | axum 0.8 + sqlx 0.9. `main.rs` (gateway, limites, CLI), `store.rs` (todas as consultas), `auth.rs` (argon2, tokens), `ids.rs` (snowflake). Migrações em `server/migrations/` (embutidas) |
| `app/` | cliente egui/eframe 0.37. `main.rs` (telas), `theme.rs` (tema Grafite e widgets), `gateway.rs`, `publisher.rs` (captura + NVENC), `viewer.rs` (decodifica transmissão) |
| `md/` | `telacast-md`: parser de Markdown do chat, sem dependências, com testes (`cargo test -p telacast-md`). O desenho fica em `app/src/md_view.rs` |
| `qs/` | `telacast-qs`: lógica do Ctrl+K (prefixos, pontuação, ordenação), sem dependências, com testes. A janela fica em `app/src/quick.rs` |
| `vendor/` | cópia ajustada do `nvidia-video-codec-sdk` |
| `server/tests/e2e.rs` | sobe o servidor de verdade num banco novo e testa o fluxo completo |

## Rodar

```bash
docker compose -f docker-compose.dev.yml up -d        # Postgres 18 (precisa de sudo/grupo docker)
cp .env.example .env                                  # DATABASE_URL e TELACAST_ADDR
cargo run -p telacast-server -- invite --uses 5 --days 30 --label dev   # o código aparece uma vez
cargo run -p telacast-server -- serve                 # ws://127.0.0.1:8080
cargo run --release -p telacast                       # o app
cargo test -p telacast-server                         # unitários + e2e (precisa do Postgres no ar)
```

Cadastro é **fechado por convite**. Senha de 8 a 128 caracteres. Não ponha senhas, tokens nem códigos de convite em arquivos do repositório.

## Regras de trabalho

- **Memória:** o PC tem 15 GB e costuma estar apertado. Antes de compilar: `free -m` e `ps -eo rss,comm --sort=-rss | head`. Com jogo aberto, não compile. Use `-j2`. Build do servidor leva ~1 min; do app em release, segundos quando incremental.
- **Ao terminar uma feature, confira o `ROADMAP.md`:** marque o que ficou pronto (e atualize "Onde estamos"), veja se algum item das fases ficou obsoleto, e escolha o próximo passo pela ordem do roadmap. Se a feature mudou uma decisão ou exigiu algo novo (protocolo, banco, limites), registre lá no mesmo commit.
- **Commits:** mensagem em português, **sem** linha `Co-Authored-By` e sem "Generated with Claude". Só commite quando algo compila e os testes passam. Não faça `git push`.
- **Testar a interface sem tocar no PC do usuário:** nunca clique, mova o mouse nem traga janelas para frente na sessão X dele. Rode o app em tela virtual: `Xvfb :99 -screen 0 1600x900x24 -nolisten tcp &`, depois `DISPLAY=:99 XDG_DATA_HOME=<pasta isolada> ./target/release/telacast`, capture com `xwd -id <janela> | converter` e **leia o PNG** para conferir. Entrada por XTest só no `:99`. Para entrar já logado, grave `server`, `user` e `token` em `<pasta isolada>/telacast/app.ron`.
- **Servidor de teste:** use banco descartável (o e2e já faz isso). Limites: `Send` 12 por 10 s, `Edit` 20 por 30 s; nomes de canal viram minúsculas com hífens.
- **Aviso antes de compilar** coisas grandes é desnecessário quando o usuário já pediu ("roda", "pode ir"); avise só se for travar a máquina.

## Visual (Grafite): o que não fazer

- Camadas de cinza `L0` a `L4`, **um acento âmbar**, filetes de 1 px, títulos em serifa (Newsreader), texto em Inter, só tema escuro. Cores e medidas em `app/src/theme.rs`.
- **Proibido:** gradientes decorativos, brilhos, fundos animados, "glass/aurora" (o usuário rejeitou como cara de IA). Animações curtas e funcionais (hover, seleção, entrada de mensagem).
- Estrutura, medidas e comportamento seguem o Discord (ver `docs/ui-spec/`); só a pele é Grafite.

## Armadilhas do egui 0.37 neste projeto

- `interaction.selectable_labels = false` é global (texto de interface não seleciona); o texto das **mensagens** liga `.selectable(true)` e abre o menu de contexto no próprio rótulo. Não crie um alvo de clique por cima da linha: ele rouba a seleção.
- Barras de hover (ações da mensagem) são desenhadas só com o `painter` e `ui.interact`, **sem** ocupar layout; `scope_builder` empurrava a lista.
- `TextEdit::frame` recebe `egui::Frame` (use `Frame::NONE`). `Id::new` está depreciado (só avisa).
- `consume_key(NONE, Enter)` engole Shift+Enter; use `take_plain_enter` (em `main.rs`).
- Markdown: o texto cru vai e volta do servidor; `md_view` monta um `LayoutJob` por bloco (negrito = `TextFormat.coords` com o eixo `wght`) e acha spoiler/link pelo cursor do galley. Fundo de pílula não tem raio (limite do `TextFormat.background`).
- Sem emoji colorido por padrão; negrito de verdade só por `TextFormat.coords` com o Inter variável.
- Lista de mensagens: `ScrollArea` com `stick_to_bottom`; o servidor devolve 50 por página (`History{before}`). Ao prepender páginas antigas a rolagem é mantida por uma âncora (id e posição da primeira mensagem) + `scroll_with_delta_animation(.., ScrollAnimation::none())`; durante um salto (`jump`/`jump_wanted`) a âncora é desligada.

## Modelo de canais (já no banco)

`channels.kind`: 0 texto, 1 voz, 2 anúncio, 3 palco, 4 categoria, 10 DM. Servidor novo nasce com a categoria "canais de texto" e o canal "geral" dentro. `read_state` guarda o último lido por usuário; o `Snapshot` traz `last_message_id` e `last_read_id` por canal e por DM.

## Trabalho em ciclos de 15 minutos (PC suspende e acorda sozinho)

Para trabalhar de madrugada sem deixar o PC ligado o tempo todo: `tools/wake/`. Instalação, **uma vez** (pede a senha): `sudo bash tools/wake/install.sh`. Depois, ao terminar uma tarefa, rode **em segundo plano** (`run_in_background`): `tools/wake/cycle.sh 15 2`.

O ciclo arma o alarme do RTC para 13 min, confere o alarme (se não conferir, **não suspende**), suspende, e ao acordar segura o PC acordado por 2 min (a rede volta); o comando termina aos 15 min e aí eu retomo. `tools/wake/awake.sh start|stop|status` impede a suspensão por inatividade enquanto trabalho; o `cycle.sh` já solta e retoma isso. Teste sem suspender: `tools/wake/cycle.sh 15 2 dry`.
