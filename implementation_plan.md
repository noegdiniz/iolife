# Plano de Geração de Mundo Procedural (`world_gen`)

Este plano descreve o desacoplamento e a criação de um gerador de mundo procedural isolado para o *Medieval Village LLM*. O objetivo é remover do núcleo de simulação (`sim_core.rs`) a lógica rígida de criação do mundo e movê-la para um novo módulo `src/world_gen.rs`. O gerador criará um mapa dinâmico de 150x100 contendo pelo menos 3 vilas independentes, interconectadas por estradas, cada uma com sua economia completa e população de agentes aleatórios gerados a partir de pools de dados históricos.

---

## Proposta de Arquitetura

### 1. Novo Módulo de Geração (`src/world_gen.rs`)
O módulo exportará uma única função principal:
```rust
pub fn generate_world(config: SimulationConfig) -> SimulationSnapshot
```

Esta função cuidará de:
1. Criar o snapshot espacial (`SpatialSnapshot`) contendo o grid, estradas, terrenos e prédios.
2. Criar os agentes procedurais com perfis, relações e posições iniciais.
3. Inicializar os estoques dos estabelecimentos e as economias domésticas.
4. Produzir o snapshot inicial completo (`SimulationSnapshot`), que será passado para a simulação iniciar.

---

## Layout do Mapa e Conexão de Vilas

### Grid e Localização
O mapa padrão passará a ser de **150x100** (configurável via CLI). Colocaremos 3 vilas em regiões distantes:
- **Vila 1 (Norte)**: Centro em `(75, 22)`
- **Vila 2 (Sudoeste)**: Centro em `(35, 72)`
- **Vila 3 (Sudeste)**: Centro em `(115, 72)`

### Template Relativo de Vila
Cada vila terá um centro `(cx, cy)` e seus prédios serão alocados em posições relativas fixas para evitar sobreposição espacial, garantindo que as portas conectem diretamente à malha de estradas:

| Prédio / Área | Tipo | Retângulo Relativo | Porta Relativa |
| :--- | :--- | :--- | :--- |
| **Estrada Horizontal** | Estrada | `(cx-22..=cx+22, cy)` | - |
| **Estrada Vertical** | Estrada | `(cx, cy-16..=cy+17)` | - |
| **Casa 1** | Residência | `(cx-20..cx-14, cy-15..cy-10)` | `(cx-17, cy-10)` |
| **Casa 2** | Residência | `(cx-10..cx-4, cy-15..cy-10)` | `(cx-7, cy-10)` |
| **Casa 3** | Residência | `(cx+4..cx+10, cy-15..cy-10)` | `(cx+7, cy-10)` |
| **Casa 4** | Residência | `(cx+14..cx+20, cy-15..cy-10)` | `(cx+17, cy-10)` |
| **Solar do Conselho** | Solar (Manor) | `(cx-20..cx-10, cy-7..cy-1)` | `(cx-15, cy-1)` |
| **Posto da Muralha** | Guarda | `(cx+10..cx+16, cy-7..cy-1)` | `(cx+13, cy-1)` |
| **Forja de Aço** | Oficina | `(cx-20..cx-12, cy+3..cy+9)` | `(cx-16, cy+3)` |
| **Taverna** | Taverna | `(cx-10..cx-1, cy+3..cy+9)` | `(cx-5, cy+3)` |
| **Padaria** | Padaria | `(cx+12..cx+20, cy+3..cy+9)` | `(cx+16, cy+3)` |
| **Galpão do Lenhal** | Lenhal | `(cx-20..cx-14, cy+12..cy+17)` | `(cx-17, cy+12)` |
| **Celeiro (Farm)** | Fazenda | `(cx-4..cx+4, cy+12..cy+17)` | `(cx, cy+12)` |
| **Pedreira** | Pedreira | `(cx+14..cx+20, cy+12..cy+17)` | `(cx+17, cy+12)` |

- Cada vila terá seus terrenos ao redor:
  - Floresta local (`TileKind::Forest`) próxima ao Galpão.
  - Pedregulho local (`TileKind::Rock`) próximo à Pedreira.
  - Campos de plantio (`TileKind::Field`) próximos ao Celeiro.

### Conexões de Estradas (Highways)
Conectaremos os centros das 3 vilas desenhando estradas retas:
- Estrada 1-2: De `(75, 22)` para `(35, 72)`
- Estrada 1-3: De `(75, 22)` para `(115, 72)`
- Estrada 2-3: De `(35, 72)` para `(115, 72)`

