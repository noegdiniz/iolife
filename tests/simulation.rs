use medieval_village_llm::agent_mind::{
    ConversationTurnInput, ConversationTurnOutput, DecisionEnvelope, DecisionInput,
    parse_conversation_turn_json, parse_decision_json, retrieve_relevant_memories,
};
use medieval_village_llm::llm_adapter::{LlmAdapter, LlmError, LlmResult, MockLlmAdapter};
use medieval_village_llm::persistence::Persistence;
use medieval_village_llm::sim_core::{Simulation, SimulationConfig};
use medieval_village_llm::world_model::{
    AgentIntent, AgentMemory, AgentRelation, AgentState, EventKind, FixtureKind, IntentKind,
    LocationKind, MemoryKind, RelationDelta, ResourceKind, SocialMove, TileCoord, WorldEvent,
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

    fn with_decision_output(mut self, agent_id: u64, envelope: DecisionEnvelope) -> Self {
        self.decision_outputs.insert(agent_id, envelope);
        self
    }

    fn timeout_decision(mut self, agent_id: u64) -> Self {
        self.fail_decision.insert(
            agent_id,
            LlmError::Timeout {
                operation: "decision".to_string(),
                attempts: 2,
                message: "operation timed out".to_string(),
            },
        );
        self
    }

    fn schema_fail_decision(mut self, agent_id: u64) -> Self {
        self.fail_decision.insert(
            agent_id,
            LlmError::Schema {
                operation: "decision".to_string(),
                message: "invalid decision schema".to_string(),
            },
        );
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
            intent: AgentIntent {
                kind: IntentKind::Andar,
                target_agent: None,
                target_semantic: Some("praca".to_string()),
                justification: "manter fluxo neutro".to_string(),
                dominant_emotion: "contido".to_string(),
                perceived_risk: 5,
                belief_updates: vec!["continuar observando".to_string()],
                priority: 1,
                social_move: None,
            },
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
    fn provider_name(&self) -> &str {
        "instrumented"
    }

    fn evaluate_and_decide(&self, input: &DecisionInput) -> LlmResult<DecisionEnvelope> {
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
        let active = self.state.active_decisions.fetch_add(1, Ordering::SeqCst) + 1;
        self.state
            .max_active_decisions
            .fetch_max(active, Ordering::SeqCst);
        if let Some(delay_ms) = self.decision_delays_ms.get(&input.actor_id) {
            thread::sleep(Duration::from_millis(*delay_ms));
        }
        self.state.active_decisions.fetch_sub(1, Ordering::SeqCst);
        if let Some(error) = self.fail_decision.get(&input.actor_id) {
            return Err(error.clone());
        }
        Ok(self
            .decision_outputs
            .get(&input.actor_id)
            .cloned()
            .unwrap_or_else(|| Self::default_envelope(input)))
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
fn parses_llm_decision_json() {
    let payload = r#"{
        "reflection": "Alda mede o acesso a comida.",
        "intent": {
            "kind": "Comer",
            "target_agent": null,
            "target_semantic": "comida",
            "justification": "Preciso encontrar alimento agora.",
            "dominant_emotion": "apreensao",
            "perceived_risk": 10,
            "belief_updates": ["Comer agora evita fraqueza."],
            "priority": 4,
            "social_move": null
        }
    }"#;
    let parsed = parse_decision_json(payload).expect("decision should parse");
    assert_eq!(parsed.intent.kind, IntentKind::Comer);
    assert_eq!(parsed.intent.target_semantic.as_deref(), Some("comida"));
}

#[test]
fn normalizes_textual_scalar_fields_in_llm_decision_json() {
    let payload = r#"{
        "reflection": "Breno precisa se recompor.",
        "intent": {
            "kind": "Descansar",
            "target_agent": "",
            "target_semantic": "Cama 2",
            "justification": "Recuperar energia agora evita erro depois.",
            "dominant_emotion": "cansaco",
            "perceived_risk": "baixo",
            "belief_updates": "descanso e necessario",
            "priority": "média",
            "social_move": "descansar sozinho na cama"
        }
    }"#;

    let parsed = parse_decision_json(payload).expect("decision should normalize");
    assert_eq!(parsed.intent.kind, IntentKind::Descansar);
    assert_eq!(parsed.intent.target_agent, None);
    assert_eq!(parsed.intent.perceived_risk, 20);
    assert_eq!(parsed.intent.priority, 5);
    assert_eq!(
        parsed.intent.belief_updates,
        vec!["descanso e necessario".to_string()]
    );
    assert_eq!(parsed.intent.social_move, None);
}

#[test]
fn normalizes_social_decision_with_numeric_target_agent_and_mapped_move() {
    let payload = r#"{
        "reflection": "Dario quer se aproximar.",
        "intent": {
            "kind": "Socializar",
            "target_agent": "6",
            "target_semantic": "Faro, o Lider Local",
            "justification": "Aproximar-se pode fortalecer a posicao social.",
            "dominant_emotion": "ansiedade",
            "perceived_risk": "medio",
            "belief_updates": ["Faro pode ser aliado."],
            "priority": "alta",
            "social_move": "aproximar"
        }
    }"#;

    let parsed = parse_decision_json(payload).expect("social decision should normalize");
    assert_eq!(parsed.intent.kind, IntentKind::Socializar);
    assert_eq!(parsed.intent.target_agent, Some(6));
    assert_eq!(parsed.intent.perceived_risk, 50);
    assert_eq!(parsed.intent.priority, 8);
    assert_eq!(parsed.intent.social_move, Some(SocialMove::Favor));
}

