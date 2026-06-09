# Medieval Village LLM

Sandbox de terminal em Rust para simular uma vila medieval persistente com agentes orientados por LLM. O projeto combina um nucleo deterministico de simulacao com uma camada cognitiva que produz intencoes, reflexoes e trocas sociais em JSON estruturado.

O foco atual do V1 e profundidade psicologica e social, nao fidelidade economica total. A vila funciona como uma pequena sociedade emergente com rotinas, necessidades, memorias, relacoes, conflitos e reconciliacoes.

## Objetivo do projeto

Este projeto existe para testar uma arquitetura em que:

- o mundo, o tempo, os recursos e a integridade do estado sao controlados por codigo deterministico;
- as decisoes subjetivas dos agentes sao delegadas ao LLM;
- o estado subjetivo de cada aldeao persiste entre execucoes;
- a observacao do sistema e feita por uma TUI detalhada em vez de apenas logs brutos.

Na pratica, isso significa que o motor nunca "escolhe" psicologicamente o que um agente quer fazer. Ele apenas:

- monta o contexto relevante do agente;
- chama o adaptador cognitivo;
- valida a resposta;
- aplica efeitos no mundo;
- registra memorias, relacoes e eventos.

## Estado atual do V1

O codigo ja implementa:

- binario Rust com modulos separados para mundo, cognicao, persistencia, adaptador LLM e TUI;
- nucleo ECS com agentes, locais, estoques, memorias, relacoes e timeline de eventos;
- persistencia em SQLite com checkpoints completos;
- retomada automatica do ultimo estado salvo;
- TUI navegavel com visao geral, lista de agentes, detalhe do agente e timeline recente;
- adaptador OpenAI-compatible e fallback para um adaptador `mock`;
- testes de parsing do JSON do LLM, recuperacao de memoria, persistencia e simulacao de uma semana.

Ainda nao esta implementado neste corte:

- servico continuo de backend;
- API externa JSON para outros clientes;
- economia complexa com precos dinamicos;
- politica institucional, faccoes ou guerra;
- cadeia de dialogo longa com transcricao literal completa;
- sumarizacao sofisticada de memoria de longo prazo.

## Arquitetura por modulo

### `src/world_model.rs`

Define os tipos serializaveis que representam o contrato do dominio.

Principais estruturas:

- `Role`: papeis sociais dos aldeoes.
- `LocationKind` e `LocationSpec`: tipagem de locais da vila.
- `AgentProfile`: tracos, valores, medos, desejos e estilo social.
- `AgentState`: estado fisiologico e emocional atual.
- `AgentMemory`: memoria episodica ou subjetiva persistente.
- `AgentRelation`: vinculo entre dois agentes.
- `AgentIntent`: decisao estruturada produzida pelo LLM.
- `SocialScene` e `RelationDelta`: resultado estruturado de interacoes sociais.
- `WorldEvent`: evento resumido da timeline do mundo.
- `SimulationSnapshot`: snapshot completo serializavel da simulacao.

Esses tipos sao usados tanto pelo motor quanto pela persistencia e pela camada de LLM.

### `src/agent_mind.rs`

Define os payloads enviados ao LLM e a logica auxiliar de cognicao.

Responsabilidades:

- modelar o contexto cognitivo (`DecisionInput`, `SocialExchangeInput`);
- recuperar memorias relevantes por peso emocional, tags e contexto recente;
- validar e normalizar a intencao retornada pelo modelo;
- fazer o parse da resposta JSON.

O metodo `retrieve_relevant_memories` faz um ranking simples que mistura:

- peso emocional da memoria;
- intersecao entre tags e foco/metas/eventos recentes;
- recencia da memoria.

Isso reduz o contexto enviado ao modelo e evita mandar o historico inteiro do agente em toda decisao.

### `src/llm_adapter.rs`

Contem a interface cognitiva e duas implementacoes.

#### `LlmAdapter`

E o contrato principal da camada de IA:

- `provider_name()`
- `evaluate_and_decide()`
- `generate_social_scene()`

