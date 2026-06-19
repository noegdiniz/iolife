# Medieval Village LLM

Sandbox de terminal em Rust que simula vilas medievais persistentes com agentes orientados por LLM. O nucleo deterministico controla grid espacial, tempo, recursos, economia feudal e combate; a camada cognitiva produz decisoes, reflexoes e dialogos em JSON estruturado — com fallback heuristico offline.

O projeto gera dezenas de anos de pre-historia genealogica e politica, depois executa a simulacao viva onde agentes trabalham, comem, socializam, cometem crimes, investigam, guerreiam, formam faccoes, disputam titulos feudais e moldam a cultura coletiva via rumores e historias.

## Objetivo

Testar uma arquitetura onde:

- mundo, grid, tempo, recursos e integridade de estado sao deterministicos;
- decisoes subjetivas sao delegadas ao LLM (ou mock heuristico);
- estado subjetivo persiste entre execucoes via SQLite;
- observacao se da por TUI interativa ou relatorios headless.

O motor nunca "escolhe" o que um agente sente ou quer. Ele apenas monta contexto, invoca o adaptador cognitivo, valida a resposta e aplica efeitos.

---

## Arquitetura de modulos

### `src/sim_core/` — Motor da simulacao (modularizado em 11 sub-modulos)

| Sub-modulo | Linhas | Responsabilidade |
|---|---|---|
| `cognition.rs` | ~1340 | Pipeline cognitivo: coleta de contexto (`AgentContext`), disparo de decisoes (sem intencao, necessidade critica, bloqueio, evento social, falha economica), montagem de `DecisionInput` com todos os dominios (psicologico, economico, legal, politico, institucional, feudal, informacional, cultural), chamadas LLM paralelas via `rayon`, Think Maker em background, turnos de conversa |
| `tick.rs` | ~450 | Loop principal: crescimento de colheitas, fronteira de dia (fechamento economico, envelhecimento, nascimentos, casamentos, luto, caravanas, rumores, historias culturais, guerras), fila de tarefas, execucao de intencoes, comportamento autonomo de sobrevivencia |
| `navigation.rs` | ~1105 | Pathfinding BFS no grid respeitando paredes/portas, resolucao de destinos por intencao (trabalho, descanso, alimentacao, social), avanco de movimento com eventos de entrada/saida de edificios, consultas de vizinhanca |
| `economy.rs` | ~1195 | Degradacao de necessidades, cura de partes do corpo (contusoes, laceraoes, fraturas, dano permanente), economia de sobrevivencia autonoma, tarefas economicas multi-etapa, transferencia de recursos, compra/venda de itens, projetos de construcao, demandas militares |
| `conflict.rs` | ~1276 | Sistema de combate com partes do corpo, armas/armaduras, descricoes viscerais, roubo/furto, fuga, acusacao/investigacao/prisao/punicao, casos criminais com testemunhas, morte com heranca (sucessao de oficio, transferencia de dinheiro, luto familiar) |
| `politics.rs` | ~1321 | Sistema feudal completo, atos politicos (decretos), faccoes, issues, pressoes, sistema de insurreicao (Agitacao → Tumulto → Revolta → Guerra Civil), relacoes externas entre polities, guerras (5 estagios) |
| `social.rs` | ~1260 | Protocolo de conversas multi-turno, espalhamento de rumores (com distorcao e credibilidade), historias culturais (ciclo de vida: emergente → estavel → canonizada/esquecida), segredos, encontros agendados, promessas, transferencias economicas durante dialogo |
| `fauna.rs` | ~849 | Criaturas magicas (4 especies), combate, loot, missoes de caca, efeitos territoriais |
| `views.rs` | ~974 | Projecoes read-only para TUI/headless/persistencia: sumarios, visao de agentes (40+ campos), overviews (economia, justica, politica, cultura, feudal), mapa ASCII |
| `helpers.rs` | ~1005 | Utilitarios: merge de recursos, pathfinding, deltas de relacao, sentencas, proficiencia de oficio, itens (instancias, equipamento, degradacao), consultas de agentes |
| `debug.rs` | ~626 | API de debug com 50+ metodos para testes e intervencao manual |

Arquivo raiz `src/sim_core.rs` (~978 linhas) declara todos os componentes ECS, `SimulationConfig`, `AgentView`, e a struct `Simulation`.

### `src/world_model.rs` (~2668 linhas) — Tipos de dominio

Todos os tipos serializaveis do projeto:

- **Grid espacial**: `TileCoord`, `TileKind` (Grass, Road, Floor, Wall, Door, Field, Forest, Rock, Water).
- **Agentes**: `AgentProfile` (tracos, valores, medos, desejos, tolerancias morais), `AgentState` (hunger, energy, health, stress, mood 0–100), `AgentLifeStatus`, `InjuryState`, `AgentMemory`, `AgentRelation`, `AgentIntent` (45+ tipos de acao).
- **Combate**: `CombatState`, body parts (15+ tipos), `InjurySeverity` (Bruised → Destroyed).
- **Crime**: `CrimeCase` com pipeline Open → Investigating → Proven → Arrested → Punished → Closed.
- **Itens**: `ItemInstance` com durabilidade, qualidade, material, stats de combate. `EquipmentSlot` (6 slots), `ItemClass` (Weapon, Armor, Clothing, Jewelry, Tool).
- **Feudal**: `FeudalTitle` (7 ranks: Rei → Oficial), `FeudalContract` (tribute, levy, corvee), `EstateHolding`, `SuccessionCrisis`, `PowerCenter`, `AuthorityOffice` (6 oficios).
- **Politica**: `PoliticalFaction`, `PoliticalIssue`, `PoliticalPressure`, `PolicyAct`, `InsurrectionState` (6 estagios), `WarState` (5 estagios), `ForeignRelation`, `Territory`.
- **Cultura**: `CulturalStory` (com lifecycle), `StoryVersion`, `StoryBelief`, `Rumor`, `Secret`.
- **Economia**: `EconomyCatalog` (recursos, papeis, receitas, precos), `HouseholdEconomy`, `EstablishmentEconomy`, `EconomicTask`.
- **Fauna**: `Creature`, `HuntingQuest`.
- **Historia**: `HistoricalBootstrapSummary`, `HistoricalEventKind` (10 categorias).
- `SimulationSnapshot`: snapshot completo serializavel com todos os subsistemas.

### `src/agent_mind.rs` (~1650 linhas) — Camada cognitiva

- `DecisionInput`: payload para o action planner com 8 dominios de contexto (psicologico, economico, legal, politico, institucional, feudal, informacional, cultural).
- `ConversationTurnInput`: contexto para turnos de dialogo.
- `ThinkMakerInput` / `ThinkMakerOutput`: payload de reflexao.
- `retrieve_relevant_memories()`: ranking por peso emocional, tags e recencia.
- Parsing robusto: markdown fences, coercao de tipos, normalizacao de texto em portugues, validacao contra restricoes do mundo.

### `src/llm_adapter.rs` (~953 linhas) — Interface LLM

Trait `LlmAdapter` com 3 metodos: `plan_actions`, `generate_thoughts`, `generate_conversation_turn`.

**MockLlmAdapter** — heuristico offline:
- Fome > 65 → Eat; Energia < 25 → Rest; Stress > 70 → Reflect.
- Horario de trabalho (06:00–18:00) → Work; fim de tarde → Socialize.
- Salario pendente → CollectPayment; despensa vazia → compra automatica.
- Contexto feudal → respostas de tributo, corveia, levy; contexto institucional → obediencia/contestacao.
- Relacoes hostis → Assault, Steal; positivas → Chat, Favor, Support.

**OpenAiCompatibleAdapter** — HTTP sincrono com retry/backoff para erros transientes. Tres prompts de sistema distintos. Respostas logadas em arquivo para analise de latencia.

### `src/world_gen.rs` (~3921 linhas) — Geracao procedural de mundo

- Grid 150×100 com terreno (grass, forest, fields, rocks).
- 1–3 vilas conectadas por estradas Manhattan.
- Edificios com interiores (paredes, portas, mobilia com estoque inicial).
- Agentes com nomes medievais portugueses e perfis deterministicos.
- Relacoes iniciais: co-vilagers positivas, cross-village com desconfianca.
- Integracao com `world_history`: consume o `HistoricalWorldState` para nomear vilas e armazenar sumario historico.

### `src/world_history.rs` (~1973 linhas) — Pre-historia deterministica

Gera 100 anos de historia genealogica e politica antes da simulacao comecar:

- **Demografia**: casamentos, nascimentos, mortes, envelhecimento.
- **Economia historica**: producao sazonal (primavera/verao/outono/inverno), escassez, construcao.
- **Politica**: tributacao, decretos, obrigacoes feudais, sucessoes de lideranca, crises de legitimidade.
- **Conflitos**: guerras entre vilas (5 estagios), insurreicoes (Agitacao → Guerra Civil).
- **Justica**: casos criminais historicos, sentencas.
- **Cultura**: transmissao de historias entre geracoes.
- Estruturas de dados: `HistoricalWorldState`, `HistoricalSettlement`, `HistoricalPerson` (com arvore genealogica, 5 skills), `HistoricalHousehold`, `HistoricalWarRecord`, `HistoricalInsurrectionSeed`, `HistoricalSuccessionSeed`.
- Seed deterministica (`--history-seed` ou fallback `--seed`).

### `src/economy_catalog.rs` (~1811 linhas) — Catalogo economico

- **35+ recursos**: graos, lenha, madeira, pedra, metal_bruto, tecido, couro, cobre, prata, pao, caldo, ferramentas, moedas, 5 armas, 5 armaduras, 4 roupas, 4 joias.
- **6 papeis sociais**: Campones, Ferreiro, Padeiro, Taverneiro, Guarda, Lider Local.
- **15+ estabelecimentos**: casa, fazenda, lenhal, pedreira, forja, padaria, taverna, posto_guarda, solar, alfaiataria, ourivesaria, armazem_oculto, taverna_secreta.
- **30+ receitas**: producao de materia-prima, forja de ferramentas/armas, costura de roupas, preparo de armaduras, ourivesaria de joias, 13 receitas de construcao.
- **Sistema de itens**: `RefinementLevel` (Rudimentar → Excepcional) com tier scaling, `ItemCombatStats` (dano, precisao, protecao), proficiencias de oficio (smithing, tailoring, jewelry, leatherwork).

### `src/persistence.rs` (~200 linhas) — Persistencia SQLite

- Tabela `checkpoints`: snapshots JSON completos com `schema_version`.
- Tabelas indexadas: `events`, `memories`, `relations`.
- Politicas de save: diario, shutdown, intervalar configuravel.
- Load: checkpoint mais recente por `total_ticks DESC`.

### `src/tui.rs` (~553 linhas) — Interface de terminal

TUI com `ratatui` + `crossterm`:

- **Lista de agentes**: nome, papel, estado, fome, energia, humor, localizacao. Filtro por papel (`f`).
- **Mapa ASCII**: viewport 44×22 centrada no agente selecionado.
- **Detalhe do agente** (40+ campos): fisiologia, intencao, pensamento, inventario, equipamento, economia domestica, posicao feudal (titulo, suserano, subordinados, obrigacoes), filiacao politica, conversa ativa, relacoes, memorias, rumores, historias, percepcao institucional.
- **Timeline de eventos** com destaque para agente selecionado.

Controles: `q` sair, `espaco` pausar, `n` avancar tick, `+/-` velocidade, `setas` selecionar agente, `f` filtrar por papel.

### `src/headless.rs` (~367 linhas) — Modo batch

Execucao sem TUI com relatorios periodicos completos:

- Estatisticas de agentes, distribuicao de papeis.
- Overview economico, de justica, politico, cultural, feudal, de encontros.
- Detalhe verbose por agente.
- Mapa ASCII completo (`--map`).
- Limites por ticks ou dias.

### `src/cli.rs` (~316 linhas) — Interface de linha de comando

| Flag | Tipo | Padrao | Descricao |
|---|---|---|---|
| `--headless` | flag | off | Modo batch sem TUI |
| `--tui` | flag | on | Forca modo interativo |
| `--new` | flag | off | Ignora save, cria mundo novo |
| `--db PATH` | string | `village_sim.sqlite` | Caminho do banco SQLite |
| `--seed N` | u64 | `1` | Seed do gerador procedural |
| `--agents N` / `--population N` | usize | `21` | Agentes iniciais |
| `--grid-width N` / `--width N` | i32 | `150` | Largura do grid |
| `--grid-height N` / `--height N` | i32 | `100` | Altura do grid |
| `--num-villages N` | usize | `3` | Quantidade de vilas |
| `--village-name NOME` | string | `Santa Bruma` | Nome da vila principal |
| `--history-years N` | u32 | `100` | Anos de pre-historia |
| `--history-founding-households N` | usize | `3` | Casas fundadoras |
| `--history-seed N` | u64 | usa `--seed` | Seed da pre-historia |
| `--ticks N` | u64 | sem limite | Encerra apos N ticks |
| `--days N` | u32 | sem limite | Encerra apos N dias |
| `--save-every N` | u64 | `24` | Checkpoint intervalar (0 desativa) |
| `--summary-every N` | u64 | `24` | Relatorio a cada N ticks |
| `--event-tail N` | usize | `8` | Eventos por relatorio |
| `--ticks-per-second N` | u32 | `1` | Ritmo headless |
| `--map` | flag | off | Mapa ASCII nos relatorios |
| `--help` | — | — | Ajuda |

