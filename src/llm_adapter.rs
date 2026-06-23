use crate::agent_mind::{
    ActionPlannerInput, ConversationTurnInput, ConversationTurnOutput, ThinkMakerInput,
    ThinkMakerOutput, parse_conversation_turn_json_with_notes, parse_think_maker_json,
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
- Acao(place_id) para qualquer destino fisico (ex: Andar(building:3), Descansar(fixture:12), Trabalhar(fixture:18))
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
- Construir(projeto) (trabalhar em projeto de obra urbana aberto)
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
- Decretar(tag) (somente lider local; muda norma por decreto. tags validas: reduzir_imposto, aumentar_imposto, justica_branda, justica_normal, justica_severa, racionamento_lares, racionamento_produtores, racionamento_civico, racionamento_equilibrado, trabalho_forcado_campos, racionamento_estrito, imposto_guerra, proibicao_tavernas, confisco_metais)
- JurarLealdade(alvo_id) (aceitar ou reforcar vinculo de vassalagem com um superior)
- RomperLealdade(alvo_id) (romper ou enfraquecer laço feudal com o suserano)
- ConcederTitulo(alvo_id) (autoridade feudal concede titulo existente a um agente)
- RevogarTitulo(alvo_id) (autoridade feudal remove titulo de um agente)
- NomearOficial(alvo_id) (autoridade feudal nomeia agente para oficio local)
- ExigirTributo(alvo_id) (cobrar tributo feudal de um dependente ou vassalo)
- CobrarCorveia(alvo_id) (impor mais dias de trabalho obrigatorio)
- ConvocarLevy(alvo_id) (convocar servico militar/levy)
- ReconhecerHerdeiro(alvo_id) (reconhecer formalmente um herdeiro ou pretendente)
- ApoiarPretendente(alvo_id) (apoiar candidato em disputa sucessoria)
- Usurpar(alvo_id) (tentar tomar a posicao/titulo de outro agente)
- ReivindicarTerritorio(place_id_territory) (reforcar reivindicacao politica sobre territorio canonico)
- NegociarSuserania(alvo_id) (renegociar submissao, protecao e termos feudais)

Regras de Validacao e Negocio:
1. Retorne entre 3 e 6 acoes em sequencia, separadas por virgula.
2. Use APENAS IDs numericos de agentes para os campos alvo_id (ex: use 3 em vez de "Alda").
2b. Para qualquer local fisico, use OBRIGATORIAMENTE um place_id exato de world_places. Nunca invente nomes livres como "taverna", "casa", "forja" ou "praca" em campos estruturados.
2c. time_context informa hora, fase do dia, luz, trabalho, refeicao e sono; use isso para escolher trabalho, refeicao, descanso e encontros.
3. Se o aldeao estiver com fome alta (hunger >= 65), inclua a tarefa "Comer" no planejamento. Se estiver muito cansado (energy <= 25), inclua "Descansar". Se estiver muito estressado (stress >= 70), inclua "Refletir".
4. Planejamento Estratégico e Tramas:
   Você NUNCA deve planejar ações sem um propósito de longo prazo ou interesse estratégico. Cada sequência de ações deve funcionar como uma trama ou esquema tático para atingir seus objetivos de vida (como acumular riqueza, garantir segurança, obter vingança contra inimigos, apoiar/opor pautas políticas para benefício próprio ou derrubar líderes indesejados). Evite ações casuais ou sem objetivo prático.
5. Personalidade, Caos e Faixas de chaos_pressure (0-100):
   - chaos_pressure 0-30: Comportamento normal, cooperativo. Violencia APENAS em defesa propria.
   - chaos_pressure 31-50: Comportamento tenso. Permitido ofender, fofocar, pressionar. Furto apenas se fome > 80.
   - chaos_pressure 51-70: Comportamento volatil. Furtar, roubar de estranhos, agredir com resentment alto.
   - chaos_pressure 71-85: Comportamento perigoso. Agredir qualquer agente proximo, roubar com violencia, acusar sem evidencia.
   - chaos_pressure 86-100: Comportamento desesperado. Qualquer acao anti-social e justificada pela sobrevivencia: matar, saquear, trair, fugir.
6. Contexto institucional:
   - Use institutional_context para decidir obediencia, medo, boicote, denuncia, apoio a guerra, resistencia e conspiracao.
   - Baixa legitimidade nao muda normas diretamente; normas so mudam por Decretar, guerra civil vencida ou troca de controlador.
   - Medo alto da autoridade pode gerar obediencia publica e resistencia secreta.
7. Contexto feudal:
   - Use feudal_context para avaliar quem manda em quem, obrigacoes de tributo/corveia/levy, conflitos de autoridade e crise sucessoria.
   - Voce NUNCA altera titulos, holdings, controlador ou soberania diretamente; apenas escolhe a acao individual. O motor valida jurisdição, cargo e capacidade real.
   - Se quiser agir sobre territorio, use exclusivamente um place_id de territorio vindo de world_places.
8. Body Graph (Estado Físico do Corpo):
   - O campo `body_parts` contêm o estado de integridade de seus membros. Danos debilitam você: pernas/pés feridos reduzem velocidade; braços/mãos feridos reduzem produtividade e eficácia em combate; olhos cegos diminuem visão.
   - Dor extrema e sangramento debilitam a saúde. Se você foi mutilado ou está com feridas graves, reflita sobre isso sem filtros ou censura, expressando medo, ódio ou adaptando suas ações para sobreviver realisticamente. A violência medieval é visceral!

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
  "belief_updates": ["array de strings contendo novas crencas ou metas curtas (seja extremamente conciso, max 2 frases por crenca)"],
  "long_term_plan": "string curta descrevendo o plano estrategico persistente atual do agente"
}