#[test]
fn social_decision_name_target_falls_back_and_invalid_move_defaults() {
    let payload = r#"{
        "reflection": "Alguem quer conversar.",
        "intent": {
            "kind": "Socializar",
            "target_agent": "Dario",
            "target_semantic": "conversar com Dario",
            "justification": "Vale medir o humor do outro.",
            "dominant_emotion": "esperanca",
            "perceived_risk": 10,
            "belief_updates": ["Dario parece util."],
            "priority": 4,
            "social_move": "amigável mas estranho"
        }
    }"#;

    let parsed = parse_decision_json(payload).expect("social decision should fallback");
    assert_eq!(parsed.intent.target_agent, None);
    assert_eq!(parsed.intent.social_move, Some(SocialMove::Chat));
}

#[test]
fn invalid_kind_still_fails_llm_decision_parse() {
    let payload = r#"{
        "reflection": "Algo estranho ocorre.",
        "intent": {
            "kind": "Dormir",
            "target_agent": null,
            "target_semantic": "cama",
            "justification": "Quero dormir.",
            "dominant_emotion": "cansaco",
            "perceived_risk": 10,
            "belief_updates": ["Dormir resolve."],
            "priority": 4,
            "social_move": null
        }
    }"#;

    let error = parse_decision_json(payload).expect_err("invalid kind should fail");
    assert!(error.to_string().contains("kind"));
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
    assert_eq!(parsed.utterance, "Boa tarde, Kelda. Estava aqui a pensar na lida do campo. Como vai a taverna?");
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
    assert_eq!(snapshot.schema_version, 5);
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
fn parallelizes_general_llm_decisions_and_applies_in_stable_agent_order() {
    let (adapter, state) = InstrumentedAdapter::new();
    let adapter = adapter
        .with_decision_delay(1, 180)
        .with_decision_delay(2, 20)
        .with_decision_output(
            1,
            DecisionEnvelope {
                reflection: "Alda quer falar primeiro.".to_string(),
                intent: AgentIntent {
                    kind: IntentKind::Socializar,
                    target_agent: Some(3),
                    target_semantic: Some("conversa".to_string()),
                    justification: "testar ordem estavel".to_string(),
                    dominant_emotion: "firme".to_string(),
                    perceived_risk: 20,
                    belief_updates: vec!["abrir conversa".to_string()],
                    priority: 3,
                    social_move: Some(SocialMove::Chat),
                },
            },
        )
        .with_decision_output(
            2,
            DecisionEnvelope {
                reflection: "Breno tambem quer falar.".to_string(),
                intent: AgentIntent {
                    kind: IntentKind::Socializar,
                    target_agent: Some(3),
                    target_semantic: Some("conversa".to_string()),
                    justification: "competir pelo mesmo parceiro".to_string(),
                    dominant_emotion: "apressado".to_string(),
                    perceived_risk: 20,
                    belief_updates: vec!["disputar atencao".to_string()],
                    priority: 3,
                    social_move: Some(SocialMove::Chat),
                },
            },
        );

    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation
        .debug_force_agent_position(1, TileCoord { x: 24, y: 13 })
        .expect("place actor one");
    simulation
        .debug_force_agent_position(2, TileCoord { x: 26, y: 13 })
        .expect("place actor two");
    simulation
        .debug_force_agent_position(3, TileCoord { x: 25, y: 13 })
        .expect("place shared target");

    simulation.tick(&adapter).expect("tick should succeed");
    let snapshot = simulation.snapshot();
    let conversation = snapshot
        .conversations
        .iter()
        .find(|conversation| {
            conversation.participants.contains(&1) && conversation.participants.contains(&3)
        })
        .expect("agent 1 should win stable apply order");
    assert_eq!(conversation.initiator_id, 1);
    assert!(
        snapshot
            .conversations
            .iter()
            .all(|conversation| !(conversation.participants.contains(&2)
                && conversation.participants.contains(&3)))
    );
    assert!(state.max_active_decisions.load(Ordering::SeqCst) > 1);
    assert!(state.decision_calls.lock().expect("decision calls").len() >= 3);
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
    assert!(state.max_active_conversations.load(Ordering::SeqCst) > 1);
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
    assert_eq!(second.status, medieval_village_llm::world_model::ConversationStatus::Interrupted);
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
        event.kind == EventKind::ConversationEnded
            && event.summary.contains("timeout_llm")
    }));
    assert!(state.max_active_conversations.load(Ordering::SeqCst) > 1);
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

    let decision_calls = state.decision_calls.lock().expect("decision calls").len();
    assert!(
        decision_calls < 8,
        "expected fewer than one decision per tick"
    );
    let snapshot = simulation.snapshot();
    assert!(snapshot.agents[0].llm_calls >= 1);
}

