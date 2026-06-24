use medieval_village_llm::agent_mind::{
    ActionPlannerInput, ConversationTurnInput, ConversationTurnOutput, DecisionEnvelope,
    DecisionInput, ProposedPromise, ProposedRumor, ProposedStoryShare, ThinkMakerInput,
    ThinkMakerOutput, parse_action_planner_output, parse_conversation_turn_json,
    parse_think_maker_json, retrieve_relevant_memories,
};
use medieval_village_llm::economy_catalog::{default_economy_catalog, validate_catalog};
use medieval_village_llm::llm_adapter::{LlmAdapter, LlmError, LlmResult, MockLlmAdapter};
use medieval_village_llm::persistence::Persistence;
use medieval_village_llm::sim_core::{Simulation, SimulationConfig};
use medieval_village_llm::world_model::{
    AgentIntent, AgentMemory, AgentState, BuildingSpec, ConstructionStatus, CrimeCaseStatus,
    CrimeType, CropStage, EconomicNode, EconomicTaskKind, EventKind, FactionObjective,
    FeudalContractStatus, FixtureKind, InstitutionalPerception, InsurrectionStage,
    InsurrectionStatus, IntentKind, ItemAffordanceKind, LocationKind, MemoryKind, MilitaryDemand,
    MilitaryDemandStatus, PartInjuryStatus, PolicyActStatus, PolicyDomain, PolicyEffect,
    PoliticalFaction, Polity, PromiseCondition, PsychologicalState, RelationDelta, ResourceKind,
    ResourceStack, SNAPSHOT_SCHEMA_VERSION, ScheduledMeetingStatus, SimplifiedTask, SocialMove,
    TileCoord, TileKind, WarStage, WarState, WarStatus, WorldEvent,
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

    fn with_conversation_output(
        mut self,
        conversation_id: u64,
        output: ConversationTurnOutput,
    ) -> Self {
        self.conversation_outputs.insert(conversation_id, output);
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
            addressed_agent_ids: input
                .participants
                .first()
                .map(|agent| vec![agent.id])
                .unwrap_or_default(),
            economic_transfer: None,
            revealed_secret: None,
            make_promise: None,
            spread_rumor: None,
            shared_story: None,
            escrow_deposit: None,
            propose_meeting: None,
            meeting_response: None,
        }
    }

    fn rumor_turn_output(
        target_agent_id: u64,
        topic: &str,
        claim: &str,
        is_true: bool,
    ) -> ConversationTurnOutput {
        ConversationTurnOutput {
            utterance: format!("Ouvi algo importante: {claim}"),
            speech_act: "fofocar".to_string(),
            emotion: "cauteloso".to_string(),
            intent_to_continue: true,
            belief_updates: vec!["testar a reacao do ouvinte ao rumor".to_string()],
            relation_delta_hint: RelationDelta::default(),
            tone: Some("confidencial".to_string()),
            risk_shift: Some(1),
            addressed_agent_ids: Vec::new(),
            economic_transfer: None,
            revealed_secret: None,
            make_promise: None,
            spread_rumor: Some(ProposedRumor {
                target_agent_id,
                topic: topic.to_string(),
                claim: Some(claim.to_string()),
                is_true,
            }),
            shared_story: None,
            escrow_deposit: None,
            propose_meeting: None,
            meeting_response: None,
        }
    }

    fn story_turn_output(title: &str, version: &str) -> ConversationTurnOutput {
        ConversationTurnOutput {
            utterance: version.to_string(),
            speech_act: "contar_historia".to_string(),
            emotion: "solene".to_string(),
            intent_to_continue: true,
            belief_updates: vec!["usar memoria coletiva para influenciar o ouvinte".to_string()],
            relation_delta_hint: RelationDelta::default(),
            tone: Some("ritual".to_string()),
            risk_shift: Some(0),
            addressed_agent_ids: Vec::new(),
            economic_transfer: None,
            revealed_secret: None,
            make_promise: None,
            spread_rumor: None,
            shared_story: Some(ProposedStoryShare {
                story_id: None,
                title: Some(title.to_string()),
                version: version.to_string(),
                kind: Some("Heroismo".to_string()),
                tone: Some("orgulhoso".to_string()),
                moral: Some("coragem protege a vila".to_string()),
                tags: vec!["heroismo".to_string(), "cultura".to_string()],
            }),
            escrow_deposit: None,
            propose_meeting: None,
            meeting_response: None,
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
            long_term_plan: format!(
                "Consolidar minha posicao como {} sem perder estabilidade.",
                input.decision_input.role
            ),
            inner_contradiction_update: None,
            melancholic_fixation: None,
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
    assert_eq!(
        parsed[1].target_semantic.as_deref(),
        Some("posto_de_trabalho")
    );
}

#[test]
fn time_context_maps_ticks_to_day_phase() {
    let config = SimulationConfig {
        ticks_per_day: 24,
        max_agents: 10,
        ..SimulationConfig::default()
    };
    let mut sim = Simulation::seeded(config);
    let llm = MockLlmAdapter;
    for _ in 0..6 {
        sim.tick(&llm).unwrap();
    }
    let time = sim.time_context();
    assert_eq!(time.time_label, "06:00");
    assert_eq!(time.day_phase, "manha");
    assert!(time.is_daylight);
    assert!(time.is_meal_time);
}

#[test]
fn world_places_are_canonical_and_unique() {
    let sim = Simulation::seeded(SimulationConfig::default());
    let places = sim.world_place_inputs();
    let mut ids = std::collections::HashSet::new();
    for place in &places {
        assert!(
            ids.insert(place.place_id.clone()),
            "duplicate {}",
            place.place_id
        );
    }
    assert!(
        places
            .iter()
            .any(|place| place.place_id.starts_with("building:"))
    );
    assert!(
        places
            .iter()
            .any(|place| place.place_id.starts_with("room:"))
    );
    assert!(
        places
            .iter()
            .any(|place| place.place_id.starts_with("fixture:"))
    );
    assert!(
        places
            .iter()
            .any(|place| place.place_id.starts_with("territory:"))
    );
    assert!(
        places
            .iter()
            .any(|place| place.place_id == "special:inter_village_trade")
    );
}

#[test]
fn historical_bootstrap_emits_only_macro_events() {
    let snapshot = Simulation::seeded(SimulationConfig::default()).snapshot();
    assert!(
        !snapshot.events.is_empty(),
        "historical bootstrap should emit events"
    );
    assert!(snapshot.events.iter().all(|event| {
        !matches!(
            event.kind,
            EventKind::ConversationStarted
                | EventKind::ConversationTurn
                | EventKind::ConversationEnded
                | EventKind::Meeting
                | EventKind::Travel
                | EventKind::Arrival
                | EventKind::Blocking
                | EventKind::CognitionFailure
        )
    }));
    assert!(snapshot.events.iter().any(|event| {
        matches!(
            event.kind,
            EventKind::Construction
                | EventKind::Scarcity
                | EventKind::Commerce
                | EventKind::Punishment
                | EventKind::PoliticalPressure
                | EventKind::InstitutionalDispute
                | EventKind::MilitarySupply
                | EventKind::TributePaid
                | EventKind::TributeRefused
                | EventKind::LevyCalled
                | EventKind::LevyRefused
                | EventKind::FeudalSanction
                | EventKind::SuccessionOpened
                | EventKind::FactionShift
        )
    }));
}

#[test]
fn historical_bootstrap_materializes_macro_state() {
    let snapshot = Simulation::seeded(SimulationConfig::default()).snapshot();
    let summary = snapshot
        .historical_summary
        .as_ref()
        .expect("historical summary");
    assert!(snapshot.world_history_years_simulated >= 1);
    assert!(!snapshot.feudal_titles.is_empty());
    assert!(!snapshot.feudal_contracts.is_empty());
    assert!(!snapshot.policy_acts.is_empty());
    assert!(!snapshot.events.is_empty());
    assert!(summary.average_territorial_stability > 0);
    assert!(!summary.dominant_households.is_empty());
    assert!(
        !summary.dominant_stories.is_empty()
            || !summary.recent_crises.is_empty()
            || !summary.active_decrees.is_empty()
    );
}

#[test]
fn historical_bootstrap_is_deterministic_for_same_seed() {
    let config = SimulationConfig {
        history_years: 40,
        history_seed: Some(4242),
        ..SimulationConfig::default()
    };
    let a = Simulation::seeded(config.clone()).snapshot();
    let b = Simulation::seeded(config).snapshot();
    assert_eq!(a.village_name, b.village_name);
    assert_eq!(a.historical_summary, b.historical_summary);
    assert_eq!(
        a.agents
            .iter()
            .map(|agent| (&agent.name, &agent.role_id, agent.age))
            .collect::<Vec<_>>(),
        b.agents
            .iter()
            .map(|agent| (&agent.name, &agent.role_id, agent.age))
            .collect::<Vec<_>>()
    );
}

#[test]
fn historical_bootstrap_varies_with_different_seed() {
    let a = Simulation::seeded(SimulationConfig {
        history_years: 40,
        history_seed: Some(101),
        ..SimulationConfig::default()
    })
    .snapshot();
    let b = Simulation::seeded(SimulationConfig {
        history_years: 40,
        history_seed: Some(202),
        ..SimulationConfig::default()
    })
    .snapshot();
    assert_ne!(a.historical_summary, b.historical_summary);
}

#[test]
fn historical_catalog_drives_macro_construction_and_events() {
    let snapshot = Simulation::seeded(SimulationConfig {
        history_years: 80,
        history_seed: Some(77),
        ..SimulationConfig::default()
    })
    .snapshot();
    let establishment_type_ids = snapshot
        .establishments
        .iter()
        .map(|establishment| establishment.establishment_type_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert!(establishment_type_ids.contains("fazenda"));
    assert!(establishment_type_ids.contains("lenhal"));
    assert!(establishment_type_ids.contains("pedreira"));
    assert!(snapshot.events.iter().any(|event| {
        matches!(
            event.kind,
            EventKind::Construction
                | EventKind::Commerce
                | EventKind::MilitarySupply
                | EventKind::CulturalStory
        )
    }));
    assert!(snapshot.agents.iter().any(|agent| {
        agent.psychological_state.trauma > 0
            || agent.psychological_state.pride > 0
            || agent.injury.pain > 0
    }));
}

#[test]
fn parses_conversation_meeting_fields() {
    let payload = r#"{
      "utterance": "Encontre-me na taverna ao anoitecer.",
      "speech_act": "marcar_encontro",
      "emotion": "cauteloso",
      "intent_to_continue": true,
      "belief_updates": ["um encontro reservado pode render apoio"],
      "relation_delta_hint": {"trust": 1, "friendship": 0, "resentment": 0, "attraction": 0, "moral_debt": 0, "reputation": 0},
      "tone": "baixo",
      "risk_shift": 1,
      "propose_meeting": {"invitee_ids": [2], "place_id": "building:7", "scheduled_day": 1, "scheduled_time": "18:30", "purpose": "negociar apoio"},
      "meeting_response": {"meeting_id": 4, "accept": true, "reason": "convem ouvir"}
    }"#;
    let parsed = parse_conversation_turn_json(payload).unwrap();
    let meeting = parsed.propose_meeting.unwrap();
    assert_eq!(meeting.invitee_ids, vec![2]);
    assert_eq!(meeting.place_id, "building:7");
    assert_eq!(meeting.scheduled_time, "18:30");
    let response = parsed.meeting_response.unwrap();
    assert_eq!(response.meeting_id, 4);
    assert!(response.accept);
}

#[test]
fn snapshot_persists_scheduled_meetings() {
    let mut sim = Simulation::seeded(SimulationConfig::default());
    sim.scheduled_meetings
        .push(medieval_village_llm::world_model::ScheduledMeeting {
            id: 1,
            proposer_id: 1,
            invitee_ids: vec![2],
            place_id: "special:inter_village_trade".to_string(),
            scheduled_day: 1,
            scheduled_tick: 10,
            purpose: "testar persistencia".to_string(),
            status: ScheduledMeetingStatus::Accepted,
            created_tick: 0,
            responses: vec![
                medieval_village_llm::world_model::MeetingParticipantResponse {
                    agent_id: 2,
                    accept: true,
                    reason: "aceito".to_string(),
                    response_tick: 0,
                },
            ],
        });
    let snapshot = sim.snapshot();
    assert_eq!(snapshot.schema_version, SNAPSHOT_SCHEMA_VERSION);
    assert_eq!(snapshot.scheduled_meetings.len(), 1);
    let mut restored = Simulation::from_snapshot(snapshot);
    assert_eq!(restored.meetings_overview().len(), 1);
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
        "belief_updates": ["Comer agora evita fraqueza."],
        "long_term_plan": "Juntar reservas sem alarmar vizinhos."
    }"#;
    let parsed = parse_think_maker_json(payload).expect("think maker should parse");
    assert_eq!(parsed.reflection, "Alda mede o acesso a comida.");
    assert_eq!(parsed.dominant_emotion, "apreensao");
    assert_eq!(
        parsed.belief_updates,
        vec!["Comer agora evita fraqueza.".to_string()]
    );
    assert_eq!(
        parsed.long_term_plan,
        "Juntar reservas sem alarmar vizinhos."
    );
}