---

## Sistemas implementados

### Grid espacial e pathfinding

- Grid 150×100 (configuravel) com 9 tipos de tile.
- Pathfinding BFS respeitando paredes, portas e tiles ocupados.
- Agentes seguem caminhos com resolucao de colisoes.
- Destinos resolvidos por intencao: trabalho → oficina/campo, descanso → cama, alimentacao → mesa.

### Cognicao (3 estagios)

1. **Action Planner**: planeja 3–6 tarefas com 45+ tipos de acao. Contexto de 8 dominios.
2. **Think Maker**: gera reflexao, emocao dominante, atualizacao de crencas.
3. **Conversation Turn**: turnos de dialogo com fala, movimento social e deltas de relacao.

Processamento assincrono com `rayon`: dispatch em background threads, join de resultados.

### Acoes (45+ tipos)

| Categoria | Acoes |
|---|---|
| Basicas | Work, Rest, Eat, Reflect, Wander |
| Sociais | Socialize, Chat, Favor, Offend, Support, Oppose, RequestSupport, Mediate |
| Economicas | Produce, Buy, Sell, Transport, CollectPayment, ReceivePayment |
| Combate | Assault, Combat, Flee |
| Crime | Steal, Loot, Accuse |
| Justica | Investigate, Arrest, Punish |
| Politicas | Pressure, Decree |
| Feudais (14 novas) | JurarLealdade, RomperLealdade, ConcederTitulo, RevogarTitulo, NomearOficial, ExigirTributo, CobrarCorveia, ConvocarLevy, ReconhecerHerdeiro, ApoiarPretendente, Usurpar, ReivindicarTerritorio, NegociarSuserania, Esconder |

### Fisiologia e combate

- Necessidades: hunger, energy, health, stress, mood (0–100). Degradacao por tick.
- **Sistema de partes do corpo** (15+ partes): cranio, olhos, pescoco, peito, abdomen, costas, ombros, bracos, maos, pernas, pes.
- **Injuries**: Bruised → Lacerated → Fractured → Severed → Destroyed. Dano permanente para Severed/Destroyed.
- **Cura**: contusoes/laceraoes em ticks, fraturas 4× mais lentas, partes perdidas sao permanentes.
- **Armas e armaduras**: 5 armas, 5 armaduras com stats de combate. Degradacao por uso.
- **Morte**: por health = 0. Heranca (sucessao de oficio, dinheiro, luto familiar).

### Sistema feudal

- **Titulos**: Rei, Duque, Conde, Barao, Senhor, Cavaleiro, Oficial.
- **Contratos**: suserano-vassalo com tribute diario, dever de levy, corveia, auxilio judicial.
- **Cadeia de tributos**: propagacao bottom-up ordenada por rank.
- **Estates**: concessoes de terra com edificios, valor anualizado, obrigacoes militares.
- **Sucessao**: regras (HerdeiroDireto, ConjugeRegente, NomeacaoDoSuserano), crises com multiplos pretendentes.
- **Oficios de autoridade**: Intendente, Coletor, JuizLocal, CapitaoDaGuarda, Carcereiro, AdministradorDoSolar.
- **Usurpacao**: tomada de titulo por forca ou manobra politica.

### Sistema de guerra

- **5 estagios**: Mobilization → Raids → Siege → DecisiveBattle → Occupation.
- **Demandas militares**: recursos, dinheiro, prazos, status de suprimento.
- **Relacoes externas**: 6 stances (Neutral, TradePartner, Ally, Rival, AtWar, Tributary).
- **Transferencia de territorio** ao vencer batalhas decisivas.

### Insurreicao

- **6 estagios**: Agitation → Riot → OrganizedRevolt → CivilWar → Suppressed/Victorious.
- Suporte popular, repressao, guerra vinculada.

### Economia de itens