Regras:
1. Baseie-se no estado do agente, chaos_pressure, traits, traumas, memorias e nas acoes planejadas.
2. Pensamento Estratégico: A 'reflection' e 'belief_updates' devem explicitar o objetivo estratégico oculto, o alvo da manipulação/influência e como a sequência de ações planejadas serve a essa trama/esquema. Nunca expresse pensamentos vazios ou triviais; foque inteiramente no ganho estratégico.
3. Seja extremamente conciso. No maximo 2 frases em reflection e em cada item de belief_updates.
4. Estado Físico e Traumas: Seu pensamento ('reflection') deve ser condizente com a dor e as lesões em seu `body_parts`. Se foi mutilado (ex: olho perdido, membro decepado), reflita sobre isso sem censura ou filtros modernos, expressando dor medieval crua, ódio, medo ou desejo de vingança.
5. `long_term_plan` deve SEMPRE existir. Ele e curto, concreto e estavel. Preserve continuidade com o plano anterior quando nada material mudou. So revise o plano se fome, trauma, guerra, promessa, oportunidade economica, crise feudal ou mudanca institucional realmente alterarem a direcao do agente.

Exemplo de Resposta Valida:
{
  "reflection": "Estou com fome e preciso comer para continuar trabalhando.",
  "dominant_emotion": "apreensivo",
  "belief_updates": ["Preciso economizar moedas para tempos dificeis."],
  "long_term_plan": "Juntar recursos sem arriscar minha posicao na vila."
}"#;