#### `MockLlmAdapter`

Implementacao local sem rede, usada quando nao ha credenciais configuradas. Ela nao e um "dummy" vazio: ja produz comportamento coerente o bastante para os testes e a TUI.

Heuristicas principais:

- muita fome prioriza `Eat`;
- pouca energia prioriza `Rest`;
- muito stress prioriza `Reflect`;
- relacoes com ressentimento alto podem virar `Offend`;
- relacoes positivas podem virar `Favor`;
- entre os ticks 6 e 14 o padrao dominante e `Work`;
- entre os ticks 16 e 21 o agente tende a socializar;
- se estiver sozinho, pode `Wander`.

Alem da intencao, ele tambem gera:

- reflexao curta;
- movimento social (`Chat`, `Favor`, `Offend` etc.);
- emocao dominante;
- atualizacao textual de crenca/meta.

#### `OpenAiCompatibleAdapter`

Implementa chamadas HTTP sincronas para uma API compativel com o formato de `chat/completions`.

Entradas:

- prompt de sistema;
- payload JSON com o contexto do agente.

Saida esperada:

- JSON puro compativel com `DecisionEnvelope` ou `SocialScene`.

Se `OPENAI_API_KEY` nao estiver definida ou a inicializacao do adaptador falhar, o projeto cai automaticamente no `MockLlmAdapter`.

### `src/sim_core.rs`

E o coracao da simulacao.

#### Componentes ECS

O mundo ECS armazena componentes separados para:

- identidade do agente (`AgentCore`);
- perfil (`ProfileComponent`);
- estado atual (`StateComponent`);
- relacoes (`RelationComponent`);
- memorias (`MemoryComponent`);
- inventario (`InventoryComponent`);
- posicao/local (`LocationRef`);
- ultima intencao (`IntentComponent`);
- ultimo pensamento/reflexao (`ThoughtComponent`);
- orcamento/cooldown de decisao (`DecisionBudgetComponent`);
- definicao do local (`LocationComponent`);
- estoque do local (`StockpileComponent`).

#### Configuracao inicial

`Simulation::seeded()` cria a primeira vila com:

- nome padrao `Santa Bruma`;
- 24 ticks por dia;
- ate 12 agentes por default;
- locais semanticos como praca, campo, forja, padaria, taverna, posto da guarda e solar;
- aldeoes seedados com papeis sociais distintos;
- memorias iniciais, relacoes iniciais e pequenos estoques.

#### Loop de tick

Cada chamada de `Simulation::tick()` executa este fluxo:

1. avanca tempo global e tempo do dia;
2. aplica degradacao de necessidades basicas;
3. coleta o contexto de cada agente;
4. filtra eventos recentes relevantes;
5. recupera memorias relevantes;
6. monta `DecisionInput`;
7. chama o adaptador LLM;
8. valida a intencao retornada;
9. agenda a acao;
10. aplica a acao ao mundo;
11. registra evento e memoria resultante.

#### Degradacao fisiologica

Por tick, o motor aumenta ou reduz:

- `hunger`;
- `energy`;
- `stress`;
- `health` em situacoes extremas.

Esses valores afetam diretamente o tipo de intencao que o agente tende a produzir, especialmente no adaptador `mock`.

#### Aplicacao de acoes

As acoes implementadas hoje sao:

- `Work`
- `Rest`
- `Eat`
- `Reflect`
- `Wander`
- `Socialize`

Exemplos de efeito:

- `Work` altera energia, fome e stress, alem de produzir recursos no local;
- `Eat` tenta consumir comida do inventario do agente ou do estoque do local;
- `Wander` move o agente entre locais;
- `Socialize` chama uma segunda etapa cognitiva para produzir uma cena social resumida e um `RelationDelta`.

#### Relacoes e eventos sociais

As interacoes sociais modificam:

- confianca;
- amizade;
- ressentimento;
- atracao;
- divida moral;
- reputacao.

Essas mudancas sao persistidas dentro de `RelationComponent` e depois serializadas no snapshot.