#[test]
fn parse_think_maker_json_requires_long_term_plan() {
    let payload = r#"{
        "reflection": "Alda mede o acesso a comida.",
        "dominant_emotion": "apreensao",
        "belief_updates": ["Comer agora evita fraqueza."]
    }"#;
    let error = parse_think_maker_json(payload).expect_err("missing plan should fail");
    assert!(error.to_string().contains("long_term_plan"));
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
        .find(|conversation| conversation.participant_ids == vec![1, 2])
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
        .find(|conversation| conversation.participant_ids == vec![1, 2])
        .expect("conversation should still exist");
    assert_eq!(conversation.turn_count, 2);
    assert_eq!(conversation.current_speaker_id, 1);
}

#[test]
fn conversation_rumor_creates_belief_and_information_context() {
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation
        .debug_force_agent_position(1, TileCoord { x: 24, y: 13 })
        .expect("place speaker");
    simulation
        .debug_force_agent_position(2, TileCoord { x: 25, y: 13 })
        .expect("place listener");
    simulation
        .debug_try_social(1, 2, &MockLlmAdapter)
        .expect("open conversation");
    let conversation_id = simulation.snapshot().conversations[0].id;
    let corruption_before = simulation
        .snapshot()
        .agents
        .iter()
        .find(|agent| agent.id == 2)
        .expect("listener before rumor")
        .institutional_perception
        .perceived_corruption;
    let output = InstrumentedAdapter::rumor_turn_output(
        3,
        "corrupcao",
        "o lider desviou graos do celeiro",
        true,
    );
    let (adapter, state) = InstrumentedAdapter::new();
    let adapter = adapter.with_conversation_output(conversation_id, output);

    simulation.tick(&adapter).expect("spread rumor tick");
    let snapshot = simulation.snapshot();
    let rumor = snapshot.rumors.first().expect("rumor created");
    assert_eq!(rumor.source_agent_id, 1);
    assert_eq!(rumor.target_agent_id, 3);
    assert!(rumor.claim.contains("desviou"));
    assert!(rumor.known_by.contains(&1));
    assert!(rumor.known_by.contains(&2));
    assert!(rumor.truth_score >= 80);
    assert!(rumor.distortion <= 20);
    let listener = snapshot
        .agents
        .iter()
        .find(|agent| agent.id == 2)
        .expect("listener");
    assert!(
        listener
            .rumor_beliefs
            .iter()
            .any(|belief| belief.rumor_id == rumor.id && belief.belief >= 50)
    );
    assert!(
        listener.institutional_perception.perceived_corruption > corruption_before,
        "corruption should increase from {} to {}",
        corruption_before,
        listener.institutional_perception.perceived_corruption
    );
    let inputs = state.conversation_inputs.lock().unwrap();
    assert!(inputs.iter().any(|input| {
        input
            .information_context
            .credibility_notes
            .iter()
            .any(|note| note.contains("ouvinte"))
    }));
}

#[test]
fn rumor_retransmission_increases_distortion_and_preserves_chain() {
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation
        .debug_force_agent_position(1, TileCoord { x: 24, y: 13 })
        .expect("place speaker");
    simulation
        .debug_force_agent_position(2, TileCoord { x: 25, y: 13 })
        .expect("place listener");
    simulation
        .debug_try_social(1, 2, &MockLlmAdapter)
        .expect("open first conversation");
    let first_id = simulation.snapshot().conversations[0].id;
    let mut first_output = InstrumentedAdapter::rumor_turn_output(
        3,
        "corrupcao",
        "o lider desviou graos do celeiro",
        true,
    );
    first_output.intent_to_continue = false;
    let (adapter, _) = InstrumentedAdapter::new();
    let adapter = adapter.with_conversation_output(first_id, first_output);
    simulation.tick(&adapter).expect("first spread");
    let mut resumed = simulation.snapshot();
    let initial_rumor = resumed.rumors[0].clone();
    resumed.conversations.clear();
    for agent in &mut resumed.agents {
        agent.active_conversation_id = None;
        agent.conversation_participant_ids.clear();
        agent.social_cooldown_until = 0;
    }
    let mut simulation = Simulation::from_snapshot(resumed);

    simulation
        .debug_force_agent_position(2, TileCoord { x: 27, y: 13 })
        .expect("move carrier");
    simulation
        .debug_force_agent_position(4, TileCoord { x: 28, y: 13 })
        .expect("place second listener");
    simulation
        .debug_try_social(2, 4, &MockLlmAdapter)
        .expect("open second conversation");
    let second_id = simulation
        .snapshot()
        .conversations
        .iter()
        .find(|conversation| conversation.participant_ids == vec![2, 4])
        .expect("second conversation")
        .id;
    let (adapter, _) = InstrumentedAdapter::new();
    let adapter = adapter.with_conversation_output(
        second_id,
        InstrumentedAdapter::rumor_turn_output(
            3,
            "corrupcao",
            "o lider desviou graos do celeiro",
            true,
        ),
    );
    simulation.tick(&adapter).expect("second spread");
    let snapshot = simulation.snapshot();
    let rumor = snapshot
        .rumors
        .iter()
        .find(|rumor| rumor.id == initial_rumor.id)
        .expect("same rumor");
    assert!(rumor.spread_count >= 2);
    assert!(rumor.distortion > initial_rumor.distortion);
    assert!(rumor.known_by.contains(&4));
    assert!(rumor.current_carrier_ids.contains(&2));
}

#[test]
fn conversation_shared_story_creates_cultural_story_and_belief() {
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation
        .debug_force_agent_position(1, TileCoord { x: 24, y: 13 })
        .expect("place speaker");
    simulation
        .debug_force_agent_position(2, TileCoord { x: 25, y: 13 })
        .expect("place listener");
    simulation
        .debug_try_social(1, 2, &MockLlmAdapter)
        .expect("open conversation");
    let conversation_id = simulation.snapshot().conversations[0].id;
    let (adapter, state) = InstrumentedAdapter::new();
    let adapter = adapter.with_conversation_output(
        conversation_id,
        InstrumentedAdapter::story_turn_output(
            "A ponte dos corajosos",
            "Na ponte velha, um aldeao enfrentou o medo para salvar a vila.",
        ),
    );

    simulation.tick(&adapter).expect("share story tick");
    let snapshot = simulation.snapshot();
    let story = snapshot
        .cultural_stories
        .iter()
        .find(|story| story.title == "A ponte dos corajosos")
        .expect("cultural story created");
    assert_eq!(story.title, "A ponte dos corajosos");
    assert_eq!(
        story.origin_kind,
        medieval_village_llm::world_model::CulturalStoryKind::Heroismo
    );
    assert!(story.cultural_strength >= 20);
    assert!(
        snapshot
            .story_versions
            .iter()
            .any(|version| version.story_id == story.id)
    );
    let listener = snapshot
        .agents
        .iter()
        .find(|agent| agent.id == 2)
        .expect("listener");
    assert!(
        listener
            .story_beliefs
            .iter()
            .any(|belief| belief.story_id == story.id && belief.belief >= 20)
    );
    let inputs = state.conversation_inputs.lock().unwrap();
    assert!(inputs.iter().any(|input| {
        input
            .cultural_context
            .known_stories
            .iter()
            .any(|story| story.contains("fundacao") || story.contains("Fundacao"))
            || !input.cultural_context.known_stories.is_empty()
    }));
}

