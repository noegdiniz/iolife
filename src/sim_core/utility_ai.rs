use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReactiveStance {
    CowedCompliance,
    HumiliatedWithdrawal,
    HonorDefense,
    ColdRevenge,
    StatusDisplay,
    ProtectiveRetreat,
    PredatoryOpportunism,
    InstitutionalAssertion,
}

impl ReactiveStance {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::CowedCompliance => "cowed_compliance",
            Self::HumiliatedWithdrawal => "humiliated_withdrawal",
            Self::HonorDefense => "honor_defense",
            Self::ColdRevenge => "cold_revenge",
            Self::StatusDisplay => "status_display",
            Self::ProtectiveRetreat => "protective_retreat",
            Self::PredatoryOpportunism => "predatory_opportunism",
            Self::InstitutionalAssertion => "institutional_assertion",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum UtilityDirectiveKind {
    ConsumirComida,
    ComprarComida,
    ReceberPagamento,
    ContinuarTaskVital,
    Descansar,
    Fugir,
    Combater,
    BuscarSeguranca,
    ResponderDeverInstitucional,
    RetirarSe,
    ConfrontarOfensa,
    CobrarPromessa,
    BuscarPrivacidade,
    ExibirStatus,
    SubmeterSe,
    AfirmarAutoridade,
    PrepararVinganca,
}

impl UtilityDirectiveKind {
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Self::ConsumirComida => "consumir_comida",
            Self::ComprarComida => "comprar_comida",
            Self::ReceberPagamento => "receber_pagamento",
            Self::ContinuarTaskVital => "continuar_task_vital",
            Self::Descansar => "descansar",
            Self::Fugir => "fugir",
            Self::Combater => "combater",
            Self::BuscarSeguranca => "buscar_seguranca",
            Self::ResponderDeverInstitucional => "responder_dever_institucional",
            Self::RetirarSe => "retirar_se",
            Self::ConfrontarOfensa => "confrontar_ofensa",
            Self::CobrarPromessa => "cobrar_promessa",
            Self::BuscarPrivacidade => "buscar_privacidade",
            Self::ExibirStatus => "exibir_status",
            Self::SubmeterSe => "submeter_se",
            Self::AfirmarAutoridade => "afirmar_autoridade",
            Self::PrepararVinganca => "preparar_vinganca",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct UtilityDecision {
    pub kind: UtilityDirectiveKind,
    pub intent: AgentIntent,
    pub score: i32,
    pub thought: String,
    pub reason: String,
    pub stance: ReactiveStance,
    pub focus_target: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct RealtimeUtilityContext {
    pub position: TileCoord,
    pub hunger: i32,
    pub energy: i32,
    pub health: i32,
    pub stress: i32,
    pub pain: i32,
    pub bleeding: i32,
    pub fear_of_authority: i32,
    pub chaos_pressure: i32,
    pub household_has_food: bool,
    pub household_has_ready_food: bool,
    pub food_crisis_level: u8,
    pub has_pending_payments: bool,
    pub planner_pending: bool,
    pub active_combat_target: Option<u64>,
    pub urgent_legal_target: Option<u64>,
    pub role_id: String,
    pub psychological_state: PsychologicalState,
    pub self_prestige_score: i32,
    pub legal_risk: i32,
    pub witness_count: usize,
    pub relevant_social_target: Option<u64>,
    pub promise_grievance_target: Option<u64>,
    pub relevant_target_resentment: i32,
    pub relevant_target_trust: i32,
    pub prestige_gap_to_target: i32,
    pub recent_public_humiliation: bool,
    pub public_humiliation_by: Option<u64>,
    pub active_revenge_target: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct UtilityScoreBreakdown {
    pub consume_food: i32,
    pub buy_food: i32,
    pub collect_payment: i32,
    pub continue_vital_task: i32,
    pub rest: i32,
    pub flee: i32,
    pub fight: i32,
    pub seek_safety: i32,
    pub institutional_duty: i32,
    pub withdraw_social: i32,
    pub confront_offense: i32,
    pub demand_promise: i32,
    pub seek_privacy: i32,
    pub display_status: i32,
    pub submit: i32,
    pub prepare_revenge: i32,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ReactivePsychologySummary {
    pub stance: String,
    pub reason: String,
    pub target_agent_id: Option<u64>,
    pub status_pressure_summary: String,
    pub revenge_summary: String,
    pub public_shame_summary: String,
    pub authority_posture_summary: String,
    pub defiance_posture_summary: String,
    pub prestige_gap_summary: String,
    pub humiliation_risk_summary: String,
    pub deference_or_revenge_summary: String,
    pub audience_summary: String,
}

#[derive(Component, Debug, Clone, Default)]
pub struct UtilityControlComponent {
    pub active: Option<ActiveUtilityDirective>,
}

#[derive(Debug, Clone)]
pub struct ActiveUtilityDirective {
    pub kind: String,
    pub intent: AgentIntent,
    pub score: i32,
    pub thought: String,
    pub reason: String,
    pub stance: String,
    pub focus_target: Option<u64>,
}

impl Simulation {
    pub(super) fn planner_pending_for_agent(&self, agent_id: u64) -> bool {
        self.pending_action_plans
            .iter()
            .any(|pending| pending.agent_id == agent_id)
    }

    pub(super) fn active_utility_intent(&self, agent_id: u64) -> Result<Option<AgentIntent>> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<UtilityControlComponent>()
            .and_then(|control| control.active.as_ref())
            .map(|directive| directive.intent.clone()))
    }

    pub(super) fn utility_control_snapshot(
        &self,
        agent_id: u64,
    ) -> Result<Option<ActiveUtilityDirective>> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<UtilityControlComponent>()
            .and_then(|control| control.active.clone()))
    }

    pub(super) fn current_reactive_psychology_summary(
        &mut self,
        agent_id: u64,
    ) -> Result<ReactivePsychologySummary> {
        let context = self.build_realtime_utility_context(agent_id)?;
        Ok(self.build_reactive_psychology_summary(&context))
    }

    pub(super) fn mark_public_humiliation(
        &mut self,
        agent_id: u64,
        by_agent_id: Option<u64>,
        severity: i32,
        note: impl Into<String>,
    ) -> Result<()> {
        let note = note.into();
        let mut delta = PsychologicalState::zero_delta();
        delta.humiliation = severity.clamp(4, 25);
        delta.status_anxiety = (severity / 2).clamp(2, 15);
        delta.anger = (severity / 3).clamp(1, 10);
        delta.submission_drive = (severity / 4).clamp(0, 10);
        self.adjust_psychological_state(agent_id, delta, &note)?;

        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        let mut psychology = entity_mut
            .get_mut::<PsychologicalStateComponent>()
            .ok_or_else(|| anyhow!("missing psychological state component"))?;
        psychology.0.last_public_humiliation_tick = self.total_ticks;
        psychology.0.last_public_humiliation_by = by_agent_id;
        if severity >= 10 {
            psychology.0.active_revenge_target = by_agent_id;
            psychology.0.revenge_drive =
                (psychology.0.revenge_drive + (severity / 2).clamp(2, 18)).clamp(-100, 100);
            psychology.0.dominance_drive =
                (psychology.0.dominance_drive + (severity / 4).clamp(1, 10)).clamp(-100, 100);
        }
        psychology.0.clamp_all();
        Ok(())
    }

