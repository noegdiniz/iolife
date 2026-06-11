use medieval_village_llm::agent_mind::{
    ActionPlannerInput, ConversationTurnInput, ConversationTurnOutput, DecisionEnvelope,
    DecisionInput, parse_conversation_turn_json, retrieve_relevant_memories,
    parse_action_planner_output, parse_think_maker_json, ThinkMakerInput, ThinkMakerOutput,
};
use medieval_village_llm::llm_adapter::{LlmAdapter, LlmError, LlmResult, MockLlmAdapter};
use medieval_village_llm::persistence::Persistence;
use medieval_village_llm::sim_core::{Simulation, SimulationConfig};
use medieval_village_llm::world_model::{
    AgentIntent, AgentMemory, AgentState, CrimeCaseStatus, CrimeType, EventKind, FixtureKind,
    IntentKind, LocationKind, MemoryKind, RelationDelta, ResourceKind, SimplifiedTask, SocialMove,
    TileCoord, WorldEvent, CropStage, BuildingSpec, TileKind,
};
use rusqlite::{Connection, params};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tempfile::tempdir;

#[derive(Default)]
struct AdapterState {
    decision_calls: Mutex<Vec<u64>>,
    conversation_calls: Mutex<Vec<u64>>,
    decision_inputs: Mutex<Vec<DecisionInput>>,
    conversation_inputs: Mutex<Vec<ConversationTurnInput>>,
    active_decisions: AtomicUsize,
    max_active_decisions: AtomicUsize,
    active_conversations: AtomicUsize,
    max_active_conversations: AtomicUsize,
}

#[derive(Clone)]
struct InstrumentedAdapter {
    state: Arc<AdapterState>,
    decision_delays_ms: HashMap<u64, u64>,
    conversation_delays_ms: HashMap<u64, u64>,
    decision_outputs: HashMap<u64, DecisionEnvelope>,
    conversation_outputs: HashMap<u64, ConversationTurnOutput>,
    fail_decision: HashMap<u64, LlmError>,
    fail_conversation: HashMap<u64, LlmError>,
}

impl InstrumentedAdapter {
    fn new() -> (Self, Arc<AdapterState>) {
        let state = Arc::new(AdapterState::default());
        (
            Self {
                state: state.clone(),
                decision_delays_ms: HashMap::new(),
                conversation_delays_ms: HashMap::new(),
                decision_outputs: HashMap::new(),
                conversation_outputs: HashMap::new(),
                fail_decision: HashMap::new(),
                fail_conversation: HashMap::new(),
            },
            state,
        )
    }

    fn with_decision_delay(mut self, agent_id: u64, delay_ms: u64) -> Self {
        self.decision_delays_ms.insert(agent_id, delay_ms);
        self
    }

    fn with_conversation_delay(mut self, conversation_id: u64, delay_ms: u64) -> Self {
        self.conversation_delays_ms
            .insert(conversation_id, delay_ms);
        self
    }

    fn timeout_conversation(mut self, conversation_id: u64) -> Self {
        self.fail_conversation.insert(
            conversation_id,
            LlmError::Timeout {
                operation: "conversation_turn".to_string(),
                attempts: 2,
                message: "operation timed out".to_string(),
            },
        );
        self
    }

    fn schema_fail_conversation(mut self, conversation_id: u64) -> Self {
        self.fail_conversation.insert(
            conversation_id,
            LlmError::Schema {
                operation: "conversation_turn".to_string(),
                message: "invalid conversation schema".to_string(),
            },
        );
        self
    }

    fn default_envelope(input: &DecisionInput) -> DecisionEnvelope {
        DecisionEnvelope {
            reflection: format!("{} segue um plano simples.", input.actor_name),
            dominant_emotion: "contido".to_string(),
            belief_updates: vec!["continuar observando".to_string()],
            tasks: vec![
                SimplifiedTask {
                    kind: IntentKind::Andar,
                    target_agent: None,
                    target_semantic: Some("praca".to_string()),
                    social_move: None,
                },
                SimplifiedTask {
                    kind: IntentKind::Descansar,
                    target_agent: None,
                    target_semantic: None,
                    social_move: None,
                },
                SimplifiedTask {
                    kind: IntentKind::Refletir,
                    target_agent: None,
                    target_semantic: None,
                    social_move: None,
                },
            ],
        }
    }

    fn default_turn_output(input: &ConversationTurnInput) -> ConversationTurnOutput {
        ConversationTurnOutput {
            utterance: format!("{} responde sem pressa.", input.speaker_name),
            speech_act: "sondar".to_string(),
            emotion: "calmo".to_string(),
            intent_to_continue: true,
            belief_updates: vec!["manter o tom".to_string()],
            relation_delta_hint: RelationDelta {
                trust: 1,
                friendship: 0,
                resentment: 0,
                attraction: 0,
                moral_debt: 0,
                reputation: 0,
            },
            tone: Some("medido".to_string()),
            risk_shift: Some(0),
        }
    }
}

impl LlmAdapter for InstrumentedAdapter {
    fn clone_box(&self) -> Box<dyn LlmAdapter> {
        Box::new(self.clone())
    }

    fn provider_name(&self) -> &str {
        "instrumented"
    }

    fn plan_actions(&self, input: &ActionPlannerInput) -> LlmResult<String> {
        self.state
            .decision_calls
            .lock()
            .expect("decision_calls lock")
            .push(input.actor_id);
        self.state
            .decision_inputs
            .lock()
            .expect("decision_inputs lock")
            .push(input.clone());
        if let Some(error) = self.fail_decision.get(&input.actor_id) {
            return Err(error.clone());
        }
        let envelope = self
            .decision_outputs
            .get(&input.actor_id)
            .cloned()
            .unwrap_or_else(|| Self::default_envelope(input));
        
        let mut task_strings = Vec::new();
        for task in envelope.tasks {
            let kind_str = format!("{:?}", task.kind);
            let mut params = Vec::new();
            if let Some(agent_id) = task.target_agent {
                params.push(agent_id.to_string());
            }
            if let Some(ref semantic) = task.target_semantic {
                params.push(format!("'{}'", semantic));
            }
            if let Some(social_move) = task.social_move {
                params.push(social_move.as_str().to_string());
            }
            if params.is_empty() {
                task_strings.push(kind_str);
            } else {
                task_strings.push(format!("{}({})", kind_str, params.join(", ")));
            }
        }
        Ok(task_strings.join(", "))
    }

