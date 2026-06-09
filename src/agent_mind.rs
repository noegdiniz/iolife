use crate::world_model::{
    AgentIntent, AgentMemory, AgentRelation, AgentState, EconomicTaskKind, EventKind,
    FixtureKind, IntentKind, RelationDelta, ResourceKind, SocialMove, WorldEvent,
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
    pub summary: String,
    pub resource: Option<ResourceKind>,
    pub amount: i32,
    pub unit_price: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicContextInput {
    pub household_name: String,
    pub household_treasury: i32,
    pub pantry: Vec<String>,
    pub pending_salary: i32,
    pub tax_pressure: i32,
    pub work_obligations: Vec<String>,
    pub local_prices: Vec<String>,
    pub base_resource_availability: Vec<String>,
    pub scarcity_signals: Vec<String>,
    pub public_treasury_status: String,
    pub open_tasks: Vec<EconomicOpportunityInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionInput {
    pub actor_id: u64,
    pub actor_name: String,
    pub role: String,
    pub day: u32,
    pub tick: u32,
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
    pub cognition_trigger: String,
    pub context_depth: String,
    pub psychological_context: PsychologicalContextInput,
    pub economic_context: EconomicContextInput,
    pub profile_summary: Vec<String>,
    pub llm_budget_remaining: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionEnvelope {
    pub intent: AgentIntent,
    pub reflection: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationContextInput {
    pub conversation_id: u64,
    pub opening_reason: String,
    pub current_area: String,
    pub current_building: Option<String>,
    pub current_room: Option<String>,
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
    pub psychological_summary: PsychologicalContextInput,
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
    pub speaker_profile_summary: Vec<String>,
    pub speaker_psychology: PsychologicalContextInput,
    pub listener: ConversationObservedAgentInput,
    pub context: ConversationContextInput,
    pub turn_trigger: String,
    pub relational_context: RelationalHistoryInput,
    pub recent_memories: Vec<RelevantMemoryInput>,
    pub recent_turns: Vec<String>,
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

pub(crate) fn parse_decision_json_with_notes(payload: &str) -> Result<(DecisionEnvelope, Vec<String>)> {
    let root: Value =
        serde_json::from_str(payload.trim()).context("failed to parse LLM decision payload")?;
    let object = root
        .as_object()
        .ok_or_else(|| anyhow!("decision payload root must be a JSON object"))?;
    let reflection = required_string_field(object, "reflection")?;
    let intent_object = object
        .get("intent")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("decision payload must contain an object intent"))?;

    let mut notes = Vec::new();
    let kind = parse_strict_intent_kind(intent_object.get("kind"))?;
    let target_agent = parse_target_agent(intent_object.get("target_agent"), &mut notes);
    let target_semantic =
        parse_optional_nonempty_string(intent_object.get("target_semantic"), "target_semantic", &mut notes);
    let justification = required_string_field(intent_object, "justification")?;
    let dominant_emotion = required_string_field(intent_object, "dominant_emotion")?;
    let perceived_risk =
        parse_u8ish_field(intent_object.get("perceived_risk"), "perceived_risk", 20, &mut notes)?
            .clamp(0, 100);
    let belief_updates = parse_belief_updates(intent_object.get("belief_updates"), &mut notes);
    let priority =
        parse_u8ish_field(intent_object.get("priority"), "priority", 5, &mut notes)?.clamp(1, 10);
    let social_move = if kind == IntentKind::Socializar {
        Some(parse_social_move(intent_object.get("social_move"), &mut notes))
    } else {
        if intent_object.get("social_move").is_some() {
            notes.push("social_move ignorado para acao nao social".to_string());
        }
        None
    };

    Ok((
        DecisionEnvelope {
            reflection,
            intent: AgentIntent {
                kind,
                target_agent,
                target_semantic,
                justification,
                dominant_emotion,
                perceived_risk,
                belief_updates,
                priority,
                social_move,
            },
        },
        notes,
    ))
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
        _ => Err(anyhow!("decision payload field `kind` has invalid value `{raw}`")),
    }
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
                notes.push(format!("target_agent string `{trimmed}` convertido para id numerico"));
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
            notes.push(format!("{field} com tipo inesperado `{}` virou None", other));
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
                notes.push(format!("{field} numerico invalido; usando default {default}"));
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
                    notes.push(format!("{field} invalido `{trimmed}`; usando default {default}"));
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

fn parse_u8ish_field(
    value: Option<&Value>,
    field: &str,
    default: u8,
    notes: &mut Vec<String>,
) -> Result<u8> {
    let Some(value) = value else {
        notes.push(format!("{field} ausente; usando default {default}"));
        return Ok(default);
    };
    match value {
        Value::Number(number) => number
            .as_u64()
            .map(|value| value.min(u8::MAX as u64) as u8)
            .ok_or_else(|| anyhow!("decision payload field `{field}` must be a positive integer")),
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                notes.push(format!("{field} vazio; usando default {default}"));
                return Ok(default);
            }
            if let Ok(parsed) = trimmed.parse::<u8>() {
                notes.push(format!("{field} string numerica `{trimmed}` convertida para numero"));
                return Ok(parsed);
            }
            if let Some(mapped) = qualitative_u8_mapping(trimmed, field) {
                notes.push(format!("{field} qualitativo `{trimmed}` normalizado para {mapped}"));
                return Ok(mapped);
            }
            notes.push(format!(
                "{field} invalido `{trimmed}`; usando default {default}"
            ));
            Ok(default)
        }
        other => {
            notes.push(format!(
                "{field} com tipo inesperado `{}`; usando default {default}",
                other
            ));
            Ok(default)
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
                notes.push(format!("{field} numerico invalido; usando default {default}"));
                default
            }
        }
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return default;
            }
            if let Ok(integer) = trimmed.parse::<i32>() {
                notes.push(format!("{field} string numerica `{trimmed}` convertida para numero"));
                return integer;
            }
            if let Ok(float) = trimmed.parse::<f64>() {
                let normalized = float.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32;
                notes.push(format!(
                    "{field} string float `{trimmed}` convertida para inteiro `{normalized}`"
                ));
                return normalized;
            }
            notes.push(format!("{field} string invalida `{trimmed}`; usando default {default}"));
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
                notes.push(format!("{field} string numerica `{trimmed}` convertida para numero"));
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
                    notes.push(format!("{field} qualitativo `{trimmed}` normalizado para {value}"));
                    value.clamp(-5, 5)
                }
                None => {
                    notes.push(format!("{field} invalido `{trimmed}`; usando default {default}"));
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

fn qualitative_u8_mapping(raw: &str, field: &str) -> Option<u8> {
    match (field, fold_text(raw).as_str()) {
        ("perceived_risk", "baixo") | ("perceived_risk", "low") => Some(20),
        ("perceived_risk", "medio") | ("perceived_risk", "medium") => Some(50),
        ("perceived_risk", "alto") | ("perceived_risk", "alta") | ("perceived_risk", "high") => {
            Some(80)
        }
        ("priority", "baixa") | ("priority", "baixo") | ("priority", "low") => Some(3),
        ("priority", "media") | ("priority", "medio") | ("priority", "medium") => Some(5),
        ("priority", "alta") | ("priority", "alto") | ("priority", "high") => Some(8),
        _ => None,
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
        "chat" | "conversa" | "conversar" | "baterpapo" | "casual" | "none" | "nenhum"
        | "neutro" | "neutral" | "manter" | "isolado" | "isolamentotemporario"
        | "prudencia" | "retirada" | "solitary" | "manutencao" => Some(SocialMove::Chat),
        "fofoca" | "fofocar" | "rumor" | "rumores" => Some(SocialMove::Gossip),
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

fn extract_first_json_object(
    payload: &str,
    label: &str,
    notes: &mut Vec<String>,
) -> Result<Value> {
    let stripped = strip_markdown_fence(payload, notes);
    let trimmed = stripped.trim();
    let start = trimmed
        .find('{')
        .ok_or_else(|| anyhow!("{label} did not contain a JSON object"))?;
    if !trimmed[..start].trim().is_empty() {
        notes.push(format!("{label}: texto antes do primeiro JSON foi ignorado"));
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
        &["reducao", "reduzir", "diminu", "menos", "alivia", "harmonia"],
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
        if contains_any(&folded, &["aproximacao", "aproximar", "aproxima", "confidencia"]) {
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
            &["aproxim", "confidenc", "gentil", "cordial", "prestativ", "apoio"],
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
