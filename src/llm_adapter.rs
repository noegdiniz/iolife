use crate::agent_mind::{
    ActionPlannerInput, ThinkMakerInput, ThinkMakerOutput,
    ConversationTurnInput, ConversationTurnOutput,
    parse_conversation_turn_json_with_notes, parse_think_maker_json,
};
use crate::world_model::RelationDelta;
use anyhow::{Context, Result as AnyResult};
use reqwest::blocking::Client;
use reqwest::{Error as ReqwestError, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::fmt;
use std::thread;
use std::time::Duration;

const ACTION_PLANNER_PROMPT: &str = r#"Voce decide a sequencia de proximos passos (plano de tarefas) de um aldeao medieval em um mundo fisico de grid.

Sua resposta deve conter APENAS uma lista de acoes separadas por virgula no formato especificado, sem preambulos, introducoes ou markdown.
NUNCA use blocos de codigo (como ``` ou ```json) ou qualquer explicacao fora das acoes.

Formatos validos de Acao:
- Acao(parametro_semantico) (ex: Comer(taverna), Trabalhar(posto_de_trabalho), Andar(casa))
- Acao(alvo_id, movimento_social) (apenas para Socializar; ex: Socializar(3, conversar), Socializar(5, fofocar))
- Acao(alvo_id) (apenas para acoes com alvo; ex: Agredir(2), Roubar(4), Prender(1))
- Acao (para acoes sem parametro; ex: Descansar, Refletir, Fugir, ReceberPagamento, Investigar)

Acoes Permitidas e Seus Parametros:
- Trabalhar(posto_de_trabalho) (executar tarefas de producao no posto de trabalho/oficina)
- Descansar (recuperar energia em uma cama)
- Comer(taverna|casa) (alimentar-se quando com fome)
- Socializar(alvo_id, social_move) (iniciar conversa; social_move valido: conversar|fofocar|prometer|ofender|reconciliar|ajudar)
- Refletir (diminuir stress)
- Andar(destino) (deslocar-se fisicamente ate um local)
- Comprar(recurso) (comprar insumos/alimentos)
- Transportar(recurso) (mover recursos de um estoque/local para outro)
- Vender(recurso) (colocar produtos a venda ou comercializar)
- ReceberPagamento (reivindicar salarios ou compensacoes financeiras devidas)
- Agredir(alvo_id) (ataque fisico imediato contra agente adjacente)
- Combater(alvo_id) (continuar combate fisico ativo contra agente adjacente)
- Roubar(alvo_id) (tomar recursos de um alvo adjacente com violencia)
- Furtar(alvo_id) (subtrair pequeno recurso sem confronto direto)
- Fugir (tentar sair de combate ou perigo)
- Acusar(alvo_id) (denunciar alguem por crime)
- Investigar (apurar caso criminal ou suspeito)
- Prender(alvo_id) (guarda/lider tenta deter suspeito adjacente)
- Punir(alvo_id) (lider/guarda aplica sentenca em suspeito detido)
- Apoiar(pauta) (registrar apoio individual a uma pauta politica aberta)
- Opor(pauta) (registrar oposicao individual a uma pauta politica aberta)
- Pressionar(alvo_id) (pressionar agente adjacente em disputa institucional)
- PedirApoio(alvo_id) (pedir apoio politico a agente adjacente)
- Mediar(alvo_id) (tentar reduzir conflito institucional)

Regras de Validacao e Negocio:
1. Retorne entre 3 e 6 acoes em sequencia, separadas por virgula.
2. Use APENAS IDs numericos de agentes para os campos alvo_id (ex: use 3 em vez de "Alda").
3. Se o aldeao estiver com fome alta (hunger >= 65), inclua a tarefa "Comer" no planejamento. Se estiver muito cansado (energy <= 25), inclua "Descansar". Se estiver muito estressado (stress >= 70), inclua "Refletir".
4. Personalidade, Caos e Faixas de chaos_pressure (0-100):
   - chaos_pressure 0-30: Comportamento normal, cooperativo. Violencia APENAS em defesa propria.
   - chaos_pressure 31-50: Comportamento tenso. Permitido ofender, fofocar, pressionar. Furto apenas se fome > 80.
   - chaos_pressure 51-70: Comportamento volatil. Furtar, roubar de estranhos, agredir com resentment alto.
   - chaos_pressure 71-85: Comportamento perigoso. Agredir qualquer agente proximo, roubar com violencia, acusar sem evidencia.
   - chaos_pressure 86-100: Comportamento desesperado. Qualquer acao anti-social e justificada pela sobrevivencia: matar, saquear, trair, fugir.

Exemplo de Resposta Valida:
Comer(taverna), Trabalhar(posto_de_trabalho), Descansar"#;

const THINK_MAKER_PROMPT: &str = r#"Voce gera o pensamento, sentimento e crencas de um aldeao medieval que acabou de planejar um conjunto de acoes.

Sua resposta deve conter EXATAMENTE um objeto JSON estruturado e NADA mais.
Nao utilize marcacoes de markdown (como ```json ou ```).
Nao escreva nenhuma explicacao, preambulo, introducao, notas de rodape ou texto adicional fora do JSON.
Nao insira comentarios no JSON.

Use EXATAMENTE esta estrutura de chaves e tipos:
{
  "reflection": "string extremamente concisa resumindo o raciocinio e motivacao atual (no maximo 2 frases)",
  "dominant_emotion": "string indicando o sentimento atual (ex: alegre, cansado, focado, apreensivo, furioso, desesperado)",
  "belief_updates": ["array de strings contendo novas crencas ou metas curtas (seja extremamente conciso, max 2 frases por crenca)"]
}

Regras:
1. Baseie-se no estado do agente, chaos_pressure, traits, traumas, memorias e nas acoes planejadas.
2. Seja extremamente conciso. No maximo 2 frases em reflection e em cada item de belief_updates.

Exemplo de Resposta Valida:
{
  "reflection": "Estou com fome e preciso comer para continuar trabalhando.",
  "dominant_emotion": "apreensivo",
  "belief_updates": ["Preciso economizar moedas para tempos dificeis."]
}"#;

const CONVERSATION_TURN_PROMPT: &str = r#"Voce responde apenas pela mente de UM unico aldeao em uma conversa social medieval.

Seja extremamente conciso nas strings de justificativa e pensamento. Use no maximo 2 frases.

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
  "belief_updates": ["string (extremamente concisa, no maximo 2 frases)"],
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
- Seja extremamente conciso nas strings de justificativa e pensamento. Use no maximo 2 frases no campo belief_updates.

Regras de Personalidade na Conversa:
O campo "chaos_pressure" e os "personality_traits"/"trauma_traits" guiam o tom e as acoes sociais:
- Se chaos_pressure >= 50: permitido 'ofender' espontaneamente, mentir em promessas, aumentar deltas negativos de resentment/trust.
- Se chaos_pressure >= 70: o agente pode ameacar, chantagear, ou encerrar a conversa com agressao (intent_to_continue=false + risk_shift alto).
- Tracos como 'manipulador' permitem speech_act 'prometer' com falsidade implicita.
- Tracos como 'paranoico' geram interpretacoes hostis de falas neutras.
- Tracos como 'cinico' reduzem trust em qualquer promessa recebida.

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
    fn clone_box(&self) -> Box<dyn LlmAdapter>;
    fn provider_name(&self) -> &str;
    fn plan_actions(&self, input: &ActionPlannerInput) -> LlmResult<String>;
    fn generate_thoughts(&self, input: &ThinkMakerInput) -> LlmResult<ThinkMakerOutput>;
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

#[derive(Clone)]
pub struct MockLlmAdapter;

impl LlmAdapter for MockLlmAdapter {
    fn clone_box(&self) -> Box<dyn LlmAdapter> {
        Box::new(self.clone())
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    fn plan_actions(&self, input: &ActionPlannerInput) -> LlmResult<String> {
        let mut tasks = Vec::new();

        if input.state.hunger >= 65 {
            tasks.push("Comer(taverna)".to_string());
        }
        if input.state.energy <= 25 {
            tasks.push("Descansar".to_string());
        }
        if input.state.stress >= 70 {
            tasks.push("Refletir".to_string());
        }
        if input.economic_context.pending_salary > 0 {
            tasks.push("ReceberPagamento".to_string());
        }

        if (input.role.to_lowercase().contains("guarda")
            || input.role.to_lowercase().contains("lider"))
            && !input.legal_context.open_cases.is_empty()
        {
            tasks.push("Investigar".to_string());
        }

        if !input.political_context.open_issues.is_empty()
            && !input.political_context.household_grievances.is_empty()
        {
            let issue = input.political_context.open_issues.first().cloned().unwrap_or_default();
            tasks.push(format!("Apoiar({})", issue));
        } else if input.role.to_lowercase().contains("lider")
            && !input.political_context.open_issues.is_empty()
        {
            let target = input.nearby_agents.first().map(|a| a.id.to_string()).unwrap_or_else(|| "null".to_string());
            tasks.push(format!("Mediar({})", target));
        }

        for task in &input.economic_context.open_tasks {
            let action = match task.kind {
                crate::world_model::EconomicTaskKind::Produzir => format!("Trabalhar({})", task.summary.to_lowercase()),
                crate::world_model::EconomicTaskKind::Comprar => format!("Comprar({})", task.summary.to_lowercase()),
                crate::world_model::EconomicTaskKind::Transportar => format!("Transportar({})", task.summary.to_lowercase()),
                crate::world_model::EconomicTaskKind::Vender => format!("Vender({})", task.summary.to_lowercase()),
                crate::world_model::EconomicTaskKind::ReceberPagamento => "ReceberPagamento".to_string(),
            };
            tasks.push(action);
        }

        if tasks.is_empty() {
            if (6..=14).contains(&input.tick) {
                tasks.push("Trabalhar(posto_de_trabalho)".to_string());
            } else if (16..=21).contains(&input.tick) && !input.nearby_agents.is_empty() {
                let target = input.nearby_agents.first().map(|a| a.id).unwrap_or(0);
                tasks.push(format!("Socializar({}, conversar)", target));
            } else {
                tasks.push("Andar(praca)".to_string());
            }
        }

        if tasks.len() < 3 {
            tasks.push("Andar(praca)".to_string());
            tasks.push("Refletir".to_string());
        }
        tasks.truncate(5);

        Ok(tasks.join(", "))
    }

    fn generate_thoughts(&self, input: &ThinkMakerInput) -> LlmResult<ThinkMakerOutput> {
        let reflection = format!(
            "{} avalia a situacao com fome={}, energia={} e stress={}.",
            input.decision_input.actor_name,
            input.decision_input.state.hunger,
            input.decision_input.state.energy,
            input.decision_input.state.stress
        );

        Ok(ThinkMakerOutput {
            reflection,
            dominant_emotion: if input.decision_input.state.stress > 50 {
                "tenso".to_string()
            } else {
                "contido".to_string()
            },
            belief_updates: vec![format!("Manter prioridades de {}.", input.decision_input.role)],
        })
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

#[derive(Clone)]
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
        let model = env::var("OPENAI_MODEL").unwrap_or_else(|_| "deepseek-v4-flash".to_string());
        let base_url = env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1/chat/completions".to_string());
        let timeout_secs = env::var("LLM_HTTP_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(180);
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

            let payload: ChatCompletionResponse =
                response.json().map_err(|error| LlmError::Parse {
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
    fn clone_box(&self) -> Box<dyn LlmAdapter> {
        Box::new(self.clone())
    }

    fn provider_name(&self) -> &str {
        "openai-compatible"
    }

    fn plan_actions(&self, input: &ActionPlannerInput) -> LlmResult<String> {
        let payload = serde_json::to_value(input).map_err(|error| LlmError::Schema {
            operation: "action_planning".to_string(),
            message: format!("failed to serialize action planning input: {}", error),
        })?;
        self.fetch_message_content("action_planning", ACTION_PLANNER_PROMPT, &payload)
    }

    fn generate_thoughts(&self, input: &ThinkMakerInput) -> LlmResult<ThinkMakerOutput> {
        let payload = serde_json::to_value(input).map_err(|error| LlmError::Schema {
            operation: "thought_generation".to_string(),
            message: format!("failed to serialize thought generation input: {}", error),
        })?;
        let content = self.fetch_message_content("thought_generation", THINK_MAKER_PROMPT, &payload)?;
        parse_think_maker_json(&content).map_err(|error| LlmError::Schema {
            operation: "thought_generation".to_string(),
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