- **35+ recursos** incluindo itens craftados com qualidade e durabilidade.
- **30+ receitas**: producao, forja, costura, ourivesaria, construcao.
- **6 slots de equipamento**: MainHand, OffHand, Body, Outer, Accessory1, Accessory2.
- **Proficiencias de oficio**: smithing, tailoring, jewelry, leatherwork.
- **Mercado externo**: precos de compra/venda para 27 recursos.
- **Projetos de construcao**: coleta de materiais, dias de trabalho, progresso.
- **Economia domestica autonoma**: comer da despensa → comprar comida → coletar salario → descansar → trabalhar.

### Crime e justica

- **Crimes**: Assault, Theft, Robbery, Homicide.
- **Pipeline**: Open → Investigating → Proven → Arrested → Punished → Closed.
- **Testemunhas**: propagacao automatica, contágio psicologico em predios.
- **Sentencas**: Restitution, Fine, Detention, Corporal.
- **Percepcao institucional**: confianca na guarda/justica afetada por severidade de punicoes.

### Sistema cultural

- **Rumores**: espalhamento com distorcao e credibilidade, decaimento diario.
- **Historias culturais**: ciclo de vida (Emergente → Estavel → Canonizada/Esquecida), forca cultural, versoes multiplas, crencas com apego emocional.
- **Segredos**: criacao por calunia investigada, revelacao durante conversas.
- **Tradicoes**: transmitidas entre geracoes.

### Fauna magica

- **4 especies**: Silvafaro (estabilidade +2/dia), Pedrapiro (agressivo, ataca agentes), Lebre-zelo (fugitivo), Brumalisco (estabilidade -2/dia).
- **Combate**: mesmas regras de partes do corpo que agentes.
- **Missoes de caca**: diarias, com recompensas em ouro (50–100).
- **Loot**: recursos magicos (essencia_silvafaro, nucleo_pedrapiro, etc.).
- **Historias lendarias**: criadas ao abater criaturas lendarias.

### Sistema social

- Relacoes bilaterais com 6 dimensoes: trust, friendship, resentment, attraction, moral_debt, reputation.
- Conversas multi-turno (ate 6 turnos) com processamento paralelo de multiplas conversas.
- 8 desfechos de conversa: MutualEnd, OneSidedExit, MaxTurns, DistanceBreak, BlockingBreak, CriticalNeed, PhysicalConflict, ProviderTimeout.
- Encontros agendados: propor, aceitar, viajar, executar.
- Transferencias economicas e promessas durante dialogo.

---

## Valores padrao

| Parametro | Padrao |
|---|---|
| Nome da vila | Santa Bruma |
| Ticks por dia | 1440 (1 tick = 1 minuto) |
| Agentes iniciais | 21 |
| Grid | 150 × 100 |
| Vilas | 3 |
| World seed | 1 |
| Anos de pre-historia | 100 |
| Casas fundadoras | 3 |
| Schema version | (dinamico) |

---

## Como rodar

### Requisitos

- Rust toolchain (edition 2024).
- Opcional: chave de API OpenAI-compatible.

### TUI com mock (padrao)

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
  --history-years 50 --history-founding-households 5 \
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

---

## Como testar

```bash
cargo test
```

~15 testes de integracao cobrindo: pathfinding com paredes, rumores e historias culturais, combate com partes do corpo, crime e justica, pressao politica e faccoes, decretos e atos politicos, guerra com demandas militares, tributos feudais recursivos, corveia feudal, expansao territorial, e fauna magica.

---

## Compatibilidade

Validado em WSL/Linux. No Windows nativo pode falhar linkedicao por falta de `link.exe` e Windows SDK — instale Visual Studio Build Tools ou use WSL.

---

## Limitacoes atuais

- Chamadas HTTP sao sincronas (paralelismo via threads rayon, nao IO assincrono).
- Sem compressao semantica de memoria de longo prazo (truncamento simples).
- Banco SQLite cresce indefinidamente (sem compactacao/retencao).
- Sem API externa JSON/WebSocket.
- Agentes nao formam familias formais com lacos de parentesco completos na simulacao viva (apenas na pre-historia).
- Nao ha envelhecimento ou substituicao populacional plena na simulacao.

---

## Proximos passos

- Compressao semantica de memorias antigas.
- Streaming e `responses` no adaptador LLM.
- API externa para observacao e controle remotos.
- Visualizacao expandida na TUI (mapa completo, grafo de relacoes).
- Familias formais e parentesco na simulacao viva.
- Politica de retencao do banco SQLite.
- Catalogo economico customizavel via arquivo externo.
- Envelhecimento e renovacao populacional completos.