#[test]
fn social_and_psychological_contexts_are_expanded_in_llm_payloads() {
    let (adapter, state) = InstrumentedAdapter::new();
    let adapter = adapter
        .with_decision_output(
            1,
            DecisionEnvelope {
                reflection: "Alda quer cobrar a promessa antiga.".to_string(),
                intent: AgentIntent {
                    kind: IntentKind::Socializar,
                    target_agent: Some(2),
                    target_semantic: Some("conversa tensa".to_string()),
                    justification: "a promessa e a ofensa ainda pesam".to_string(),
                    dominant_emotion: "tensa".to_string(),
                    perceived_risk: 40,
                    belief_updates: vec!["resolver o passado".to_string()],
                    priority: 4,
                    social_move: Some(SocialMove::Promise),
                },
            },
        )
        .with_decision_output(
            2,
            DecisionEnvelope {
                reflection: "Breno aceita falar.".to_string(),
                intent: AgentIntent {
                    kind: IntentKind::Socializar,
                    target_agent: Some(1),
                    target_semantic: Some("responder a Alda".to_string()),
                    justification: "a tensao pede resposta".to_string(),
                    dominant_emotion: "desconfiado".to_string(),
                    perceived_risk: 35,
                    belief_updates: vec!["medir o dano".to_string()],
                    priority: 4,
                    social_move: Some(SocialMove::Chat),
                },
            },
        );
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation
        .debug_set_relation(
            1,
            2,
            AgentRelation {
                trust: -10,
                friendship: -5,
                resentment: 35,
                attraction: 0,
                moral_debt: 0,
                reputation: -4,
                last_updated_day: 1,
                notes: vec!["velha disputa".to_string()],
            },
        )
        .expect("set relation 1->2");
    simulation
        .debug_set_relation(
            2,
            1,
            AgentRelation {
                trust: -8,
                friendship: -4,
                resentment: 32,
                attraction: 0,
                moral_debt: 0,
                reputation: -3,
                last_updated_day: 1,
                notes: vec!["velha disputa".to_string()],
            },
        )
        .expect("set relation 2->1");
    simulation
        .debug_add_memory(
            1,
            MemoryKind::Promise,
            "Alda prometeu pagar Breno pela ferramenta.".to_string(),
            vec!["social".to_string(), "promessa".to_string()],
            20,
            vec![2],
        )
        .expect("promise memory");
    simulation
        .debug_add_memory(
            1,
            MemoryKind::Offense,
            "Alda ainda lembra da ofensa de Breno na praca.".to_string(),
            vec![
                "social".to_string(),
                "ofensa".to_string(),
                "ressentimento".to_string(),
            ],
            24,
            vec![2],
        )
        .expect("offense memory");
    simulation
        .debug_force_agent_position(1, TileCoord { x: 24, y: 13 })
        .expect("place agent one");
    simulation
        .debug_force_agent_position(2, TileCoord { x: 25, y: 13 })
        .expect("place agent two");

    simulation.tick(&adapter).expect("tick should succeed");
    simulation
        .tick(&adapter)
        .expect("conversation turn should run");
    let decision_inputs = state.decision_inputs.lock().expect("decision inputs");
    let decision_input = decision_inputs
        .iter()
        .find(|input| input.actor_id == 1)
        .expect("agent 1 decision input");
    assert!(!decision_input.psychological_context.core_values.is_empty());
    assert!(
        !decision_input
            .psychological_context
            .inner_conflicts
            .is_empty()
    );
    assert!(
        !decision_input
            .psychological_context
            .recent_self_narrative
            .is_empty()
    );
    drop(decision_inputs);

    let conversation_inputs = state
        .conversation_inputs
        .lock()
        .expect("conversation inputs");
    let conversation_input = conversation_inputs
        .iter()
        .find(|input| input.speaker_id == 1 || input.speaker_id == 2)
        .expect("conversation payload");
    assert!(
        !conversation_input
            .relational_context
            .open_promises
            .is_empty()
    );
    assert!(
        !conversation_input
            .relational_context
            .unresolved_offenses
            .is_empty()
    );
    assert!(
        !conversation_input
            .speaker_psychology
            .inner_conflicts
            .is_empty()
    );
    assert_eq!(conversation_input.turn_trigger, "fala_social");
}