const CONVERSATION_TURN_PROMPT: &str = r#"Voce responde apenas pela mente de UM unico aldeao in uma conversa social medieval.

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
  "risk_shift": 0,
  "economic_transfer": null ou {
    "recipient_id": numero inteiro ou null,
    "amount": numero inteiro,
    "resource_id": "moedas" ou "graos",
    "use_public_treasury": boolean
  },
  "revealed_secret": null ou {
    "secret_id": numero inteiro,
    "recipient_id": numero inteiro
  },
  "make_promise": null ou {
    "recipient_id": numero inteiro,
    "condition_type": "DeliverResource" ou "VoteForPolicy" ou "KeepSecret",
    "resource_id": "moedas" ou "graos" ou null,
    "amount": numero inteiro ou null,
    "policy_domain": "taxa_imposto" ou null,
    "policy_value": "10" ou null,
    "secret_id": numero inteiro ou null,
    "duration_ticks": numero inteiro
  },
  "spread_rumor": null ou {
    "target_agent_id": numero inteiro,
    "topic": "encontro_com_armas" ou "roubo_planejado" ou "desvio_recursos",
    "claim": "frase curta do que esta sendo alegado",
    "is_true": boolean
  },
  "shared_story": null ou {
    "story_id": numero inteiro ou null,
    "title": "titulo curto da historia" ou null,
    "version": "versao curta contada em uma frase",
    "kind": "Lenda" ou "HistoriaFamiliar" ou "CantoDeGuerra" ou "Martirio" ou "Milagre" ou "Assombracao" ou "Fundacao" ou "Traicao" ou "Heroismo" ou "AdvertenciaMoral" ou null,
    "tone": "tom da narrativa" ou null,
    "moral": "moral curta" ou null,
    "tags": ["tags culturais curtas"]
  },
  "escrow_deposit": null ou {
    "target_agent_id": numero inteiro,
    "resource_id": "moedas" ou "graos",
    "amount": numero inteiro,
    "condition_secret_id": numero inteiro
  },
  "propose_meeting": null ou {
    "invitee_ids": [numeros inteiros],
    "place_id": "place_id exato de world_places",
    "scheduled_day": numero inteiro,
    "scheduled_time": "HH:MM",
    "purpose": "string curta"
  },
  "addressed_agent_ids": [numeros inteiros validos entre os participantes] ou [],
  "meeting_response": null ou {
    "meeting_id": numero inteiro,
    "accept": boolean,
    "reason": "string curta"
  }
}

Tipos obrigatorios:
- utterance: string nao vazia
- speech_act: string nao vazia
- emotion: string nao vazia
- intent_to_continue: boolean true ou false, never numero e never string
- belief_updates: array de strings, mesmo com 1 item
- relation_delta_hint: objeto, never string
- trust/friendship/resentment/attraction/moral_debt/reputation: inteiros pequenos entre -2 e 2
- tone: string ou null
- risk_shift: inteiro pequeno entre -5 e 5, never string
- addressed_agent_ids: array de inteiros validos entre os participantes da conversa, ou []
- economic_transfer: null ou objeto contendo recipient_id (inteiro/null), amount (inteiro), resource_id ("moedas"/"graos"), use_public_treasury (boolean)
- revealed_secret: null ou objeto contendo secret_id (inteiro) e recipient_id (inteiro)
- make_promise: null ou objeto com recipient_id (inteiro), condition_type ("DeliverResource"/"VoteForPolicy"/"KeepSecret"), resource_id/amount/policy_domain/policy_value/secret_id (valores ou null) e duration_ticks (inteiro)
- spread_rumor: null ou objeto com target_agent_id (inteiro), topic (string), claim (string curta opcional) e is_true (boolean)
- shared_story: null ou objeto com story_id (inteiro/null), title (string/null), version (string curta obrigatoria), kind (string/null), tone (string/null), moral (string/null), tags (array de strings)
- escrow_deposit: null ou objeto com target_agent_id (inteiro), resource_id ("moedas"/"graos"), amount (inteiro) e condition_secret_id (inteiro)
- propose_meeting: null ou objeto com invitee_ids (array de inteiros validos), place_id (string exata de world_places), scheduled_day (inteiro), scheduled_time ("HH:MM") e purpose (string curta)
- meeting_response: null ou objeto com meeting_id (inteiro), accept (boolean) e reason (string curta)