    pub(super) fn mark_revenge_target(
        &mut self,
        agent_id: u64,
        target_id: u64,
        severity: i32,
        note: impl Into<String>,
    ) -> Result<()> {
        let note = note.into();
        let mut delta = PsychologicalState::zero_delta();
        delta.revenge_drive = severity.clamp(4, 25);
        delta.anger = (severity / 2).clamp(2, 15);
        delta.dominance_drive = (severity / 3).clamp(1, 10);
        self.adjust_psychological_state(agent_id, delta, &note)?;

        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        let mut psychology = entity_mut
            .get_mut::<PsychologicalStateComponent>()
            .ok_or_else(|| anyhow!("missing psychological state component"))?;
        psychology.0.active_revenge_target = Some(target_id);
        psychology.0.clamp_all();
        Ok(())
    }

    pub(super) fn refresh_realtime_utility_control(&mut self, agent_id: u64) -> Result<()> {
        let planner_intent = self.sync_intent_with_locked_task(agent_id)?;
        let planner_intent = if planner_intent.is_some() {
            planner_intent
        } else {
            let entity = self.find_agent_entity(agent_id)?;
            self.world
                .entity(entity)
                .get::<IntentComponent>()
                .ok_or_else(|| anyhow!("missing intent component"))?
                .0
                .clone()
        };
        let entity = self.find_agent_entity(agent_id)?;
        let has_active_path = !self
            .world
            .entity(entity)
            .get::<PathComponent>()
            .ok_or_else(|| anyhow!("missing path component"))?
            .0
            .is_empty();
        let utility_already_active = self
            .world
            .entity(entity)
            .get::<UtilityControlComponent>()
            .and_then(|control| control.active.as_ref())
            .is_some();
        if planner_intent.is_none() && has_active_path && !utility_already_active {
            return Ok(());
        }

        let Some(decision) = self.evaluate_utility_for_agent(agent_id)? else {
            self.clear_utility_control(agent_id)?;
            return Ok(());
        };

        if !self.should_preempt_planner(agent_id, planner_intent.as_ref(), &decision)? {
            self.clear_utility_control(agent_id)?;
            return Ok(());
        }

        self.activate_utility_control(agent_id, decision)
    }

    pub(super) fn clear_utility_control(&mut self, agent_id: u64) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        self.world
            .entity_mut(entity)
            .get_mut::<UtilityControlComponent>()
            .ok_or_else(|| anyhow!("missing utility control component"))?
            .active = None;
        Ok(())
    }

    fn activate_utility_control(&mut self, agent_id: u64, decision: UtilityDecision) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let current = self
            .world
            .entity(entity)
            .get::<UtilityControlComponent>()
            .and_then(|control| control.active.clone());
        let changed = current
            .as_ref()
            .map(|directive| directive.kind != decision.kind.as_str())
            .unwrap_or(true)
            || current
                .as_ref()
                .map(|directive| {
                    directive.intent.kind != decision.intent.kind
                        || directive.intent.target_agent != decision.intent.target_agent
                        || directive.intent.target_semantic != decision.intent.target_semantic
                        || directive.intent.social_move != decision.intent.social_move
                        || directive.focus_target != decision.focus_target
                })
                .unwrap_or(true);

        {
            let mut entity_mut = self.world.entity_mut(entity);
            entity_mut
                .get_mut::<UtilityControlComponent>()
                .ok_or_else(|| anyhow!("missing utility control component"))?
                .active = Some(ActiveUtilityDirective {
                kind: decision.kind.as_str().to_string(),
                intent: decision.intent.clone(),
                score: decision.score,
                thought: decision.thought.clone(),
                reason: decision.reason.clone(),
                stance: decision.stance.as_str().to_string(),
                focus_target: decision.focus_target,
            });
            entity_mut
                .get_mut::<ThoughtComponent>()
                .ok_or_else(|| anyhow!("missing thought component"))?
                .0 = decision.thought.clone();
        }

