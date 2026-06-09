use crate::agent_mind::{
    ConversationTurnInput, ConversationTurnOutput, DecisionEnvelope, DecisionInput,
    parse_conversation_turn_json_with_notes, parse_decision_json,
};
use crate::world_model::{AgentIntent, IntentKind, RelationDelta, SocialMove};
use anyhow::{Context, Result as AnyResult};
use reqwest::blocking::Client;
use reqwest::{Error as ReqwestError, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::fmt;
use std::thread;
use std::time::Duration;

const DECISION_PROMPT: &str = r#"Voce decide o proximo passo de um aldeao medieval em um mundo fisico de grid.

Responda com EXATAMENTE UM objeto JSON valido e nada mais.
Nao escreva markdown.
Nao use ```json.
Nao escreva explicacoes antes ou depois.
Nao escreva comentarios.
Nao escreva texto fora do JSON.

Use EXATAMENTE este shape:
{
  "reflection": "string",
  "intent": {
    "kind": "Trabalhar|Descansar|Comer|Socializar|Refletir|Andar|Comprar|Transportar|Vender|ReceberPagamento",
    "target_agent": null ou inteiro,
    "target_semantic": null ou string,
    "justification": "string",
    "dominant_emotion": "string",
    "perceived_risk": inteiro de 0 a 100,
    "belief_updates": ["string"],
    "priority": inteiro de 1 a 10,
    "social_move": null ou "conversar|fofocar|prometer|ofender|reconciliar|ajudar"
  }
}

Regras obrigatorias:
- kind deve usar exatamente um destes nomes: Trabalhar, Descansar, Comer, Socializar, Refletir, Andar, Comprar, Transportar, Vender, ReceberPagamento.
- target_agent deve ser null ou um inteiro. Nunca use nome de agente aqui.
- target_semantic deve ser um alvo semantico, nunca coordenadas.
- perceived_risk deve ser numero inteiro, nunca string como "baixo" ou "medio".
- belief_updates deve ser sempre um array de strings, mesmo com 1 item.
- priority deve ser numero inteiro, nunca string como "alta" ou "media".
- social_move so deve ser preenchido quando kind == Socializar; caso contrario use null.
- Nunca inclua chaves extras.
- Nunca inclua desired_duration.

Se estiver em duvida sobre um campo:
- target_agent = null
- target_semantic = null
- perceived_risk = 20
- belief_updates = []
- priority = 5
- social_move = null"#;

const CONVERSATION_TURN_PROMPT: &str = r#"Voce responde apenas pela mente de UM unico aldeao em uma conversa social medieval.

Responda com EXATAMENTE UM objeto JSON valido e nada mais.
Nao escreva markdown.
Nao use ```json.
Nao escreva explicacoes antes ou depois.
Nao escreva comentarios.
Nao escreva texto fora do JSON.
Nao narre o outro agente.
Nao controle a ordem da conversa.

Use EXATAMENTE este shape:
{
  "utterance": "string",
  "speech_act": "string",
  "emotion": "string",
  "intent_to_continue": true,
  "belief_updates": ["string"],
  "relation_delta_hint": {
    "trust": 0,
    "friendship": 0,
    "resentment": 0,
    "attraction": 0,
    "moral_debt": 0,
    "reputation": 0
  },
  "tone": "string",
  "risk_shift": 0
}

Tipos obrigatorios:
- utterance: string nao vazia
- speech_act: string nao vazia
- emotion: string nao vazia
- intent_to_continue: boolean true ou false, nunca numero e nunca string
- belief_updates: array de strings, mesmo com 1 item
- relation_delta_hint: objeto, nunca string
- trust/friendship/resentment/attraction/moral_debt/reputation: inteiros pequenos entre -2 e 2
- tone: string ou null
- risk_shift: inteiro pequeno entre -5 e 5, nunca string

Regras obrigatorias:
- Se a fala for amigavel, use deltas pequenos e coerentes.
- Se a fala reduzir tensao, diminua resentment em vez de descrever isso em texto.
- Se a fala aproximar os agentes, aumente trust e/ou friendship numericamente.
- Nunca substitua relation_delta_hint por descricao textual.
- Nunca substitua belief_updates por string unica.
- Nunca substitua intent_to_continue por score como 0.8.
- Nunca inclua chaves extras.

Se estiver em duvida sobre um campo:
- intent_to_continue = true
- belief_updates = []
- relation_delta_hint = { "trust": 0, "friendship": 0, "resentment": 0, "attraction": 0, "moral_debt": 0, "reputation": 0 }
- tone = null
- risk_shift = 0

Exemplo valido:
{
  "utterance": "Bom te ver. Podemos falar com calma.",
  "speech_act": "aproximar",
  "emotion": "calmo",
  "intent_to_continue": true,
  "belief_updates": ["Vale manter o tom cordial."],
  "relation_delta_hint": {
    "trust": 1,
    "friendship": 1,
    "resentment": 0,
    "attraction": 0,
    "moral_debt": 0,
    "reputation": 0
  },
  "tone": "cordial",
  "risk_shift": -1
}"#;

pub type LlmResult<T> = std::result::Result<T, LlmError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmError {
    Timeout {
        operation: String,
        attempts: u32,
        message: String,
    },
    Transport {
        operation: String,
        attempts: u32,
        message: String,
    },
    HttpStatus {
        operation: String,
        status: u16,
        attempts: u32,
        message: String,
    },
    Parse {
        operation: String,
        message: String,
    },
    Schema {
        operation: String,
        message: String,
    },
}

impl LlmError {
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Timeout { .. } | Self::Transport { .. } => true,
            Self::HttpStatus { status, .. } => *status == 429 || *status >= 500,
            Self::Parse { .. } | Self::Schema { .. } => false,
        }
    }
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout {
                operation,
                attempts,
                message,
            } => write!(
                f,
                "timeout durante {} apos {} tentativa(s): {}",
                operation, attempts, message
            ),
            Self::Transport {
                operation,
                attempts,
                message,
            } => write!(
                f,
                "erro de transporte durante {} apos {} tentativa(s): {}",
                operation, attempts, message
            ),
            Self::HttpStatus {
                operation,
                status,
                attempts,
                message,
            } => write!(
                f,
                "status HTTP {} durante {} apos {} tentativa(s): {}",
                status, operation, attempts, message
            ),
            Self::Parse { operation, message } => {
                write!(f, "erro de parse durante {}: {}", operation, message)
            }
            Self::Schema { operation, message } => {
                write!(f, "erro de schema durante {}: {}", operation, message)
            }
        }
    }
}