Regras obrigatorias:
- Se a fala for amigavel, use deltas pequenos e coerentes.
- Se a fala reduzir tensao, diminua resentment em vez de descrever isso em texto.
- Se a fala aproximar os agentes, aumente trust e/ou friendship numericamente.
- Nunca substitua relation_delta_hint por descricao textual.
- Nunca substitua belief_updates por string unica.
- Nunca substitua intent_to_continue por score como 0.8.
- Nunca inclua chaves extras.
- Seja extremamente conciso nas strings de justificativa e pensamento. Use no maximo 2 frases no campo belief_updates.
- Rumores: voce pode contar, negar, distorcer ou evitar rumores, mas o motor decide credibilidade, distorcao e consequencias. Nunca trate rumor como prova judicial definitiva.
- Se usar spread_rumor, topic deve ser categoria curta e claim deve ser a alegacao concreta em uma frase curta.
- Historias culturais: voce pode contar, reinterpretar, negar ou evitar uma historia conhecida em cultural_context. Se usar shared_story, nao decida se ela vira verdade ou norma; o motor calcula forca cultural, distorcao e efeitos.
- Se usar shared_story, version deve ser uma frase curta e concreta, diferente de rumor factual recente.
- Encontros: voce pode propor ou responder a encontro, mas qualquer local estruturado DEVE usar place_id exato de world_places. Se propuser encontro em grupo, use apenas invitee_ids validos do grupo atual. O motor valida horario futuro, pathfinding e presenca fisica.

Regras de Diálogo e Manipulação:
- NUNCA participe de conversas fúteis, casuais ou conversas fiadas amigáveis sem interesse prático.
- Cada fala (`utterance`) deve ter como único propósito manipular o interlocutor, extrair informações cruciais, defender seus próprios interesses, negociar termos favoráveis, ou alinhar o outro agente aos seus objetivos de longo prazo.
- Se o tom for amigável, deve ser de forma puramente tática e calculada (para ganhar confiança e manipular). Se o tom for hostil, deve ser para pressionar, intimidar ou ameaçar para atingir seu objetivo.
- Use speech_acts como:
  * 'bajular' (elogiar falsamente e fingir simpatia para diminuir as defesas do outro e obter cooperação);
  * 'desinformar' (espalhar boatos falsos, desinformação ou mentiras sobre oponentes para desestabilizar ou minar alianças);
  * 'chantagear' (pressionar o alvo usando segredos, ameaças de denúncia ou retaliação física/social);
  * 'subornar' (oferecer moedas, favores materiais ou de influência para comprar apoio político ou silêncio);
  * 'prometer' (fechar acordos táticos com promessas que você pode quebrar depois);
  * 'ofender' ou 'pressionar' (para extrair concessões ou intimidar oponentes).

Regras de Personalidade na Conversa:
O campo "chaos_pressure" e os "personality_traits"/"trauma_traits" guiam o tom e as acoes sociais:
- Se chaos_pressure >= 50: permitido 'ofender' espontaneamente, mentir em promessas, aumentar deltas negativos de resentment/trust.
- Se chaos_pressure >= 70: o agente pode ameacar, chantagear, ou encerrar a conversa com agressao (intent_to_continue=false + risk_shift alto).
- Tracos como 'manipulador' permitem speech_act 'prometer' ou 'bajular' com falsidade implicita.
- Tracos como 'paranoico' geram interpretacoes hostis de falas neutras.
- Tracos como 'cinico' reduzem trust em qualquer promessa recebida.
- Regras de Expressão Física e Mutilação: Se você tiver ferimentos ou membros decepados/destruídos em seu `body_parts`, você deve se queixar da dor, relatar a mutilação para o interlocutor com termos viscerais e sem censura medieval (ex: gritar de dor, falar da mão decepada ou do olho cego) caso isso sirva aos seus fins ou mostre debilidade real.

