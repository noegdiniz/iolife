# Medieval Village LLM

Sandbox de terminal em Rust para simular vilas medievais persistentes com agentes orientados por LLM. O projeto combina um nucleo deterministico de simulacao (grid espacial, fisica, tempo, recursos, economia) com uma camada cognitiva que produz decisoes, reflexoes e trocas sociais em JSON estruturado.

O foco do V1 e profundidade psicologica e social com emergencia de comportamentos coletivos: economia de subsistencia, crimes, investigacoes, punicoes, faccoes politicas e disputas por normas locais.

## Objetivo do projeto

Este projeto existe para testar uma arquitetura em que:

- o mundo, o tempo, os recursos e a integridade do estado sao controlados por codigo deterministico;
- as decisoes subjetivas dos agentes sao delegadas ao LLM (ou a um adaptador mock heuristico);
- o estado subjetivo de cada aldeao persiste entre execucoes via SQLite;
- a observacao do sistema e feita por uma TUI detalhada ou por relatorios headless.

Na pratica, o motor nunca "escolhe" psicologicamente o que um agente quer fazer. Ele apenas:

- monta o contexto cognitivo completo do agente (fisiologia, perfil, memorias, relacoes, economia, politica, legal);
- chama o adaptador cognitivo;
- valida e normaliza a resposta;
- aplica efeitos no mundo;
- registra memorias, relacoes e eventos.

## Arquitetura por modulo

### `src/world_model.rs` — Tipos de dominio

Define todos os tipos serializaveis que formam o contrato do dominio. Principais estruturas:

- `TileCoord`, `TileKind`: grid espacial tile-based (grass, road, floor, wall, door, field, forest, rock, water).
- `AgentProfile`: tracos, valores, medos, desejos de longo prazo, tolerancias morais, estilo social.
- `AgentState`: estado fisiologico e emocional (hunger, energy, health, stress, mood, 0–100).
- `AgentLifeStatus`: Vivo, Incapacitado, Morto.
- `InjuryState`: ferimentos leves/graves, dor, sangramento, ticks de recuperacao.
- `AgentMemory`: memoria episodica com peso emocional, tags, entidades relacionadas, dia e tick.
- `AgentRelation`: vinculo bilateral entre dois agentes (trust, friendship, resentment, attraction, moral_debt, reputation).
- `AgentIntent`: decisao estruturada produzida pelo LLM com ate 30+ tipos de acao.
- `ConversationState` / `ConversationTurn`: dialogo multi-turno com ate 6 turnos por conversa.
- `CombatState`: rounds de combate, desfechos (Fled, Incapacitation, Death, DistanceBreak).
- `CrimeCase`: casos criminais com pipeline Open → Investigating → Proven → Arrested → Punished → Closed.
- `PoliticalFaction` / `PoliticalIssue` / `PoliticalPressure`: faccoes com agendas, issues com votacao, pressoes por mudanca de normas.
- `EconomyCatalog`, `HouseholdEconomy`, `EstablishmentEconomy`, `EconomicTask`: economia com recursos, receitas, precos, tarefas economicas multi-etapa.
- `FixtureSpec`, `BuildingSpec`, `RoomSpec`: mobiliario e edificios com interiores.
- `SimulationSnapshot`: snapshot completo serializavel com grid, agentes, edificios, economias, crimes, faccoes.

### `src/agent_mind.rs` — Camada cognitiva

Define os payloads enviados ao LLM e a logica auxiliar de cognicao:

- `DecisionInput`: contexto completo para planejamento de acoes (fisiologia, perfil, memorias, relacoes, contexto psicologico, economico, legal e politico).
- `ConversationTurnInput`: contexto para geracao de turnos de dialogo.
- `ThinkMakerInput` / `ThinkMakerOutput`: payload para geracao de pensamentos/reflexoes.
- `retrieve_relevant_memories()`: ranking de memorias por peso emocional, sobreposicao de tags e recencia.
- `parse_decision_json()`: parsing robusto de JSON do LLM com normalizacao de markdown fences, coercao de tipos, validacao de campos.
- `parse_conversation_turn_json_with_notes()`: parsing de turnos de conversa com normalizacao de texto em portugues.
- `validate_intent()`: sanity-check da saida do LLM contra restricoes do mundo.

### `src/llm_adapter.rs` — Interface LLM

Contem o trait `LlmAdapter` com tres metodos:

- `plan_actions()`: planejamento de tarefas (action planner).
- `generate_thoughts()`: geracao de reflexoes, emocao dominante, atualizacao de crencas (think maker).
- `generate_conversation_turn()`: geracao de fala, movimento social e deltas de relacao.

