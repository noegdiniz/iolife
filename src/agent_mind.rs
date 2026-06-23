use crate::world_model::{
    AgentIntent, AgentMemory, AgentRelation, AgentState, EconomicTaskClass, EconomicTaskKind,
    EventKind, FixtureKind, IntentKind, PromiseCondition, RelationDelta, SimplifiedTask,
    SocialMove, WorldEvent,
};
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NearbyAgentInput {
    pub id: u64,
    pub name: String,
    pub role: String,
    pub distance: i32,
    pub same_room: bool,
    pub perceived_status: String,
    pub visible_equipment: String,
    pub relation: Option<AgentRelation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NearbyFixtureInput {
    pub id: u64,
    pub name: String,
    pub kind: FixtureKind,
    pub distance: i32,
    pub building_name: Option<String>,
    pub room_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelevantMemoryInput {
    pub id: u64,
    pub summary: String,
    pub weight: i32,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentEventInput {
    pub day: u32,
    pub tick: u32,
    pub kind: EventKind,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PsychologicalContextInput {
    pub core_values: Vec<String>,
    pub long_term_desires: Vec<String>,
    pub fears: Vec<String>,
    pub social_style: String,
    pub moral_tolerances: Vec<String>,
    pub inner_conflicts: Vec<String>,
    pub current_identity_tension: String,
    pub dominant_preoccupations: Vec<String>,
    pub recent_self_narrative: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicOpportunityInput {
    pub kind: EconomicTaskKind,
    pub class: EconomicTaskClass,
    pub priority: u8,
    pub summary: String,
    pub resource_id: Option<String>,
    pub amount: i32,
    pub unit_price: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicContextInput {
    pub household_name: String,
    pub household_treasury: i32,
    pub pantry: Vec<String>,
    pub reserved_food: Vec<String>,
    pub food_crisis_level: u8,
    pub reserved_food_workers: u8,
    pub open_food_tasks: usize,
    pub has_food_purchase_in_transit: bool,
    pub can_eat_from_reserve: bool,
    pub pending_salary: i32,
    pub tax_pressure: i32,
    pub work_obligations: Vec<String>,
    pub local_prices: Vec<String>,
    pub equipment_market_offers: Vec<String>,
    pub base_resource_availability: Vec<String>,
    pub scarcity_signals: Vec<String>,
    pub grain_availability: String,
    pub external_grain_offer: Option<String>,
    pub public_treasury_status: String,
    pub war_supply_status: Vec<String>,
    pub open_tasks: Vec<EconomicOpportunityInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalContextInput {
    pub life_status: String,
    pub injury_summary: String,
    pub active_combat: Option<String>,
    pub nearby_threats: Vec<String>,
    pub open_cases: Vec<String>,
    pub cases_against_actor: Vec<String>,
    pub cases_involving_actor: Vec<String>,
    pub witness_risk: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoliticalContextInput {
    pub local_norms: Vec<String>,
    pub relevant_factions: Vec<String>,
    pub open_issues: Vec<String>,
    pub likely_position: String,
    pub household_grievances: Vec<String>,
    pub opposition_risks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstitutionalContextInput {
    pub leader_legitimacy: i32,
    pub justice_legitimacy: i32,
    pub tax_legitimacy: i32,
    pub rationing_legitimacy: i32,
    pub guard_trust: i32,
    pub war_support: i32,
    pub fear_of_authority: i32,
    pub perceived_corruption: i32,
    pub perceived_fairness: i32,
    pub summary: String,
    pub likely_reactions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeudalContextInput {
    pub title: Option<String>,
    pub direct_lord: Option<String>,
    pub subordinate_summaries: Vec<String>,
    pub holdings: Vec<String>,
    pub obligations: Vec<String>,
    pub contract_pressures: Vec<String>,
    pub succession_status: Vec<String>,
    pub authority_conflicts: Vec<String>,
    pub sanction_risk: String,
    pub power_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InformationContextInput {
    pub known_rumors: Vec<String>,
    pub believed_rumors: Vec<String>,
    pub known_secrets: Vec<String>,
    pub credibility_notes: Vec<String>,
    pub slander_risks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CulturalContextInput {
    pub known_stories: Vec<String>,
    pub locally_relevant_stories: Vec<String>,
    pub family_stories: Vec<String>,
    pub faction_stories: Vec<String>,
    pub stories_likely_to_tell: Vec<String>,
    pub cultural_risks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeContextInput {
    pub day: u32,
    pub tick_of_day: u32,
    pub hour: u32,
    pub minute: u32,
    pub time_label: String,
    pub day_phase: String,
    pub is_daylight: bool,
    pub is_work_time: bool,
    pub is_meal_time: bool,
    pub is_sleep_time: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldPlaceInput {
    pub place_id: String,
    pub display_name: String,
    pub kind: String,
    pub semantic_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionInput {
    pub actor_id: u64,
    pub actor_name: String,
    pub role: String,
    pub day: u32,
    pub tick: u32,
    pub time_context: TimeContextInput,
    pub world_places: Vec<WorldPlaceInput>,
    pub current_area: String,
    pub current_building: Option<String>,
    pub current_building_kind: Option<String>,
    pub current_room: Option<String>,
    pub accessible_exits: Vec<String>,
    pub nearby_fixtures: Vec<NearbyFixtureInput>,
    pub nearby_agents: Vec<NearbyAgentInput>,
    pub relevant_memories: Vec<RelevantMemoryInput>,
    pub recent_events: Vec<RecentEventInput>,
    pub current_goals: Vec<String>,
    pub known_destination: Option<String>,
    pub blockers: Vec<String>,
    pub state: AgentState,
    pub self_equipment_summary: String,
    pub self_prestige_summary: String,
    pub self_prestige_score: i32,
    pub reactive_stance: String,
    pub status_pressure_summary: String,
    pub revenge_summary: String,
    pub public_shame_summary: String,
    pub authority_posture_summary: String,
    pub cognition_trigger: String,
    pub context_depth: String,
    pub psychological_context: PsychologicalContextInput,
    pub economic_context: EconomicContextInput,
    pub legal_context: LegalContextInput,
    pub political_context: PoliticalContextInput,
    pub institutional_context: InstitutionalContextInput,
    pub feudal_context: FeudalContextInput,
    pub information_context: InformationContextInput,
    pub cultural_context: CulturalContextInput,
    pub profile_summary: Vec<String>,
    pub llm_budget_remaining: u32,
    pub chaos_pressure: u32,
    pub personality_traits: Vec<String>,
    pub trauma_traits: Vec<String>,
    #[serde(default)]
    pub body_parts: Vec<crate::world_model::BodyPartState>,
}

pub type ActionPlannerInput = DecisionInput;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkMakerInput {
    pub decision_input: DecisionInput,
    pub planned_tasks: Vec<SimplifiedTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkMakerOutput {
    pub reflection: String,
    pub dominant_emotion: String,
    pub belief_updates: Vec<String>,
    pub long_term_plan: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionEnvelope {
    pub tasks: Vec<SimplifiedTask>,
    pub reflection: String,
    pub dominant_emotion: String,
    pub belief_updates: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationContextInput {
    pub conversation_id: u64,
    pub opening_reason: String,
    pub current_area: String,
    pub current_building: Option<String>,
    pub current_room: Option<String>,
    pub current_room_place_id: Option<String>,
    pub participant_ids: Vec<u64>,
    pub participant_names: Vec<String>,
    pub group_size: usize,
    pub max_turns: u32,
    pub turn_count: u32,
    pub turns_remaining: u32,
    pub conversation_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationObservedAgentInput {
    pub id: u64,
    pub name: String,
    pub role: String,
    pub state: AgentState,
    pub relation: AgentRelation,
    pub perceived_status: String,
    pub visible_equipment_summary: String,
    pub psychological_summary: PsychologicalContextInput,
    pub distance_tiles: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationalHistoryInput {
    pub relationship_summary: String,
    pub shared_history: Vec<String>,
    pub open_promises: Vec<String>,
    pub unresolved_offenses: Vec<String>,
    pub recent_favors: Vec<String>,
    pub trust_trajectory: String,
    pub resentment_trajectory: String,
    pub social_imbalance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurnInput {
    pub speaker_id: u64,
    pub speaker_name: String,
    pub speaker_role: String,
    pub speaker_state: AgentState,
    pub time_context: TimeContextInput,
    pub world_places: Vec<WorldPlaceInput>,
    pub speaker_profile_summary: Vec<String>,
    pub speaker_psychology: PsychologicalContextInput,
    pub speaker_equipment_summary: String,
    pub speaker_prestige_summary: String,
    pub speaker_prestige_score: i32,
    pub reactive_stance: String,
    pub status_pressure_summary: String,
    pub revenge_summary: String,
    pub public_shame_summary: String,
    pub authority_posture_summary: String,
    pub prestige_gap_summary: String,
    pub humiliation_risk_summary: String,
    pub deference_or_revenge_summary: String,
    pub audience_summary: String,
    pub audience_size: usize,
    pub is_group_conversation: bool,
    pub public_pressure_summary: String,
    pub participants: Vec<ConversationObservedAgentInput>,
    pub context: ConversationContextInput,
    pub turn_trigger: String,
    pub relational_context: RelationalHistoryInput,
    pub recent_memories: Vec<RelevantMemoryInput>,
    pub recent_turns: Vec<String>,
    pub recent_speakers: Vec<u64>,
    pub recent_targets: Vec<u64>,
    pub chaos_pressure: u32,
    pub personality_traits: Vec<String>,
    pub trauma_traits: Vec<String>,
    #[serde(default)]
    pub known_secrets: Vec<String>,
    #[serde(default)]
    pub information_context: InformationContextInput,
    #[serde(default)]
    pub cultural_context: CulturalContextInput,
    #[serde(default)]
    pub body_parts: Vec<crate::world_model::BodyPartState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EconomicTransfer {
    pub recipient_id: Option<u64>,
    pub amount: i32,
    pub resource_id: String,       // "moedas" ou "graos"
    pub use_public_treasury: bool, // Corrupção
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RevealedSecret {
    pub secret_id: u64,
    pub recipient_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProposedPromise {
    pub recipient_id: u64,
    pub condition: PromiseCondition,
    pub duration_ticks: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProposedRumor {
    pub target_agent_id: u64,
    pub topic: String,
    pub claim: Option<String>,
    pub is_true: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProposedStoryShare {
    pub story_id: Option<u64>,
    pub title: Option<String>,
    pub version: String,
    pub kind: Option<String>,
    pub tone: Option<String>,
    pub moral: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProposedEscrow {
    pub target_agent_id: u64,
    pub resource_id: String,
    pub amount: i32,
    pub condition_secret_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProposedMeeting {
    pub invitee_ids: Vec<u64>,
    pub place_id: String,
    pub scheduled_day: u32,
    pub scheduled_time: String,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MeetingResponse {
    pub meeting_id: u64,
    pub accept: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurnOutput {
    pub utterance: String,
    pub speech_act: String,
    pub emotion: String,
    pub intent_to_continue: bool,
    pub belief_updates: Vec<String>,
    pub relation_delta_hint: RelationDelta,
    pub tone: Option<String>,
    pub risk_shift: Option<i32>,
    #[serde(default)]
    pub addressed_agent_ids: Vec<u64>,
    #[serde(default)]
    pub economic_transfer: Option<EconomicTransfer>,
    #[serde(default)]
    pub revealed_secret: Option<RevealedSecret>,
    #[serde(default)]
    pub make_promise: Option<ProposedPromise>,
    #[serde(default)]
    pub spread_rumor: Option<ProposedRumor>,
    #[serde(default)]
    pub shared_story: Option<ProposedStoryShare>,
    #[serde(default)]
    pub escrow_deposit: Option<ProposedEscrow>,
    #[serde(default)]
    pub propose_meeting: Option<ProposedMeeting>,
    #[serde(default)]
    pub meeting_response: Option<MeetingResponse>,
}

pub fn retrieve_relevant_memories(
    memories: &[AgentMemory],
    state: &AgentState,
    recent_events: &[WorldEvent],
    limit: usize,
) -> Vec<RelevantMemoryInput> {
    let mut keywords = vec![state.current_focus.to_lowercase()];
    keywords.extend(state.active_goals.iter().map(|goal| goal.to_lowercase()));
    for event in recent_events.iter().rev().take(6) {
        keywords.extend(event.impact_tags.iter().map(|tag| tag.to_lowercase()));
    }

    let mut scored: Vec<(i32, &AgentMemory)> = memories
        .iter()
        .map(|memory| {
            let overlap = memory
                .tags
                .iter()
                .filter(|tag| {
                    keywords
                        .iter()
                        .any(|keyword| keyword.contains(&tag.to_lowercase()))
                })
                .count() as i32;
            let recency = memory.day as i32 * 10 + memory.tick as i32;
            (
                memory.emotional_weight + overlap * 12 + recency / 50,
                memory,
            )
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, memory)| RelevantMemoryInput {
            id: memory.id,
            summary: memory.summary.clone(),
            weight: memory.emotional_weight,
            tags: memory.tags.clone(),
        })
        .collect()
}

pub fn retrieve_relational_memories(
    memories: &[AgentMemory],
    other_agent_id: u64,
    limit: usize,
) -> Vec<RelevantMemoryInput> {
    let mut scored = memories
        .iter()
        .filter(|memory| memory.about.contains(&other_agent_id))
        .map(|memory| {
            let relation_bonus = memory
                .tags
                .iter()
                .filter(|tag| {
                    matches!(
                        tag.as_str(),
                        "social" | "promessa" | "ofensa" | "favor" | "ressentimento" | "divida"
                    )
                })
                .count() as i32
                * 10;
            let recency = memory.day as i32 * 10 + memory.tick as i32;
            (
                memory.emotional_weight + relation_bonus + recency / 50,
                memory,
            )
        })
        .collect::<Vec<_>>();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, memory)| RelevantMemoryInput {
            id: memory.id,
            summary: memory.summary.clone(),
            weight: memory.emotional_weight,
            tags: memory.tags.clone(),
        })
        .collect()
}

pub fn parse_decision_json(payload: &str) -> Result<DecisionEnvelope> {
    parse_decision_json_with_notes(payload).map(|(envelope, _)| envelope)
}

pub(crate) fn parse_decision_json_with_notes(
    payload: &str,
) -> Result<(DecisionEnvelope, Vec<String>)> {
    let mut notes = Vec::new();
    let root = extract_first_json_object(payload, "decision payload", &mut notes)?;
    let object = root
        .as_object()
        .ok_or_else(|| anyhow!("decision payload root must be a JSON object"))?;
    let reflection = required_string_field(object, "reflection")?;
    let dominant_emotion =
        required_string_field(object, "dominant_emotion").unwrap_or_else(|_| "contido".to_string());
    let belief_updates = parse_belief_updates(object.get("belief_updates"), &mut notes);

    let tasks_val = object
        .get("tasks")
        .ok_or_else(|| anyhow!("decision payload must contain a 'tasks' array"))?;
    let tasks_arr = tasks_val
        .as_array()
        .ok_or_else(|| anyhow!("'tasks' must be a JSON array"))?;

    let mut tasks = Vec::new();
    for (i, item) in tasks_arr.iter().enumerate() {
        let task_obj = item
            .as_object()
            .ok_or_else(|| anyhow!("task at index {i} must be a JSON object"))?;
        let kind = parse_strict_intent_kind(task_obj.get("kind"))?;
        let target_agent = parse_target_agent(task_obj.get("target_agent"), &mut notes);
        let target_semantic = parse_optional_nonempty_string(
            task_obj.get("target_semantic"),
            "target_semantic",
            &mut notes,
        );
        let social_move = if kind == IntentKind::Socializar {
            Some(parse_social_move(task_obj.get("social_move"), &mut notes))
        } else {
            None
        };
        tasks.push(SimplifiedTask {
            kind,
            target_semantic,
            target_agent,
            social_move,
        });
    }

    Ok((
        DecisionEnvelope {
            tasks,
            reflection,
            dominant_emotion,
            belief_updates,
        },
        notes,
    ))
}

pub fn parse_action_planner_output(raw: &str) -> Vec<SimplifiedTask> {
    let mut tasks = Vec::new();
    let mut tokens = Vec::new();
    let mut current_token = String::new();
    let mut paren_depth = 0;

    for c in raw.chars() {
        match c {
            '(' => {
                paren_depth += 1;
                current_token.push(c);
            }
            ')' => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
                current_token.push(c);
            }
            ',' if paren_depth == 0 => {
                tokens.push(current_token.trim().to_string());
                current_token.clear();
            }
            _ => {
                current_token.push(c);
            }
        }
    }
    if !current_token.trim().is_empty() {
        tokens.push(current_token.trim().to_string());
    }

    let mut mock_notes = Vec::new();
    for token in tokens {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if let Some(open_paren_idx) = token.find('(') {
            let kind_part = token[..open_paren_idx].trim();
            let val_kind = Value::String(kind_part.to_string());
            let kind = match parse_strict_intent_kind(Some(&val_kind)) {
                Ok(k) => k,
                Err(_) => continue,
            };
            let close_paren_idx = token.rfind(')').unwrap_or(token.len());
            let params_str = &token[open_paren_idx + 1..close_paren_idx];
            let params: Vec<&str> = params_str
                .split(',')
                .map(|p| p.trim_matches(|c| c == '\'' || c == '"' || c == ' '))
                .collect();

            let mut target_agent = None;
            let mut target_semantic = None;
            let mut social_move = None;

            if !params.is_empty() && !params[0].is_empty() {
                if let Ok(agent_id) = params[0].parse::<u64>() {
                    target_agent = Some(agent_id);
                } else {
                    target_semantic = Some(params[0].to_string());
                }
            }

            if params.len() > 1 && !params[1].is_empty() {
                if kind == IntentKind::Socializar {
                    let val_social = Value::String(params[1].to_string());
                    social_move = Some(parse_social_move(Some(&val_social), &mut mock_notes));
                } else {
                    if target_agent.is_some() {
                        target_semantic = Some(params[1].to_string());
                    } else if target_semantic.is_some() {
                        if let Ok(agent_id) = params[1].parse::<u64>() {
                            target_agent = Some(agent_id);
                        }
                    }
                }
            }

            tasks.push(SimplifiedTask {
                kind,
                target_semantic,
                target_agent,
                social_move,
            });
        } else {
            let val_kind = Value::String(token.to_string());
            if let Ok(kind) = parse_strict_intent_kind(Some(&val_kind)) {
                tasks.push(SimplifiedTask {
                    kind,
                    target_semantic: None,
                    target_agent: None,
                    social_move: None,
                });
            }
        }
    }

    tasks
}

pub fn parse_think_maker_json(payload: &str) -> Result<ThinkMakerOutput> {
    let mut notes = Vec::new();
    let root = extract_first_json_object(payload, "think maker payload", &mut notes)?;
    let object = root
        .as_object()
        .ok_or_else(|| anyhow!("think maker payload root must be a JSON object"))?;
    let reflection = required_string_field(object, "reflection")?;
    let dominant_emotion =
        required_string_field(object, "dominant_emotion").unwrap_or_else(|_| "contido".to_string());
    let belief_updates = parse_belief_updates(object.get("belief_updates"), &mut notes);
    let long_term_plan = required_string_field(object, "long_term_plan")?;

    Ok(ThinkMakerOutput {
        reflection,
        dominant_emotion,
        belief_updates,
        long_term_plan,
    })
}

pub fn parse_conversation_turn_json(payload: &str) -> Result<ConversationTurnOutput> {
    parse_conversation_turn_json_with_notes(payload).map(|(output, _)| output)
}

pub(crate) fn parse_conversation_turn_json_with_notes(
    payload: &str,
) -> Result<(ConversationTurnOutput, Vec<String>)> {
    let mut notes = Vec::new();
    let root = extract_first_json_object(payload, "conversation turn payload", &mut notes)?;
    let object = root
        .as_object()
        .ok_or_else(|| anyhow!("conversation turn payload root must be a JSON object"))?;

    let utterance = required_textish_field(object.get("utterance"), "utterance", &mut notes)?;
    let speech_act = required_textish_field(object.get("speech_act"), "speech_act", &mut notes)?;
    let emotion = required_textish_field(object.get("emotion"), "emotion", &mut notes)?;
    let intent_to_continue = parse_boolish_field(
        object.get("intent_to_continue"),
        "intent_to_continue",
        true,
        &mut notes,
    );
    let belief_updates = parse_belief_updates(object.get("belief_updates"), &mut notes);
    let relation_delta_hint =
        parse_relation_delta_hint(object.get("relation_delta_hint"), &mut notes);
    let tone = parse_optional_tone_field(object.get("tone"), &mut notes);
    let risk_shift = Some(parse_risk_shift_field(
        object.get("risk_shift"),
        "risk_shift",
        0,
        &mut notes,
    ));

    let economic_transfer = parse_economic_transfer(object.get("economic_transfer"), &mut notes);
    let revealed_secret = parse_revealed_secret(object.get("revealed_secret"), &mut notes);
    let make_promise = parse_proposed_promise(object.get("make_promise"), &mut notes);
    let spread_rumor = parse_proposed_rumor(object.get("spread_rumor"), &mut notes);
    let shared_story = parse_proposed_story_share(object.get("shared_story"), &mut notes);
    let escrow_deposit = parse_proposed_escrow(object.get("escrow_deposit"), &mut notes);
    let propose_meeting = parse_proposed_meeting(object.get("propose_meeting"), &mut notes);
    let meeting_response = parse_meeting_response(object.get("meeting_response"), &mut notes);

    let addressed_agent_ids = parse_target_agent_ids(object.get("addressed_agent_ids"), &mut notes);

    Ok((
        ConversationTurnOutput {
            utterance,
            speech_act,
            emotion,
            intent_to_continue,
            belief_updates,
            relation_delta_hint,
            tone,
            risk_shift,
            addressed_agent_ids,
            economic_transfer,
            revealed_secret,
            make_promise,
            spread_rumor,
            shared_story,
            escrow_deposit,
            propose_meeting,
            meeting_response,
        },
        notes,
    ))
}

pub fn validate_intent(mut intent: AgentIntent, nearby_ids: &[u64]) -> AgentIntent {
    if intent.kind == IntentKind::Socializar
        && intent.target_agent.is_none()
        && !nearby_ids.is_empty()
    {
        intent.target_agent = nearby_ids.first().copied();
    }
    if let Some(target_agent) = intent.target_agent {
        if !nearby_ids.contains(&target_agent) && intent.kind != IntentKind::Socializar {
            intent.target_agent = None;
        }
    }
    if intent.priority == 0 {
        intent.priority = 1;
    }
    intent
}

fn required_string_field(object: &Map<String, Value>, field: &str) -> Result<String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("decision payload field `{field}` must be a non-empty string"))
}

fn required_textish_field(
    value: Option<&Value>,
    field: &str,
    notes: &mut Vec<String>,
) -> Result<String> {
    let Some(value) = value else {
        return Err(anyhow!(
            "conversation payload field `{field}` must be present and non-empty"
        ));
    };
    match value {
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Err(anyhow!(
                    "conversation payload field `{field}` must be a non-empty string"
                ))
            } else {
                Ok(trimmed.to_string())
            }
        }
        Value::Number(number) => {
            notes.push(format!(
                "{field} numerico convertido para string `{}`",
                number
            ));
            Ok(number.to_string())
        }
        Value::Bool(boolean) => {
            notes.push(format!(
                "{field} booleano convertido para string `{}`",
                boolean
            ));
            Ok(boolean.to_string())
        }
        _ => Err(anyhow!(
            "conversation payload field `{field}` must be string or scalar textual"
        )),
    }
}

fn parse_strict_intent_kind(value: Option<&Value>) -> Result<IntentKind> {
    let raw = value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
        .ok_or_else(|| anyhow!("decision payload field `kind` must be a non-empty string"))?;
    match fold_text(raw).as_str() {
        "trabalhar" => Ok(IntentKind::Trabalhar),
        "descansar" => Ok(IntentKind::Descansar),
        "comer" => Ok(IntentKind::Comer),
        "socializar" => Ok(IntentKind::Socializar),
        "refletir" => Ok(IntentKind::Refletir),
        "andar" => Ok(IntentKind::Andar),
        "comprar" => Ok(IntentKind::Comprar),
        "transportar" => Ok(IntentKind::Transportar),
        "vender" => Ok(IntentKind::Vender),
        "receberpagamento" => Ok(IntentKind::ReceberPagamento),
        "construir" => Ok(IntentKind::Construir),
        "agredir" => Ok(IntentKind::Agredir),
        "combater" => Ok(IntentKind::Combater),
        "roubar" => Ok(IntentKind::Roubar),
        "furtar" => Ok(IntentKind::Furtar),
        "fugir" => Ok(IntentKind::Fugir),
        "acusar" => Ok(IntentKind::Acusar),
        "investigar" => Ok(IntentKind::Investigar),
        "prender" => Ok(IntentKind::Prender),
        "punir" => Ok(IntentKind::Punir),
        "apoiar" => Ok(IntentKind::Apoiar),
        "opor" => Ok(IntentKind::Opor),
        "pedirapoio" => Ok(IntentKind::PedirApoio),
        "mediar" => Ok(IntentKind::Mediar),
        "decretar" => Ok(IntentKind::Decretar),
        "jurarlealdade" => Ok(IntentKind::JurarLealdade),
        "romperlealdade" => Ok(IntentKind::RomperLealdade),
        "concedertitulo" => Ok(IntentKind::ConcederTitulo),
        "revogartitulo" => Ok(IntentKind::RevogarTitulo),
        "nomearoficial" => Ok(IntentKind::NomearOficial),
        "exigirtributo" => Ok(IntentKind::ExigirTributo),
        "cobrarcorveia" => Ok(IntentKind::CobrarCorveia),
        "convocarlevy" => Ok(IntentKind::ConvocarLevy),
        "reconhecerherdeiro" => Ok(IntentKind::ReconhecerHerdeiro),
        "apoiarpretendente" => Ok(IntentKind::ApoiarPretendente),
        "usurpar" => Ok(IntentKind::Usurpar),
        "reivindicarterritorio" => Ok(IntentKind::ReivindicarTerritorio),
        "negociarsuserania" => Ok(IntentKind::NegociarSuserania),
        "esconder" => Ok(IntentKind::Esconder),
        _ => Err(anyhow!(
            "decision payload field `kind` has invalid value `{raw}`"
        )),
    }
}

fn parse_target_agent_ids(value: Option<&Value>, notes: &mut Vec<String>) -> Vec<u64> {
    let Some(value) = value else {
        return Vec::new();
    };
    if let Some(array) = value.as_array() {
        let mut ids = Vec::new();
        for entry in array {
            if let Some(id) = parse_target_agent(Some(entry), notes) {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        return ids;
    }
    parse_target_agent(Some(value), notes).into_iter().collect()
}

fn parse_target_agent(value: Option<&Value>, notes: &mut Vec<String>) -> Option<u64> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::Number(number)) => number.as_u64(),
        Some(Value::String(raw)) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                notes.push("target_agent vazio convertido para None".to_string());
                None
            } else if let Ok(parsed) = trimmed.parse::<u64>() {
                notes.push(format!(
                    "target_agent string `{trimmed}` convertido para id numerico"
                ));
                Some(parsed)
            } else {
                notes.push(format!(
                    "target_agent livre `{trimmed}` nao foi resolvido para id e virou None"
                ));
                None
            }
        }
        Some(other) => {
            notes.push(format!(
                "target_agent com tipo inesperado `{}` virou None",
                other
            ));
            None
        }
    }
}

fn parse_optional_nonempty_string(
    value: Option<&Value>,
    field: &str,
    notes: &mut Vec<String>,
) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(raw)) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                notes.push(format!("{field} vazio convertido para None"));
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Some(other) => {
            notes.push(format!(
                "{field} com tipo inesperado `{}` virou None",
                other
            ));
            None
        }
    }
}

fn parse_optional_tone_field(value: Option<&Value>, notes: &mut Vec<String>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(raw)) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                notes.push("tone vazio convertido para None".to_string());
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Some(other) => {
            notes.push(format!("tone com tipo inesperado `{}` virou None", other));
            None
        }
    }
}

fn parse_belief_updates(value: Option<&Value>, notes: &mut Vec<String>) -> Vec<String> {
    match value {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::String(raw)) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                notes.push("belief_updates string unica convertida para vetor".to_string());
                vec![trimmed.to_string()]
            }
        }
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::trim))
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        Some(other) => {
            notes.push(format!(
                "belief_updates com tipo inesperado `{}` virou vetor vazio",
                other
            ));
            Vec::new()
        }
    }
}

fn parse_boolish_field(
    value: Option<&Value>,
    field: &str,
    default: bool,
    notes: &mut Vec<String>,
) -> bool {
    let Some(value) = value else {
        notes.push(format!("{field} ausente; usando default {default}"));
        return default;
    };
    match value {
        Value::Bool(boolean) => *boolean,
        Value::Number(number) => {
            if let Some(float) = number.as_f64() {
                let normalized = float >= 0.5;
                notes.push(format!(
                    "{field} numerico `{}` normalizado para `{}`",
                    float, normalized
                ));
                normalized
            } else {
                notes.push(format!(
                    "{field} numerico invalido; usando default {default}"
                ));
                default
            }
        }
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                notes.push(format!("{field} vazio; usando default {default}"));
                return default;
            }
            if let Ok(float) = trimmed.parse::<f64>() {
                let normalized = float >= 0.5;
                notes.push(format!(
                    "{field} string numerica `{trimmed}` normalizada para `{normalized}`"
                ));
                return normalized;
            }
            let folded = semantic_fold_text(trimmed);
            let normalized = match folded.as_str() {
                "true" | "sim" | "yes" | "continuar" | "continue" => Some(true),
                "false" | "nao" | "no" | "encerrar" | "parar" | "stop" => Some(false),
                _ => None,
            };
            match normalized {
                Some(boolean) => {
                    notes.push(format!(
                        "{field} string `{trimmed}` normalizada para `{boolean}`"
                    ));
                    boolean
                }
                None => {
                    notes.push(format!(
                        "{field} invalido `{trimmed}`; usando default {default}"
                    ));
                    default
                }
            }
        }
        other => {
            notes.push(format!(
                "{field} com tipo inesperado `{}`; usando default {default}",
                other
            ));
            default
        }
    }
}

fn parse_i32ish_field(
    value: Option<&Value>,
    field: &str,
    default: i32,
    notes: &mut Vec<String>,
) -> i32 {
    let Some(value) = value else {
        return default;
    };
    match value {
        Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                integer.clamp(i32::MIN as i64, i32::MAX as i64) as i32
            } else if let Some(float) = number.as_f64() {
                float.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
            } else {
                notes.push(format!(
                    "{field} numerico invalido; usando default {default}"
                ));
                default
            }
        }
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return default;
            }
            if let Ok(integer) = trimmed.parse::<i32>() {
                notes.push(format!(
                    "{field} string numerica `{trimmed}` convertida para numero"
                ));
                return integer;
            }
            if let Ok(float) = trimmed.parse::<f64>() {
                let normalized = float.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32;
                notes.push(format!(
                    "{field} string float `{trimmed}` convertida para inteiro `{normalized}`"
                ));
                return normalized;
            }
            notes.push(format!(
                "{field} string invalida `{trimmed}`; usando default {default}"
            ));
            default
        }
        other => {
            notes.push(format!(
                "{field} com tipo inesperado `{}`; usando default {default}",
                other
            ));
            default
        }
    }
}

fn parse_risk_shift_field(
    value: Option<&Value>,
    field: &str,
    default: i32,
    notes: &mut Vec<String>,
) -> i32 {
    let Some(value) = value else {
        notes.push(format!("{field} ausente; usando default {default}"));
        return default;
    };
    match value {
        Value::Number(_) => parse_i32ish_field(Some(value), field, default, notes).clamp(-5, 5),
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                notes.push(format!("{field} vazio; usando default {default}"));
                return default;
            }
            if let Ok(integer) = trimmed.parse::<i32>() {
                notes.push(format!(
                    "{field} string numerica `{trimmed}` convertida para numero"
                ));
                return integer.clamp(-5, 5);
            }
            if let Ok(float) = trimmed.parse::<f64>() {
                let normalized = float.round() as i32;
                notes.push(format!(
                    "{field} string float `{trimmed}` convertida para inteiro `{normalized}`"
                ));
                return normalized.clamp(-5, 5);
            }
            let normalized = interpret_risk_shift_text(trimmed);
            match normalized {
                Some(value) => {
                    notes.push(format!(
                        "{field} qualitativo `{trimmed}` normalizado para {value}"
                    ));
                    value.clamp(-5, 5)
                }
                None => {
                    notes.push(format!(
                        "{field} invalido `{trimmed}`; usando default {default}"
                    ));
                    default
                }
            }
        }
        other => {
            notes.push(format!(
                "{field} com tipo inesperado `{}`; usando default {default}",
                other
            ));
            default
        }
    }
}

fn parse_relation_delta_hint(value: Option<&Value>, notes: &mut Vec<String>) -> RelationDelta {
    let Some(value) = value else {
        notes.push("relation_delta_hint ausente; usando delta neutro".to_string());
        return RelationDelta::default();
    };
    match value {
        Value::Object(object) => RelationDelta {
            trust: parse_i32ish_field(object.get("trust"), "trust", 0, notes),
            friendship: parse_i32ish_field(object.get("friendship"), "friendship", 0, notes),
            resentment: parse_i32ish_field(object.get("resentment"), "resentment", 0, notes),
            attraction: parse_i32ish_field(object.get("attraction"), "attraction", 0, notes),
            moral_debt: parse_i32ish_field(object.get("moral_debt"), "moral_debt", 0, notes),
            reputation: parse_i32ish_field(object.get("reputation"), "reputation", 0, notes),
        },
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                notes.push("relation_delta_hint vazio; usando delta neutro".to_string());
                RelationDelta::default()
            } else {
                let normalized = infer_relation_delta_from_text(trimmed);
                if relation_delta_is_neutral(&normalized) {
                    notes.push(format!(
                        "relation_delta_hint textual `{trimmed}` virou delta neutro"
                    ));
                } else {
                    notes.push(format!(
                        "relation_delta_hint textual `{trimmed}` normalizado para delta estruturado"
                    ));
                }
                normalized
            }
        }
        other => {
            notes.push(format!(
                "relation_delta_hint com tipo inesperado `{}` virou delta neutro",
                other
            ));
            RelationDelta::default()
        }
    }
}