Isto garante que a malha rodoviária inteira é **100% conectada** e navegável por BFS.

---

## Nomes e Agentes Procedurais

### Geração de Nomes Coesos
- **Vilas**: Escolhidas de um pool temático (`Santa Bruma`, `Vale Verde`, `Pedra Ruiva`, `Montes Belos`, `Porto Real`, `Rio Claro`).
- **Prédios**: Nomeados de acordo com o nome da vila (ex: `"Padaria de Santa Bruma"`, `"Forja de Pedra Ruiva"`).
- **Agentes**: Nomes aleatórios extraídos de um pool de ~50 nomes medievais tradicionais portugueses.

### População da Vila
Geraremos 7 agentes por vila (total de 21 agentes):
- **Vila 1**: 7 agentes (ID 1 a 7)
- **Vila 2**: 7 agentes (ID 8 a 14)
- **Vila 3**: 7 agentes (ID 15 a 21)

Em cada vila, distribuiremos os seguintes papéis sociais (`Role`):
1. **Líder Local (Headman)** - trabalha no Solar.
2. **Guarda (Guard)** - trabalha no Posto da Guarda.
3. **Ferreiro (Blacksmith)** - trabalha na Forja.
4. **Padeiro (Baker)** - trabalha na Padaria.
5. **Taverneiro (TavernKeeper)** - trabalha na Taverna.
6. **2 Camponeses (Farmer)** - um trabalha no Celeiro (Farm) e outro no Galpão/Pedreira.

### Relações e Estados Iniciais
- **Relações Bilaterais**: Agentes da mesma vila iniciam com relações ligeiramente amigáveis/neutras (afinidade e amizade positivas). Agentes de vilas diferentes iniciam com relação neutra ou de desconfiança (amizade = 0, confiança levemente negativa para simular xenofobia medieval).
- **Estados Fisiológicos**: Fome (20 a 35), Energia (60 a 75), Stress (10 a 25), Humor (50 a 65).
- **Camas**: Mapeadas diretamente a camas livres localizadas nas 4 casas residenciais de sua respectiva vila, garantindo que o `home_building_id` seja uma moradia válida.

---

## Modificações Propostas

### 1. Criação do Módulo de Geração (`src/world_gen.rs`)
- **[NEW] [world_gen.rs](file:///c:/PROJETOS/Projeto1/src/world_gen.rs)**
  - Implementar o template relativo de prédios.
  - Implementar algoritmos procedurais para desenhar estradas de interligação.
  - Implementar a geração de agentes aleatórios, perfis, relações estruturadas e stocks iniciais para estabelecimentos e residências.
  - Exportar `generate_world`.

### 2. Ajustes na Configuração e CLI (`src/cli.rs`)
- **[MODIFY] [cli.rs](file:///c:/PROJETOS/Projeto1/src/cli.rs)**
  - Adicionar argumentos CLI:
    - `--num-villages`: Número de vilas a gerar (padrão 3).
    - `--width`: Largura do mapa (padrão 150).
    - `--height`: Altura do mapa (padrão 100).
    - `--population`: Número total de agentes a gerar (padrão 21).

### 3. Ajustes no Núcleo de Simulação (`src/sim_core.rs`)
- **[MODIFY] [sim_core.rs](file:///c:/PROJETOS/Projeto1/src/sim_core.rs)**
  - Remover as funções privadas `generate_village`, `seeded_agents`, `initialize_economy_state` e o helper `fixture`.
  - Modificar `Simulation::seeded(config)` para receber o snapshot inicial a partir do `world_gen` ao invés de codificar a geração de forma estática.
  - Atualizar imports.

### 4. Registro no Módulo Central (`src/lib.rs`)
- **[MODIFY] [lib.rs](file:///c:/PROJETOS/Projeto1/src/lib.rs)**
  - Adicionar `pub mod world_gen;`.

---

## Plano de Verificação

### Testes Automatizados
- Executar `cargo check` para garantir consistência estrutural e tipagem corretas.
- Adicionar ou atualizar testes de integração existentes em `tests/` para verificar se a simulação inicia e funciona perfeitamente com o mundo dinâmico e 21 agentes.

### Verificação Manual
- Executar a simulação no modo headless por pelo menos 1 dia simulado:
  `cargo run -- --headless --new --ticks 48 --summary-every 12`
- Inspecionar visualmente o mapa gerado na TUI ou no log de eventos para atestar que os agentes de todas as 3 vilas conseguem transitar, trabalhar, receber salários e comprar comida sem travar em caminhos inválidos.