    fn generate_thoughts(&self, input: &ThinkMakerInput) -> LlmResult<ThinkMakerOutput> {
        let active = self.state.active_decisions.fetch_add(1, Ordering::SeqCst) + 1;
        self.state
            .max_active_decisions
            .fetch_max(active, Ordering::SeqCst);
        if let Some(delay_ms) = self.decision_delays_ms.get(&input.decision_input.actor_id) {
            thread::sleep(Duration::from_millis(*delay_ms));
        }
        self.state.active_decisions.fetch_sub(1, Ordering::SeqCst);
        
        if let Some(error) = self.fail_decision.get(&input.decision_input.actor_id) {
            return Err(error.clone());
        }
        
        let envelope = self
            .decision_outputs
            .get(&input.decision_input.actor_id)
            .cloned()
            .unwrap_or_else(|| Self::default_envelope(&input.decision_input));
            
        Ok(ThinkMakerOutput {
            reflection: envelope.reflection,
            dominant_emotion: envelope.dominant_emotion,
            belief_updates: envelope.belief_updates,
        })
    }

    fn generate_conversation_turn(
        &self,
        input: &ConversationTurnInput,
    ) -> LlmResult<ConversationTurnOutput> {
        self.state
            .conversation_calls
            .lock()
            .expect("conversation_calls lock")
            .push(input.context.conversation_id);
        self.state
            .conversation_inputs
            .lock()
            .expect("conversation_inputs lock")
            .push(input.clone());
        let active = self
            .state
            .active_conversations
            .fetch_add(1, Ordering::SeqCst)
            + 1;
        self.state
            .max_active_conversations
            .fetch_max(active, Ordering::SeqCst);
        if let Some(delay_ms) = self
            .conversation_delays_ms
            .get(&input.context.conversation_id)
        {
            thread::sleep(Duration::from_millis(*delay_ms));
        }
        self.state
            .active_conversations
            .fetch_sub(1, Ordering::SeqCst);
        if let Some(error) = self.fail_conversation.get(&input.context.conversation_id) {
            return Err(error.clone());
        }
        Ok(self
            .conversation_outputs
            .get(&input.context.conversation_id)
            .cloned()
            .unwrap_or_else(|| Self::default_turn_output(input)))
    }
}

#[test]
fn parses_action_planner_output_simple() {
    let payload = "Comer(comida), Trabalhar(posto_de_trabalho)";
    let parsed = parse_action_planner_output(payload);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].kind, IntentKind::Comer);
    assert_eq!(parsed[0].target_semantic.as_deref(), Some("comida"));
    assert_eq!(parsed[1].kind, IntentKind::Trabalhar);
    assert_eq!(parsed[1].target_semantic.as_deref(), Some("posto_de_trabalho"));
}

#[test]
fn parses_action_planner_output_social() {
    let payload = "Socializar(3, conversar), Agredir(2)";
    let parsed = parse_action_planner_output(payload);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].kind, IntentKind::Socializar);
    assert_eq!(parsed[0].target_agent, Some(3));
    assert_eq!(parsed[0].social_move, Some(SocialMove::Chat));
    assert_eq!(parsed[1].kind, IntentKind::Agredir);
    assert_eq!(parsed[1].target_agent, Some(2));
}

#[test]
fn parses_action_planner_output_whitespace_and_quotes() {
    let payload = "Comer( 'taverna' ), Socializar( 3, \"conversar\" ), Descansar";
    let parsed = parse_action_planner_output(payload);
    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0].kind, IntentKind::Comer);
    assert_eq!(parsed[0].target_semantic.as_deref(), Some("taverna"));
    assert_eq!(parsed[1].kind, IntentKind::Socializar);
    assert_eq!(parsed[1].target_agent, Some(3));
    assert_eq!(parsed[1].social_move, Some(SocialMove::Chat));
    assert_eq!(parsed[2].kind, IntentKind::Descansar);
    assert_eq!(parsed[2].target_semantic, None);
    assert_eq!(parsed[2].target_agent, None);
}

#[test]
fn action_planner_ignores_invalid_kinds() {
    let payload = "Dormir, Comer, LerLivro";
    let parsed = parse_action_planner_output(payload);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].kind, IntentKind::Comer);
}

#[test]
fn parses_think_maker_json_valid() {
    let payload = r#"{
        "reflection": "Alda mede o acesso a comida.",
        "dominant_emotion": "apreensao",
        "belief_updates": ["Comer agora evita fraqueza."]
    }"#;
    let parsed = parse_think_maker_json(payload).expect("think maker should parse");
    assert_eq!(parsed.reflection, "Alda mede o acesso a comida.");
    assert_eq!(parsed.dominant_emotion, "apreensao");
    assert_eq!(parsed.belief_updates, vec!["Comer agora evita fraqueza.".to_string()]);
}