fn parse_social_move(value: Option<&Value>, notes: &mut Vec<String>) -> SocialMove {
    let default = SocialMove::Chat;
    let Some(value) = value else {
        notes.push("social_move ausente em Socializar; usando Chat".to_string());
        return default;
    };
    let Some(raw) = value.as_str() else {
        notes.push(format!(
            "social_move com tipo inesperado `{}` em Socializar; usando Chat",
            value
        ));
        return default;
    };
    let folded = fold_text(raw);
    let mapped = match folded.as_str() {
        "chat"
        | "conversa"
        | "conversar"
        | "baterpapo"
        | "casual"
        | "none"
        | "nenhum"
        | "neutro"
        | "neutral"
        | "manter"
        | "isolado"
        | "isolamentotemporario"
        | "prudencia"
        | "retirada"
        | "solitary"
        | "manutencao" => Some(SocialMove::Chat),
        "fofoca" | "fofocar" | "rumor" | "rumores" => Some(SocialMove::Gossip),
        "contarhistoria" | "historia" | "lenda" | "contarlenda" | "narrar" | "memoriacultural" => {
            Some(SocialMove::TellStory)
        }
        "promessa" | "prometer" | "compromisso" => Some(SocialMove::Promise),
        "ofensa" | "ofender" | "hostil" | "pressionar" | "cobrar" => Some(SocialMove::Offend),
        "reconciliar" | "reconciliacao" | "desculpa" | "fazeraspazes" => {
            Some(SocialMove::Reconcile)
        }
        "favor" | "ajudar" | "aproximar" | "aproximacao" | "amistoso" | "amigavel" => {
            Some(SocialMove::Favor)
        }
        _ => None,
    };
    match mapped {
        Some(move_kind) => {
            if move_kind.as_str() != raw.trim().to_lowercase() {
                notes.push(format!(
                    "social_move `{}` normalizado para `{}`",
                    raw,
                    move_kind.as_str()
                ));
            }
            move_kind
        }
        None => {
            notes.push(format!(
                "social_move invalido `{}` em Socializar; usando Chat",
                raw
            ));
            default
        }
    }
}