impl std::error::Error for LlmError {}

pub trait LlmAdapter: Send + Sync {
    fn provider_name(&self) -> &str;
    fn evaluate_and_decide(&self, input: &DecisionInput) -> LlmResult<DecisionEnvelope>;
    fn generate_conversation_turn(
        &self,
        input: &ConversationTurnInput,
    ) -> LlmResult<ConversationTurnOutput>;
}

pub fn adapter_from_env() -> Box<dyn LlmAdapter> {
    match OpenAiCompatibleAdapter::from_env() {
        Ok(adapter) => Box::new(adapter),
        Err(_) => Box::new(MockLlmAdapter),
    }
}

pub struct MockLlmAdapter;

impl MockLlmAdapter {
    fn choose_intent(&self, input: &DecisionInput) -> AgentIntent {
        let mut target_agent = input
            .nearby_agents
            .iter()
            .find(|other| other.distance <= 1)
            .map(|other| other.id);
        let mut social_move = Some(SocialMove::Chat);
        let (kind, target_semantic) = if input.economic_context.pending_salary > 0 {
            social_move = None;
            (
                IntentKind::ReceberPagamento,
                Some("receber pagamentos pendentes".to_string()),
            )
        } else if let Some(task) = input
            .economic_context
            .open_tasks
            .iter()
            .find(|task| task.kind == crate::world_model::EconomicTaskKind::Produzir)
        {
            social_move = None;
            (
                IntentKind::Trabalhar,
                Some(task.summary.to_lowercase()),
            )
        } else if input
            .economic_context
            .open_tasks
            .iter()
            .any(|task| task.kind == crate::world_model::EconomicTaskKind::Comprar)
            && input.state.hunger >= 50
        {
            social_move = None;
            (IntentKind::Comprar, Some("comprar comida para o lar".to_string()))
        } else if input
            .economic_context
            .open_tasks
            .iter()
            .any(|task| task.kind == crate::world_model::EconomicTaskKind::Transportar)
        {
            social_move = None;
            (
                IntentKind::Transportar,
                Some("transportar insumos ou producao".to_string()),
            )
        } else if input
            .economic_context
            .open_tasks
            .iter()
            .any(|task| task.kind == crate::world_model::EconomicTaskKind::Vender)
        {
            social_move = None;
            (IntentKind::Vender, Some("vender excedente".to_string()))
        } else if input.state.hunger >= 65 {
            (IntentKind::Comer, Some("comida".to_string()))
        } else if input.state.energy <= 25 {
            (IntentKind::Descansar, Some("cama".to_string()))
        } else if input.state.stress >= 70 {
            (IntentKind::Refletir, Some("assento silencioso".to_string()))
        } else if let Some(conflict) = input.nearby_agents.iter().find(|other| {
            other
                .relation
                .as_ref()
                .map(|relation| relation.resentment > 35)
                .unwrap_or(false)
        }) {
            target_agent = Some(conflict.id);
            social_move = Some(SocialMove::Offend);
            (
                IntentKind::Socializar,
                Some("iniciar conversa tensa".to_string()),
            )
        } else if let Some(friend) = input.nearby_agents.iter().find(|other| {
            other
                .relation
                .as_ref()
                .map(|relation| relation.friendship > 30 || relation.trust > 30)
                .unwrap_or(false)
        }) {
            target_agent = Some(friend.id);
            social_move = Some(SocialMove::Favor);
            (
                IntentKind::Socializar,
                Some("iniciar conversa amistosa".to_string()),
            )
        } else if (6..=14).contains(&input.tick) {
            (IntentKind::Trabalhar, Some("posto de trabalho".to_string()))
        } else if (16..=21).contains(&input.tick) && !input.nearby_agents.is_empty() {
            if let Some(nearby) = input.nearby_agents.first() {
                target_agent = Some(nearby.id);
            }
            (
                IntentKind::Socializar,
                Some("puxar conversa casual".to_string()),
            )
        } else if input.current_building.is_some() {
            (IntentKind::Andar, Some("porta externa".to_string()))
        } else {
            social_move = None;
            (IntentKind::Andar, Some("praca".to_string()))
        };

        AgentIntent {
            kind,
            target_agent,
            target_semantic,
            justification: format!(
                "{} equilibra necessidade imediata, reputacao, {} e espaco acessivel.",
                input.actor_name,
                input
                    .psychological_context
                    .current_identity_tension
                    .to_lowercase()
            ),
            dominant_emotion: if input.state.stress > 50 {
                "tenso".to_string()
            } else {
                "contido".to_string()
            },
            perceived_risk: if kind == IntentKind::Socializar { 45 } else { 20 },
            belief_updates: vec![format!(
                "Meu foco deve honrar {}.",
                input
                    .psychological_context
                    .core_values
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "minhas prioridades".to_string())
            )],
            priority: 3,
            social_move,
        }
    }
}