Se estiver em duvida sobre um campo:
- intent_to_continue = true
- belief_updates = []
- relation_delta_hint = { "trust": 0, "friendship": 0, "resentment": 0, "attraction": 0, "moral_debt": 0, "reputation": 0 }
- tone = null
- risk_shift = 0
- economic_transfer = null
- revealed_secret = null
- shared_story = null
- propose_meeting = null
- meeting_response = null

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
  "risk_shift": -1,
  "economic_transfer": null,
  "revealed_secret": null,
  "shared_story": null,
  "propose_meeting": null,
  "meeting_response": null
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

pub fn adapter_from_env() -> AnyResult<Box<dyn LlmAdapter>> {
    OpenAiCompatibleAdapter::from_env()
        .map(|adapter| Box::new(adapter) as Box<dyn LlmAdapter>)
        .context(
            "falha ao inicializar provider LLM real; configure o adapter compatível antes de rodar",
        )
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
        let place_with_tag = |needle: &str| {
            input
                .world_places
                .iter()
                .find(|place| place.semantic_tags.iter().any(|tag| tag.contains(needle)))
                .map(|place| place.place_id.clone())
        };
        let food_place = place_with_tag("taverna")
            .or_else(|| place_with_tag("mesa"))
            .or_else(|| place_with_tag("social"))
            .unwrap_or_else(|| "special:external_market".to_string());
        let work_place = place_with_tag("trabalho")
            .or_else(|| place_with_tag("oficina"))
            .unwrap_or_else(|| "special:external_market".to_string());
        let walk_place = place_with_tag("praca")
            .or_else(|| {
                input
                    .world_places
                    .first()
                    .map(|place| place.place_id.clone())
            })
            .unwrap_or_else(|| "special:external_market".to_string());

        if input.state.hunger >= 65 {
            tasks.push(format!("Comer({food_place})"));
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
            let issue = input
                .political_context
                .open_issues
                .first()
                .cloned()
                .unwrap_or_default();
            tasks.push(format!("Apoiar({})", issue));
        } else if input.role.to_lowercase().contains("lider")
            && !input.political_context.open_issues.is_empty()
        {
            let target = input
                .nearby_agents
                .first()
                .map(|a| a.id.to_string())
                .unwrap_or_else(|| "null".to_string());
            tasks.push(format!("Mediar({})", target));
        }

        for task in &input.economic_context.open_tasks {
            let action = match task.kind {
                crate::world_model::EconomicTaskKind::Produzir => {
                    format!("Trabalhar({})", task.summary.to_lowercase())
                }
                crate::world_model::EconomicTaskKind::Comprar => {
                    format!("Comprar({})", task.summary.to_lowercase())
                }
                crate::world_model::EconomicTaskKind::Transportar => {
                    format!("Transportar({})", task.summary.to_lowercase())
                }
                crate::world_model::EconomicTaskKind::Construir => {
                    format!("Construir({})", task.summary.to_lowercase())
                }
                crate::world_model::EconomicTaskKind::Vender => {
                    format!("Vender({})", task.summary.to_lowercase())
                }
                crate::world_model::EconomicTaskKind::ReceberPagamento => {
                    "ReceberPagamento".to_string()
                }
            };
            tasks.push(action);
        }

        if tasks.is_empty() {
            if (6..=14).contains(&input.tick) {
                tasks.push(format!("Trabalhar({work_place})"));
            } else if (16..=21).contains(&input.tick) && !input.nearby_agents.is_empty() {
                let target = input.nearby_agents.first().map(|a| a.id).unwrap_or(0);
                tasks.push(format!("Socializar({}, conversar)", target));
            } else {
                tasks.push(format!("Andar({walk_place})"));
            }
        }

        if tasks.len() < 3 {
            tasks.push(format!("Andar({walk_place})"));
            tasks.push("Refletir".to_string());
        }
        tasks.truncate(5);

        Ok(tasks.join(", "))
    }

    fn generate_thoughts(&self, input: &ThinkMakerInput) -> LlmResult<ThinkMakerOutput> {
        let mut reflection = format!(
            "{} avalia a situacao com fome={}, energia={} e stress={}.",
            input.decision_input.actor_name,
            input.decision_input.state.hunger,
            input.decision_input.state.energy,
            input.decision_input.state.stress
        );
        for part in &input.decision_input.body_parts {
            if part.status != crate::world_model::PartInjuryStatus::Intact {
                reflection.push_str(&format!(
                    " Sinto dor de status {:?} no(a) {}.",
                    part.status,
                    part.kind.display_name()
                ));
            }
        }

        Ok(ThinkMakerOutput {
            reflection,
            dominant_emotion: if input.decision_input.state.stress > 50 {
                "tenso".to_string()
            } else {
                "contido".to_string()
            },
            belief_updates: vec![format!(
                "Manter prioridades de {}.",
                input.decision_input.role
            )],
            long_term_plan: format!(
                "Consolidar minha posicao como {} sem perder estabilidade.",
                input.decision_input.role
            ),
        })
    }

    fn generate_conversation_turn(
        &self,
        input: &ConversationTurnInput,
    ) -> LlmResult<ConversationTurnOutput> {
        let primary_listener = input.participants.first();
        let fallback_relation = crate::world_model::AgentRelation::default();
        let relation = primary_listener
            .map(|listener| &listener.relation)
            .unwrap_or(&fallback_relation);
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
        let mut utterance = if hostile {
            format!(
                "{} encara {} e cobra explicacoes.",
                input.speaker_name,
                primary_listener
                    .map(|listener| listener.name.as_str())
                    .unwrap_or("o grupo")
            )
        } else if friendly {
            format!(
                "{} fala com {} em tom acolhedor.",
                input.speaker_name,
                primary_listener
                    .map(|listener| listener.name.as_str())
                    .unwrap_or("o grupo")
            )
        } else {
            format!(
                "{} testa o humor de {} com uma frase curta.",
                input.speaker_name,
                primary_listener
                    .map(|listener| listener.name.as_str())
                    .unwrap_or("o grupo")
            )
        };
        for part in &input.body_parts {
            if part.status != crate::world_model::PartInjuryStatus::Intact {
                utterance = format!(
                    "{} geme de dor no(a) {} e diz: 'Meu(Minha) {} esta {:?}.'",
                    input.speaker_name,
                    part.kind.display_name(),
                    part.kind.display_name(),
                    part.status
                );
                break;
            }
        }
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
            addressed_agent_ids: primary_listener
                .into_iter()
                .map(|listener| listener.id)
                .collect(),
            economic_transfer: None,
            revealed_secret: None,
            make_promise: None,
            spread_rumor: None,
            shared_story: None,
            escrow_deposit: None,
            propose_meeting: None,
            meeting_response: None,
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

fn log_action_planner_time(actor_name: &str, actor_id: u64, duration_ms: u128) {
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("action_planner_response_times.txt")
    {
        use std::io::Write;
        let timestamp = chrono::Utc::now().to_rfc3339();
        let _ = writeln!(
            file,
            "[{}] Agent {} (ID: {}): {} ms",
            timestamp, actor_name, actor_id, duration_ms
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
        let start = std::time::Instant::now();
        let payload = serde_json::to_value(input).map_err(|error| LlmError::Schema {
            operation: "action_planning".to_string(),
            message: format!("failed to serialize action planning input: {}", error),
        })?;
        let res = self.fetch_message_content("action_planning", ACTION_PLANNER_PROMPT, &payload);
        let duration = start.elapsed().as_millis();
        log_action_planner_time(&input.actor_name, input.actor_id, duration);
        res
    }

    fn generate_thoughts(&self, input: &ThinkMakerInput) -> LlmResult<ThinkMakerOutput> {
        let payload = serde_json::to_value(input).map_err(|error| LlmError::Schema {
            operation: "thought_generation".to_string(),
            message: format!("failed to serialize thought generation input: {}", error),
        })?;
        let content =
            self.fetch_message_content("thought_generation", THINK_MAKER_PROMPT, &payload)?;
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