fn extract_first_json_object(payload: &str, label: &str, notes: &mut Vec<String>) -> Result<Value> {
    let stripped = strip_markdown_fence(payload, notes);
    let trimmed = stripped.trim();
    let start = trimmed
        .find('{')
        .ok_or_else(|| anyhow!("{label} did not contain a JSON object"))?;
    if !trimmed[..start].trim().is_empty() {
        notes.push(format!(
            "{label}: texto antes do primeiro JSON foi ignorado"
        ));
    }
    let candidate = &trimmed[start..];
    let mut stream = serde_json::Deserializer::from_str(candidate).into_iter::<Value>();
    let root = stream
        .next()
        .transpose()
        .context(format!("failed to parse {label}"))?
        .ok_or_else(|| anyhow!("{label} did not contain a JSON value"))?;
    if !candidate[stream.byte_offset()..].trim().is_empty() {
        notes.push(format!("{label}: texto apos o primeiro JSON foi ignorado"));
    }
    Ok(root)
}

fn strip_markdown_fence<'a>(payload: &'a str, notes: &mut Vec<String>) -> &'a str {
    let trimmed = payload.trim();
    if !trimmed.starts_with("```") {
        return trimmed;
    }
    let Some(first_newline) = trimmed.find('\n') else {
        return trimmed;
    };
    let body = &trimmed[first_newline + 1..];
    if let Some(last_fence) = body.rfind("```") {
        notes.push("bloco markdown ```json removido antes do parse".to_string());
        body[..last_fence].trim()
    } else {
        notes.push("bloco markdown sem fence final; corpo restante usado no parse".to_string());
        body.trim()
    }
}