#[test]
fn cultural_story_persists_through_save_load() {
    let db_dir = tempdir().expect("tempdir");
    let db = db_dir.path().join("culture.sqlite");
    let mut persistence = Persistence::open(&db).expect("db open");
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation
        .debug_force_agent_position(1, TileCoord { x: 24, y: 13 })
        .expect("place speaker");
    simulation
        .debug_force_agent_position(2, TileCoord { x: 25, y: 13 })
        .expect("place listener");
    simulation
        .debug_try_social(1, 2, &MockLlmAdapter)
        .expect("open conversation");
    let conversation_id = simulation.snapshot().conversations[0].id;
    let (adapter, _) = InstrumentedAdapter::new();
    let adapter = adapter.with_conversation_output(
        conversation_id,
        InstrumentedAdapter::story_turn_output(
            "A historia do forno",
            "O primeiro forno alimentou os famintos quando a vila duvidava.",
        ),
    );
    simulation.tick(&adapter).expect("share story");
    persistence.save(&mut simulation, "culture").expect("save");

    let snapshot = Persistence::open(&db)
        .expect("reopen")
        .load_latest()
        .expect("load")
        .expect("snapshot");
    assert!(
        snapshot
            .cultural_stories
            .iter()
            .any(|story| story.title == "A historia do forno")
    );
    assert!(
        snapshot
            .agents
            .iter()
            .any(|agent| !agent.story_beliefs.is_empty())
    );
}

#[test]
fn catalog_declares_affordances_multiple_recipes_and_construction() {
    let catalog = default_economy_catalog();
    validate_catalog(&catalog).expect("catalog should remain valid");

    let madeira = catalog
        .resources
        .iter()
        .find(|resource| resource.id == ResourceKind::Madeira.id())
        .expect("madeira should exist");
    assert!(
        madeira
            .affordances
            .iter()
            .any(|affordance| affordance.kind == ItemAffordanceKind::ConstructionMaterial)
    );

    let pedra = catalog
        .resources
        .iter()
        .find(|resource| resource.id == ResourceKind::Pedra.id())
        .expect("pedra should exist");
    assert!(
        pedra
            .affordances
            .iter()
            .any(|affordance| affordance.kind == ItemAffordanceKind::ConstructionMaterial)
    );

    let lenhal = catalog
        .establishment_types
        .iter()
        .find(|entry| entry.id == "lenhal")
        .expect("lenhal should exist");
    assert!(
        lenhal
            .production_recipe_ids
            .contains(&"coleta_lenha".to_string())
    );
    assert!(
        lenhal
            .production_recipe_ids
            .contains(&"corte_madeira".to_string())
    );
    assert!(lenhal.construction_recipe_id.is_some());

    let pedreira = catalog
        .establishment_types
        .iter()
        .find(|entry| entry.id == "pedreira")
        .expect("pedreira should exist");
    assert!(
        pedreira
            .production_recipe_ids
            .contains(&"extracao_metal".to_string())
    );
    assert!(
        pedreira
            .production_recipe_ids
            .contains(&"extracao_pedra".to_string())
    );
    assert!(pedreira.construction_recipe_id.is_some());
}

#[test]
fn construction_project_is_created_from_housing_pressure() {
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation.debug_remove_all_beds();
    simulation
        .tick(&MockLlmAdapter)
        .expect("tick should open systemic construction projects");
    let snapshot = simulation.snapshot();

    let housing_project = snapshot
        .construction_projects
        .iter()
        .find(|project| project.establishment_type_id == "casa")
        .expect("housing pressure should create a house project");
    assert!(matches!(
        housing_project.status,
        ConstructionStatus::Planned | ConstructionStatus::GatheringMaterials
    ));
    assert!(!housing_project.planned_footprint.is_empty());
    assert!(
        housing_project
            .materials_required
            .iter()
            .any(|stack| stack.resource_id == ResourceKind::Madeira.id())
    );
    assert!(
        housing_project
            .materials_required
            .iter()
            .any(|stack| stack.resource_id == ResourceKind::Pedra.id())
    );

    let overview = simulation.economy_overview();
    assert!(overview.iter().any(|line| line.contains("obra #")));
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
    assert_eq!(snapshot.schema_version, SNAPSHOT_SCHEMA_VERSION);
    assert!(!snapshot.spatial.buildings.is_empty());
    assert!(!snapshot.spatial.fixtures.is_empty());
    assert!(snapshot.agents.iter().all(|agent| agent.position.x >= 0));
    let conversation = snapshot
        .conversations
        .iter()
        .find(|conversation| conversation.participant_ids == vec![1, 2])
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
        .find(|conversation| conversation.participant_ids == vec![1, 2])
        .expect("first conversation")
        .id;
    let second_id = before
        .conversations
        .iter()
        .find(|conversation| conversation.participant_ids == vec![3, 4])
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
        .find(|conversation| conversation.participant_ids == vec![1, 2])
        .expect("first conversation")
        .id;
    let second_id = before
        .conversations
        .iter()
        .find(|conversation| conversation.participant_ids == vec![3, 4])
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
        .find(|conversation| conversation.participant_ids == vec![1, 2])
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
        snapshot
            .spatial
            .buildings
            .iter()
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
fn institutional_perception_persists_and_is_visible_in_agent_view() {
    let mut snapshot = Simulation::seeded(SimulationConfig::default()).snapshot();
    let agent_id = snapshot.agents[0].id;
    snapshot.agents[0].institutional_perception = InstitutionalPerception {
        leader_legitimacy: -40,
        justice_legitimacy: -25,
        tax_legitimacy: -35,
        rationing_legitimacy: -15,
        guard_trust: -30,
        war_support: -20,
        fear_of_authority: 55,
        perceived_corruption: 45,
        perceived_fairness: -50,
        last_updated_day: snapshot.day,
        notes: vec!["teste institucional".to_string()],
    };

    let mut simulation = Simulation::from_snapshot(snapshot);
    let restored = simulation.snapshot();
    let restored_agent = restored
        .agents
        .iter()
        .find(|agent| agent.id == agent_id)
        .expect("restored agent");
    assert_eq!(restored_agent.institutional_perception.tax_legitimacy, -35);
    let view = simulation
        .agent_views()
        .into_iter()
        .find(|view| view.id == agent_id)
        .expect("agent view");
    assert_eq!(view.institutional_perception.guard_trust, -30);
}

#[test]
fn psychological_state_persists_and_is_visible_in_agent_view() {
    let mut snapshot = Simulation::seeded(SimulationConfig::default()).snapshot();
    snapshot.agents[0].psychological_state = PsychologicalState {
        grief: 42,
        humiliation: 31,
        fear: 27,
        pride: 18,
        trauma: 36,
        anger: 22,
        hope: 11,
        guilt: 9,
        status_anxiety: 14,
        revenge_drive: 12,
        submission_drive: 10,
        dominance_drive: 8,
        last_public_humiliation_tick: 0,
        last_public_humiliation_by: None,
        active_revenge_target: None,
        long_term_plan: "Acumular margem para atravessar a proxima escassez.".to_string(),
        personal_symbols: vec![medieval_village_llm::world_model::PersonalSymbol {
            target_kind: medieval_village_llm::world_model::PersonalSymbolTargetKind::Event,
            target_id: None,
            text: "fome antiga".to_string(),
            meaning: "nao confiar na abundancia".to_string(),
            emotion: "melancolia".to_string(),
            intensity: 44,
            origin_memory_id: None,
        }],
        coping_patterns: vec![medieval_village_llm::world_model::CopingPattern {
            kind: medieval_village_llm::world_model::CopingPatternKind::RitualReturn,
            trigger: "medo de escassez".to_string(),
            behavior_hint: "voltar ao deposito antes de dormir".to_string(),
            strength: 33,
            last_triggered_tick: 0,
        }],
        inner_contradictions: vec![medieval_village_llm::world_model::InnerContradiction {
            desire: "guardar recursos".to_string(),
            fear: "parecer mesquinho".to_string(),
            compromise: "ajudar so quando houver lastro".to_string(),
            pressure: 28,
        }],
        melancholic_fixation: Some("a despensa nunca parece cheia o bastante".to_string()),
        last_updated_day: snapshot.day,
        notes: vec!["teste psicologico".to_string()],
    };

    let mut simulation = Simulation::from_snapshot(snapshot);
    let restored = simulation.snapshot();
    let restored_agent = &restored.agents[0];
    assert_eq!(restored_agent.psychological_state.grief, 42);
    assert_eq!(restored_agent.psychological_state.trauma, 36);
    assert_eq!(restored_agent.psychological_state.personal_symbols.len(), 1);
    assert_eq!(restored_agent.psychological_state.coping_patterns.len(), 1);
    assert_eq!(
        restored_agent
            .psychological_state
            .inner_contradictions
            .len(),
        1
    );
    assert_eq!(
        restored_agent
            .psychological_state
            .melancholic_fixation
            .as_deref(),
        Some("a despensa nunca parece cheia o bastante")
    );
    assert_eq!(
        restored_agent.psychological_state.long_term_plan,
        "Acumular margem para atravessar a proxima escassez."
    );
    assert!(
        restored_agent
            .psychological_state
            .notes
            .iter()
            .any(|note| note == "teste psicologico")
    );

    let view = simulation
        .agent_views()
        .into_iter()
        .find(|view| view.id == restored_agent.id)
        .expect("agent view");
    assert_eq!(view.psychological_state.humiliation, 31);
    assert!(view.psychological_state.summary().contains("luto=42"));
    assert_eq!(
        view.psychological_state.long_term_plan,
        "Acumular margem para atravessar a proxima escassez."
    );
}

#[test]
fn think_maker_updates_persistent_long_term_plan() {
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    let actor_snapshot = simulation
        .snapshot()
        .agents
        .iter()
        .find(|agent| agent.role_id == "campones")
        .cloned()
        .expect("farmer agent for think maker test");
    let output = ThinkMakerOutput {
        reflection: "Quero agir com discricao enquanto fortaleco minha base.".to_string(),
        dominant_emotion: "calculista".to_string(),
        belief_updates: vec!["Observar antes de arriscar recursos.".to_string()],
        long_term_plan: "Consolidar minha posicao como campones sem perder estabilidade."
            .to_string(),
        inner_contradiction_update: Some("quer estabilidade, mas teme invisibilidade".to_string()),
        melancholic_fixation: Some("voltar ao lugar onde se sentiu seguro".to_string()),
    };
    let actor_id = actor_snapshot.id;
    simulation
        .debug_apply_think_maker_output(actor_id, output)
        .expect("apply think maker output");

    let restored = simulation.snapshot();
    let actor = restored
        .agents
        .iter()
        .find(|agent| agent.id == actor_id)
        .expect("actor after think maker");
    assert_eq!(
        actor.psychological_state.long_term_plan,
        "Consolidar minha posicao como campones sem perder estabilidade."
    );
    assert!(
        actor
            .memories
            .iter()
            .any(|memory| { memory.summary.contains("Plano de longo prazo atualizado:") })
    );
}

#[test]
fn low_institutional_legitimacy_generates_political_pressures() {
    let mut snapshot = Simulation::seeded(SimulationConfig::default()).snapshot();
    let agent_id = snapshot.agents[0].id;
    snapshot.agents[0].institutional_perception.tax_legitimacy = -60;
    snapshot.agents[0]
        .institutional_perception
        .justice_legitimacy = -55;
    snapshot.agents[0]
        .institutional_perception
        .rationing_legitimacy = -50;
    snapshot.agents[0]
        .institutional_perception
        .leader_legitimacy = -65;
    snapshot.agents[0]
        .institutional_perception
        .perceived_corruption = 40;

    let mut simulation = Simulation::from_snapshot(snapshot);
    simulation
        .debug_refresh_politics()
        .expect("refresh politics");
    let snapshot = simulation.snapshot();
    let pressures = snapshot
        .political_pressures
        .iter()
        .filter(|pressure| pressure.actor_id == agent_id)
        .map(|pressure| pressure.agenda_tag.as_str())
        .collect::<Vec<_>>();
    assert!(pressures.contains(&"boicote_imposto"));
    assert!(pressures.contains(&"justica_vigilante"));
    assert!(pressures.contains(&"motim_comida"));
    assert!(pressures.contains(&"depor_lider"));
}

#[test]
fn decision_input_contains_institutional_context() {
    let (adapter, state) = InstrumentedAdapter::new();
    let mut snapshot = Simulation::seeded(SimulationConfig::default()).snapshot();
    for agent in &mut snapshot.agents {
        agent.institutional_perception.guard_trust = -45;
        agent.institutional_perception.fear_of_authority = 60;
        agent.institutional_perception.leader_legitimacy = -35;
        agent.llm_cooldown_until = 0;
        agent.next_reconsideration_tick = 0;
    }
    let mut simulation = Simulation::from_snapshot(snapshot);

    simulation.tick(&adapter).expect("tick with decision input");
    let inputs = state.decision_inputs.lock().unwrap();
    assert!(inputs.iter().any(|input| {
        input.institutional_context.guard_trust <= -45
            && input
                .institutional_context
                .likely_reactions
                .iter()
                .any(|reaction| reaction.contains("guardas"))
    }));
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
            || issue_after.supporter_ids.contains(&actor)
    );
}