Duas implementacoes:

#### `MockLlmAdapter`

Adaptador heuristico local (sem rede) que produz comportamento coerente com regras deterministicas:

- hunger > 65 → Eat; energy < 25 → Rest; stress > 70 → Reflect;
- horario de trabalho (ticks 360–1080 / 06:00–18:00) → Work;
- final de tarde (ticks 1080–1260) → Socialize;
- salario pendente → CollectPayment;
- despensa vazia e fome → compra automatica de comida;
- relacoes com ressentimento alto → Assault, Steal ou movimento social agressivo;
- relacoes positivas → Chat, Favor, Support;
- sozinho → Wander.

#### `OpenAiCompatibleAdapter`

Chamadas HTTP sincronas para API compativel com `chat/completions`. Configurado via variaveis de ambiente:

- `OPENAI_API_KEY`
- `OPENAI_MODEL` (ex: `gpt-4.1-mini`)
- `OPENAI_BASE_URL` (ex: `https://api.openai.com/v1/chat/completions`)

Inclui retry com backoff para erros transientes (timeout, 429, 5xx). Tres prompts de sistema distintos para action planner, think maker e conversation turn. Se as credenciais nao estiverem configuradas, cai automaticamente no `MockLlmAdapter`.

### `src/sim_core.rs` — Motor da simulacao (~10.400 linhas)

O coracao do projeto. Implementa:

#### Sistema ECS

ECS baseado em `bevy_ecs` com 15+ componentes: `AgentCore`, `ProfileComponent`, `StateComponent`, `RelationComponent`, `MemoryComponent`, `InventoryComponent`, `PositionComponent`, `PathComponent`, `IntentComponent`, `ThoughtComponent`, `TaskQueueComponent`, `ConversationStatusComponent`, `EconomicActivityComponent`, `TraumaTrackerComponent`.

#### Grid espacial e pathfinding

- Grid padrao de 150×100 tiles com tipos: grass, road, floor, wall, door, field, forest, rock.
- Pathfinding BFS considerando tiles caminhaveis.
- Agentes seguem caminhos, respeitam tiles ocupados e re-roteiam em colisoes.

#### Ciclo de tick

Cada tick (1 minuto simulado, 1440 por dia) executa:

1. Avanca tempo global e degrada necessidades fisiologicas (hunger, energy, stress, health).
2. Processa fila de tarefas assincronas pendentes (acoes em andamento como viajar, produzir).
3. Colhe tarefas economicas completadas (producao, coleta de materia-prima).
4. Avanca conversas ativas (ate 6 turnos, processamento paralelo com `rayon`).
5. Dispara novas decisoes para agentes com budget cognitivo disponivel (chamadas LLM em background threads com `rayon`).
6. Coleta decisoes assincronas ja concluidas e as aplica ao mundo.
7. Resolve politica diaria (issues, faccoes, mudancas de normas).
8. Coleta impostos diarios (1 coin por household/dia).
9. Salva checkpoint se em fronteira de dia ou intervalo configurado.

#### Acoes implementadas (30+)

| Categoria | Acoes |
|-----------|-------|
| Basicas | Work, Rest, Eat, Reflect, Wander |
| Sociais | Socialize, Chat, Favor, Offend, Support, Oppose, RequestSupport, Mediate |
| Economicas | Produce, Buy, Sell, Transport, CollectPayment, ReceivePayment |
| Combate | Assault, Combat, Flee |
| Crime | Steal, Loot |
| Legais | Accuse, Investigate, Arrest, Punish |
| Politicas | Pressure |

#### Combate

- Sistema de rounds com estados Active/Ended.
- Desfechos: Fled (fuga), Incapacitation (incapacitacao), Death (morte), DistanceBreak (quebra por distancia).
- Ferimentos leves e graves, dor, sangramento e recuperacao ao longo do tempo.
- Morte por health = 0 (fome extrema ou ferimentos graves).

#### Sistema legal/criminal

- Tipos de crime: Assault, Theft, Robbery, Homicide.
- Pipeline: Open → Investigating → Proven → Arrested → Punished → Closed.
- Testemunhas: agentes proximos viram testemunhas e geram crime cases automaticamente.
- Sentencas: Restitution (restituicao), Fine (multa), Detention (detencao), Corporal (castigo corporal).
- Guardas e Lideres executam Investigate, Arrest e Punish.

#### Economia