fn interpret_risk_shift_text(raw: &str) -> Option<i32> {
    let folded = semantic_fold_text(raw);
    let mentions_tension = contains_any(&folded, &["tensao", "risco", "conflito"]);
    let mentions_reduction = contains_any(
        &folded,
        &[
            "reducao", "reduzir", "diminu", "menos", "alivia", "harmonia",
        ],
    );
    let mentions_increase = contains_any(
        &folded,
        &["aumento", "aumentar", "mais", "escalada", "agrava"],
    );
    let mentions_minimum = contains_any(&folded, &["minimo", "minima"]);
    let mentions_light = contains_any(&folded, &["leve", "ligeir"]);

    if mentions_minimum {
        return Some(-1);
    }
    if mentions_reduction && mentions_light {
        return Some(-2);
    }
    if mentions_increase && mentions_tension {
        return Some(2);
    }
    if mentions_increase {
        return Some(1);
    }
    if mentions_reduction || contains_any(&folded, &["prudente", "seguro", "estavel"]) {
        return Some(-1);
    }
    None
}

fn infer_relation_delta_from_text(raw: &str) -> RelationDelta {
    let folded = semantic_fold_text(raw);
    let amount = if contains_any(&folded, &["forte", "grande", "muito"]) {
        2
    } else {
        1
    };
    let mut delta = RelationDelta::default();
    let mut matched = false;

    if contains_any(&folded, &["confianca", "trust", "respeito"]) {
        matched = true;
        delta.trust += if relation_mentions_reduction(&folded, "confianca") {
            -amount
        } else {
            amount
        };
    }

    if contains_any(
        &folded,
        &[
            "amizade",
            "aproximacao",
            "aproximar",
            "aproxima",
            "confidencia",
            "harmonia",
            "cordial",
        ],
    ) {
        matched = true;
        delta.friendship += if relation_mentions_reduction(&folded, "amizade") {
            -amount
        } else {
            amount
        };
        if contains_any(
            &folded,
            &["aproximacao", "aproximar", "aproxima", "confidencia"],
        ) {
            delta.trust += amount;
        }
    }

    if contains_any(&folded, &["ressentimento"]) {
        matched = true;
        delta.resentment += if relation_mentions_reduction(&folded, "ressentimento") {
            -amount
        } else {
            amount
        };
    }

    if contains_any(&folded, &["tensao", "conflito", "hostilidade", "ofensa"]) {
        matched = true;
        let change = if relation_mentions_reduction(&folded, "tensao") {
            -amount
        } else {
            amount
        };
        delta.resentment += change;
        if change > 0 {
            delta.trust -= 1;
            delta.friendship -= 1;
        }
    }

    if contains_any(&folded, &["atracao"]) {
        matched = true;
        delta.attraction += if relation_mentions_reduction(&folded, "atracao") {
            -amount
        } else {
            amount
        };
    }

    if contains_any(&folded, &["divida", "obrigacao", "favor"]) {
        matched = true;
        delta.moral_debt += if relation_mentions_reduction(&folded, "divida") {
            -amount
        } else {
            amount
        };
    }

    if contains_any(&folded, &["reputacao"]) {
        matched = true;
        delta.reputation += if relation_mentions_reduction(&folded, "reputacao") {
            -amount
        } else {
            amount
        };
    }

    if !matched
        && contains_any(
            &folded,
            &[
                "aproxim",
                "confidenc",
                "gentil",
                "cordial",
                "prestativ",
                "apoio",
            ],
        )
    {
        delta.trust += amount;
        delta.friendship += amount;
        matched = true;
    }

    if matched {
        delta
    } else {
        RelationDelta::default()
    }
}