impl LlmAdapter for MockLlmAdapter {
    fn provider_name(&self) -> &str {
        "mock"
    }

    fn evaluate_and_decide(&self, input: &DecisionInput) -> LlmResult<DecisionEnvelope> {
        let intent = self.choose_intent(input);
        let reflection = format!(
            "{} avalia area={}, fome={}, energia={} e stress={}.",
            input.actor_name,
            input.current_area,
            input.state.hunger,
            input.state.energy,
            input.state.stress
        );
        Ok(DecisionEnvelope { intent, reflection })
    }

    fn generate_conversation_turn(
        &self,
        input: &ConversationTurnInput,
    ) -> LlmResult<ConversationTurnOutput> {
        let relation = &input.listener.relation;
        let hostile = relation.resentment > 35
            || input.speaker_state.stress > 70
            || !input.relational_context.unresolved_offenses.is_empty();
        let friendly = relation.friendship > 25
            || relation.trust > 25
            || !input.relational_context.open_promises.is_empty()
            || !input.relational_context.recent_favors.is_empty();
        let speech_act = if hostile {
            "pressionar".to_string()
        } else if friendly {
            "aproximar".to_string()
        } else {
            "sondar".to_string()
        };
        let utterance = if hostile {
            format!(
                "{} encara {} e cobra explicacoes.",
                input.speaker_name, input.listener.name
            )
        } else if friendly {
            format!(
                "{} fala com {} em tom acolhedor.",
                input.speaker_name, input.listener.name
            )
        } else {
            format!(
                "{} testa o humor de {} com uma frase curta.",
                input.speaker_name, input.listener.name
            )
        };
        let emotion = if hostile {
            "irritado".to_string()
        } else if friendly {
            "caloroso".to_string()
        } else {
            "cauteloso".to_string()
        };
        let relation_delta_hint = if hostile {
            RelationDelta {
                trust: -2,
                friendship: -1,
                resentment: 4,
                attraction: 0,
                moral_debt: 0,
                reputation: -1,
            }
        } else if friendly {
            RelationDelta {
                trust: 3,
                friendship: 2,
                resentment: -1,
                attraction: 1,
                moral_debt: 1,
                reputation: 0,
            }
        } else {
            RelationDelta {
                trust: 1,
                friendship: 0,
                resentment: 0,
                attraction: 0,
                moral_debt: 0,
                reputation: 0,
            }
        };
        let continue_conversation = input.context.turns_remaining > 1
            && input.speaker_state.hunger < 90
            && input.speaker_state.energy > 10;
        Ok(ConversationTurnOutput {
            utterance,
            speech_act: speech_act.clone(),
            emotion,
            intent_to_continue: continue_conversation,
            belief_updates: vec![format!(
                "Minha proxima fala deve {} a conversa.",
                if hostile { "pressionar" } else { "moldar" }
            )],
            relation_delta_hint,
            tone: Some(if hostile {
                "duro".to_string()
            } else if friendly {
                "gentil".to_string()
            } else {
                "medido".to_string()
            }),
            risk_shift: Some(if hostile { 2 } else { -1 }),
        })
    }
}