- Catalogo economico unificado com recursos, affordances, papeis, estabelecimentos, receitas de producao e receitas de construcao.
- Recursos incluem comida, combustivel, ferramenta/capital, moeda e materiais de construcao como madeira e pedra.
- Estabelecimentos podem ter multiplas receitas: lenhal produz lenha/madeira e pedreira produz metal bruto/pedra.
- Economia domestica (HouseholdEconomy): treasury, pantry, reserved_food, tax_arrears.
- Economia de estabelecimento (EstablishmentEconomy): cash, stock, precos, salarios.
- Mercado externo com precos de compra/venda por recurso.
- Tarefas economicas multi-etapa: AwaitingPickup → InTransit → AwaitingPayment → Completed/Failed.
- Coleta automatica de salario, compra de comida em emergencia, racionamento.
- Producao de materia-prima: fazendas produzem grains, woodlots produzem firewood, quarries produzem raw metal.
- Construcao urbana emergente: pressoes sistemicas abrem projetos, materiais sao entregues fisicamente e o predio concluido entra no grid.
- Colheitas: campos tem ciclo plant → growing → ready, colheita produz grains.

#### Sistema politico

- Issues (pautas) com suporte/oposicao nos dominios: Tax, Justice, Rationing.
- Faccoes politicas com agenda, influencia e objetivos emergentes (Food Riot, Tax Boycott, Depose Leader, Vigilante Justice).
- Pressoes politicas: agentes aplicam pressao em issues.
- Resolucao diaria: normas locais mudam quando issues tem suporte suficiente.
- Normas locais: Justice severity (Lenient/Normal/Severe), Rationing policy (HouseholdFirst, ProducersFirst, CivicFirst, Balanced).
- Chaos pressure: 0–100, afeta probabilidade de violencia, agressividade social, tolerancia a roubo.

#### Sistema social

- Relacoes bilaterais com 6 dimensoes: trust, friendship, resentment, attraction, moral_debt, reputation.
- Conversas multi-turno (ate 6 turnos) com processamento paralelo.
- Desfechos de conversa: MutualEnd, OneSidedExit, MaxTurns, DistanceBreak, BlockingBreak, CriticalNeed, PhysicalConflict, ProviderTimeout.
- Deltas de relacao por turno (mudancas incrementais em cada dimensao).

#### Fisiologia e saude

- Necessidades: hunger, energy, health, stress, mood (escala 0–100).
- Degradacao por tick: fome aumenta, energia diminui, stress varia conforme contexto.
- Ferimentos: light_wounds, severe_wounds, pain, bleeding, recovery_ticks.
- Trauma tracker: ticks consecutivos de fome/stress/riqueza, violencia testemunhada.
- Morte: health = 0 por fome extrema ou ferimentos nao tratados.

#### Processamento assincrono

- Dispatch de decisoes LLM em threads background via `rayon`.
- Workers de action planner e think maker separados.
- Loop assincrono: junta resultados de threads conforme completam.
- Faccao emergente com objetivo dinamico definido pela IA.

### `src/world_gen.rs` — Geracao procedural de mundo

Cria um `SimulationSnapshot` completo a partir de `SimulationConfig`:

- Grid espacial com terreno (grass, forest, fields, rocks).
- 1–3 vilas com estradas conectando centros (estilo Manhattan).
- Edificios por vila: 4 casas, solar/manor, guard post, forge, taverna, bakery, woodlot, farm/celeiro, quarry.
- Interiores com paredes, portas, mobilia (camas, mesas, estacoes de trabalho, armazenamento com estoque inicial).
- Agentes (ate 21) com nomes medievais portugueses e perfis deterministicos (traits, values, fears, desires tematicos).
- Relacoes iniciais: co-vilagers positivas, cross-village com desconfianca xenofobica.
- Economias domesticas e de estabelecimento pre-configuradas.

### `src/persistence.rs` — Persistencia SQLite

- Tabela `checkpoints`: snapshots completos em JSON com `schema_version` (atual: 11).
- Tabelas `events`, `memories`, `relations`: copias indexadas para queries offline.
- Politicas de save: diario (`kind = "daily"`), encerramento (`kind = "shutdown"`), intervalar configuravel.
- Load: carrega checkpoint mais recente por `total_ticks DESC`, a menos que `--new`.
- Validacao de schema: rejeita snapshots legados sem grid espacial/economia/construcao atualizada.

### `src/tui.rs` — Interface de terminal

TUI com `ratatui` + `crossterm`, 3 paineis:

- **Lista de agentes**: nome, papel, estado, fome, energia, humor, localizacao. Filtro por papel (`f`).
- **Mapa ASCII**: viewport 44×22 centralizada no agente selecionado, mostrando edificios, estradas, agentes e colheitas.
- **Detalhe do agente**: posicao, estado fisiologico, intencao, pensamento, inventario, economia domestica, filiacao politica, conversa ativa, relacoes, memorias recentes, ferimentos.
- **Timeline de eventos**: eventos recentes com destaque para o agente selecionado.
- **Barra de controles**: ajuda com todos os atalhos.