fn relation_mentions_reduction(folded: &str, axis: &str) -> bool {
    contains_any(
        folded,
        &[
            &format!("reducaode{axis}"),
            &format!("reducaoda{axis}"),
            &format!("menos{axis}"),
            &format!("diminuicaode{axis}"),
            &format!("queda{axis}"),
        ],
    ) || (matches!(axis, "tensao" | "ressentimento" | "conflito" | "risco")
        && contains_any(folded, &["reducao", "diminu", "menos", "alivia"])
        && contains_any(folded, &["tensao", "ressentimento", "conflito", "risco"]))
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn relation_delta_is_neutral(delta: &RelationDelta) -> bool {
    delta.trust == 0
        && delta.friendship == 0
        && delta.resentment == 0
        && delta.attraction == 0
        && delta.moral_debt == 0
        && delta.reputation == 0
}

fn semantic_fold_text(raw: &str) -> String {
    raw.to_lowercase()
        .chars()
        .filter_map(|ch| match ch {
            'á' | 'à' | 'â' | 'ã' | 'ä' => Some('a'),
            'é' | 'è' | 'ê' | 'ẽ' | 'ë' => Some('e'),
            'í' | 'ì' | 'î' | 'ĩ' | 'ï' => Some('i'),
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => Some('o'),
            'ú' | 'ù' | 'û' | 'ũ' | 'ü' => Some('u'),
            'ç' => Some('c'),
            ch if ch.is_ascii_alphanumeric() => Some(ch),
            ch if ch.is_whitespace() || matches!(ch, '_' | '-' | '/') => None,
            _ => None,
        })
        .collect()
}

fn parse_economic_transfer(
    value: Option<&Value>,
    notes: &mut Vec<String>,
) -> Option<EconomicTransfer> {
    let val = value?;
    if val.is_null() {
        return None;
    }
    let obj = val.as_object()?;
    let recipient_id = parse_target_agent(obj.get("recipient_id"), notes);
    let amount = obj.get("amount").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let resource_id = obj
        .get("resource_id")
        .and_then(|v| v.as_str())
        .unwrap_or("moedas")
        .to_string();
    let use_public_treasury = obj
        .get("use_public_treasury")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Some(EconomicTransfer {
        recipient_id,
        amount,
        resource_id,
        use_public_treasury,
    })
}

fn parse_revealed_secret(value: Option<&Value>, notes: &mut Vec<String>) -> Option<RevealedSecret> {
    let val = value?;
    if val.is_null() {
        return None;
    }
    let obj = val.as_object()?;
    let secret_id = obj.get("secret_id").and_then(|v| v.as_u64())?;
    let recipient_id = obj.get("recipient_id").and_then(|v| v.as_u64())?;
    Some(RevealedSecret {
        secret_id,
        recipient_id,
    })
}

fn fold_text(raw: &str) -> String {
    raw.chars()
        .filter_map(|ch| match ch {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => Some('a'),
            'é' | 'è' | 'ê' | 'ẽ' | 'ë' | 'É' | 'È' | 'Ê' | 'Ẽ' | 'Ë' => Some('e'),
            'í' | 'ì' | 'î' | 'ĩ' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ĩ' | 'Ï' => Some('i'),
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => Some('o'),
            'ú' | 'ù' | 'û' | 'ũ' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ũ' | 'Ü' => Some('u'),
            'ç' | 'Ç' => Some('c'),
            ch if ch.is_ascii_alphanumeric() => Some(ch.to_ascii_lowercase()),
            ch if ch.is_whitespace() || matches!(ch, '_' | '-' | '/') => None,
            _ => None,
        })
        .collect()
}

fn parse_proposed_promise(
    value: Option<&Value>,
    notes: &mut Vec<String>,
) -> Option<ProposedPromise> {
    let val = value?;
    if val.is_null() {
        return None;
    }
    let obj = val.as_object()?;
    let recipient_id = parse_target_agent(obj.get("recipient_id"), notes)?;
    let condition_type = obj
        .get("condition_type")
        .and_then(|v| v.as_str())?
        .to_string();
    let resource_id = obj
        .get("resource_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let amount = obj.get("amount").and_then(|v| v.as_i64()).map(|v| v as i32);
    let policy_domain = obj
        .get("policy_domain")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let policy_value = obj
        .get("policy_value")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let secret_id = obj.get("secret_id").and_then(|v| v.as_u64());
    let duration_ticks = obj
        .get("duration_ticks")
        .and_then(|v| v.as_u64())
        .unwrap_or(24) as u32;

    let condition = match condition_type.as_str() {
        "DeliverResource" => PromiseCondition::DeliverResource {
            resource_id: resource_id.unwrap_or_else(|| "moedas".to_string()),
            amount: amount.unwrap_or(0),
        },
        "VoteForPolicy" => PromiseCondition::VoteForPolicy {
            domain: policy_domain.unwrap_or_else(|| "taxa_imposto".to_string()),
            value: policy_value.unwrap_or_else(|| "10".to_string()),
        },
        "KeepSecret" => PromiseCondition::KeepSecret {
            secret_id: secret_id.unwrap_or(0),
        },
        _ => return None,
    };

    Some(ProposedPromise {
        recipient_id,
        condition,
        duration_ticks,
    })
}

fn parse_proposed_rumor(value: Option<&Value>, notes: &mut Vec<String>) -> Option<ProposedRumor> {
    let val = value?;
    if val.is_null() {
        return None;
    }
    let obj = val.as_object()?;
    let target_agent_id = parse_target_agent(obj.get("target_agent_id"), notes)?;
    let topic = obj
        .get("topic")
        .and_then(|v| v.as_str())
        .unwrap_or("assunto_geral")
        .to_string();
    let claim = obj
        .get("claim")
        .and_then(|v| v.as_str())
        .filter(|entry| !entry.trim().is_empty())
        .map(|entry| entry.trim().to_string());
    let is_true = obj.get("is_true").and_then(|v| v.as_bool()).unwrap_or(true);
    Some(ProposedRumor {
        target_agent_id,
        topic,
        claim,
        is_true,
    })
}

fn parse_proposed_story_share(
    value: Option<&Value>,
    notes: &mut Vec<String>,
) -> Option<ProposedStoryShare> {
    let val = value?;
    if val.is_null() {
        return None;
    }
    let obj = match val.as_object() {
        Some(obj) => obj,
        None => {
            notes.push("shared_story ignorado: valor nao e objeto".to_string());
            return None;
        }
    };
    let story_id = parse_target_agent(obj.get("story_id"), notes);
    let version = obj
        .get("version")
        .or_else(|| obj.get("short_version"))
        .or_else(|| obj.get("claim"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToOwned::to_owned)?;
    let title = obj
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToOwned::to_owned);
    let kind = obj
        .get("kind")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToOwned::to_owned);
    let tone = obj
        .get("tone")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToOwned::to_owned);
    let moral = obj
        .get("moral")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToOwned::to_owned);
    let tags = obj
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .take(6)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(ProposedStoryShare {
        story_id,
        title,
        version,
        kind,
        tone,
        moral,
        tags,
    })
}