#[test]
fn decision_batch_skips_transient_timeout_without_partial_abort() {
    let (adapter, state) = InstrumentedAdapter::new();
    let adapter = adapter.timeout_decision(2);
    let mut simulation = Simulation::seeded(SimulationConfig {
        max_agents: 2,
        ..SimulationConfig::default()
    });
    simulation
        .debug_force_agent_position(1, TileCoord { x: 24, y: 13 })
        .expect("place one");
    simulation
        .debug_force_agent_position(2, TileCoord { x: 28, y: 13 })
        .expect("place two");

    simulation.tick(&adapter).expect("tick should continue");
    let snapshot = simulation.snapshot();
    let agent_one = snapshot
        .agents
        .iter()
        .find(|agent| agent.id == 1)
        .expect("agent one");
    let agent_two = snapshot
        .agents
        .iter()
        .find(|agent| agent.id == 2)
        .expect("agent two");
    assert!(agent_one.llm_calls >= 1);
    assert!(agent_one.last_intent.is_some());
    assert_eq!(agent_two.llm_calls, 0);
    assert!(agent_two.last_intent.is_none());
    assert!(snapshot.events.iter().any(|event| {
        event.kind == EventKind::CognitionFailure && event.actor == 2
    }));
    assert!(state.max_active_decisions.load(Ordering::SeqCst) >= 1);
}

#[test]
fn schema_error_in_decision_batch_remains_fatal() {
    let (adapter, _) = InstrumentedAdapter::new();
    let adapter = adapter.schema_fail_decision(2);
    let mut simulation = Simulation::seeded(SimulationConfig {
        max_agents: 2,
        ..SimulationConfig::default()
    });

    let error = simulation.tick(&adapter).expect_err("schema error should stay fatal");
    assert!(error.to_string().contains("invalid decision schema"));
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
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    for _ in 0..(7 * 24) {
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
    let unique_positions = snapshot
        .agents
        .iter()
        .map(|agent| agent.position)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(unique_positions.len(), snapshot.agents.len());
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
    assert!(spatial
        .buildings
        .iter()
        .any(|building| building.kind == LocationKind::Woodlot));
    assert!(spatial
        .buildings
        .iter()
        .any(|building| building.kind == LocationKind::Quarry));
    assert!(spatial.fixtures.iter().any(|fixture| fixture.name == "Clareira de Coleta"));
    assert!(spatial.fixtures.iter().any(|fixture| fixture.name == "Face da Pedreira"));
}

#[test]
fn daily_taxes_feed_public_treasury_without_local_minting() {
    let llm = MockLlmAdapter;
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    let initial = simulation.snapshot();
    let initial_money = total_money(&initial);
    let initial_public = initial.village_economy.public_treasury;

    for _ in 0..24 {
        simulation.tick(&llm).expect("tick should succeed");
    }

    let snapshot = simulation.snapshot();
    let current_money = total_money(&snapshot);
    assert!(snapshot.village_economy.public_treasury >= initial_public);
    assert!(snapshot.events.iter().any(|event| event.kind == EventKind::Tax));
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
        .find(|establishment| establishment.kind == LocationKind::Woodlot)
        .expect("woodlot establishment");
    let quarry = snapshot
        .establishments
        .iter()
        .find(|establishment| establishment.kind == LocationKind::Quarry)
        .expect("quarry establishment");
    assert!(woodlot
        .stock
        .iter()
        .any(|stack| stack.kind == ResourceKind::Lenha && stack.amount >= 0));
    assert!(quarry
        .stock
        .iter()
        .any(|stack| stack.kind == ResourceKind::MetalBruto && stack.amount >= 0));
}

fn total_money(snapshot: &medieval_village_llm::world_model::SimulationSnapshot) -> i32 {
    let household_money: i32 = snapshot.households.iter().map(|household| household.treasury).sum();
    let establishment_money: i32 = snapshot
        .establishments
        .iter()
        .map(|establishment| establishment.cash)
        .sum();
    household_money + establishment_money + snapshot.village_economy.public_treasury
}