pub struct OpenAiCompatibleAdapter {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
    retry_count: u32,
    retry_backoff_ms: u64,
}

impl OpenAiCompatibleAdapter {
    pub fn from_env() -> AnyResult<Self> {
        let api_key = env::var("OPENAI_API_KEY").context("OPENAI_API_KEY not set")?;
        let model = env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4.1-mini".to_string());
        let base_url = env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1/chat/completions".to_string());
        let timeout_secs = env::var("LLM_HTTP_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(60);
        let retry_count = env::var("LLM_HTTP_RETRY_COUNT")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(2);
        let retry_backoff_ms = env::var("LLM_HTTP_RETRY_BACKOFF_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(750);
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self {
            client,
            base_url,
            api_key,
            model,
            retry_count,
            retry_backoff_ms,
        })
    }

    fn max_attempts(&self) -> u32 {
        self.retry_count.saturating_add(1)
    }

    fn backoff_delay(&self, attempt: u32) -> Duration {
        Duration::from_millis(
            self.retry_backoff_ms
                .saturating_mul(u64::from(attempt.max(1))),
        )
    }

    fn classify_transport_error(
        &self,
        operation: &str,
        attempts: u32,
        error: ReqwestError,
    ) -> LlmError {
        if error.is_timeout() {
            LlmError::Timeout {
                operation: operation.to_string(),
                attempts,
                message: error.to_string(),
            }
        } else {
            LlmError::Transport {
                operation: operation.to_string(),
                attempts,
                message: error.to_string(),
            }
        }
    }

    fn classify_http_status(
        &self,
        operation: &str,
        attempts: u32,
        status: StatusCode,
        message: String,
    ) -> LlmError {
        LlmError::HttpStatus {
            operation: operation.to_string(),
            status: status.as_u16(),
            attempts,
            message,
        }
    }

    fn fetch_message_content(
        &self,
        operation: &str,
        system_prompt: &str,
        user_payload: &serde_json::Value,
    ) -> LlmResult<String> {
        for attempt in 1..=self.max_attempts() {
            let response = self
                .client
                .post(&self.base_url)
                .bearer_auth(&self.api_key)
                .json(&json!({
                    "model": self.model,
                    "temperature": 0.8,
                    "messages": [
                        { "role": "system", "content": system_prompt },
                        { "role": "user", "content": user_payload.to_string() }
                    ]
                }))
                .send();

            let response = match response {
                Ok(response) => response,
                Err(error) => {
                    let llm_error = self.classify_transport_error(operation, attempt, error);
                    if llm_error.is_transient() && attempt < self.max_attempts() {
                        eprintln!(
                            "LLM retry {}/{} for {} after transient transport error: {}",
                            attempt,
                            self.max_attempts(),
                            operation,
                            llm_error
                        );
                        thread::sleep(self.backoff_delay(attempt));
                        continue;
                    }
                    return Err(llm_error);
                }
            };

            let status = response.status();
            if !status.is_success() {
                let body = response
                    .text()
                    .unwrap_or_else(|error| format!("failed to read response body: {}", error));
                let llm_error = self.classify_http_status(operation, attempt, status, body);
                if llm_error.is_transient() && attempt < self.max_attempts() {
                    eprintln!(
                        "LLM retry {}/{} for {} after transient HTTP status: {}",
                        attempt,
                        self.max_attempts(),
                        operation,
                        llm_error
                    );
                    thread::sleep(self.backoff_delay(attempt));
                    continue;
                }
                return Err(llm_error);
            }

            let payload: ChatCompletionResponse = response.json().map_err(|error| LlmError::Parse {
                operation: operation.to_string(),
                message: format!("invalid LLM HTTP response: {}", error),
            })?;
            return payload
                .choices
                .into_iter()
                .next()
                .and_then(|choice| choice.message.content)
                .ok_or_else(|| LlmError::Parse {
                    operation: operation.to_string(),
                    message: "LLM provider returned no message content".to_string(),
                });
        }

        Err(LlmError::Transport {
            operation: operation.to_string(),
            attempts: self.max_attempts(),
            message: "LLM call exhausted retries unexpectedly".to_string(),
        })
    }

    fn log_normalization_notes(&self, operation: &str, raw: &str, notes: &[String]) {
        if notes.is_empty() {
            return;
        }
        eprintln!(
            "LLM normalization notes for {}: {}\nRaw response: {}",
            operation,
            notes.join(" | "),
            raw
        );
    }
}

impl LlmAdapter for OpenAiCompatibleAdapter {
    fn provider_name(&self) -> &str {
        "openai-compatible"
    }