Controles:
- `q` — sair
- `espaco` — pausar/retomar
- `n` — avancar um tick
- `+`/`-` — acelerar/desacelerar
- `setas` — selecionar agente
- `f` — alternar filtro por papel

### `src/headless.rs` — Modo batch

Execucao sem TUI para lotes, geracao de checkpoints e observacao via stdout:

- Relatorios periodicos com estatisticas de agentes, visao geral economica, legal e politica.
- Mapa ASCII completo opcional (`--map`).
- Limites de ticks ou dias simulados.
- Save intervalar configuravel.
- Ritmo de simulacao configuravel (`--ticks-per-second`).

### `src/economy_catalog.rs` — Catalogo economico padrao

Define `default_economy_catalog()` com todos os dados de referencia:

- 7 recursos, 7 papeis, 9 arquetipos espaciais, 8 tipos de estabelecimento.
- 6 receitas de producao com inputs/outputs.
- Precos de compra/venda no mercado externo.
- Funcao `validate_catalog()` que verifica integridade referencial de todos os IDs.

### `src/cli.rs` — Interface de linha de comando

Parser de argumentos com suporte a:

| Flag | Descricao |
|------|-----------|
| `--headless` | Modo batch sem TUI |
| `--tui` | Forca modo interativo (padrao) |
| `--new` | Ignora save e cria mundo novo |
| `--db PATH` | Caminho do banco SQLite |
| `--seed N` | Seed do gerador procedural |
| `--agents N` / `--population N` | Quantidade de agentes iniciais |
| `--grid-width N` / `--width N` | Largura do grid |
| `--grid-height N` / `--height N` | Altura do grid |
| `--num-villages N` | Quantidade de vilas |
| `--village-name NOME` | Nome da vila principal |
| `--ticks N` | Encerra apos N ticks (headless) |
| `--days N` | Encerra apos N dias (headless) |
| `--save-every N` | Checkpoint intervalar (0 desativa) |
| `--summary-every N` | Relatorio a cada N ticks |
| `--event-tail N` | Eventos recentes no relatorio |
| `--ticks-per-second N` | Ritmo da simulacao headless |
| `--map` | Inclui mapa ASCII nos relatorios |
| `--help` | Mostra ajuda |

## Sistema cognitivo em detalhe

O comportamento do agente e dividido em tres estagios:

### 1. Action Planner (planejamento de acoes)

Entrada (`DecisionInput`):
- Identidade, papel, dia/tick, localizacao, estado fisiologico.
- Perfil psicologico (traits, values, fears, desires, moral tolerances).
- Memorias relevantes (ranking por peso emocional, tags e recencia).
- Eventos recentes, metas atuais, orcamento residual.
- Contexto psicologico (PsychologicalContextInput).
- Contexto economico (EconomicContextInput): household treasury, pantry, salario pendente.
- Contexto legal (LegalContextInput): crime cases proximos, testemunhas.
- Contexto politico (PoliticalContextInput): faccoes, issues, pressoes ativas.

Saida (`DecisionEnvelope`):
- Sequencia de 3–6 tarefas com tipo, alvo, local, justificativa, prioridade, risco percebido.
- Emocao dominante, atualizacoes de crenca, movimento social.

### 2. Think Maker (geracao de reflexoes)

Entrada (`ThinkMakerInput`):
- Contexto similar ao action planner, focado em estado interno.

Saida (`ThinkMakerOutput`):
- `thought`: reflexao textual curta.
- `dominant_emotion`: emocao atual.
- `belief_updates`: mudancas em crencas/metas.
- `memory_tags`: tags para indexacao da memoria gerada.

### 3. Conversation Turn (turnos de dialogo)

Quando a acao e social, o motor solicita turnos de dialogo:

- Ate 6 turnos por conversa, alternando entre falantes.
- Cada turno gera fala, movimento social e deltas de relacao.
- Desfechos variados (mutuo, unilateral, timeout, escalacao para conflito fisico).
- Processamento paralelo de multiplas conversas no mesmo tick.

### Parsing robusto

O parser de JSON do LLM lida com:
- Markdown fences (```json ... ```).
- Coercao de tipos (string → number, booleanos textuais em portugues/ingles).
- Normalizacao de nomes de movimentos sociais em portugues.
- Validacao de intencoes contra restricoes do mundo.

## Valores padrao da simulacao