O motor tambem grava `WorldEvent` para explicar o que ocorreu. Isso alimenta a timeline da TUI e o contexto cognitivo futuro.

#### Memoria

As memorias sao adicionadas quando:

- o agente reflete;
- trabalha;
- vive uma impressao social;
- sofre ou causa ofensa;
- realiza outras acoes significativas.

Cada memoria guarda:

- dia e tick;
- tipo;
- resumo;
- detalhes;
- peso emocional;
- entidades relacionadas;
- tags.

Existe truncamento simples do historico por agente para evitar crescimento ilimitado do vetor em memoria.

### `src/persistence.rs`

Responsavel por persistencia local em SQLite.

#### Estrategia

O projeto salva snapshots completos em JSON dentro da tabela `checkpoints` e tambem armazena copias indexadas de:

- eventos;
- memorias;
- relacoes.

Isso cria duas camadas:

- uma forma direta de restaurar a simulacao;
- uma base relacional minima para inspecao futura e queries offline.

#### Tabelas

- `checkpoints`
- `events`
- `memories`
- `relations`

#### Politicas de save

O codigo salva:

- ao fim de cada dia simulado com `kind = "daily"`;
- ao encerrar a aplicacao com `kind = "shutdown"`.

Na inicializacao, o binario tenta carregar o checkpoint mais recente ordenado por `total_ticks`.

### `src/tui.rs`

Interface terminal baseada em `ratatui` e `crossterm`.

#### Paineis atuais

- cabecalho com estado da simulacao, provedor LLM, velocidade e filtro;
- lista de aldeoes;
- detalhe do aldeao selecionado;
- timeline recente com destaque para eventos ligados ao agente selecionado;
- barra de ajuda/controles.

#### Informacoes por agente

O painel de detalhe mostra:

- nome;
- papel;
- local atual;
- foco atual;
- humor, energia, saude, fome e stress;
- ultima intencao;
- ultimo pensamento;
- relacao mais forte;
- memorias recentes.

#### Controles

- `q`: sair
- `espaco`: pausar/retomar
- `n`: avancar um tick manualmente
- `+`: acelerar
- `-`: desacelerar
- `seta para cima/baixo`: trocar agente selecionado
- `f`: alternar filtro por papel

## Fluxo de execucao do programa

O binario em `src/main.rs` faz o bootstrap nesta ordem:

1. le `VILLAGE_DB_PATH` ou usa `village_sim.sqlite`;
2. abre/inicializa o banco SQLite;
3. interpreta os argumentos de CLI e escolhe entre TUI ou headless;
4. tenta carregar o ultimo snapshot persistido, a menos que `--new` tenha sido usado;
5. se nao houver snapshot, cria uma vila seeded;
6. escolhe o adaptador LLM com `adapter_from_env()`;
7. entra na TUI ou no loop headless;
8. salva um checkpoint de encerramento ao sair.

## Modelo cognitivo em detalhe

O comportamento do agente hoje e dividido em duas decisoes LLM-compativeis:

### 1. Decisao principal

Entrada:

- identidade do agente;
- papel;
- dia e tick;
- local atual;
- estado emocional/fisiologico;
- resumo do perfil;
- agentes proximos;
- memorias relevantes;
- eventos recentes;
- metas atuais;
- orcamento residual.

Saida esperada:

- `reflection`
- `intent.kind`
- `intent.target_agent`
- `intent.target_location`
- `intent.justification`
- `intent.dominant_emotion`
- `intent.perceived_risk`
- `intent.belief_updates`
- `intent.priority`
- `intent.social_move`

### 2. Cena social

Quando a acao e social, o motor pede uma segunda resposta estruturada com:

- resumo textual da troca;
- nota subjetiva do ator;
- nota subjetiva do alvo;
- delta de relacao.

## Persistencia e retomada

O projeto ja e persistente por design.

Detalhes relevantes:

- o arquivo padrao do banco e `village_sim.sqlite`;
- o snapshot salvo inclui agentes, locais, relacoes, memorias, estoques, ultimo pensamento, ultima intencao e timeline;
- ao reiniciar, a simulacao retoma do ultimo checkpoint encontrado;
- o banco cresce com o tempo porque nao ha politica de compactacao ou retencao historica ainda.

## Como rodar

### Requisitos

- Rust toolchain recente;
- ambiente capaz de compilar o target do seu sistema;
- opcionalmente, uma chave de API compativel com o adaptador OpenAI.

### Execucao com mock local na TUI

Sem configurar credenciais, o projeto usa automaticamente o `MockLlmAdapter`.

```bash
cargo run
```

### Execucao headless

O binario tambem suporta execucao sem interface para rodar lotes, gerar checkpoints e observar a vila pelo stdout.

Exemplo curto:

```bash
cargo run -- --headless --new --ticks 48 --summary-every 6 --event-tail 6
```

Exemplo com mapa ASCII e save intervalar:

```bash
cargo run -- --headless --ticks 96 --save-every 24 --summary-every 12 --map
```

Flags principais:

- `--headless`: ativa o modo sem TUI;
- `--new`: ignora o save atual e cria uma vila nova;
- `--ticks N`: encerra apos `N` ticks executados neste processo;
- `--days N`: encerra apos `N` dias simulados neste processo;
- `--save-every N`: grava checkpoint intervalar a cada `N` ticks; `0` desativa esse save intervalar;
- `--summary-every N`: imprime um relatorio a cada `N` ticks;
- `--event-tail N`: define quantos eventos recentes entram em cada relatorio;
- `--map`: inclui o mapa ASCII completo nos relatorios;
- `--db PATH`: sobrescreve `VILLAGE_DB_PATH`;
- `--seed`, `--agents`, `--ticks-per-day`, `--grid-width`, `--grid-height`, `--village-name`: configuram a criacao de um mundo novo.

### Execucao com adaptador OpenAI-compatible

Defina as variaveis:

```bash
OPENAI_API_KEY=...
OPENAI_MODEL=gpt-4.1-mini
OPENAI_BASE_URL=https://api.openai.com/v1/chat/completions
VILLAGE_DB_PATH=village_sim.sqlite
```

Depois execute:

```bash
cargo run
```

## Como testar

```bash
cargo test
```

Os testes atuais cobrem:

- parse do JSON cognitivo;
- ranking de memorias relevantes;
- persistencia e restauracao de snapshot;
- simulacao de uma semana com eventos sociais.

## Compatibilidade de ambiente

Neste workspace, a validacao foi feita com sucesso via WSL usando `cargo test`.

No Windows nativo desta maquina, o toolchain Rust nao estava totalmente funcional para linkedicao porque faltavam:

- `link.exe`
- bibliotecas do Windows SDK como `kernel32.lib`

Se isso acontecer no seu ambiente Windows, ha dois caminhos praticos:

- instalar Visual Studio Build Tools com o SDK de C++;
- rodar o projeto via WSL/Linux.

## Limitacoes atuais

- o adaptador OpenAI-compatible usa `chat/completions`, nao `responses`;
- chamadas LLM sao sincronas;
- ainda nao ha retry/backoff sofisticado;
- o orcamento de chamadas por agente e basico e hoje serve mais como metadado do que como limitador rigido;
- os agentes ainda nao formam instituicoes, familias formais ou cadeias economicas profundas;
- a explicabilidade e boa na TUI, mas ainda resumida;
- a memoria de longo prazo ainda nao tem compressao semantica.

## Proximos passos recomendados

- adicionar configuracao externa para tamanho da populacao, ticks por dia e seed;
- introduzir politica real de orcamento de tokens e cooldown cognitivo;
- expandir o sistema de dialogo para conversas multi-turno;
- separar evento factual de interpretacao subjetiva com mais rigor;
- adicionar inspecao por relacao bilateral na TUI;
- criar export de snapshots para analise offline;
- evoluir o adaptador para `responses` e/ou streaming;
- introduzir economia mais consistente entre producao, consumo e escassez.