    fn evaluate_and_decide(&self, input: &DecisionInput) -> LlmResult<DecisionEnvelope> {
        let payload = serde_json::to_value(input).map_err(|error| LlmError::Schema {
            operation: "decision".to_string(),
            message: format!("failed to serialize decision input: {}", error),
        })?;
        let content = self.fetch_message_content("decision", DECISION_PROMPT, &payload)?;
        parse_decision_json(&content).map_err(|error| LlmError::Schema {
            operation: "decision".to_string(),
            message: error.to_string(),
        })
    }

    fn generate_conversation_turn(
        &self,
        input: &ConversationTurnInput,
    ) -> LlmResult<ConversationTurnOutput> {
        let payload = serde_json::to_value(input).map_err(|error| LlmError::Schema {
            operation: "conversation_turn".to_string(),
            message: format!("failed to serialize conversation input: {}", error),
        })?;
        let content =
            self.fetch_message_content("conversation_turn", CONVERSATION_TURN_PROMPT, &payload)?;
        let (output, notes) = match parse_conversation_turn_json_with_notes(&content) {
            Ok(parsed) => parsed,
            Err(error) => {
                eprintln!(
                    "LLM conversation schema parse failed: {}\nRaw response: {}",
                    error, content
                );
                return Err(LlmError::Schema {
                    operation: "conversation_turn".to_string(),
                    message: error.to_string(),
                });
            }
        };
        self.log_normalization_notes("conversation_turn", &content, &notes);
        Ok(output)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    content: Option<String>,
}