#[test]
fn daily_political_resolution_does_not_change_tax_without_decree() {
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    simulation.debug_set_public_treasury(0);
    let initial_tax = simulation.snapshot().village_economy.daily_household_tax;
    simulation
        .debug_resolve_daily_politics()
        .expect("resolve politics");

    let snapshot = simulation.snapshot();
    assert_eq!(snapshot.village_economy.daily_household_tax, initial_tax);
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| event.kind == EventKind::InstitutionalDispute)
    );
    assert!(
        snapshot
            .political_issues
            .iter()
            .any(|issue| issue.proposed_value == "aumentar")
    );
}

#[test]
fn leader_decree_creates_typed_policy_act() {
    let llm = MockLlmAdapter;
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    let leader_id = simulation
        .snapshot()
        .agents
        .iter()
        .find(|agent| agent.role_id == "lider_local")
        .map(|agent| agent.id)
        .expect("leader exists");

    simulation
        .debug_assign_intent(
            leader_id,
            AgentIntent {
                kind: IntentKind::Decretar,
                target_agent: None,
                target_semantic: Some("imposto_guerra".to_string()),
                justification: "teste de decreto".to_string(),
                dominant_emotion: "autoridade".to_string(),
                perceived_risk: 5,
                belief_updates: Vec::new(),
                priority: 10,
                social_move: None,
            },
        )
        .expect("assign decree");
    simulation.tick(&llm).expect("execute decree");

    let snapshot = simulation.snapshot();
    let act = snapshot
        .policy_acts
        .iter()
        .rev()
        .find(|act| {
            act.agenda_tag == "imposto_guerra"
                && act.effects.iter().any(|effect| {
                    matches!(
                        effect,
                        PolicyEffect::TaxModifier {
                            multiplier_percent: 200
                        }
                    )
                })
        })
        .expect("typed policy act");
    assert_eq!(act.status, PolicyActStatus::Active);
    assert!(
        snapshot
            .political_issues
            .iter()
            .any(|issue| issue.agenda_tag == "imposto_guerra")
    );
}

#[test]
fn leader_decree_changes_local_tax_norm() {
    let llm = MockLlmAdapter;
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    let initial_tax = simulation.snapshot().village_economy.daily_household_tax;
    let leader_id = simulation
        .snapshot()
        .agents
        .iter()
        .find(|agent| agent.role_id == "lider_local")
        .map(|agent| agent.id)
        .expect("leader exists");

    simulation
        .debug_assign_intent(
            leader_id,
            AgentIntent {
                kind: IntentKind::Decretar,
                target_agent: None,
                target_semantic: Some("aumentar_imposto".to_string()),
                justification: "teste de decreto fiscal".to_string(),
                dominant_emotion: "autoridade".to_string(),
                perceived_risk: 5,
                belief_updates: Vec::new(),
                priority: 10,
                social_move: None,
            },
        )
        .expect("assign decree");
    simulation.tick(&llm).expect("execute decree");

    let snapshot = simulation.snapshot();
    assert_eq!(
        snapshot.village_economy.daily_household_tax,
        initial_tax + 1
    );
    assert!(snapshot.events.iter().any(|event| {
        event.kind == EventKind::NormChanged && event.impact_tags.iter().any(|tag| tag == "decreto")
    }));
}

#[test]
fn territorial_control_changes_only_after_abstract_war_reaches_100() {
    let mut snapshot = Simulation::seeded(SimulationConfig::default()).snapshot();
    snapshot.polities.push(Polity {
        id: 2,
        name: "Baronato rival".to_string(),
        ruler_agent_id: None,
        capital_territory_id: None,
        treasury: 50,
        military_readiness: 40,
    });
    snapshot.next_polity_id = 3;
    snapshot.tick_of_day = snapshot.ticks_per_day - 1;
    let target_territory_id = snapshot.territories[0].id;
    let original_controller = snapshot.territories[0].controller_polity_id;
    snapshot.wars.push(WarState {
        id: 1,
        attacker_polity_id: 2,
        defender_polity_id: original_controller,
        target_territory_ids: vec![target_territory_id],
        attacker_score: 99,
        defender_score: 0,
        stage: WarStage::DecisiveBattle,
        status: WarStatus::Active,
        winner_polity_id: None,
        started_day: snapshot.day,
        ended_day: None,
        summary: "guerra abstrata de teste".to_string(),
    });
    snapshot.next_war_id = 2;

    let llm = MockLlmAdapter;
    let mut simulation = Simulation::from_snapshot(snapshot);
    simulation.tick(&llm).expect("cross day");
    let snapshot = simulation.snapshot();

    assert_eq!(snapshot.territories[0].controller_polity_id, 2);
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| { event.impact_tags.iter().any(|tag| tag == "territorio") })
    );
}

#[test]
fn war_siege_applies_economic_social_and_political_pressure_without_control_change() {
    let mut snapshot = Simulation::seeded(SimulationConfig::default()).snapshot();
    snapshot.polities.push(Polity {
        id: 2,
        name: "Baronato rival".to_string(),
        ruler_agent_id: None,
        capital_territory_id: None,
        treasury: 80,
        military_readiness: 30,
    });
    snapshot.next_polity_id = 3;
    snapshot.tick_of_day = snapshot.ticks_per_day - 1;
    let original_controller = snapshot.territories[0].controller_polity_id;
    let target_territory_id = snapshot.territories[0].id;
    let initial_treasury = snapshot.village_economy.public_treasury;
    snapshot.wars.push(WarState {
        id: 1,
        attacker_polity_id: 2,
        defender_polity_id: original_controller,
        target_territory_ids: vec![target_territory_id],
        attacker_score: 20,
        defender_score: 20,
        stage: WarStage::Siege,
        status: WarStatus::Active,
        winner_polity_id: None,
        started_day: snapshot.day,
        ended_day: None,
        summary: "cerco abstrato de teste".to_string(),
    });
    snapshot.next_war_id = 2;

    let llm = MockLlmAdapter;
    let mut simulation = Simulation::from_snapshot(snapshot);
    simulation.tick(&llm).expect("cross day with war impact");
    let snapshot = simulation.snapshot();

    assert!(snapshot.village_economy.public_treasury < initial_treasury);
    assert_eq!(
        snapshot.territories[0].controller_polity_id,
        original_controller
    );
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| { event.impact_tags.iter().any(|tag| tag == "impacto_guerra") })
    );
    assert!(snapshot.political_pressures.iter().any(|pressure| {
        matches!(
            pressure.agenda_tag.as_str(),
            "motim_comida" | "boicote_imposto"
        )
    }));
}

#[test]
fn active_war_creates_explicit_military_demand() {
    let mut snapshot = Simulation::seeded(SimulationConfig::default()).snapshot();
    snapshot.polities.push(Polity {
        id: 2,
        name: "Baronato rival".to_string(),
        ruler_agent_id: None,
        capital_territory_id: None,
        treasury: 80,
        military_readiness: 30,
    });
    snapshot.next_polity_id = 3;
    snapshot.tick_of_day = snapshot.ticks_per_day - 1;
    let local_polity_id = snapshot.territories[0].controller_polity_id;
    snapshot.wars.push(WarState {
        id: 1,
        attacker_polity_id: 2,
        defender_polity_id: local_polity_id,
        target_territory_ids: vec![snapshot.territories[0].id],
        attacker_score: 10,
        defender_score: 10,
        stage: WarStage::Mobilization,
        status: WarStatus::Active,
        winner_polity_id: None,
        started_day: snapshot.day,
        ended_day: None,
        summary: "mobilizacao de teste".to_string(),
    });
    snapshot.next_war_id = 2;

    let llm = MockLlmAdapter;
    let mut simulation = Simulation::from_snapshot(snapshot);
    simulation.tick(&llm).expect("cross day");
    let snapshot = simulation.snapshot();

    let demand = snapshot
        .military_demands
        .iter()
        .find(|demand| demand.war_id == 1)
        .expect("military demand should be created");
    assert_eq!(demand.polity_id, local_polity_id);
    assert_eq!(demand.status, MilitaryDemandStatus::Open);
    assert!(demand.required.iter().any(|stack| stack.amount > 0));
    assert!(snapshot.events.iter().any(|event| {
        event.kind == EventKind::MilitarySupply
            && event
                .impact_tags
                .iter()
                .any(|tag| tag == "suprimento_militar")
    }));
}