fn parse_proposed_escrow(value: Option<&Value>, notes: &mut Vec<String>) -> Option<ProposedEscrow> {
    let val = value?;
    if val.is_null() {
        return None;
    }
    let obj = val.as_object()?;
    let target_agent_id = parse_target_agent(obj.get("target_agent_id"), notes)?;
    let resource_id = obj
        .get("resource_id")
        .and_then(|v| v.as_str())
        .unwrap_or("moedas")
        .to_string();
    let amount = obj.get("amount").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let condition_secret_id = obj
        .get("condition_secret_id")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    Some(ProposedEscrow {
        target_agent_id,
        resource_id,
        amount,
        condition_secret_id,
    })
}

fn parse_proposed_meeting(
    value: Option<&Value>,
    notes: &mut Vec<String>,
) -> Option<ProposedMeeting> {
    let val = value?;
    if val.is_null() {
        return None;
    }
    let obj = match val.as_object() {
        Some(obj) => obj,
        None => {
            notes.push("propose_meeting ignorado: valor nao e objeto".to_string());
            return None;
        }
    };
    let invitee_ids = parse_target_agent_ids(obj.get("invitee_ids"), notes);
    if invitee_ids.is_empty() {
        notes.push("propose_meeting ignorado: invitee_ids vazio ou invalido".to_string());
        return None;
    }
    let place_id = obj
        .get("place_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToOwned::to_owned)?;
    let scheduled_day = obj
        .get("scheduled_day")
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|raw| raw.trim().parse::<u64>().ok()))
        })
        .unwrap_or(0) as u32;
    let scheduled_time = obj
        .get("scheduled_time")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToOwned::to_owned)?;
    let purpose = obj
        .get("purpose")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .unwrap_or("encontro")
        .to_string();
    Some(ProposedMeeting {
        invitee_ids,
        place_id,
        scheduled_day,
        scheduled_time,
        purpose,
    })
}

fn parse_meeting_response(
    value: Option<&Value>,
    notes: &mut Vec<String>,
) -> Option<MeetingResponse> {
    let val = value?;
    if val.is_null() {
        return None;
    }
    let obj = match val.as_object() {
        Some(obj) => obj,
        None => {
            notes.push("meeting_response ignorado: valor nao e objeto".to_string());
            return None;
        }
    };
    let meeting_id = obj.get("meeting_id").and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_str().and_then(|raw| raw.trim().parse::<u64>().ok()))
    })?;
    let accept = parse_boolish_field(obj.get("accept"), "meeting_response.accept", false, notes);
    let reason = obj
        .get("reason")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .unwrap_or(if accept { "aceito" } else { "recusado" })
        .to_string();
    Some(MeetingResponse {
        meeting_id,
        accept,
        reason,
    })
}