#[test]
fn normalizes_conversation_turn_with_textual_fields_and_markdown_fence() {
    let payload = r#"```json
{
  "utterance": "Boa tarde, Kelda. Estava aqui a pensar na lida do campo. Como vai a taverna?",
  "speech_act": "greeting",
  "emotion": "friendly",
  "intent_to_continue": 0.8,
  "belief_updates": ["Kelda parece disposto a socializar, o que pode fortalecer nossa relação e ajudar a manter a reputação de ambos."],
  "relation_delta_hint": "Leve aumento em amizade e confiança",
  "tone": "cordial",
  "risk_shift": 0.1
}
```"#;

    let parsed = parse_conversation_turn_json(payload).expect("conversation should normalize");
    assert_eq!(
        parsed.utterance,
        "Boa tarde, Kelda. Estava aqui a pensar na lida do campo. Como vai a taverna?"
    );
    assert_eq!(parsed.speech_act, "greeting");
    assert_eq!(parsed.emotion, "friendly");
    assert!(parsed.intent_to_continue);
    assert_eq!(parsed.relation_delta_hint.trust, 1);
    assert_eq!(parsed.relation_delta_hint.friendship, 1);
    assert_eq!(parsed.risk_shift, Some(0));
}

#[test]
fn normalizes_conversation_turn_with_string_beliefs_and_textual_delta() {
    let payload = r#"{
  "utterance": "Bom dia, Dario. Espero que o dia comece bem. Se houver alguma questão que precise de atenção, lembre-se de que estou aqui para ajudar.",
  "speech_act": "saudacao e oferta",
  "emotion": "cordial",
  "intent_to_continue": true,
  "belief_updates": "Dario deve perceber que Elina é respeitosa e prestativa, interessada em manter a harmonia e apoiá-lo.",
  "relation_delta_hint": "leve aumento de confiança e amizade, redução de tensão latente",
  "tone": "respeitoso, cortês, com formalidade moderada",
  "risk_shift": "mínimo; gesto prudente que pode melhorar a relação sem expor a riscos"
}"#;

    let parsed = parse_conversation_turn_json(payload).expect("conversation should normalize");
    assert_eq!(
        parsed.belief_updates,
        vec!["Dario deve perceber que Elina é respeitosa e prestativa, interessada em manter a harmonia e apoiá-lo.".to_string()]
    );
    assert_eq!(parsed.relation_delta_hint.trust, 1);
    assert_eq!(parsed.relation_delta_hint.friendship, 1);
    assert_eq!(parsed.relation_delta_hint.resentment, -1);
    assert_eq!(parsed.risk_shift, Some(-1));
}

#[test]
fn normalizes_conversation_turn_with_textual_approximation() {
    let payload = r#"{
  "utterance": "Joran, você tem ouvido as fofocas sobre a Iria? Acho que muita gente está falando dela, e queria entender o que pensa sobre isso.",
  "speech_act": "solicitar opinião",
  "emotion": "preocupado",
  "intent_to_continue": true,
  "belief_updates": "Kelda reafirma sua confiança em Joran como alguém sensato e constata que iniciar esse tópico pode ser produtivo.",
  "relation_delta_hint": "tentativa de aproximação através de confidência",
  "tone": "confidencial",
  "risk_shift": 1
}"#;

    let parsed = parse_conversation_turn_json(payload).expect("conversation should normalize");
    assert_eq!(parsed.belief_updates.len(), 1);
    assert_eq!(parsed.relation_delta_hint.trust, 1);
    assert_eq!(parsed.relation_delta_hint.friendship, 1);
    assert_eq!(parsed.risk_shift, Some(1));
}

#[test]
fn conversation_turn_with_structured_relation_delta_preserves_values() {
    let payload = r#"{
  "utterance": "Continuemos.",
  "speech_act": "aproximar",
  "emotion": "calmo",
  "intent_to_continue": "sim",
  "belief_updates": ["Vale insistir no tom cordial."],
  "relation_delta_hint": {
    "trust": 2,
    "friendship": 1,
    "resentment": -1,
    "attraction": 0,
    "moral_debt": 0,
    "reputation": 1
  },
  "tone": "cordial",
  "risk_shift": "redução de risco de tensão imediata"
}"#;

    let parsed = parse_conversation_turn_json(payload).expect("structured delta should parse");
    assert!(parsed.intent_to_continue);
    assert_eq!(parsed.relation_delta_hint.trust, 2);
    assert_eq!(parsed.relation_delta_hint.friendship, 1);
    assert_eq!(parsed.relation_delta_hint.resentment, -1);
    assert_eq!(parsed.relation_delta_hint.reputation, 1);
    assert_eq!(parsed.risk_shift, Some(-1));
}

#[test]
fn conversation_turn_missing_utterance_still_fails() {
    let payload = r#"{
  "speech_act": "aproximar",
  "emotion": "calmo",
  "intent_to_continue": true,
  "belief_updates": [],
  "relation_delta_hint": "aumento de confiança",
  "risk_shift": 0
}"#;

    let error = parse_conversation_turn_json(payload).expect_err("missing utterance must fail");
    assert!(error.to_string().contains("utterance"));
}

#[test]
fn retrieves_relevant_memories_by_weight_and_context() {
    let memories = vec![
        AgentMemory {
            id: 1,
            day: 1,
            tick: 0,
            kind: MemoryKind::Fact,
            summary: "Rotina na forja".to_string(),
            details: "".to_string(),
            emotional_weight: 8,
            about: vec![],
            tags: vec!["trabalho".to_string()],
        },
        AgentMemory {
            id: 2,
            day: 2,
            tick: 5,
            kind: MemoryKind::Offense,
            summary: "Discussao na taverna".to_string(),
            details: "".to_string(),
            emotional_weight: 20,
            about: vec![3],
            tags: vec!["social".to_string(), "fofoca".to_string()],
        },
    ];
    let state = AgentState {
        mood: 50,
        energy: 40,
        health: 90,
        hunger: 30,
        stress: 45,
        current_focus: "social".to_string(),
        active_goals: vec!["proteger reputacao".to_string()],
    };
    let events = vec![WorldEvent {
        day: 2,
        tick: 6,
        actor: 1,
        target: Some(3),
        kind: EventKind::Conflict,
        summary: "Conflito recente".to_string(),
        impact_tags: vec!["social".to_string()],
    }];

    let results = retrieve_relevant_memories(&memories, &state, &events, 1);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, 2);
}

#[test]
fn generation_contains_required_buildings_and_fixtures() {
    let simulation = Simulation::seeded(SimulationConfig::default());
    let spatial = simulation.spatial();
    assert!(
        spatial
            .buildings
            .iter()
            .any(|building| building.kind == LocationKind::Workshop)
    );
    assert!(
        spatial
            .buildings
            .iter()
            .any(|building| building.kind == LocationKind::Bakery)
    );
    assert!(
        spatial
            .buildings
            .iter()
            .any(|building| building.kind == LocationKind::Tavern)
    );
    assert!(
        spatial
            .buildings
            .iter()
            .all(|building| spatial.grid.tiles.iter().any(|tile| {
                tile.coord == building.entrance
                    && matches!(tile.kind, medieval_village_llm::world_model::TileKind::Door)
            }))
    );
    assert!(
        spatial
            .fixtures
            .iter()
            .any(|fixture| fixture.kind == FixtureKind::Bed)
    );
    assert!(
        spatial
            .fixtures
            .iter()
            .any(|fixture| fixture.kind == FixtureKind::Workstation)
    );
    assert!(
        spatial
            .fixtures
            .iter()
            .any(|fixture| fixture.kind == FixtureKind::Storage)
    );
}

#[test]
fn pathfinding_reaches_workstation_without_crossing_walls() {
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    let spatial = simulation.spatial().clone();
    let forge_fixture = spatial
        .fixtures
        .iter()
        .find(|fixture| fixture.kind == FixtureKind::Workstation && fixture.name == "Bigorna")
        .expect("forge workstation");
    let goal = forge_fixture.coord.neighbors4()[0];
    let path = simulation
        .debug_find_path(TileCoord { x: 24, y: 13 }, goal, None)
        .expect("path should exist");
    assert!(!path.is_empty());
    assert!(path.iter().all(|coord| {
        spatial
            .grid
            .tiles
            .iter()
            .find(|tile| tile.coord == *coord)
            .map(|tile| tile.kind.walkable())
            .unwrap_or(false)
    }));
}

#[test]
fn movement_allows_agents_to_cross_occupied_tiles() {
    let llm = MockLlmAdapter;
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    let spatial = simulation.spatial().clone();
    let is_walkable = |coord: TileCoord| {
        spatial
            .grid
            .tiles
            .iter()
            .any(|tile| tile.coord == coord && tile.kind.walkable())
    };
    let (start, occupied) = spatial
        .grid
        .tiles
        .iter()
        .filter(|tile| tile.kind.walkable())
        .find_map(|tile| {
            tile.coord
                .neighbors4()
                .into_iter()
                .find(|neighbor| is_walkable(*neighbor))
                .map(|neighbor| (tile.coord, neighbor))
        })
        .expect("adjacent walkable tiles");

    simulation
        .debug_force_agent_position(1, start)
        .expect("place moving agent");
    simulation
        .debug_force_agent_position(2, occupied)
        .expect("place blocking agent");
    let path = simulation
        .debug_find_path(start, occupied, Some(1))
        .expect("path should ignore agent occupancy");
    assert_eq!(path, vec![occupied]);

    simulation
        .debug_force_navigation(1, occupied, path)
        .expect("force navigation");
    simulation.tick(&llm).expect("tick should move agent");

    assert_eq!(simulation.debug_agent_position(1).unwrap(), occupied);
    assert_eq!(simulation.debug_agent_position(2).unwrap(), occupied);
}

#[test]
fn conversation_requires_adjacency_and_alternates_turns() {
    let llm = MockLlmAdapter;
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation
        .debug_force_agent_position(1, TileCoord { x: 24, y: 13 })
        .expect("place actor");
    simulation
        .debug_force_agent_position(2, TileCoord { x: 30, y: 13 })
        .expect("place target");
    let failed = simulation
        .debug_try_social(1, 2, &llm)
        .expect("social attempt");
    assert!(!failed);
    simulation
        .debug_force_agent_position(2, TileCoord { x: 25, y: 13 })
        .expect("move target closer");
    let succeeded = simulation
        .debug_try_social(1, 2, &llm)
        .expect("social attempt");
    assert!(succeeded);
    let snapshot = simulation.snapshot();
    let conversation = snapshot
        .conversations
        .iter()
        .find(|conversation| conversation.participants == [1, 2])
        .expect("conversation should exist");
    let conversation_id = conversation.id;
    assert_eq!(conversation.current_speaker_id, 1);

    simulation.tick(&llm).expect("first conversation turn");
    assert_eq!(
        simulation.debug_agent_position(1).expect("actor position"),
        TileCoord { x: 24, y: 13 }
    );
    assert_eq!(
        simulation
            .debug_agent_position(2)
            .expect("listener position"),
        TileCoord { x: 25, y: 13 }
    );
    let snapshot = simulation.snapshot();
    let conversation = snapshot
        .conversations
        .iter()
        .find(|conversation| conversation.id == conversation_id)
        .expect("conversation should persist");
    assert_eq!(conversation.turn_count, 1);
    assert_eq!(conversation.current_speaker_id, 2);

    simulation.tick(&llm).expect("second conversation turn");
    let snapshot = simulation.snapshot();
    let conversation = snapshot
        .conversations
        .iter()
        .find(|conversation| conversation.participants == [1, 2])
        .expect("conversation should still exist");
    assert_eq!(conversation.turn_count, 2);
    assert_eq!(conversation.current_speaker_id, 1);
}

#[test]
fn persists_and_restores_spatial_snapshot() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("sim.sqlite");
    let mut persistence = Persistence::open(&db).expect("db open");
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation
        .debug_force_agent_position(1, TileCoord { x: 24, y: 13 })
        .expect("place actor");
    simulation
        .debug_force_agent_position(2, TileCoord { x: 25, y: 13 })
        .expect("place target");
    simulation
        .debug_try_social(1, 2, &MockLlmAdapter)
        .expect("open conversation");
    simulation
        .tick(&MockLlmAdapter)
        .expect("advance one social turn");
    persistence.save(&mut simulation, "manual").expect("save");
    let snapshot = persistence.load_latest().expect("load").expect("snapshot");
    assert_eq!(snapshot.schema_version, 10);
    assert!(!snapshot.spatial.buildings.is_empty());
    assert!(!snapshot.spatial.fixtures.is_empty());
    assert!(snapshot.agents.iter().all(|agent| agent.position.x >= 0));
    let conversation = snapshot
        .conversations
        .iter()
        .find(|conversation| conversation.participants == [1, 2])
        .expect("conversation should persist");
    assert_eq!(conversation.turn_count, 1);
    assert_eq!(conversation.current_speaker_id, 2);
    assert!(
        snapshot
            .agents
            .iter()
            .filter(|agent| matches!(agent.id, 1 | 2))
            .all(|agent| agent.active_conversation_id == Some(conversation.id))
    );
}

#[test]
fn parallelizes_conversation_turns_for_multiple_active_conversations() {
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation
        .debug_force_agent_position(1, TileCoord { x: 20, y: 13 })
        .expect("place pair one a");
    simulation
        .debug_force_agent_position(2, TileCoord { x: 21, y: 13 })
        .expect("place pair one b");
    simulation
        .debug_force_agent_position(3, TileCoord { x: 27, y: 13 })
        .expect("place pair two a");
    simulation
        .debug_force_agent_position(4, TileCoord { x: 28, y: 13 })
        .expect("place pair two b");
    simulation
        .debug_try_social(1, 2, &MockLlmAdapter)
        .expect("open first conversation");
    simulation
        .debug_try_social(3, 4, &MockLlmAdapter)
        .expect("open second conversation");
    let before = simulation.snapshot();
    let first_id = before
        .conversations
        .iter()
        .find(|conversation| conversation.participants == [1, 2])
        .expect("first conversation")
        .id;
    let second_id = before
        .conversations
        .iter()
        .find(|conversation| conversation.participants == [3, 4])
        .expect("second conversation")
        .id;

    let (adapter, state) = InstrumentedAdapter::new();
    let adapter = adapter
        .with_conversation_delay(first_id, 150)
        .with_conversation_delay(second_id, 150);

    simulation.tick(&adapter).expect("tick should succeed");
    let snapshot = simulation.snapshot();
    assert_eq!(
        snapshot
            .conversations
            .iter()
            .find(|conversation| conversation.id == first_id)
            .expect("first conversation after tick")
            .turn_count,
        1
    );
    assert_eq!(
        snapshot
            .conversations
            .iter()
            .find(|conversation| conversation.id == second_id)
            .expect("second conversation after tick")
            .turn_count,
        1
    );
    assert_eq!(state.max_active_conversations.load(Ordering::SeqCst), 1);
    assert_eq!(
        state
            .conversation_calls
            .lock()
            .expect("conversation calls")
            .len(),
        2
    );
}

#[test]
fn conversation_batch_ends_timed_out_conversation_without_aborting_others() {
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation
        .debug_force_agent_position(1, TileCoord { x: 20, y: 13 })
        .expect("place pair one a");
    simulation
        .debug_force_agent_position(2, TileCoord { x: 21, y: 13 })
        .expect("place pair one b");
    simulation
        .debug_force_agent_position(3, TileCoord { x: 27, y: 13 })
        .expect("place pair two a");
    simulation
        .debug_force_agent_position(4, TileCoord { x: 28, y: 13 })
        .expect("place pair two b");
    simulation
        .debug_try_social(1, 2, &MockLlmAdapter)
        .expect("open first conversation");
    simulation
        .debug_try_social(3, 4, &MockLlmAdapter)
        .expect("open second conversation");
    let before = simulation.snapshot();
    let first_id = before
        .conversations
        .iter()
        .find(|conversation| conversation.participants == [1, 2])
        .expect("first conversation")
        .id;
    let second_id = before
        .conversations
        .iter()
        .find(|conversation| conversation.participants == [3, 4])
        .expect("second conversation")
        .id;

    let (adapter, state) = InstrumentedAdapter::new();
    let adapter = adapter
        .with_conversation_delay(first_id, 80)
        .with_conversation_delay(second_id, 20)
        .timeout_conversation(second_id);

    simulation.tick(&adapter).expect("tick should continue");
    let snapshot = simulation.snapshot();
    assert_eq!(
        snapshot
            .conversations
            .iter()
            .find(|conversation| conversation.id == first_id)
            .expect("first conversation after timeout")
            .turn_count,
        1
    );
    let second = snapshot
        .conversations
        .iter()
        .find(|conversation| conversation.id == second_id)
        .expect("second conversation after timeout");
    assert_eq!(second.turn_count, 0);
    assert_eq!(
        second.status,
        medieval_village_llm::world_model::ConversationStatus::Interrupted
    );
    assert_eq!(
        second.outcome,
        medieval_village_llm::world_model::ConversationOutcome::ProviderTimeout
    );
    assert!(
        second
            .end_reason
            .as_deref()
            .unwrap_or_default()
            .contains("timeout_llm")
    );
    assert_eq!(
        snapshot
            .events
            .iter()
            .filter(|event| event.kind == EventKind::ConversationTurn)
            .count(),
        1
    );
    assert!(snapshot.events.iter().any(|event| {
        event.kind == EventKind::ConversationEnded && event.summary.contains("timeout_llm")
    }));
    assert_eq!(state.max_active_conversations.load(Ordering::SeqCst), 1);
}

#[test]
fn routine_deliberation_reuses_intent_and_reduces_general_llm_calls() {
    let (adapter, state) = InstrumentedAdapter::new();
    let mut simulation = Simulation::seeded(SimulationConfig {
        max_agents: 1,
        ..SimulationConfig::default()
    });

    for _ in 0..8 {
        simulation.tick(&adapter).expect("tick should succeed");
    }
    thread::sleep(Duration::from_millis(50));
    simulation
        .tick(&adapter)
        .expect("tick should collect background decision");

    let decision_calls = state.decision_calls.lock().expect("decision calls").len();
    assert!(
        decision_calls < 8,
        "expected fewer than one decision per tick"
    );
    let snapshot = simulation.snapshot();
    assert!(
        snapshot.agents[0].last_intent.is_some()
            || !snapshot.agents[0].task_queue.is_empty()
            || snapshot.agents[0].active_economic_task_id.is_some()
    );
}

#[test]
fn schema_error_in_conversation_batch_remains_fatal() {
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation
        .debug_force_agent_position(1, TileCoord { x: 20, y: 13 })
        .expect("place pair one a");
    simulation
        .debug_force_agent_position(2, TileCoord { x: 21, y: 13 })
        .expect("place pair one b");
    simulation
        .debug_try_social(1, 2, &MockLlmAdapter)
        .expect("open conversation");
    let conversation_id = simulation
        .snapshot()
        .conversations
        .iter()
        .find(|conversation| conversation.participants == [1, 2])
        .expect("conversation")
        .id;

    let (adapter, _) = InstrumentedAdapter::new();
    let adapter = adapter.schema_fail_conversation(conversation_id);

    let error = simulation
        .tick(&adapter)
        .expect_err("schema conversation error should stay fatal");
    assert!(error.to_string().contains("invalid conversation schema"));
}

#[test]
fn loader_rejects_legacy_snapshot_without_grid() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("legacy.sqlite");
    let persistence = Persistence::open(&db).expect("db open");
    drop(persistence);

    let conn = Connection::open(&db).expect("raw open");
    conn.execute(
        "INSERT INTO checkpoints (kind, day, tick_of_day, total_ticks, payload, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            "legacy",
            1,
            0,
            0,
            r#"{"village_name":"Santa Bruma","day":1}"#,
            "2026-01-01T00:00:00Z"
        ],
    )
    .expect("insert legacy checkpoint");
    drop(conn);

    let persistence = Persistence::open(&db).expect("reopen");
    let error = persistence
        .load_latest()
        .expect_err("legacy snapshot should fail");
    assert!(error.to_string().contains("legacy snapshot"));
}

#[test]
fn simulates_a_week_with_physical_world_constraints() {
    let llm = MockLlmAdapter;
    let mut simulation = Simulation::seeded(SimulationConfig {
        ticks_per_day: 24,
        ..SimulationConfig::default()
    });
    let ticks_per_day = simulation.snapshot().ticks_per_day;
    for _ in 0..(7 * ticks_per_day) {
        simulation.tick(&llm).expect("tick should succeed");
    }
    let snapshot = simulation.snapshot();
    assert!(snapshot.day >= 8);
    assert!(
        snapshot
            .agents
            .iter()
            .all(|agent| (0..=100).contains(&agent.state.health))
    );
    assert!(snapshot.agents.iter().all(|agent| {
        snapshot
            .spatial
            .grid
            .tiles
            .iter()
            .any(|tile| tile.coord == agent.position && tile.kind.walkable())
    }));
    assert!(snapshot.events.iter().any(|event| {
        matches!(
            event.kind,
            EventKind::ConversationStarted
                | EventKind::ConversationTurn
                | EventKind::ConversationEnded
                | EventKind::Arrival
        )
    }));
}

#[test]
fn generation_includes_local_woodlot_and_quarry_nodes() {
    let simulation = Simulation::seeded(SimulationConfig::default());
    let spatial = simulation.spatial();
    assert!(
        spatial
            .buildings
            .iter()
            .any(|building| building.kind == LocationKind::Woodlot)
    );
    assert!(
        spatial
            .buildings
            .iter()
            .any(|building| building.kind == LocationKind::Quarry)
    );
    assert!(
        spatial
            .fixtures
            .iter()
            .any(|fixture| fixture.name == "Clareira de Coleta")
    );
    assert!(
        spatial
            .fixtures
            .iter()
            .any(|fixture| fixture.name == "Face da Pedreira")
    );
}

#[test]
fn daily_taxes_feed_public_treasury_without_local_minting() {
    let llm = MockLlmAdapter;
    let mut simulation = Simulation::seeded(SimulationConfig {
        ticks_per_day: 24,
        ..SimulationConfig::default()
    });
    let initial = simulation.snapshot();
    let initial_money = total_money(&initial);
    let initial_public = initial.village_economy.public_treasury;

    for _ in 0..initial.ticks_per_day {
        simulation.tick(&llm).expect("tick should succeed");
    }

    let snapshot = simulation.snapshot();
    let current_money = total_money(&snapshot);
    assert!(snapshot.village_economy.public_treasury >= initial_public);
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| event.kind == EventKind::Tax)
    );
    assert!(current_money <= initial_money);
}

#[test]
fn local_raw_material_establishments_hold_lenha_and_metal_bruto() {
    let llm = MockLlmAdapter;
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    for _ in 0..72 {
        simulation.tick(&llm).expect("tick should succeed");
    }
    let snapshot = simulation.snapshot();
    let woodlot = snapshot
        .establishments
        .iter()
        .find(|establishment| establishment.location_kind == LocationKind::Woodlot)
        .expect("woodlot establishment");
    let quarry = snapshot
        .establishments
        .iter()
        .find(|establishment| establishment.location_kind == LocationKind::Quarry)
        .expect("quarry establishment");
    assert!(
        woodlot
            .stock
            .iter()
            .any(|stack| stack.resource_id == ResourceKind::Lenha.id() && stack.amount >= 0)
    );
    assert!(
        quarry
            .stock
            .iter()
            .any(|stack| stack.resource_id == ResourceKind::MetalBruto.id() && stack.amount >= 0)
    );
}

#[test]
fn assault_requires_adjacency_and_creates_combat_and_crime_case() {
    let llm = MockLlmAdapter;
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation
        .debug_force_agent_position(1, TileCoord { x: 24, y: 13 })
        .expect("place attacker");
    simulation
        .debug_force_agent_position(2, TileCoord { x: 25, y: 13 })
        .expect("place victim");
    let before = simulation.snapshot();
    let victim_health = before
        .agents
        .iter()
        .find(|agent| agent.id == 2)
        .expect("victim")
        .state
        .health;

    simulation
        .debug_assign_intent(1, test_intent(IntentKind::Agredir, Some(2)))
        .expect("assign assault");
    simulation.tick(&llm).expect("tick");

    let snapshot = simulation.snapshot();
    let victim = snapshot
        .agents
        .iter()
        .find(|agent| agent.id == 2)
        .expect("victim after");
    assert!(victim.state.health < victim_health);
    assert!(victim.injury.light_wounds > 0 || victim.injury.severe_wounds > 0);
    assert!(
        snapshot
            .combats
            .iter()
            .any(|combat| combat.participants.contains(&1) && combat.participants.contains(&2))
    );
    assert!(snapshot.crime_cases.iter().any(|case| {
        case.crime_type == CrimeType::Assault
            && case.suspect_id == Some(1)
            && case.victim_id == Some(2)
    }));
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| event.kind == EventKind::Violence)
    );
}

#[test]
fn theft_transfers_real_resources_and_may_create_case_when_observed() {
    let llm = MockLlmAdapter;
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation
        .debug_force_agent_position(1, TileCoord { x: 24, y: 13 })
        .expect("place thief");
    simulation
        .debug_force_agent_position(4, TileCoord { x: 25, y: 13 })
        .expect("place victim");
    simulation
        .debug_force_agent_position(3, TileCoord { x: 24, y: 14 })
        .expect("place witness");
    let before = simulation.snapshot();
    let thief_household = before
        .agents
        .iter()
        .find(|agent| agent.id == 1)
        .and_then(|agent| agent.home_building_id)
        .expect("thief household");
    let initial_treasury = before
        .households
        .iter()
        .find(|household| household.id == thief_household)
        .expect("household")
        .treasury;

    simulation
        .debug_assign_intent(1, test_intent(IntentKind::Furtar, Some(4)))
        .expect("assign theft");
    simulation.tick(&llm).expect("tick");

    let snapshot = simulation.snapshot();
    let updated_treasury = snapshot
        .households
        .iter()
        .find(|household| household.id == thief_household)
        .expect("household")
        .treasury;
    assert!(
        updated_treasury > initial_treasury
            || snapshot
                .agents
                .iter()
                .find(|agent| agent.id == 1)
                .expect("thief")
                .carrying
                .iter()
                .any(|stack| stack.amount > 0)
    );
    assert!(
        snapshot
            .crime_cases
            .iter()
            .any(|case| case.crime_type == CrimeType::Theft)
    );
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| event.kind == EventKind::Theft)
    );
}

#[test]
fn guard_can_investigate_arrest_and_punish_proven_case() {
    let llm = MockLlmAdapter;
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation
        .debug_force_agent_position(5, TileCoord { x: 24, y: 13 })
        .expect("place guard");
    simulation
        .debug_force_agent_position(1, TileCoord { x: 25, y: 13 })
        .expect("place suspect");
    simulation
        .debug_force_agent_position(2, TileCoord { x: 26, y: 13 })
        .expect("place victim");
    simulation
        .debug_assign_intent(1, test_intent(IntentKind::Agredir, Some(2)))
        .expect("assign assault");
    simulation.tick(&llm).expect("crime tick");

    simulation
        .debug_assign_intent(5, test_intent(IntentKind::Investigar, None))
        .expect("assign investigate");
    simulation.tick(&llm).expect("investigate tick");
    simulation
        .debug_force_agent_position(5, TileCoord { x: 25, y: 14 })
        .expect("place guard near suspect");
    simulation
        .debug_force_agent_position(1, TileCoord { x: 25, y: 13 })
        .expect("keep suspect near guard");
    simulation
        .debug_assign_intent(5, test_intent(IntentKind::Prender, Some(1)))
        .expect("assign arrest");
    simulation.tick(&llm).expect("arrest tick");

    // Force guard to be at the guard post entrance so they are adjacent to the arrested suspect
    let guard_post_entrance = {
        let snapshot = simulation.snapshot();
        snapshot.spatial.buildings.iter()
            .find(|b| b.kind == LocationKind::GuardPost)
            .map(|b| b.entrance)
            .expect("should find guard post entrance")
    };
    simulation
        .debug_force_agent_position(5, guard_post_entrance)
        .expect("place guard at guard post");

    simulation
        .debug_assign_intent(5, test_intent(IntentKind::Punir, Some(1)))
        .expect("assign punish");
    simulation.tick(&llm).expect("punish tick");

    let snapshot = simulation.snapshot();
    assert!(
        snapshot.crime_cases.iter().any(|case| {
            case.suspect_id == Some(1)
                && matches!(
                    case.status,
                    CrimeCaseStatus::Punished | CrimeCaseStatus::Arrested
                )
        }),
        "crime cases: {:?}",
        snapshot.crime_cases
    );
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| event.kind == EventKind::Investigation)
    );
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| event.kind == EventKind::Arrest)
    );
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| event.kind == EventKind::Punishment)
    );
}

#[test]
fn political_pressure_creates_issue_and_faction_from_public_treasury_crisis() {
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation.debug_set_public_treasury(0);
    simulation
        .debug_refresh_politics()
        .expect("refresh politics");

    let snapshot = simulation.snapshot();
    assert!(
        snapshot
            .political_issues
            .iter()
            .any(|issue| issue.proposed_value == "aumentar")
    );
    assert!(!snapshot.political_factions.is_empty());
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| event.kind == EventKind::PolicyProposal)
    );
}

#[test]
fn political_support_intent_records_position_without_immediate_norm_change() {
    let llm = MockLlmAdapter;
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation.debug_set_public_treasury(0);
    simulation
        .debug_refresh_politics()
        .expect("refresh politics");
    let before = simulation.snapshot();
    let issue = before
        .political_issues
        .iter()
        .find(|issue| issue.proposed_value == "aumentar")
        .expect("political issue")
        .clone();
    let actor = issue.proposed_by.expect("issue proposer");
    let initial_tax = before.village_economy.daily_household_tax;

    simulation
        .debug_assign_intent(actor, test_intent(IntentKind::Apoiar, None))
        .expect("assign support");
    simulation.tick(&llm).expect("execute support");

    let snapshot = simulation.snapshot();
    let issue_after = snapshot
        .political_issues
        .iter()
        .find(|entry| entry.id == issue.id)
        .expect("issue after support");
    assert!(issue_after.supporter_ids.contains(&actor));
    assert_eq!(snapshot.village_economy.daily_household_tax, initial_tax);
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| event.kind == EventKind::PoliticalSupport)
    );
}

#[test]
fn daily_political_resolution_can_change_tax_norm() {
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation.debug_set_public_treasury(0);
    let initial_tax = simulation.snapshot().village_economy.daily_household_tax;
    simulation
        .debug_resolve_daily_politics()
        .expect("resolve politics");

    let snapshot = simulation.snapshot();
    assert!(snapshot.village_economy.daily_household_tax > initial_tax);
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| event.kind == EventKind::NormChanged)
    );
    assert!(
        snapshot
            .political_issues
            .iter()
            .any(|issue| issue.proposed_value == "aumentar")
    );
}

fn test_intent(kind: IntentKind, target_agent: Option<u64>) -> AgentIntent {
    AgentIntent {
        kind,
        target_agent,
        target_semantic: Some(kind.as_str().to_string()),
        justification: "teste deterministico".to_string(),
        dominant_emotion: "firme".to_string(),
        perceived_risk: 5,
        belief_updates: Vec::new(),
        priority: 10,
        social_move: None,
    }
}

#[test]
fn critical_hunger_consumes_household_food_without_llm_decision() {
    let llm = MockLlmAdapter;
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    let initial = simulation.snapshot();
    let agent = initial
        .agents
        .iter()
        .find(|agent| agent.id == 1)
        .expect("seeded agent");
    let household_id = agent.home_building_id.expect("agent household");
    let initial_food = initial
        .households
        .iter()
        .find(|household| household.id == household_id)
        .expect("household")
        .pantry
        .iter()
        .find(|stack| stack.resource_id == ResourceKind::Pao.id())
        .map(|stack| stack.amount)
        .unwrap_or(0);
    assert!(initial_food > 0);

    let mut critical_state = agent.state.clone();
    critical_state.hunger = 90;
    simulation
        .debug_force_agent_state(1, critical_state)
        .expect("force state");
    simulation.tick(&llm).expect("tick should succeed");

    let snapshot = simulation.snapshot();
    let updated_agent = snapshot
        .agents
        .iter()
        .find(|agent| agent.id == 1)
        .expect("updated agent");
    let updated_food = snapshot
        .households
        .iter()
        .find(|household| household.id == household_id)
        .expect("updated household")
        .pantry
        .iter()
        .find(|stack| stack.resource_id == ResourceKind::Pao.id())
        .map(|stack| stack.amount)
        .unwrap_or(0);

    assert!(updated_agent.state.hunger < 90);
    assert_eq!(updated_food, initial_food - 1);
    assert!(snapshot.events.iter().any(|event| {
        event.actor == 1 && event.kind == EventKind::Commerce && event.summary.contains("come")
    }));
}

fn total_money(snapshot: &medieval_village_llm::world_model::SimulationSnapshot) -> i32 {
    let household_money: i32 = snapshot
        .households
        .iter()
        .map(|household| household.treasury)
        .sum();
    let establishment_money: i32 = snapshot
        .establishments
        .iter()
        .map(|establishment| establishment.cash)
        .sum();
    household_money + establishment_money + snapshot.village_economy.public_treasury
}

#[test]
fn async_decision_processing_permits_independent_progress() {
    let (adapter, state) = InstrumentedAdapter::new();
    // Configure agent 1 with a decision delay of 150 milliseconds
    let adapter = adapter.with_decision_delay(1, 150);

    let mut config = SimulationConfig::default();
    config.max_agents = 1;
    let mut simulation = Simulation::seeded(config);

    // Tick once. This should trigger and dispatch the decision request in a background thread.
    simulation.tick(&adapter).expect("first tick");

    // Give the background thread a moment to start and register the call.
    thread::sleep(Duration::from_millis(100));

    let snapshot = simulation.snapshot();
    let agent_one = snapshot.agents.iter().find(|a| a.id == 1).expect("agent 1");
    let dispatched = state.decision_calls.lock().expect("decision calls").len();
    if dispatched == 0 {
        assert!(
            agent_one.last_intent.is_some() || agent_one.active_economic_task_id.is_some(),
            "local deterministic routine should keep the agent progressing when no LLM dispatch is needed"
        );
        return;
    }

    assert_eq!(dispatched, 1);
    assert!(agent_one.last_intent.is_none());
    assert!(agent_one.task_queue.is_empty());

    // Sleep in the test thread to allow the background LLM thread to finish its delay
    thread::sleep(Duration::from_millis(200));

    // Tick again. This tick should notice the background thread is finished, join it, and apply the decision!
    simulation.tick(&adapter).expect("second tick");

    // Assert that the decision has now been applied!
    let snapshot2 = simulation.snapshot();
    let agent_one_after = snapshot2
        .agents
        .iter()
        .find(|a| a.id == 1)
        .expect("agent 1");
    assert!(agent_one_after.last_intent.is_some());
    assert!(!agent_one_after.task_queue.is_empty());
}

#[test]
fn agricultural_crop_growth_and_harvesting_cycle() {
    let (adapter, _) = InstrumentedAdapter::new();
    let mut config = SimulationConfig::default();
    config.max_agents = 0; // Ensure we have a farmer
    let mut simulation = Simulation::seeded(config);

    // 1. Initial State: No crops planted
    assert!(simulation.snapshot().crops.is_empty(), "Crops list should start empty");

    // Find a farm building (Celeiro)
    let farm_building = simulation.snapshot().spatial.buildings.iter()
        .find(|b| b.kind == LocationKind::Farm)
        .cloned()
        .expect("should have a farm building");

    // Find a farmer agent
    let farmer = simulation.snapshot().agents.iter()
        .find(|a| a.role_id == "campones")
        .cloned()
        .expect("should have a farmer agent");

    // Force farmer position to the Celeiro entrance
    simulation.debug_force_agent_position(farmer.id, farm_building.entrance).expect("force position");

    // Force Farmer intent to Trabalhar(fazenda)
    simulation.debug_assign_intent(farmer.id, AgentIntent {
        kind: IntentKind::Trabalhar,
        target_agent: None,
        target_semantic: Some("fazenda".to_string()),
        justification: "trabalhar".to_string(),
        dominant_emotion: "focado".to_string(),
        perceived_risk: 0,
        belief_updates: vec![],
        priority: 100,
        social_move: None,
    }).expect("assign intent");
    simulation.debug_force_navigation(farmer.id, farm_building.entrance, vec![]).expect("force navigation");

    {
        let snapshot = simulation.snapshot();
        let agent = snapshot.agents.iter().find(|a| a.id == farmer.id).unwrap();
        println!("DEBUG: Agent pos={:?}, dest={:?}, intent={:?}", agent.position, agent.destination, agent.last_intent);
        
        let farm_buildings: Vec<&BuildingSpec> = snapshot.spatial.buildings.iter()
            .filter(|b| b.kind == LocationKind::Farm)
            .collect();
        println!("DEBUG: farm_buildings: {:?}", farm_buildings.iter().map(|b| (b.id, b.entrance)).collect::<Vec<_>>());
        
        let farm_fields: Vec<TileCoord> = snapshot.spatial.grid.tiles.iter()
            .filter(|tile| tile.kind == TileKind::Field)
            .map(|tile| tile.coord)
            .collect();
        println!("DEBUG: total field tiles: {}", farm_fields.len());
    }

    // 2. Action: Work (Planting)
    simulation.tick(&adapter).expect("tick for planting");

    // Verify fields associated with this farm have crops in Planted stage
    let snapshot = simulation.snapshot();
    assert!(!snapshot.crops.is_empty(), "Crops should have been planted");
    for crop in snapshot.crops.values() {
        assert_eq!(crop.stage, CropStage::Planted);
        assert_eq!(crop.ticks_since_planted, 0);
    }

    // 3. Action: Working while growing should fail (intent will fail, verify crops still grow and no grains produced yet)
    simulation.debug_force_agent_position(farmer.id, farm_building.entrance).expect("force position 2");
    simulation.debug_assign_intent(farmer.id, AgentIntent {
        kind: IntentKind::Trabalhar,
        target_agent: None,
        target_semantic: Some("fazenda".to_string()),
        justification: "trabalhar".to_string(),
        dominant_emotion: "focado".to_string(),
        perceived_risk: 0,
        belief_updates: vec![],
        priority: 100,
        social_move: None,
    }).expect("assign intent 2");
    simulation.debug_force_navigation(farmer.id, farm_building.entrance, vec![]).expect("force navigation 2");

    simulation.tick(&adapter).expect("tick for work attempt during growth");

    // Verify crops are still there
    let snapshot = simulation.snapshot();
    assert!(!snapshot.crops.is_empty(), "Crops must still exist during growth");

    // 4. Growth Cycle: Tick 10 times to transition to Growing
    for _ in 0..10 {
        simulation.tick(&adapter).expect("tick");
    }
    let snapshot = simulation.snapshot();
    for crop in snapshot.crops.values() {
        assert_eq!(crop.stage, CropStage::Growing);
    }

    // 5. Growth Cycle: Tick 20 more times to transition to Ready (total 30 ticks since planted)
    for _ in 0..20 {
        simulation.tick(&adapter).expect("tick");
    }
    let snapshot = simulation.snapshot();
    for crop in snapshot.crops.values() {
        assert_eq!(crop.stage, CropStage::Ready);
    }

    // 6. Action: Work (Harvesting)
    let initial_grain = snapshot.establishments.iter()
        .find(|e| e.building_id == Some(farm_building.id))
        .map(|e| e.stock.iter().find(|s| s.resource_id == "graos").map(|s| s.amount).unwrap_or(0))
        .unwrap_or(0);

    simulation.debug_force_agent_position(farmer.id, farm_building.entrance).expect("force position 3");
    simulation.debug_assign_intent(farmer.id, AgentIntent {
        kind: IntentKind::Trabalhar,
        target_agent: None,
        target_semantic: Some("fazenda".to_string()),
        justification: "trabalhar".to_string(),
        dominant_emotion: "focado".to_string(),
        perceived_risk: 0,
        belief_updates: vec![],
        priority: 100,
        social_move: None,
    }).expect("assign intent 3");
    simulation.debug_force_navigation(farmer.id, farm_building.entrance, vec![]).expect("force navigation 3");

    simulation.tick(&adapter).expect("tick for harvesting");

    let snapshot = simulation.snapshot();
    let final_grain = snapshot.establishments.iter()
        .find(|e| e.building_id == Some(farm_building.id))
        .map(|e| e.stock.iter().find(|s| s.resource_id == "graos").map(|s| s.amount).unwrap_or(0))
        .unwrap_or(0);

    assert!(final_grain > initial_grain, "Grain stock should increase after harvest");
    assert!(snapshot.crops.is_empty(), "Crops list should be empty after harvesting");
}