#[test]
fn satisfied_military_demand_increases_readiness_and_war_score() {
    let mut snapshot = Simulation::seeded(SimulationConfig::default()).snapshot();
    let local_polity_id = snapshot.territories[0].controller_polity_id;
    let initial_readiness = snapshot
        .polities
        .iter()
        .find(|polity| polity.id == local_polity_id)
        .map(|polity| polity.military_readiness)
        .unwrap_or_default();
    snapshot.polities.push(Polity {
        id: 2,
        name: "Baronato rival".to_string(),
        ruler_agent_id: None,
        capital_territory_id: None,
        treasury: 50,
        military_readiness: 15,
    });
    snapshot.next_polity_id = 3;
    snapshot.tick_of_day = snapshot.ticks_per_day - 1;
    snapshot.wars.push(WarState {
        id: 1,
        attacker_polity_id: 2,
        defender_polity_id: local_polity_id,
        target_territory_ids: vec![snapshot.territories[0].id],
        attacker_score: 0,
        defender_score: 0,
        stage: WarStage::Mobilization,
        status: WarStatus::Active,
        winner_polity_id: None,
        started_day: snapshot.day,
        ended_day: None,
        summary: "suprimento atendido".to_string(),
    });
    snapshot.next_war_id = 2;
    snapshot.military_demands.push(MilitaryDemand {
        id: 1,
        war_id: 1,
        polity_id: local_polity_id,
        stage: WarStage::Mobilization,
        required: vec![ResourceStack {
            resource_id: ResourceKind::Graos.id().to_string(),
            amount: 4,
        }],
        delivered: vec![ResourceStack {
            resource_id: ResourceKind::Graos.id().to_string(),
            amount: 4,
        }],
        cash_required: 8,
        cash_delivered: 8,
        target_territory_id: Some(snapshot.territories[0].id),
        priority: 80,
        deadline_day: snapshot.day + 1,
        status: MilitaryDemandStatus::PartiallySupplied,
        shortage_score: 0,
        created_day: snapshot.day,
    });
    snapshot.next_military_demand_id = 2;

    let llm = MockLlmAdapter;
    let mut simulation = Simulation::from_snapshot(snapshot);
    simulation.tick(&llm).expect("settle military demand");
    let snapshot = simulation.snapshot();

    let local_polity = snapshot
        .polities
        .iter()
        .find(|polity| polity.id == local_polity_id)
        .expect("local polity");
    assert!(local_polity.military_readiness > initial_readiness);
    let war = snapshot.wars.iter().find(|war| war.id == 1).expect("war");
    assert!(war.defender_score > 0);
    assert_eq!(
        snapshot
            .military_demands
            .iter()
            .find(|demand| demand.id == 1)
            .map(|demand| demand.status),
        Some(MilitaryDemandStatus::Satisfied)
    );
}

#[test]
fn failed_military_demand_creates_political_pressure() {
    let mut snapshot = Simulation::seeded(SimulationConfig::default()).snapshot();
    let local_polity_id = snapshot.territories[0].controller_polity_id;
    snapshot.polities.push(Polity {
        id: 2,
        name: "Baronato rival".to_string(),
        ruler_agent_id: None,
        capital_territory_id: None,
        treasury: 80,
        military_readiness: 30,
    });
    snapshot.next_polity_id = 3;
    snapshot.tick_of_day = snapshot.ticks_per_day - 1;
    snapshot.wars.push(WarState {
        id: 1,
        attacker_polity_id: 2,
        defender_polity_id: local_polity_id,
        target_territory_ids: vec![snapshot.territories[0].id],
        attacker_score: 10,
        defender_score: 10,
        stage: WarStage::Siege,
        status: WarStatus::Active,
        winner_polity_id: None,
        started_day: snapshot.day,
        ended_day: None,
        summary: "suprimento falho".to_string(),
    });
    snapshot.next_war_id = 2;
    snapshot.military_demands.push(MilitaryDemand {
        id: 1,
        war_id: 1,
        polity_id: local_polity_id,
        stage: WarStage::Siege,
        required: vec![ResourceStack {
            resource_id: ResourceKind::Graos.id().to_string(),
            amount: 8,
        }],
        delivered: Vec::new(),
        cash_required: 12,
        cash_delivered: 0,
        target_territory_id: Some(snapshot.territories[0].id),
        priority: 94,
        deadline_day: snapshot.day + 1,
        status: MilitaryDemandStatus::Open,
        shortage_score: 20,
        created_day: snapshot.day,
    });
    snapshot.next_military_demand_id = 2;

    let llm = MockLlmAdapter;
    let mut simulation = Simulation::from_snapshot(snapshot);
    simulation.tick(&llm).expect("settle failed demand");
    let snapshot = simulation.snapshot();

    assert_eq!(
        snapshot
            .military_demands
            .iter()
            .find(|demand| demand.id == 1)
            .map(|demand| demand.status),
        Some(MilitaryDemandStatus::Failed)
    );
    assert!(snapshot.political_pressures.iter().any(|pressure| {
        matches!(
            pressure.agenda_tag.as_str(),
            "motim_comida" | "boicote_imposto" | "depor_lider"
        ) && pressure.reason.contains("demanda militar")
    }));
}

#[test]
fn decisive_battle_creates_injuries_and_war_memories() {
    let mut snapshot = Simulation::seeded(SimulationConfig::default()).snapshot();
    snapshot.polities.push(Polity {
        id: 2,
        name: "Baronato rival".to_string(),
        ruler_agent_id: None,
        capital_territory_id: None,
        treasury: 80,
        military_readiness: 30,
    });
    snapshot.next_polity_id = 3;
    snapshot.tick_of_day = snapshot.ticks_per_day - 1;
    let original_controller = snapshot.territories[0].controller_polity_id;
    snapshot.wars.push(WarState {
        id: 1,
        attacker_polity_id: 2,
        defender_polity_id: original_controller,
        target_territory_ids: vec![snapshot.territories[0].id],
        attacker_score: 70,
        defender_score: 70,
        stage: WarStage::DecisiveBattle,
        status: WarStatus::Active,
        winner_polity_id: None,
        started_day: snapshot.day,
        ended_day: None,
        summary: "batalha abstrata de teste".to_string(),
    });
    snapshot.next_war_id = 2;

    let llm = MockLlmAdapter;
    let mut simulation = Simulation::from_snapshot(snapshot);
    simulation.tick(&llm).expect("cross day with battle");
    let snapshot = simulation.snapshot();

    assert!(snapshot.agents.iter().any(|agent| agent.injury.pain > 0));
    assert!(snapshot.agents.iter().any(|agent| {
        agent
            .memories
            .iter()
            .any(|memory| memory.tags.iter().any(|tag| tag == "guerra"))
    }));
}