        if changed {
            self.clear_navigation_keep_intent(agent_id)?;
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: agent_id,
                target: decision.focus_target.or(decision.intent.target_agent),
                kind: EventKind::Routine,
                summary: format!(
                    "{} assume controle utilitario [{}]: {}.",
                    self.agent_name(agent_id)?,
                    decision.stance.as_str(),
                    decision.reason
                ),
                impact_tags: vec![
                    "utility_ai".to_string(),
                    decision.kind.as_str().to_string(),
                    decision.stance.as_str().to_string(),
                ],
            });
        }
        Ok(())
    }

    fn should_preempt_planner(
        &mut self,
        agent_id: u64,
        planner_intent: Option<&AgentIntent>,
        decision: &UtilityDecision,
    ) -> Result<bool> {
        let current = self.utility_control_snapshot(agent_id)?;
        if current
            .as_ref()
            .map(|directive| directive.kind == decision.kind.as_str())
            .unwrap_or(false)
        {
            return Ok(true);
        }
        let planner_score = planner_intent
            .map(|intent| self.planner_intent_commitment_score(intent))
            .unwrap_or(0);
        let stance_bonus = match decision.stance {
            ReactiveStance::ProtectiveRetreat | ReactiveStance::InstitutionalAssertion => 10,
            ReactiveStance::HonorDefense | ReactiveStance::ColdRevenge => 5,
            _ => 0,
        };
        Ok(planner_intent.is_none() || decision.score + stance_bonus >= planner_score + 10)
    }

    fn planner_intent_commitment_score(&self, intent: &AgentIntent) -> i32 {
        match intent.kind {
            IntentKind::Agredir
            | IntentKind::Combater
            | IntentKind::Roubar
            | IntentKind::Fugir
            | IntentKind::Prender
            | IntentKind::Punir => 75,
            IntentKind::Comprar
            | IntentKind::Transportar
            | IntentKind::Vender
            | IntentKind::ReceberPagamento
            | IntentKind::Construir => 60,
            IntentKind::Trabalhar => 45,
            IntentKind::Socializar => 35,
            IntentKind::Descansar | IntentKind::Comer | IntentKind::Refletir => 30,
            _ => 25,
        }
    }

    fn evaluate_utility_for_agent(&mut self, agent_id: u64) -> Result<Option<UtilityDecision>> {
        let context = self.build_realtime_utility_context(agent_id)?;
        let breakdown = self.score_realtime_utility(&context, agent_id)?;
        let stance = self.reactive_stance_for_context(&context);
        let mut best: Option<UtilityDecision> = None;

        let mut consider = |candidate: Option<UtilityDecision>| {
            if let Some(candidate) = candidate
                && best
                    .as_ref()
                    .map(|current| candidate.score > current.score)
                    .unwrap_or(true)
            {
                best = Some(candidate);
            }
        };

        consider(self.consume_food_decision(agent_id, &context, breakdown.consume_food, stance)?);
        consider(self.buy_food_decision(agent_id, &context, breakdown.buy_food, stance)?);
        consider(self.collect_payment_decision(
            agent_id,
            &context,
            breakdown.collect_payment,
            stance,
        )?);
        consider(self.continue_vital_task_decision(
            agent_id,
            &context,
            breakdown.continue_vital_task,
            stance,
        )?);
        consider(self.rest_decision(agent_id, &context, breakdown.rest, stance)?);
        consider(self.flee_decision(agent_id, &context, breakdown.flee, stance)?);
        consider(self.fight_decision(agent_id, &context, breakdown.fight, stance)?);
        consider(self.seek_safety_decision(agent_id, &context, breakdown.seek_safety, stance)?);
        consider(self.institutional_duty_decision(
            agent_id,
            &context,
            breakdown.institutional_duty,
            stance,
        )?);
        consider(self.withdraw_social_decision(
            agent_id,
            &context,
            breakdown.withdraw_social,
            stance,
        )?);
        consider(self.confront_offense_decision(
            agent_id,
            &context,
            breakdown.confront_offense,
            stance,
        )?);
        consider(self.promise_grievance_decision(
            agent_id,
            &context,
            breakdown.demand_promise,
            stance,
        )?);
        consider(self.seek_privacy_decision(agent_id, &context, breakdown.seek_privacy, stance)?);
        consider(self.status_display_decision(
            agent_id,
            &context,
            breakdown.display_status,
            stance,
        )?);
        consider(self.submission_decision(agent_id, &context, breakdown.submit, stance)?);
        consider(self.prepare_revenge_decision(
            agent_id,
            &context,
            breakdown.prepare_revenge,
            stance,
        )?);

        Ok(best.filter(|decision| decision.score >= 35))
    }

    fn build_realtime_utility_context(&mut self, agent_id: u64) -> Result<RealtimeUtilityContext> {
        let state = self.agent_state(agent_id)?;
        let injury = self.agent_injury(agent_id)?;
        let institutional = self.institutional_perception(agent_id).unwrap_or_default();
        let role_id = self.agent_role_id(agent_id)?;
        let household_id = self.household_id_for_agent(agent_id);
        let household_has_food = self.household_has_food_available(agent_id)?;
        let household_has_ready_food = household_id
            .map(|id| self.household_has_ready_food_available(id))
            .unwrap_or(false);
        let household = household_id
            .and_then(|id| self.household_by_id(id))
            .cloned();
        let has_pending_payments = household
            .as_ref()
            .map(|household| !household.pending_payments.is_empty())
            .unwrap_or(false);
        let psychological_state = self.psychological_state_for_agent(agent_id)?;
        let position = self.agent_position(agent_id)?;
        let witness_count = self.witnesses_near(agent_id, position, 3).len();
        let active_combat_target = self.active_combat_target(agent_id);
        let urgent_legal_target = self.urgent_legal_target_for(agent_id);
        let promise_grievance_target = self.recent_broken_promise_target(agent_id);
        let relevant_social_target = self.relevant_social_target_for_utility(
            agent_id,
            position,
            active_combat_target,
            urgent_legal_target,
            promise_grievance_target,
            &psychological_state,
        )?;
        let (relevant_target_resentment, relevant_target_trust, prestige_gap_to_target) =
            if let Some(target_id) = relevant_social_target {
                let relation = self.relation_between(agent_id, target_id);
                (
                    relation.resentment,
                    relation.trust,
                    self.perceived_status_score(target_id) - self.perceived_status_score(agent_id),
                )
            } else {
                (0, 0, 0)
            };
        let recent_public_humiliation = psychological_state.last_public_humiliation_tick > 0
            && self
                .total_ticks
                .saturating_sub(psychological_state.last_public_humiliation_tick)
                <= 240;

        Ok(RealtimeUtilityContext {
            position,
            hunger: state.hunger,
            energy: state.energy,
            health: state.health,
            stress: state.stress,
            pain: injury.pain,
            bleeding: injury.bleeding,
            fear_of_authority: institutional.fear_of_authority,
            chaos_pressure: self.agent_chaos_pressure(agent_id).unwrap_or(0) as i32,
            household_has_food,
            household_has_ready_food,
            food_crisis_level: household.as_ref().map(|h| h.food_crisis_level).unwrap_or(0),
            has_pending_payments,
            planner_pending: self.planner_pending_for_agent(agent_id),
            active_combat_target,
            urgent_legal_target,
            role_id,
            psychological_state: psychological_state.clone(),
            self_prestige_score: self.perceived_status_score(agent_id),
            legal_risk: self.legal_risk_for(agent_id, relevant_social_target),
            witness_count,
            relevant_social_target,
            promise_grievance_target,
            relevant_target_resentment,
            relevant_target_trust,
            prestige_gap_to_target,
            recent_public_humiliation,
            public_humiliation_by: psychological_state.last_public_humiliation_by,
            active_revenge_target: psychological_state.active_revenge_target,
        })
    }

    fn reactive_stance_for_context(&self, context: &RealtimeUtilityContext) -> ReactiveStance {
        let psych = &context.psychological_state;
        let threat_pressure = (100 - context.health).max(0) + context.pain + context.bleeding * 3;
        if context.urgent_legal_target.is_some() && self.is_authority_role(&context.role_id) {
            return ReactiveStance::InstitutionalAssertion;
        }
        if context.active_combat_target.is_some()
            && (threat_pressure >= 55 || psych.fear >= psych.anger + 10)
        {
            return ReactiveStance::ProtectiveRetreat;
        }
        if context.recent_public_humiliation {
            if psych.pride + psych.anger + psych.dominance_drive
                >= psych.fear + psych.submission_drive + 10
            {
                return ReactiveStance::HonorDefense;
            }
            return ReactiveStance::HumiliatedWithdrawal;
        }
        if psych.active_revenge_target.is_some()
            && psych.revenge_drive + context.relevant_target_resentment > psych.fear + 10
        {
            return ReactiveStance::ColdRevenge;
        }
        if psych.status_anxiety >= 25 && context.witness_count > 0 {
            return ReactiveStance::StatusDisplay;
        }
        if psych.submission_drive + psych.fear >= psych.pride + psych.dominance_drive + 15 {
            return ReactiveStance::CowedCompliance;
        }
        if context.chaos_pressure >= 50 && psych.dominance_drive + psych.anger >= 35 {
            return ReactiveStance::PredatoryOpportunism;
        }
        ReactiveStance::ProtectiveRetreat
    }

    fn build_reactive_psychology_summary(
        &self,
        context: &RealtimeUtilityContext,
    ) -> ReactivePsychologySummary {
        let stance = self.reactive_stance_for_context(context);
        let psych = &context.psychological_state;
        let status_pressure_summary = if psych.status_anxiety >= 35 {
            "sente forte ansiedade de status e observa como esta sendo julgado".to_string()
        } else if psych.status_anxiety >= 15 {
            "esta sensivel a sinais de status e respeito".to_string()
        } else {
            "nao esta especialmente obcecado por status neste momento".to_string()
        };
        let revenge_summary = if let Some(target_id) = psych.active_revenge_target {
            format!("mantem impulso de vinganca contra agente {}", target_id)
        } else {
            "nao tem alvo de vinganca dominante agora".to_string()
        };
        let public_shame_summary = if context.recent_public_humiliation {
            "foi humilhado publicamente recentemente e isso ainda pesa".to_string()
        } else {
            "nao esta sob vergonha publica recente".to_string()
        };
        let authority_posture_summary = if self.is_authority_role(&context.role_id) {
            if context.urgent_legal_target.is_some() {
                "sente dever de afirmar autoridade diante de desordem".to_string()
            } else {
                "carrega postura institucional, mas sem urgencia legal imediata".to_string()
            }
        } else if context.fear_of_authority >= 30 {
            "teme a autoridade e mede o risco institucional das acoes".to_string()
        } else {
            "nao esta particularmente orientado pela autoridade agora".to_string()
        };
        let defiance_posture_summary =
            if psych.dominance_drive + psych.anger > psych.submission_drive + psych.fear {
                "inclinado a desafiar e testar limites".to_string()
            } else {
                "inclinado a ceder ou evitar confronto direto".to_string()
            };
        let prestige_gap_summary = if context.relevant_social_target.is_some() {
            if context.prestige_gap_to_target >= 15 {
                "o outro parece socialmente acima dele".to_string()
            } else if context.prestige_gap_to_target <= -15 {
                "ele se percebe acima do outro em status visivel".to_string()
            } else {
                "nao percebe grande abismo de prestigio no alvo relevante".to_string()
            }
        } else {
            "sem alvo social relevante para comparar prestigio agora".to_string()
        };
        let humiliation_risk_summary =
            if context.witness_count >= 2 || context.recent_public_humiliation {
                "ha plateia suficiente para transformar conflito em vergonha publica".to_string()
            } else {
                "o risco imediato de vergonha publica e moderado".to_string()
            };
        let deference_or_revenge_summary =
            if psych.active_revenge_target.is_some() && psych.revenge_drive >= 20 {
                "tende mais a cobrar, retaliar ou guardar rancor do que a ceder".to_string()
            } else if psych.submission_drive + psych.fear >= 30 {
                "tende a se submeter quando o custo social ou legal parece alto".to_string()
            } else {
                "oscila entre prudencia e afirmacao de si".to_string()
            };
        let audience_summary = if context.witness_count == 0 {
            "sem plateia relevante por perto".to_string()
        } else {
            format!(
                "ha {} testemunhas ou observadores proximos",
                context.witness_count
            )
        };
        let reason = match stance {
            ReactiveStance::InstitutionalAssertion => {
                "dever institucional urgente domina a postura".to_string()
            }
            ReactiveStance::ProtectiveRetreat => {
                "risco fisico alto empurra para protecao e retirada".to_string()
            }
            ReactiveStance::HonorDefense => "orgulho ferido e raiva superam o medo".to_string(),
            ReactiveStance::HumiliatedWithdrawal => {
                "vergonha publica e medo puxam retirada e recolhimento".to_string()
            }
            ReactiveStance::ColdRevenge => {
                "ressentimento persistente organiza a reacao em torno de vinganca".to_string()
            }
            ReactiveStance::StatusDisplay => {
                "ansiedade de status e plateia elevam a necessidade de parecer relevante"
                    .to_string()
            }
            ReactiveStance::CowedCompliance => {
                "medo, submissao e risco de sancao puxam obediencia".to_string()
            }
            ReactiveStance::PredatoryOpportunism => {
                "caos, dominancia e baixo freio legal abrem oportunismo agressivo".to_string()
            }
        };

        ReactivePsychologySummary {
            stance: stance.as_str().to_string(),
            reason,
            target_agent_id: context
                .relevant_social_target
                .or(context.active_revenge_target)
                .or(context.urgent_legal_target)
                .or(context.active_combat_target),
            status_pressure_summary,
            revenge_summary,
            public_shame_summary,
            authority_posture_summary,
            defiance_posture_summary,
            prestige_gap_summary,
            humiliation_risk_summary,
            deference_or_revenge_summary,
            audience_summary,
        }
    }

    fn score_realtime_utility(
        &mut self,
        context: &RealtimeUtilityContext,
        agent_id: u64,
    ) -> Result<UtilityScoreBreakdown> {
        let active_task = self.active_economic_task_for_agent(agent_id).cloned();
        let hunger_pressure = (context.hunger - 35).max(0);
        let fatigue_pressure = (35 - context.energy).max(0);
        let danger_pressure = (100 - context.health).max(0) + context.pain + context.bleeding * 2;
        let chaos_bonus = (context.chaos_pressure / 8).max(0);
        let psych = &context.psychological_state;
        let stance = self.reactive_stance_for_context(context);
        let mut scores = UtilityScoreBreakdown::default();

        if context.household_has_food {
            scores.consume_food = hunger_pressure + 35;
            if context.household_has_ready_food {
                scores.consume_food += 10;
            }
        }
        if context.hunger >= 55 && !context.household_has_food {
            scores.buy_food = hunger_pressure + 30 + i32::from(context.food_crisis_level) * 5;
            if active_task
                .as_ref()
                .map(|task| task.kind == EconomicTaskKind::Comprar)
                .unwrap_or(false)
            {
                scores.buy_food += 20;
            }
        }
        if context.has_pending_payments {
            scores.collect_payment =
                40 + hunger_pressure / 2 + i32::from(context.food_crisis_level) * 3;
        }
        if let Some(task) = active_task.as_ref()
            && task.lock_until_complete
        {
            scores.continue_vital_task = i32::from(task.priority.clamp(1, 10)) * 8;
            if matches!(
                task.class,
                EconomicTaskClass::HouseholdFoodPurchase
                    | EconomicTaskClass::FoodSupplyTransport
                    | EconomicTaskClass::FoodProduction
                    | EconomicTaskClass::EssentialProduction
                    | EconomicTaskClass::MilitarySupply
            ) {
                scores.continue_vital_task += 15;
            }
        }
        if context.energy <= 25 {
            scores.rest = fatigue_pressure + 45;
        }
        if context.active_combat_target.is_some() {
            scores.fight = 70 + chaos_bonus + psych.dominance_drive.max(0) / 4;
            scores.flee = 55 + danger_pressure / 3 + psych.fear.max(0) / 3;
            if context.health <= 35 || context.bleeding >= 5 {
                scores.flee += 20;
            }
        }
        if context.health <= 30 && context.active_combat_target.is_none() {
            scores.seek_safety = 50 + danger_pressure / 2;
        }
        if context.urgent_legal_target.is_some() && self.is_authority_role(&context.role_id) {
            scores.institutional_duty =
                65 + context.fear_of_authority.max(0) / 4 + psych.pride.max(0) / 5;
        }
        if context.recent_public_humiliation {
            scores.withdraw_social = psych.humiliation.max(0)
                + psych.submission_drive.max(0) / 2
                + context.fear_of_authority.max(0) / 4;
            scores.confront_offense =
                psych.pride.max(0) / 2 + psych.anger.max(0) / 2 + psych.dominance_drive.max(0) / 3;
            scores.seek_privacy = psych.humiliation.max(0) / 2 + psych.fear.max(0) / 3;
        }
        if context.promise_grievance_target.is_some() {
            scores.demand_promise =
                45 + context.relevant_target_resentment.max(0) / 2 + psych.revenge_drive.max(0) / 3;
        }
        if context.witness_count > 0 && psych.status_anxiety >= 20 {
            scores.display_status =
                psych.status_anxiety + psych.pride.max(0) / 2 + psych.dominance_drive.max(0) / 3;
        }
        if context.relevant_social_target.is_some() && context.legal_risk >= 10 {
            scores.submit = psych.submission_drive.max(0)
                + psych.fear.max(0) / 2
                + context.fear_of_authority.max(0) / 3;
        }
        if context.active_revenge_target.is_some() {
            scores.prepare_revenge =
                35 + psych.revenge_drive.max(0) / 2 + context.relevant_target_resentment.max(0) / 2
                    - context.legal_risk / 4;
        }
        match stance {
            ReactiveStance::InstitutionalAssertion => {
                scores.institutional_duty += 15;
                scores.submit = 0;
            }
            ReactiveStance::ProtectiveRetreat => {
                scores.flee += 15;
                scores.seek_safety += 10;
                scores.fight -= 10;
            }
            ReactiveStance::HonorDefense => {
                scores.confront_offense += 20;
                scores.withdraw_social -= 10;
                scores.submit -= 10;
            }
            ReactiveStance::HumiliatedWithdrawal => {
                scores.withdraw_social += 20;
                scores.seek_privacy += 15;
                scores.confront_offense -= 10;
            }
            ReactiveStance::ColdRevenge => {
                scores.prepare_revenge += 20;
                scores.confront_offense += 5;
            }
            ReactiveStance::StatusDisplay => {
                scores.display_status += 20;
            }
            ReactiveStance::CowedCompliance => {
                scores.submit += 20;
                scores.withdraw_social += 5;
            }
            ReactiveStance::PredatoryOpportunism => {
                scores.confront_offense += 10;
                scores.prepare_revenge += 10;
            }
        }
        if context.planner_pending {
            scores.consume_food += 5;
            scores.buy_food += 5;
            scores.continue_vital_task += 5;
            scores.institutional_duty += 5;
            scores.withdraw_social += 5;
            scores.prepare_revenge += 5;
        }

        Ok(scores)
    }

    fn consume_food_decision(
        &mut self,
        agent_id: u64,
        context: &RealtimeUtilityContext,
        score: i32,
        stance: ReactiveStance,
    ) -> Result<Option<UtilityDecision>> {
        if score <= 0
            || !context.household_has_food
            || context.hunger < 85
            || !self.is_most_hungry_household_member(agent_id, context.hunger)?
        {
            return Ok(None);
        }
        Ok(Some(UtilityDecision {
            kind: UtilityDirectiveKind::ConsumirComida,
            intent: AgentIntent {
                kind: IntentKind::Comer,
                target_agent: None,
                target_semantic: Some("comida da despensa".to_string()),
                justification: "Prioridade local: resolver a fome imediatamente.".to_string(),
                dominant_emotion: "urgencia".to_string(),
                perceived_risk: 1,
                belief_updates: vec!["A fome precisa ser reduzida agora.".to_string()],
                priority: 10,
                social_move: None,
            },
            score,
            thought: "Minha fome domina o momento; preciso comer agora.".to_string(),
            reason: "fome com alimento imediato disponivel".to_string(),
            stance,
            focus_target: None,
        }))
    }

    fn buy_food_decision(
        &mut self,
        _agent_id: u64,
        context: &RealtimeUtilityContext,
        score: i32,
        stance: ReactiveStance,
    ) -> Result<Option<UtilityDecision>> {
        if score <= 0 || context.household_has_food {
            return Ok(None);
        }
        Ok(Some(UtilityDecision {
            kind: UtilityDirectiveKind::ComprarComida,
            intent: AgentIntent {
                kind: IntentKind::Comprar,
                target_agent: None,
                target_semantic: Some("comida para a despensa".to_string()),
                justification: "Sem comida no lar, abastecimento imediato e prioritario."
                    .to_string(),
                dominant_emotion: "urgencia".to_string(),
                perceived_risk: 4,
                belief_updates: vec!["Preciso repor comida antes que a fome piore.".to_string()],
                priority: 10,
                social_move: None,
            },
            score,
            thought: "Sem comida em casa, preciso abastecer o lar antes de qualquer outra coisa."
                .to_string(),
            reason: "abastecimento alimentar emergente".to_string(),
            stance,
            focus_target: None,
        }))
    }

    fn collect_payment_decision(
        &mut self,
        _agent_id: u64,
        context: &RealtimeUtilityContext,
        score: i32,
        stance: ReactiveStance,
    ) -> Result<Option<UtilityDecision>> {
        if score <= 0 || !context.has_pending_payments {
            return Ok(None);
        }
        Ok(Some(UtilityDecision {
            kind: UtilityDirectiveKind::ReceberPagamento,
            intent: AgentIntent {
                kind: IntentKind::ReceberPagamento,
                target_agent: None,
                target_semantic: Some("pagamentos pendentes".to_string()),
                justification: "Recuperar caixa do lar e destravar recursos urgentes.".to_string(),
                dominant_emotion: "determinado".to_string(),
                perceived_risk: 2,
                belief_updates: vec!["Sem receber, o lar segue pressionado.".to_string()],
                priority: 8,
                social_move: None,
            },
            score,
            thought: "Ha pagamento pendente; preciso recuperar esse recurso agora.".to_string(),
            reason: "caixa do lar sob pressao".to_string(),
            stance,
            focus_target: None,
        }))
    }

    fn continue_vital_task_decision(
        &mut self,
        agent_id: u64,
        _context: &RealtimeUtilityContext,
        score: i32,
        stance: ReactiveStance,
    ) -> Result<Option<UtilityDecision>> {
        if score <= 0 {
            return Ok(None);
        }
        let Some(task) = self.active_economic_task_for_agent(agent_id).cloned() else {
            return Ok(None);
        };
        Ok(Some(UtilityDecision {
            kind: UtilityDirectiveKind::ContinuarTaskVital,
            intent: Self::intent_for_economic_task(&task),
            score,
            thought: format!(
                "Preciso concluir a tarefa vital em curso: {}.",
                task.description
            ),
            reason: format!("continuidade de tarefa vital: {}", task.description),
            stance,
            focus_target: None,
        }))
    }

    fn rest_decision(
        &mut self,
        _agent_id: u64,
        context: &RealtimeUtilityContext,
        score: i32,
        stance: ReactiveStance,
    ) -> Result<Option<UtilityDecision>> {
        if score <= 0 || context.active_combat_target.is_some() {
            return Ok(None);
        }
        Ok(Some(UtilityDecision {
            kind: UtilityDirectiveKind::Descansar,
            intent: AgentIntent {
                kind: IntentKind::Descansar,
                target_agent: None,
                target_semantic: Some("cama".to_string()),
                justification: "Recuperar energia antes de colapsar.".to_string(),
                dominant_emotion: "exaustao".to_string(),
                perceived_risk: 1,
                belief_updates: vec!["Sem energia, minhas outras acoes perdem valor.".to_string()],
                priority: 8,
                social_move: None,
            },
            score,
            thought: "Estou exausto demais para manter o resto da rotina com eficacia.".to_string(),
            reason: "exaustao fisica".to_string(),
            stance,
            focus_target: None,
        }))
    }

    fn flee_decision(
        &mut self,
        _agent_id: u64,
        context: &RealtimeUtilityContext,
        score: i32,
        stance: ReactiveStance,
    ) -> Result<Option<UtilityDecision>> {
        if score <= 0 || context.active_combat_target.is_none() {
            return Ok(None);
        }
        Ok(Some(UtilityDecision {
            kind: UtilityDirectiveKind::Fugir,
            intent: AgentIntent {
                kind: IntentKind::Fugir,
                target_agent: None,
                target_semantic: None,
                justification: "Sobreviver e sair do perigo imediato tem prioridade total."
                    .to_string(),
                dominant_emotion: "panico".to_string(),
                perceived_risk: 9,
                belief_updates: vec!["Se eu ficar, posso morrer.".to_string()],
                priority: 10,
                social_move: None,
            },
            score,
            thought: "Se eu nao fugir agora, o combate pode me destruir.".to_string(),
            reason: "risco fisico agudo".to_string(),
            stance,
            focus_target: context.active_combat_target,
        }))
    }

    fn fight_decision(
        &mut self,
        _agent_id: u64,
        context: &RealtimeUtilityContext,
        score: i32,
        stance: ReactiveStance,
    ) -> Result<Option<UtilityDecision>> {
        let Some(target_id) = context.active_combat_target else {
            return Ok(None);
        };
        if score <= 0 {
            return Ok(None);
        }
        Ok(Some(UtilityDecision {
            kind: UtilityDirectiveKind::Combater,
            intent: AgentIntent {
                kind: IntentKind::Combater,
                target_agent: Some(target_id),
                target_semantic: None,
                justification: "Ja estou em confronto; preciso reagir ao inimigo imediato."
                    .to_string(),
                dominant_emotion: "furia".to_string(),
                perceived_risk: 8,
                belief_updates: vec!["O perigo esta diante de mim.".to_string()],
                priority: 10,
                social_move: None,
            },
            score,
            thought: "O conflito ja esta aberto; preciso responder ao atacante agora.".to_string(),
            reason: "combate ativo".to_string(),
            stance,
            focus_target: Some(target_id),
        }))
    }

    fn seek_safety_decision(
        &mut self,
        agent_id: u64,
        _context: &RealtimeUtilityContext,
        score: i32,
        stance: ReactiveStance,
    ) -> Result<Option<UtilityDecision>> {
        if score <= 0 {
            return Ok(None);
        }
        let target_semantic = self
            .agent_home_building_id(agent_id)?
            .map(|building_id| format!("building:{building_id}"));
        Ok(Some(UtilityDecision {
            kind: UtilityDirectiveKind::BuscarSeguranca,
            intent: AgentIntent {
                kind: IntentKind::Andar,
                target_agent: None,
                target_semantic,
                justification: "Preciso me afastar e buscar um lugar mais seguro.".to_string(),
                dominant_emotion: "medo".to_string(),
                perceived_risk: 6,
                belief_updates: vec!["Ficar exposto agora seria imprudente.".to_string()],
                priority: 8,
                social_move: None,
            },
            score,
            thought: "Meu corpo esta fraco demais; preciso buscar abrigo e reduzir a exposicao."
                .to_string(),
            reason: "seguranca imediata".to_string(),
            stance,
            focus_target: None,
        }))
    }

    fn institutional_duty_decision(
        &mut self,
        _agent_id: u64,
        context: &RealtimeUtilityContext,
        score: i32,
        stance: ReactiveStance,
    ) -> Result<Option<UtilityDecision>> {
        if score <= 0 {
            return Ok(None);
        }
        let intent = if let Some(target_id) = context.urgent_legal_target {
            AgentIntent {
                kind: IntentKind::Prender,
                target_agent: Some(target_id),
                target_semantic: None,
                justification: "Um alvo institucional urgente exige acao imediata.".to_string(),
                dominant_emotion: "dever".to_string(),
                perceived_risk: 7,
                belief_updates: vec!["Nao posso ignorar esta urgencia institucional.".to_string()],
                priority: 9,
                social_move: None,
            }
        } else {
            AgentIntent {
                kind: IntentKind::Investigar,
                target_agent: None,
                target_semantic: None,
                justification: "Ha pressao institucional urgente pedindo resposta agora."
                    .to_string(),
                dominant_emotion: "vigilancia".to_string(),
                perceived_risk: 5,
                belief_updates: vec!["Meu papel exige resposta imediata.".to_string()],
                priority: 8,
                social_move: None,
            }
        };
        Ok(Some(UtilityDecision {
            kind: UtilityDirectiveKind::ResponderDeverInstitucional,
            intent,
            score,
            thought: "Meu posto exige resposta agora; nao posso largar essa urgencia.".to_string(),
            reason: "dever institucional urgente".to_string(),
            stance,
            focus_target: context.urgent_legal_target,
        }))
    }

    fn withdraw_social_decision(
        &mut self,
        agent_id: u64,
        context: &RealtimeUtilityContext,
        score: i32,
        stance: ReactiveStance,
    ) -> Result<Option<UtilityDecision>> {
        if score <= 0
            || !matches!(
                stance,
                ReactiveStance::HumiliatedWithdrawal | ReactiveStance::CowedCompliance
            )
        {
            return Ok(None);
        }
        Ok(Some(UtilityDecision {
            kind: UtilityDirectiveKind::RetirarSe,
            intent: AgentIntent {
                kind: IntentKind::Andar,
                target_agent: None,
                target_semantic: self.private_retreat_place(agent_id)?,
                justification: "Preciso sair de cena e reduzir a exposicao social imediata."
                    .to_string(),
                dominant_emotion: "vergonha".to_string(),
                perceived_risk: 4,
                belief_updates: vec!["Ficar em publico agora so piora a humilhacao.".to_string()],
                priority: 8,
                social_move: None,
            },
            score,
            thought: "Preciso me retirar antes que esta humilhacao se agrave diante dos outros."
                .to_string(),
            reason: "retirada social apos vergonha ou submissao".to_string(),
            stance,
            focus_target: context.public_humiliation_by,
        }))
    }

    fn confront_offense_decision(
        &mut self,
        _agent_id: u64,
        context: &RealtimeUtilityContext,
        score: i32,
        stance: ReactiveStance,
    ) -> Result<Option<UtilityDecision>> {
        let Some(target_id) = context
            .public_humiliation_by
            .or(context.relevant_social_target)
        else {
            return Ok(None);
        };
        if score <= 0
            || !matches!(
                stance,
                ReactiveStance::HonorDefense | ReactiveStance::PredatoryOpportunism
            )
        {
            return Ok(None);
        }
        Ok(Some(UtilityDecision {
            kind: UtilityDirectiveKind::ConfrontarOfensa,
            intent: AgentIntent {
                kind: IntentKind::Socializar,
                target_agent: Some(target_id),
                target_semantic: None,
                justification: "Minha honra pede confronto imediato com a ofensa sofrida."
                    .to_string(),
                dominant_emotion: "raiva".to_string(),
                perceived_risk: 6,
                belief_updates: vec!["Nao posso deixar esta ofensa sem resposta.".to_string()],
                priority: 9,
                social_move: Some(SocialMove::Offend),
            },
            score,
            thought: "Se eu aceitar isso calado, perco mais do que uma disputa: perco rosto."
                .to_string(),
            reason: "defesa imediata da honra".to_string(),
            stance,
            focus_target: Some(target_id),
        }))
    }

    fn promise_grievance_decision(
        &mut self,
        _agent_id: u64,
        context: &RealtimeUtilityContext,
        score: i32,
        stance: ReactiveStance,
    ) -> Result<Option<UtilityDecision>> {
        let Some(target_id) = context.promise_grievance_target else {
            return Ok(None);
        };
        if score <= 0 {
            return Ok(None);
        }
        Ok(Some(UtilityDecision {
            kind: UtilityDirectiveKind::CobrarPromessa,
            intent: AgentIntent {
                kind: IntentKind::Socializar,
                target_agent: Some(target_id),
                target_semantic: None,
                justification:
                    "Ha uma promessa quebrada ou duvidosa que precisa ser cobrada agora."
                        .to_string(),
                dominant_emotion: "desconfianca".to_string(),
                perceived_risk: 4,
                belief_updates: vec!["Nao devo deixar a divida moral sumir no tempo.".to_string()],
                priority: 8,
                social_move: Some(SocialMove::Chat),
            },
            score,
            thought: "Ele me deve explicacoes; preciso cobrar isso antes que vire vazio."
                .to_string(),
            reason: "cobranca de promessa ou divida moral recente".to_string(),
            stance,
            focus_target: Some(target_id),
        }))
    }

    fn seek_privacy_decision(
        &mut self,
        agent_id: u64,
        _context: &RealtimeUtilityContext,
        score: i32,
        stance: ReactiveStance,
    ) -> Result<Option<UtilityDecision>> {
        if score <= 0 {
            return Ok(None);
        }
        Ok(Some(UtilityDecision {
            kind: UtilityDirectiveKind::BuscarPrivacidade,
            intent: AgentIntent {
                kind: IntentKind::Andar,
                target_agent: None,
                target_semantic: self.private_retreat_place(agent_id)?,
                justification:
                    "Preciso de um lugar mais reservado para recompor postura e controle."
                        .to_string(),
                dominant_emotion: "introspeccao".to_string(),
                perceived_risk: 2,
                belief_updates: vec![
                    "Privacidade reduz a exposicao e me devolve controle.".to_string(),
                ],
                priority: 6,
                social_move: None,
            },
            score,
            thought: "Um pouco de privacidade agora vale mais do que continuar exposto."
                .to_string(),
            reason: "recomposicao em ambiente privado".to_string(),
            stance,
            focus_target: None,
        }))
    }

    fn status_display_decision(
        &mut self,
        _agent_id: u64,
        context: &RealtimeUtilityContext,
        score: i32,
        stance: ReactiveStance,
    ) -> Result<Option<UtilityDecision>> {
        let Some(target_id) = context.relevant_social_target else {
            return Ok(None);
        };
        if score <= 0 || !matches!(stance, ReactiveStance::StatusDisplay) {
            return Ok(None);
        }
        Ok(Some(UtilityDecision {
            kind: UtilityDirectiveKind::ExibirStatus,
            intent: AgentIntent {
                kind: IntentKind::Socializar,
                target_agent: Some(target_id),
                target_semantic: None,
                justification: "Preciso parecer composto, digno e relevante diante dos outros."
                    .to_string(),
                dominant_emotion: "orgulho".to_string(),
                perceived_risk: 3,
                belief_updates: vec![
                    "Status visivel influencia como os outros me tratam.".to_string(),
                ],
                priority: 6,
                social_move: Some(SocialMove::Chat),
            },
            score,
            thought: "Se eu nao ocupar o espaco social agora, outros me lerao como fraco."
                .to_string(),
            reason: "afirmacao simbolica de status".to_string(),
            stance,
            focus_target: Some(target_id),
        }))
    }

    fn submission_decision(
        &mut self,
        _agent_id: u64,
        context: &RealtimeUtilityContext,
        score: i32,
        stance: ReactiveStance,
    ) -> Result<Option<UtilityDecision>> {
        let Some(target_id) = context
            .relevant_social_target
            .or(context.urgent_legal_target)
        else {
            return Ok(None);
        };
        if score <= 0 || !matches!(stance, ReactiveStance::CowedCompliance) {
            return Ok(None);
        }
        Ok(Some(UtilityDecision {
            kind: UtilityDirectiveKind::SubmeterSe,
            intent: AgentIntent {
                kind: IntentKind::Socializar,
                target_agent: Some(target_id),
                target_semantic: None,
                justification: "Submissao tatica reduz risco social e institucional imediato."
                    .to_string(),
                dominant_emotion: "cautela".to_string(),
                perceived_risk: 2,
                belief_updates: vec![
                    "Conter o confronto agora pode me poupar dano maior.".to_string(),
                ],
                priority: 7,
                social_move: Some(SocialMove::Reconcile),
            },
            score,
            thought: "Melhor ceder agora do que pagar um preco maior por desafio aberto."
                .to_string(),
            reason: "obediencia taticamente motivada por medo".to_string(),
            stance,
            focus_target: Some(target_id),
        }))
    }

    fn prepare_revenge_decision(
        &mut self,
        _agent_id: u64,
        context: &RealtimeUtilityContext,
        score: i32,
        stance: ReactiveStance,
    ) -> Result<Option<UtilityDecision>> {
        let Some(target_id) = context.active_revenge_target else {
            return Ok(None);
        };
        if score <= 0
            || !matches!(
                stance,
                ReactiveStance::ColdRevenge | ReactiveStance::PredatoryOpportunism
            )
        {
            return Ok(None);
        }
        Ok(Some(UtilityDecision {
            kind: UtilityDirectiveKind::PrepararVinganca,
            intent: AgentIntent {
                kind: IntentKind::Socializar,
                target_agent: Some(target_id),
                target_semantic: None,
                justification: "Preciso recolocar a divida moral e a ofensa na frente do alvo.".to_string(),
                dominant_emotion: "frieza".to_string(),
                perceived_risk: 5,
                belief_updates: vec!["A vinganca nao precisa ser cega para ser real.".to_string()],
                priority: 7,
                social_move: Some(SocialMove::Offend),
            },
            score,
            thought: "Ainda nao esqueci; quero reposicionar a relacao em meu favor antes de golpear mais fundo.".to_string(),
            reason: "pressao persistente de vinganca".to_string(),
            stance,
            focus_target: Some(target_id),
        }))
    }

    fn active_combat_target(&self, agent_id: u64) -> Option<u64> {
        self.combats
            .iter()
            .find(|combat| {
                combat.status == CombatStatus::Active && combat.participants.contains(&agent_id)
            })
            .and_then(|combat| {
                combat
                    .participants
                    .iter()
                    .copied()
                    .find(|participant| *participant != agent_id)
            })
    }

    fn urgent_legal_target_for(&self, agent_id: u64) -> Option<u64> {
        self.crime_cases
            .iter()
            .find(|case| {
                matches!(
                    case.status,
                    CrimeCaseStatus::Open
                        | CrimeCaseStatus::Investigating
                        | CrimeCaseStatus::Proven
                ) && (case.witnesses.contains(&agent_id) || case.severity >= 60)
            })
            .and_then(|case| case.suspect_id)
    }

    fn recent_broken_promise_target(&self, agent_id: u64) -> Option<u64> {
        self.secrets
            .iter()
            .rev()
            .find(|secret| {
                secret.kind == SecretKind::BrokenPromise && secret.known_by.contains(&agent_id)
            })
            .map(|secret| secret.target_id)
    }

    fn relevant_social_target_for_utility(
        &mut self,
        agent_id: u64,
        position: TileCoord,
        combat_target: Option<u64>,
        legal_target: Option<u64>,
        promise_target: Option<u64>,
        psychology: &PsychologicalState,
    ) -> Result<Option<u64>> {
        if let Some(target_id) = combat_target {
            return Ok(Some(target_id));
        }
        if let Some(target_id) = psychology.last_public_humiliation_by
            && self
                .agent_distance_from_immutable(position, target_id)
                .is_some_and(|d| d <= 3)
        {
            return Ok(Some(target_id));
        }
        if let Some(target_id) = psychology.active_revenge_target
            && self
                .agent_distance_from_immutable(position, target_id)
                .is_some_and(|d| d <= 4)
        {
            return Ok(Some(target_id));
        }
        if let Some(target_id) = promise_target
            && self
                .agent_distance_from_immutable(position, target_id)
                .is_some_and(|d| d <= 4)
        {
            return Ok(Some(target_id));
        }
        if let Some(target_id) = legal_target
            && self
                .agent_distance_from_immutable(position, target_id)
                .is_some_and(|d| d <= 5)
        {
            return Ok(Some(target_id));
        }
        let current_room_id = self.tile_at(position).and_then(|tile| tile.room_id);
        let empty_relations = std::collections::HashMap::new();
        Ok(self
            .nearby_agent_inputs(agent_id, position, current_room_id, &empty_relations)
            .into_iter()
            .find(|other| other.id != agent_id)
            .map(|other| other.id))
    }

    fn private_retreat_place(&mut self, agent_id: u64) -> Result<Option<String>> {
        if let Some(building_id) = self.agent_home_building_id(agent_id)? {
            return Ok(Some(format!("building:{building_id}")));
        }
        Ok(Some("porta_externa".to_string()))
    }

    fn legal_risk_for(&mut self, agent_id: u64, target_id: Option<u64>) -> i32 {
        let mut risk = self
            .institutional_perception(agent_id)
            .map(|p| p.fear_of_authority.max(0) / 4)
            .unwrap_or(0);
        if let Some(target_id) = target_id {
            let relation = self.relation_between(agent_id, target_id);
            if relation.reputation < 0 {
                risk += 2;
            }
        }
        risk += self
            .crime_cases
            .iter()
            .filter(|case| {
                matches!(
                    case.status,
                    CrimeCaseStatus::Open
                        | CrimeCaseStatus::Investigating
                        | CrimeCaseStatus::Proven
                ) && (case.suspect_id == Some(agent_id) || case.victim_id == Some(agent_id))
            })
            .count() as i32
            * 3;
        risk.clamp(0, 40)
    }

    fn is_authority_role(&self, role_id: &str) -> bool {
        role_id.contains("guarda")
            || role_id.contains("lider")
            || role_id.contains("capitao")
            || role_id.contains("oficial")
    }

    fn is_most_hungry_household_member(&mut self, agent_id: u64, hunger: i32) -> Result<bool> {
        let Some(household_id) = self.household_id_for_agent(agent_id) else {
            return Ok(true);
        };
        let mut query = self
            .world
            .query::<(&AgentCore, &StateComponent, &LifeStatusComponent)>();
        for (core, state, life_status) in query.iter(&self.world) {
            if core.id != agent_id
                && core.home_building_id == Some(household_id)
                && life_status.0 != AgentLifeStatus::Morto
                && state.0.hunger > hunger
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