| Parametro | Valor padrao |
|-----------|-------------|
| Nome da vila | Santa Bruma |
| Ticks por dia | 1440 (1 tick = 1 minuto) |
| Agentes iniciais | 21 |
| Grid | 150 × 100 tiles |
| Vilas | 3 |
| World seed | 1 |
| Banco de dados | `village_sim.sqlite` |
| Schema version | 10 |

## Como rodar

### Requisitos

- Rust toolchain recente (edition 2024).
- Opcional: chave de API compativel com OpenAI para o adaptador real.

### TUI com mock local (padrao)

```bash
cargo run
```

### Headless (batch)

```bash
# Curto: 48 ticks, relatorio a cada 6
cargo run -- --headless --new --ticks 48 --summary-every 6 --event-tail 6

# Longo com mapa ASCII e save intervalar
cargo run -- --headless --ticks 96 --save-every 24 --summary-every 12 --map

# Mundo customizado
cargo run -- --headless --new --seed 42 --agents 30 --num-villages 2 \
  --grid-width 200 --grid-height 120 --village-name "Pedra Clara" \
  --ticks 240 --summary-every 24 --save-every 48 --map

# Por dias simulados
cargo run -- --headless --new --days 3 --summary-every 24 --event-tail 10
```

### Com adaptador OpenAI

```bash
export OPENAI_API_KEY=sk-...
export OPENAI_MODEL=gpt-4.1-mini
export OPENAI_BASE_URL=https://api.openai.com/v1/chat/completions
cargo run
```

## Como testar

```bash
cargo test
```

29 testes integrados cobrindo:

- Parsing de JSON cognitivo (action planner, think maker, conversation turn).
- Normalizacao de markdown fences, coercao de tipos, texto em portugues.
- Recuperacao e ranking de memorias.
- Geracao procedural de mundo (edificios, mobilia, agentes).
- Pathfinding e movimento com tiles ocupados.
- Mecanica de conversas (adjacencia, alternancia de turnos, timeout).
- Persistencia e restauracao de snapshot (round-trip).
- Processamento paralelo de conversas.
- Rejeicao de snapshots de schema legado.
- Simulacao de uma semana completa com restricoes fisicas do grid.
- Geracao de woodlot/quarry.
- Coleta diaria de impostos e producao de materia-prima.
- Combate (assault → combat rounds → crime case).
- Roubo (theft → transferencia de recursos → crime case).
- Pipeline de investigacao/arresto/punicao pela guarda.
- Pressao politica, criacao de issues, formacao de faccoes, mudanca de normas.
- Consumo emergencial de comida em fome critica.
- Processamento assincrono de decisoes (background LLM threads com rayon).
- Ciclo agricola completo (plant → growing → ready → colheita).

## Compatibilidade de ambiente

Validado com sucesso via WSL usando `cargo test`.

No Windows nativo, o toolchain Rust pode falhar na linkedicao por falta de `link.exe` e bibliotecas do Windows SDK. Solucoes:

- Instalar Visual Studio Build Tools com o SDK de C++.
- Rodar o projeto via WSL/Linux.

## Limitacoes atuais

- Chamadas LLM usam `chat/completions` (nao `responses` nem streaming).
- Chamadas HTTP sao sincronas (o paralelismo vem do dispatch em multiplas threads, nao de IO assincrono).
- A compressao semantica de memoria de longo prazo ainda nao foi implementada (truncamento simples por vetor).
- O banco SQLite cresce indefinidamente (sem politica de compactacao/retencao).
- Nao ha API externa JSON para clientes remotos.
- Agentes nao formam familias formais ou lacos de parentesco.
- A explicabilidade na TUI e boa mas resumida (sem visualizacao de grid completa ou grafo de relacoes).
- Nao ha conceito de geracoes, envelhecimento ou substituicao populacional.

## Proximos passos

- Compressao semantica de memoria (sumarizacao periodica de memorias antigas).
- Evoluir adaptador LLM para `responses` e/ou streaming.
- API externa JSON/WebSocket para observacao e controle remotos.
- Visualizacao expandida na TUI (mapa completo, grafo de relacoes, dashboard economico).
- Relacoes de parentesco e familias formais.
- Export de snapshots para analise offline.
- Politica de retencao/compactacao do banco SQLite.
- Suporte a customizacao de catalogo economico via arquivo externo.


passar para os agentes a lista completa de lugares do mundo de forma semnatica e obrigar a llm a usar estritamente esses lugares na conversas. e açoes adicionar a possibilidade dos agentes marcarem encontros em lugares especifcos em horarios especificos, adicionar ciclor de dia e noita e horas do dia que sao passados para o contexto cognitivo dos agentes. 