#[test]
fn active_rebel_faction_escalates_to_insurrection_and_civil_war() {
    let mut snapshot = Simulation::seeded(SimulationConfig::default()).snapshot();
    let founder_id = snapshot.agents[0].id;
    snapshot.political_factions.push(PoliticalFaction {
        id: 1,
        name: "Liga Rebelde de Teste".to_string(),
        agenda_tag: "depor_lider".to_string(),
        domain: PolicyDomain::Justice,
        proposed_value: "normal".to_string(),
        founder_id,
        member_ids: snapshot
            .agents
            .iter()
            .take(4)
            .map(|agent| agent.id)
            .collect(),
        influence: 160,
        support_issue_ids: Vec::new(),
        opposition_issue_ids: Vec::new(),
        objective: Some(FactionObjective::TaxBoycott {
            day_activated: snapshot.day,
        }),
        is_action_active: true,
        rage: 80,
    });
    snapshot.next_political_faction_id = 2;

    let llm = MockLlmAdapter;
    let mut simulation = Simulation::from_snapshot(snapshot);
    simulation.tick(&llm).expect("process insurrection");
    let snapshot = simulation.snapshot();

    let insurrection = snapshot
        .insurrections
        .iter()
        .find(|insurrection| insurrection.status == InsurrectionStatus::Active)
        .expect("active insurrection");
    assert_eq!(insurrection.stage, InsurrectionStage::CivilWar);
    assert!(insurrection.linked_war_id.is_some());
    assert!(snapshot.wars.iter().any(|war| {
        Some(war.id) == insurrection.linked_war_id && war.status == WarStatus::Active
    }));
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
fn grain_collapse_without_material_supplier_does_not_create_purchase() {
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    let mut snapshot = simulation.snapshot();
    for household in &mut snapshot.households {
        household
            .pantry
            .retain(|stack| stack.resource_id != ResourceKind::Graos.id());
        household
            .reserved_food
            .retain(|stack| stack.resource_id != ResourceKind::Graos.id());
    }
    for establishment in &mut snapshot.establishments {
        establishment
            .stock
            .retain(|stack| stack.resource_id != ResourceKind::Graos.id());
        if establishment.establishment_type_id == "taverna" {
            establishment.cash = 80;
        }
    }
    let mut simulation = Simulation::from_snapshot(snapshot);
    simulation
        .debug_refresh_economy()
        .expect("refresh economy should succeed");

    let snapshot = simulation.snapshot();
    assert!(!snapshot.economic_tasks.iter().any(|task| {
        task.kind == EconomicTaskKind::Comprar
            && task.resource_id.as_deref() == Some(ResourceKind::Graos.id())
            && matches!(task.destination, EconomicNode::Establishment(_))
    }));
    assert!(snapshot.events.iter().any(|event| {
        event.kind == EventKind::Scarcity
            && event
                .impact_tags
                .iter()
                .any(|tag| tag == "sem_origem_material" || tag == "sem_fornecedor_com_estoque")
    }));
}

#[test]
fn unbacked_resource_promise_is_rejected() {
    let mut simulation = Simulation::seeded(SimulationConfig::default());
    let snapshot = simulation.snapshot();
    let household_id = snapshot
        .agents
        .iter()
        .find(|agent| agent.id == 1)
        .and_then(|agent| agent.home_building_id)
        .expect("speaker household");
    simulation
        .debug_set_household_treasury(household_id, 0)
        .expect("set treasury");
    let promise = ProposedPromise {
        recipient_id: 2,
        condition: PromiseCondition::DeliverResource {
            resource_id: ResourceKind::Graos.id().to_string(),
            amount: 999,
        },
        duration_ticks: 120,
    };

    simulation
        .debug_execute_make_promise(1, &promise)
        .expect("promise evaluation");
    let snapshot = simulation.snapshot();

    assert!(snapshot.promises.is_empty());
    assert!(snapshot.events.iter().any(|event| {
        event.actor == 1
            && event.target == Some(2)
            && event.summary.contains("sem estoque, escrow, task ou caixa")
    }));
}

#[test]
fn repeated_support_does_not_emit_duplicate_support_event() {
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
        .find(|issue| issue.status == medieval_village_llm::world_model::PoliticalIssueStatus::Open)
        .expect("open political issue")
        .clone();
    let actor = issue.proposed_by.expect("issue proposer");

    simulation
        .debug_assign_intent(actor, test_intent(IntentKind::Apoiar, None))
        .expect("assign support");
    simulation.tick(&llm).expect("first support tick");
    let first_count = simulation
        .snapshot()
        .events
        .iter()
        .filter(|event| event.kind == EventKind::PoliticalSupport && event.actor == actor)
        .count();

    simulation
        .debug_assign_intent(actor, test_intent(IntentKind::Apoiar, None))
        .expect("assign repeated support");
    simulation.tick(&llm).expect("second support tick");
    let second_count = simulation
        .snapshot()
        .events
        .iter()
        .filter(|event| event.kind == EventKind::PoliticalSupport && event.actor == actor)
        .count();

    assert_eq!(first_count, second_count);
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
    config.max_agents = 10; // Ensure we have enough historical agents to include a farmer
    let mut simulation = Simulation::seeded(config);

    // 1. Initial State: No crops planted
    assert!(
        simulation.snapshot().crops.is_empty(),
        "Crops list should start empty"
    );

    // Find a farm building (Celeiro)
    let farm_building = simulation
        .snapshot()
        .spatial
        .buildings
        .iter()
        .find(|b| b.kind == LocationKind::Farm)
        .cloned()
        .expect("should have a farm building");

    // Find a farmer agent
    let farmer = simulation
        .snapshot()
        .agents
        .iter()
        .find(|a| a.role_id == "campones")
        .cloned()
        .expect("should have a farmer agent");

    // Force farmer position to the Celeiro entrance
    simulation
        .debug_force_agent_position(farmer.id, farm_building.entrance)
        .expect("force position");

    // Force Farmer intent to Trabalhar(fazenda)
    simulation
        .debug_assign_intent(
            farmer.id,
            AgentIntent {
                kind: IntentKind::Trabalhar,
                target_agent: None,
                target_semantic: Some("fazenda".to_string()),
                justification: "trabalhar".to_string(),
                dominant_emotion: "focado".to_string(),
                perceived_risk: 0,
                belief_updates: vec![],
                priority: 100,
                social_move: None,
            },
        )
        .expect("assign intent");
    simulation
        .debug_force_navigation(farmer.id, farm_building.entrance, vec![])
        .expect("force navigation");

    {
        let snapshot = simulation.snapshot();
        let agent = snapshot.agents.iter().find(|a| a.id == farmer.id).unwrap();
        println!(
            "DEBUG: Agent pos={:?}, dest={:?}, intent={:?}",
            agent.position, agent.destination, agent.last_intent
        );

        let farm_buildings: Vec<&BuildingSpec> = snapshot
            .spatial
            .buildings
            .iter()
            .filter(|b| b.kind == LocationKind::Farm)
            .collect();
        println!(
            "DEBUG: farm_buildings: {:?}",
            farm_buildings
                .iter()
                .map(|b| (b.id, b.entrance))
                .collect::<Vec<_>>()
        );

        let farm_fields: Vec<TileCoord> = snapshot
            .spatial
            .grid
            .tiles
            .iter()
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
    simulation
        .debug_force_agent_position(farmer.id, farm_building.entrance)
        .expect("force position 2");
    simulation
        .debug_assign_intent(
            farmer.id,
            AgentIntent {
                kind: IntentKind::Trabalhar,
                target_agent: None,
                target_semantic: Some("fazenda".to_string()),
                justification: "trabalhar".to_string(),
                dominant_emotion: "focado".to_string(),
                perceived_risk: 0,
                belief_updates: vec![],
                priority: 100,
                social_move: None,
            },
        )
        .expect("assign intent 2");
    simulation
        .debug_force_navigation(farmer.id, farm_building.entrance, vec![])
        .expect("force navigation 2");

    simulation
        .tick(&adapter)
        .expect("tick for work attempt during growth");

    // Verify crops are still there
    let snapshot = simulation.snapshot();
    assert!(
        !snapshot.crops.is_empty(),
        "Crops must still exist during growth"
    );

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
    let initial_grain = snapshot
        .establishments
        .iter()
        .find(|e| e.building_id == Some(farm_building.id))
        .map(|e| {
            e.stock
                .iter()
                .find(|s| s.resource_id == "graos")
                .map(|s| s.amount)
                .unwrap_or(0)
        })
        .unwrap_or(0);

    simulation
        .debug_force_agent_position(farmer.id, farm_building.entrance)
        .expect("force position 3");
    simulation
        .debug_assign_intent(
            farmer.id,
            AgentIntent {
                kind: IntentKind::Trabalhar,
                target_agent: None,
                target_semantic: Some("fazenda".to_string()),
                justification: "trabalhar".to_string(),
                dominant_emotion: "focado".to_string(),
                perceived_risk: 0,
                belief_updates: vec![],
                priority: 100,
                social_move: None,
            },
        )
        .expect("assign intent 3");
    simulation
        .debug_force_navigation(farmer.id, farm_building.entrance, vec![])
        .expect("force navigation 3");

    simulation.tick(&adapter).expect("tick for harvesting");

    let snapshot = simulation.snapshot();
    let final_grain = snapshot
        .establishments
        .iter()
        .find(|e| e.building_id == Some(farm_building.id))
        .map(|e| {
            e.stock
                .iter()
                .find(|s| s.resource_id == "graos")
                .map(|s| s.amount)
                .unwrap_or(0)
        })
        .unwrap_or(0);
    assert!(
        final_grain > initial_grain,
        "Grain stock should increase after harvest"
    );
    assert!(
        snapshot.crops.is_empty(),
        "Crops list should be empty after harvesting"
    );
}

#[test]
fn test_body_graph_damage_and_visceral_combat() {
    let mut config = SimulationConfig::default();
    config.max_agents = 2; // Let's seed at least 2 agents
    let mut simulation = Simulation::seeded(config);

    let snapshot = simulation.snapshot();
    assert!(snapshot.agents.len() >= 2);
    let a1 = snapshot.agents[0].id;
    let a2 = snapshot.agents[1].id;

    // Force position of both agents to be adjacent
    let p1 = TileCoord { x: 5, y: 5 };
    let p2 = TileCoord { x: 5, y: 6 };
    simulation.debug_force_agent_position(a1, p1).unwrap();
    simulation.debug_force_agent_position(a2, p2).unwrap();

    // Verify initial body graph of target is fully intact
    let target_injury_before = simulation.agent_injury(a2).unwrap();
    assert_eq!(target_injury_before.body_parts.len(), 15);
    for part in &target_injury_before.body_parts {
        assert_eq!(part.status, PartInjuryStatus::Intact);
        assert_eq!(part.health, 100);
        assert_eq!(part.pain, 0);
        assert_eq!(part.bleeding, 0);
    }

    // Trigger physical attack from agent 1 to agent 2
    simulation.apply_attack(a1, a2, false).unwrap();

    // Verify target body parts are damaged
    let target_injury_after = simulation.agent_injury(a2).unwrap();
    let damaged_parts: Vec<_> = target_injury_after
        .body_parts
        .iter()
        .filter(|p| p.status != PartInjuryStatus::Intact)
        .collect();

    assert!(
        !damaged_parts.is_empty(),
        "Pelo menos uma parte do corpo deve ter sido atingida e danificada"
    );
    let hit_part = damaged_parts[0];
    assert!(hit_part.health < 100);
    assert!(hit_part.pain > 0 || hit_part.bleeding > 0);

    // Verify global pain and bleeding are aggregated correctly
    assert_eq!(
        target_injury_after.pain,
        target_injury_after
            .body_parts
            .iter()
            .map(|p| p.pain)
            .sum::<i32>()
            .clamp(0, 100)
    );
    assert_eq!(
        target_injury_after.bleeding,
        target_injury_after
            .body_parts
            .iter()
            .map(|p| p.bleeding)
            .sum::<i32>()
            .clamp(0, 10)
    );

    // Verify visceral description is generated in world events
    let snapshot_after = simulation.snapshot();
    let combat_events: Vec<_> = snapshot_after
        .events
        .iter()
        .filter(|e| e.actor == a1 && e.target == Some(a2))
        .collect();

    assert!(!combat_events.is_empty());
    let visceral_summary = &combat_events[0].summary;
    println!("VISCERAL SUMMARY LOGGED: {}", visceral_summary);

    // Check if the summary is visceral and contains details about the hit body part
    assert!(
        visceral_summary.contains("golpe")
            || visceral_summary.contains("perfurou")
            || visceral_summary.contains("esmagou")
            || visceral_summary.contains("cravou")
            || visceral_summary.contains("dilacerou")
            || visceral_summary.contains("quebrando")
            || visceral_summary.contains("cortou")
            || visceral_summary.contains("soco"),
        "Deve conter descrição visceral do ataque"
    );

    // Verify persistence serialization/deserialization by performing a save/load cycle
    let db_dir = tempdir().unwrap();
    let db_path = db_dir.path().join("test_body_graph.db");
    let mut persistence = Persistence::open(&db_path).unwrap();
    persistence.save(&mut simulation, "combat_test").unwrap();

    let restored_snapshot = persistence
        .load_latest()
        .unwrap()
        .expect("should restore snapshot");
    let restored_agent2 = restored_snapshot
        .agents
        .iter()
        .find(|a| a.id == a2)
        .unwrap();

    // Ensure body graph state is recovered correctly
    let restored_hit_part = restored_agent2
        .injury
        .body_parts
        .iter()
        .find(|p| p.kind == hit_part.kind)
        .unwrap();
    assert_eq!(restored_hit_part.status, hit_part.status);
    assert_eq!(restored_hit_part.health, hit_part.health);
    assert_eq!(restored_hit_part.pain, hit_part.pain);
    assert_eq!(restored_hit_part.bleeding, hit_part.bleeding);
}

#[test]
fn test_recursive_tribute_and_dynamic_levy_call() {
    let mut config = SimulationConfig::default();
    config.max_agents = 6;
    let mut simulation = Simulation::seeded(config);

    // Locate vassal contract
    let snapshot = simulation.snapshot();
    let contract = snapshot
        .feudal_contracts
        .iter()
        .find(|c| c.status == FeudalContractStatus::Active)
        .cloned()
        .expect("should have seeded active feudal contract");

    let suzerain_id = contract.suzerain_agent_id;
    let vassal_id = contract.vassal_agent_id;

    let vassal_house_id = snapshot
        .agents
        .iter()
        .find(|a| a.id == vassal_id)
        .and_then(|a| a.home_building_id)
        .expect("vassal household");

    let suzerain_house_id = snapshot
        .agents
        .iter()
        .find(|a| a.id == suzerain_id)
        .and_then(|a| a.home_building_id)
        .expect("suzerain household");

    // Set initial treasuries: vassal has 10, suzerain has 0
    simulation
        .debug_set_household_treasury(vassal_house_id, 10)
        .unwrap();
    simulation
        .debug_set_household_treasury(suzerain_house_id, 0)
        .unwrap();

    // Verify initial contract stats
    let contract_tribute = contract.tribute_due_per_day;
    assert!(contract_tribute > 0);

    // 1. Manually trigger apply_daily_feudal_obligations with force = true
    simulation.apply_daily_feudal_obligations(true).unwrap();

    // After next day, check vassal paid tribute to suzerain
    let snapshot_day2 = simulation.snapshot();
    let _vassal_treasury_day2 = snapshot_day2
        .households
        .iter()
        .find(|h| h.id == vassal_house_id)
        .unwrap()
        .treasury;

    let suzerain_treasury_day2 = snapshot_day2
        .households
        .iter()
        .find(|h| h.id == suzerain_house_id)
        .unwrap()
        .treasury;

    // Assert payments happened (suzerain treasury should receive at least the contract_tribute)
    assert!(suzerain_treasury_day2 >= contract_tribute);

    // 2. Test apply_levy_call_intent
    // Make sure we have enough food in vassal's pantry
    if let Some(household) = simulation.household_by_id_mut(vassal_house_id) {
        household.pantry.push(ResourceStack {
            resource_id: "graos".to_string(),
            amount: 50,
        });
    }

    // Force contract parameters to have 100% acceptance chance by setting loyalty high
    simulation
        .apply_feudal_oath_intent(vassal_id, Some(suzerain_id))
        .unwrap();

    let initial_readiness = simulation
        .snapshot()
        .polities
        .first()
        .map(|p| p.military_readiness)
        .unwrap_or(0);

    // Call levy
    simulation
        .apply_levy_call_intent(suzerain_id, Some(vassal_id))
        .unwrap();

    // Verify polity military readiness increased
    let final_snapshot = simulation.snapshot();
    let final_readiness = final_snapshot
        .polities
        .first()
        .map(|p| p.military_readiness)
        .unwrap_or(0);

    assert!(
        final_readiness > initial_readiness,
        "Military readiness should increase after successful levy call"
    );

    // Verify stress increased on the vassal agent
    let vassal_agent = final_snapshot
        .agents
        .iter()
        .find(|a| a.id == vassal_id)
        .unwrap();
    assert!(
        vassal_agent.state.stress > 0,
        "Vassal stress should increase after sending levy"
    );

    // Verify a LevyCalled event is logged
    let has_levy_event = final_snapshot
        .events
        .iter()
        .any(|e| e.kind == EventKind::LevyCalled);
    assert!(has_levy_event, "LevyCalled event should be recorded");
}

#[test]
fn test_feudal_corvee_labor_execution() {
    let mut config = SimulationConfig::default();
    config.max_agents = 6;
    let mut simulation = Simulation::seeded(config);

    let snapshot = simulation.snapshot();
    let contract = snapshot
        .feudal_contracts
        .iter()
        .find(|c| c.status == FeudalContractStatus::Active)
        .cloned()
        .expect("should have seeded active feudal contract");

    let suzerain_id = contract.suzerain_agent_id;
    let vassal_id = contract.vassal_agent_id;

    let vassal_house_id = snapshot
        .agents
        .iter()
        .find(|a| a.id == vassal_id)
        .and_then(|a| a.home_building_id)
        .expect("vassal household");

    let suzerain_house_id = snapshot
        .agents
        .iter()
        .find(|a| a.id == suzerain_id)
        .and_then(|a| a.home_building_id)
        .expect("suzerain household");

    // Find a lenhal or pedreira establishment
    let est = snapshot
        .establishments
        .iter()
        .find(|e| e.establishment_type_id == "lenhal" || e.establishment_type_id == "pedreira")
        .cloned()
        .expect("should find a lenhal or pedreira establishment");

    let est_id = est.id;

    // Set vassal to work at this establishment
    simulation
        .debug_set_agent_work_building(vassal_id, est.building_id)
        .unwrap();

    // Associate establishment with suzerain: set owner_household_ids to suzerain's household
    simulation
        .debug_set_establishment_owner_household(est_id, suzerain_house_id)
        .unwrap();

    // Add establishment to suzerain's EstateHolding if it exists
    let _ = simulation.debug_add_establishment_to_estate_holding(suzerain_id, est_id);

    // Configure vassal household corvee_days_due
    if let Some(h) = simulation.household_by_id_mut(vassal_house_id) {
        h.corvee_days_due = 1;
        h.direct_lord_agent_id = Some(suzerain_id);
        h.pending_payments.clear();
    }

    // Clear suzerain's pantry
    if let Some(h) = simulation.household_by_id_mut(suzerain_house_id) {
        h.pantry.clear();
    }

    // Keep track of initial agent state
    let initial_state = simulation.agent_state(vassal_id).unwrap();

    // Trigger apply_work
    simulation.apply_work(vassal_id).unwrap();

    // Verify corvee day was decremented
    let final_snapshot = simulation.snapshot();
    let vassal_house = final_snapshot
        .households
        .iter()
        .find(|h| h.id == vassal_house_id)
        .unwrap();
    assert_eq!(
        vassal_house.corvee_days_due, 0,
        "Corvee days due should decrement to 0"
    );

    // Verify vassal did NOT receive any salary claim
    assert!(
        vassal_house.pending_payments.is_empty(),
        "Peasant should not receive wages for corvee labor"
    );

    // Verify suzerain received the produced resource in their pantry
    let suzerain_house = final_snapshot
        .households
        .iter()
        .find(|h| h.id == suzerain_house_id)
        .unwrap();

    let produced_in_pantry = suzerain_house.pantry.iter().any(|stack| stack.amount > 0);
    assert!(
        produced_in_pantry,
        "Suzerain's pantry should receive the resources produced by corvee labor"
    );

    // Verify physical/emotional penalties: stress increased by +5, mood decreased by -3
    let final_state = final_snapshot
        .agents
        .iter()
        .find(|a| a.id == vassal_id)
        .unwrap()
        .state
        .clone();
    let expected_stress = (initial_state.stress + 5).clamp(0, 100);
    let expected_mood = (initial_state.mood - 3).clamp(0, 100);

    assert_eq!(
        final_state.stress, expected_stress,
        "Stress should increase by 5 (2 normal + 3 penalty)"
    );
    assert_eq!(
        final_state.mood, expected_mood,
        "Mood should decrease by 3 (-3 net change)"
    );

    // Verify event was logged
    let has_corvee_event = final_snapshot.events.iter().any(|e| {
        e.kind == EventKind::FeudalSanction && e.impact_tags.contains(&"corveia".to_string())
    });
    assert!(
        has_corvee_event,
        "Should log a FeudalSanction event with 'corveia' tag"
    );
}

#[test]
fn test_chunk_dynamic_value_and_organic_expansion() {
    use medieval_village_llm::sim_core::LifeStatusComponent;
    use medieval_village_llm::world_model::{
        AgentLifeStatus, ConstructionProject, EconomicTask, EconomicTaskClass, EconomicTaskPhase,
    };

    let mut config = SimulationConfig::default();
    config.max_agents = 6;
    let mut simulation = Simulation::seeded(config);

    // 1. Valor dinâmico
    simulation.debug_recalculate_territory_values();
    let snapshot = simulation.snapshot();
    for t in &snapshot.territories {
        assert!(t.strategic_value >= 0 && t.strategic_value <= 200);
    }

    // Prepare clean state for tests
    let ruler_id = {
        let p1 = &mut simulation.debug_polities_mut()[0];
        p1.military_readiness = 100;
        p1.treasury = 500;
        p1.ruler_agent_id.unwrap()
    };

    // Make sure ruler is alive
    let ruler_entity = simulation.find_agent_entity(ruler_id).unwrap();
    if let Some(mut life) = simulation
        .debug_world_mut()
        .entity_mut(ruler_entity)
        .get_mut::<LifeStatusComponent>()
    {
        life.0 = AgentLifeStatus::Vivo;
    }

    // Set territory 1 coordinates
    let t1_id = simulation.debug_territories()[0].id;
    {
        let t1 = &mut simulation.debug_territories_mut()[0];
        t1.controller_polity_id = 1;
        t1.tile_coords = vec![TileCoord { x: 5, y: 5 }];
    }

    // 2. Território livre annexation
    let t2_id = simulation.debug_territories()[1].id;
    {
        let t2 = &mut simulation.debug_territories_mut()[1];
        t2.controller_polity_id = 999; // Free polity
        t2.tile_coords = vec![TileCoord { x: 5, y: 6 }]; // Adjacent
        t2.strategic_value = 50;
    }

    simulation.debug_apply_daily_organic_expansion();
    assert_eq!(
        simulation
            .debug_territories()
            .iter()
            .find(|t| t.id == t2_id)
            .unwrap()
            .controller_polity_id,
        1,
        "Territory 2 should be claimed by Polity 1"
    );

    // 3. Guerra por território controlado
    // Reset Territory 2 to another active polity (id 2)
    let p2_id = 2;
    {
        let t2 = &mut simulation.debug_territories_mut()[1];
        t2.controller_polity_id = p2_id;
        t2.strategic_value = 50;
    }
    // Create polity 2
    simulation.debug_polities_mut().push(Polity {
        id: p2_id,
        name: "Polity 2".to_string(),
        ruler_agent_id: Some(2),
        capital_territory_id: Some(t2_id),
        treasury: 100,
        military_readiness: 50,
    });
    // Make sure polity 2 ruler is alive
    let p2_ruler_entity = simulation.find_agent_entity(2).unwrap();
    if let Some(mut life) = simulation
        .debug_world_mut()
        .entity_mut(p2_ruler_entity)
        .get_mut::<LifeStatusComponent>()
    {
        life.0 = AgentLifeStatus::Vivo;
    }

    simulation.debug_wars_mut().clear();
    simulation.debug_apply_daily_organic_expansion();

    assert_eq!(
        simulation
            .debug_territories()
            .iter()
            .find(|t| t.id == t2_id)
            .unwrap()
            .controller_polity_id,
        2,
        "Controller of Territory 2 should still be Polity 2"
    );
    assert!(
        simulation
            .debug_wars()
            .iter()
            .any(|w| w.attacker_polity_id == 1
                && w.defender_polity_id == 2
                && w.target_territory_ids.contains(&t2_id)),
        "A WarState should be created for Territory 2"
    );

    // 4. Nome gerado
    let name = simulation.debug_generate_emergent_polity_name(ruler_id);
    assert!(!name.is_empty(), "Generated name should not be empty");

    // 5. Construção financiada por polity
    // Let's create a project
    let project_id = simulation.debug_next_construction_project_id();
    simulation.debug_set_next_construction_project_id(project_id + 1);
    simulation
        .debug_construction_projects_mut()
        .push(ConstructionProject {
            id: project_id,
            establishment_type_id: "casa".to_string(),
            building_name: "Casa Teste".to_string(),
            planned_footprint: vec![TileCoord { x: 5, y: 5 }],
            entrance: TileCoord { x: 5, y: 5 },
            materials_required: vec![ResourceStack {
                resource_id: "madeira".to_string(),
                amount: 1,
            }],
            materials_delivered: Vec::new(),
            labor_required: 10,
            labor_done: 0,
            status: ConstructionStatus::Planned,
            priority: 50,
            systemic_reason: "teste".to_string(),
            resulting_building_id: None,
            funding_polity_id: Some(1),
        });

    // Seed some madeira in establishment 1
    if let Some(est) = simulation
        .debug_establishments_mut()
        .iter_mut()
        .find(|e| e.id == 1)
    {
        est.stock = vec![ResourceStack {
            resource_id: "madeira".to_string(),
            amount: 10,
        }];
        est.cash = 100;
    }

    // Execute construction material task
    let task = EconomicTask {
        id: 9999,
        kind: EconomicTaskKind::Construir,
        class: EconomicTaskClass::GeneralCommerce,
        priority: 50,
        lock_until_complete: true,
        creates_household_reserve: false,
        actor_household_id: 1,
        assigned_agent_id: None,
        source: EconomicNode::Establishment(1),
        destination: EconomicNode::ConstructionProject(project_id),
        resource_id: Some("madeira".to_string()),
        amount: 1,
        unit_price: 50,
        total_price: 50,
        description: "Obter madeira".to_string(),
        phase: EconomicTaskPhase::AwaitingPickup,
        related_establishment_id: None,
        related_construction_project_id: Some(project_id),
    };

    let initial_treasury = simulation
        .debug_polities()
        .iter()
        .find(|p| p.id == 1)
        .unwrap()
        .treasury;
    let initial_cash = simulation
        .debug_establishments()
        .iter()
        .find(|e| e.id == 1)
        .unwrap()
        .cash;

    simulation
        .debug_execute_construction_material_task(ruler_id, task, project_id)
        .unwrap();

    let final_treasury = simulation
        .debug_polities()
        .iter()
        .find(|p| p.id == 1)
        .unwrap()
        .treasury;
    let final_cash = simulation
        .debug_establishments()
        .iter()
        .find(|e| e.id == 1)
        .unwrap()
        .cash;

    assert_eq!(
        final_treasury,
        initial_treasury - 50,
        "Polity treasury should be debited by 50"
    );
    assert_eq!(
        final_cash,
        initial_cash + 50,
        "Establishment cash should be credited by 50"
    );

    // 6. Indexação de building
    // Let's materialize the project
    let project = simulation
        .debug_construction_projects_mut()
        .iter()
        .find(|p| p.id == project_id)
        .unwrap()
        .clone();
    let building_id = simulation
        .debug_materialize_construction_project(&project)
        .unwrap();

    // Check that building is in territory 1 building_ids
    let t1 = simulation
        .debug_territories()
        .iter()
        .find(|t| t.id == t1_id)
        .unwrap();
    assert!(
        t1.building_ids.contains(&building_id),
        "Territory 1 should index the materialized building"
    );
}

#[test]
fn test_magical_fauna_system() {
    use medieval_village_llm::sim_core::{CreatureStateComponent, SimulationConfig};
    use medieval_village_llm::world_model::{
        BodyPartKind, HuntingQuest, PartInjuryStatus, ResourceKind, TileCoord,
    };

    let mut config = SimulationConfig::default();
    config.max_agents = 4;
    let mut sim = Simulation::seeded(config);

    // ── 1. Naming Verification ──
    let name_common = sim.generate_creature_name(10, false);
    assert!(
        !name_common.is_empty(),
        "Common creature name should not be empty"
    );
    assert!(
        name_common.len() > 5,
        "Common creature name should be a compound word"
    );

    let name_legendary = sim.generate_creature_name(5, true);
    assert!(
        name_legendary.contains(", "),
        "Legendary name should contain name + epithet separator"
    );

    // ── 2. Spawn Verification ──
    let start_creatures_len = sim.debug_creatures().len();
    // Spawning a test creature manually
    let creature_id = 9991;
    let habitat_tid = 1;
    let start_pos = TileCoord { x: 5, y: 5 };
    sim.debug_spawn_creature(
        creature_id,
        "Drakigniserpe".to_string(),
        "Pedrapiro".to_string(),
        false,
        start_pos,
        habitat_tid,
        100, // hp
        20,  // attack
    );

    let creatures = sim.debug_creatures();
    assert_eq!(
        creatures.len(),
        start_creatures_len + 1,
        "Should have spawned 1 creature"
    );
    let c = creatures.iter().find(|cr| cr.id == creature_id).unwrap();
    assert_eq!(c.name, "Drakigniserpe");
    assert_eq!(c.species, "Pedrapiro");
    assert_eq!(c.health, 100);
    assert_eq!(c.position, start_pos);

    // ── 3. ECS Isolation Verification ──
    // Get all agents, verify they have AgentCore, and do not contain our creature ID
    let agent_ids: Vec<u64> = sim.snapshot().agents.iter().map(|a| a.id).collect();
    assert!(
        !agent_ids.contains(&creature_id),
        "Creature should not be in agent_ids list"
    );
    assert!(
        sim.debug_is_creature(creature_id),
        "ID should be classified as creature"
    );

    // ── 4. Body Graph & Injury Verification ──
    // Creature should have standard body parts intact
    assert_eq!(
        c.injury.body_parts.len(),
        15,
        "Creature should have standard human body graph parts (15)"
    );
    let head = c
        .injury
        .body_parts
        .iter()
        .find(|p| p.kind == BodyPartKind::Head)
        .unwrap();
    assert_eq!(head.status, PartInjuryStatus::Intact);

    // Apply attack from a human agent on the creature
    let hunter_id = agent_ids[0];
    sim.debug_apply_attack_on_creature(hunter_id, creature_id, false)
        .unwrap();

    // Health should have decreased
    let hp_after = sim.debug_creature_health(creature_id).unwrap();
    assert!(
        hp_after < 100,
        "Creature health should have decreased after attack"
    );

    // Body part state should have changed to Bruised/Lacerated
    let creatures_after = sim.debug_creatures();
    let c_after = creatures_after
        .iter()
        .find(|cr| cr.id == creature_id)
        .unwrap();
    let injured_parts = c_after
        .injury
        .body_parts
        .iter()
        .filter(|p| p.status != PartInjuryStatus::Intact)
        .count();
    assert!(
        injured_parts > 0,
        "Creature body graph should have recorded an injury"
    );

    // ── 5. Combates, Quests, Drops & Gold Verification ──
    // Create a hunting quest targeting this creature
    let quest_id = 8801;
    sim.hunting_quests.push(HuntingQuest {
        id: quest_id,
        target_creature_id: creature_id,
        reward_gold: 150,
        funding_polity_id: None,
        funding_household_id: None,
    });

    let initial_gold = {
        let snapshot = sim.snapshot();
        let hunter_snap = snapshot.agents.iter().find(|a| a.id == hunter_id).unwrap();
        hunter_snap
            .inventory
            .iter()
            .find(|s| s.resource_id == ResourceKind::Moedas.id())
            .map(|s| s.amount)
            .unwrap_or(0)
    };

    // Kill the creature (reduce health to 0 manually, then run attack)
    {
        let ent = sim.find_creature_entity(creature_id).unwrap();
        sim.debug_world_mut()
            .entity_mut(ent)
            .get_mut::<CreatureStateComponent>()
            .unwrap()
            .health = 1;
    }
    sim.debug_apply_attack_on_creature(hunter_id, creature_id, true)
        .unwrap();

    // Creature should be dead
    let creatures_final = sim.debug_creatures();
    let c_final = creatures_final
        .iter()
        .find(|cr| cr.id == creature_id)
        .unwrap();
    assert!(!c_final.active, "Creature should be dead/inactive");

    // Drop ("nucleo_pedrapiro" for Pedrapiro) should be in hunter's inventory
    let snapshot_final = sim.snapshot();
    let hunter_final = snapshot_final
        .agents
        .iter()
        .find(|a| a.id == hunter_id)
        .unwrap();
    let drop_stack = hunter_final
        .inventory
        .iter()
        .find(|s| s.resource_id == "nucleo_pedrapiro");
    assert!(
        drop_stack.is_some(),
        "Hunter should have received the drop: nucleo_pedrapiro"
    );
    assert_eq!(drop_stack.unwrap().amount, 1);

    // Quest should be resolved and reward gold paid
    let gold_final = hunter_final
        .inventory
        .iter()
        .find(|s| s.resource_id == ResourceKind::Moedas.id())
        .map(|s| s.amount)
        .unwrap_or(0);
    assert_eq!(
        gold_final,
        initial_gold + 150,
        "Hunter should have received 150 gold reward"
    );
    assert!(
        !sim.hunting_quests.iter().any(|q| q.id == quest_id),
        "Hunting quest should have been resolved/removed"
    );

    println!("✅ All magical fauna integration tests passed!");
}
